//! Windows shell context menu (task T1.16): `IContextMenu` from the folder's
//! `IShellFolder`, shown by a hidden window on a COM STA thread. Submenus the
//! shell draws itself ("Open with", "Send to") get their messages through
//! `IContextMenu3::HandleMenuMsg2`.
//!
//! Commands fx must run itself (so they go through its queue, Recycle Bin and
//! undo) come back as verbs; everything else is invoked by the shell.

use std::cell::RefCell;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Ole::OleInitialize;
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    CMF_CANRENAME, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
    GCS_VERBW, IContextMenu, IContextMenu3, IShellFolder, SHBindToParent, SHGetDesktopFolder,
    SHParseDisplayName,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, GetCursorPos,
    MF_SEPARATOR, MF_STRING, PostMessageW, RegisterClassW, SW_SHOWNORMAL, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WINDOW_EX_STYLE, WM_DRAWITEM,
    WM_INITMENUPOPUP, WM_MEASUREITEM, WM_MENUCHAR, WM_NULL, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::{Interface, PCSTR, PCWSTR, PSTR, w};

use crate::menu::{
    MENU_PREPARE_LIMIT, MenuOutcome, MenuReply, MenuRequest, MenuService, MenuShower,
};

/// Menu ids: fx's items from 1, the shell's from `SHELL_FIRST`.
const FX_FIRST: u32 = 1;
const SHELL_FIRST: u32 = 0x100;
const SHELL_LAST: u32 = 0x7FFF;
/// `CMIC_MASK_UNICODE` (not in this windows-rs version).
const CMIC_MASK_UNICODE: u32 = 0x4000;
/// Shell verbs fx performs itself.
const OWN_VERBS: [&str; 5] = ["delete", "rename", "copy", "cut", "paste"];

thread_local! {
    /// The menu being shown, for the messages its owner-drawn parts need.
    static ACTIVE: RefCell<Option<IContextMenu3>> = const { RefCell::new(None) };
    static HOST: RefCell<Option<HWND>> = const { RefCell::new(None) };
}

unsafe extern "system" fn host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(
        msg,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        let handled = ACTIVE.with(|a| {
            let menu = a.try_borrow().ok()?.clone()?;
            let mut result = LRESULT(0);
            // SAFETY: `menu` is the live context menu of this thread; the
            // message parameters come straight from the system.
            unsafe { menu.HandleMenuMsg2(msg, wparam, lparam, Some(&mut result)) }
                .ok()
                .map(|()| result)
        });
        if let Some(r) = handled {
            return r;
        }
    }
    // SAFETY: default handling of a message for our own window.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The hidden window that owns menus on this thread (created once).
fn host() -> Option<HWND> {
    HOST.with(|h| {
        if let Some(hwnd) = *h.borrow() {
            return Some(hwnd);
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            lpszClassName: w!("fx-menu-host"),
            ..Default::default()
        };
        // SAFETY: `class` is fully initialized with a static name and a valid
        // window procedure; registering twice just fails harmlessly.
        unsafe { RegisterClassW(&class) };
        // SAFETY: an invisible popup tool window of the class above.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(WS_EX_TOOLWINDOW.0),
                w!("fx-menu-host"),
                w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )
        }
        .ok()?;
        *h.borrow_mut() = Some(hwnd);
        Some(hwnd)
    })
}

/// Absolute item ids, freed on drop.
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Drop for Pidls {
    fn drop(&mut self) {
        for p in &self.0 {
            // SAFETY: each id came from SHParseDisplayName and is freed once.
            unsafe { CoTaskMemFree(Some(p.cast_const().cast())) };
        }
    }
}

fn parse(path: &Path) -> Option<*mut ITEMIDLIST> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
    // SAFETY: `wide` is NUL-terminated; `pidl` receives a new id we own.
    unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) }.ok()?;
    Some(pidl)
}

/// The context menu of `items` (all in one folder), or of `folder`'s
/// background when `items` is empty.
fn context_menu(req: &MenuRequest, hwnd: HWND, pidls: &mut Pidls) -> Option<IContextMenu> {
    if req.items.is_empty() {
        let pidl = parse(&req.folder)?;
        pidls.0.push(pidl);
        // SAFETY: plain call; returns the desktop folder.
        let desktop = unsafe { SHGetDesktopFolder() }.ok()?;
        // SAFETY: `pidl` is a valid absolute id alive in `pidls`.
        let folder: IShellFolder = unsafe { desktop.BindToObject(pidl, None) }.ok()?;
        // SAFETY: valid folder; the background menu of a folder view.
        return unsafe { folder.CreateViewObject(hwnd) }.ok();
    }
    let mut parent: Option<IShellFolder> = None;
    let mut children: Vec<*const ITEMIDLIST> = Vec::new();
    for item in &req.items {
        let pidl = parse(item)?;
        pidls.0.push(pidl);
        let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
        // SAFETY: `pidl` is valid; `child` points into it (not freed apart).
        let folder: IShellFolder = unsafe { SHBindToParent(pidl, Some(&mut child)) }.ok()?;
        parent.get_or_insert(folder);
        children.push(child.cast_const());
    }
    // SAFETY: `children` are relative ids of items in `parent`, alive in `pidls`.
    unsafe { parent?.GetUIObjectOf(hwnd, &children, None) }.ok()
}

/// Canonical verb of shell command `id` ("delete", "properties"...).
fn verb_of(menu: &IContextMenu, id: u32) -> Option<String> {
    let mut buf = [0u16; 64];
    // SAFETY: GCS_VERBW writes at most `buf.len()` UTF-16 units into `buf`.
    unsafe {
        menu.GetCommandString(
            id as usize,
            GCS_VERBW,
            None,
            PSTR(buf.as_mut_ptr().cast()),
            buf.len() as u32,
        )
    }
    .ok()?;
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..end]))
}

fn show(req: &MenuRequest, ready: &dyn Fn()) -> MenuOutcome {
    let unavailable = |why: &str| MenuOutcome::Unavailable(why.to_owned());
    let Some(hwnd) = host() else {
        return unavailable("janela do menu indisponível");
    };
    let mut pidls = Pidls(Vec::new());
    let Some(menu) = context_menu(req, hwnd, &mut pidls) else {
        return unavailable("o shell não deu um menu para esses itens");
    };
    // SAFETY: plain call creating an empty popup menu, destroyed below.
    let Ok(popup) = (unsafe { CreatePopupMenu() }) else {
        return unavailable("sem menu");
    };
    for (k, label) in req.fx_items.iter().enumerate() {
        let text: Vec<u16> = label.encode_utf16().chain([0]).collect();
        // SAFETY: `popup` is ours and `text` NUL-terminated for the call.
        let _ = unsafe {
            AppendMenuW(
                popup,
                MF_STRING,
                (FX_FIRST + k as u32) as usize,
                PCWSTR(text.as_ptr()),
            )
        };
    }
    if !req.fx_items.is_empty() {
        // SAFETY: as above.
        let _ = unsafe { AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null()) };
    }
    let index = req.fx_items.len() as u32 + u32::from(!req.fx_items.is_empty());
    // SAFETY: `popup` is ours; the shell appends its items in the id range.
    let _ = unsafe {
        menu.QueryContextMenu(
            popup,
            index,
            SHELL_FIRST,
            SHELL_LAST,
            CMF_NORMAL | CMF_CANRENAME,
        )
    };
    ACTIVE.with(|a| *a.borrow_mut() = menu.cast::<IContextMenu3>().ok());
    if req.warm_up {
        ACTIVE.with(|a| *a.borrow_mut() = None);
        // SAFETY: `popup` was created above and is destroyed once.
        let _ = unsafe { DestroyMenu(popup) };
        return MenuOutcome::Cancelled;
    }
    let mut at = POINT::default();
    // SAFETY: `at` is a writable POINT.
    let _ = unsafe { GetCursorPos(&mut at) };
    ready();
    // SAFETY: the menu needs its owner in the foreground to close properly
    // (fx is the foreground process, so this is allowed); `popup` and `hwnd`
    // are ours. TPM_RETURNCMD returns the chosen id instead of posting it.
    let chosen = unsafe {
        let _ = SetForegroundWindow(hwnd);
        let id = TrackPopupMenuEx(
            popup,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            at.x,
            at.y,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        id.0 as u32
    };
    ACTIVE.with(|a| *a.borrow_mut() = None);
    let outcome = if chosen == 0 {
        MenuOutcome::Cancelled
    } else if chosen < SHELL_FIRST {
        MenuOutcome::Fx((chosen - FX_FIRST) as usize)
    } else {
        let id = chosen - SHELL_FIRST;
        match verb_of(&menu, id) {
            Some(v) if OWN_VERBS.contains(&v.to_lowercase().as_str()) => {
                MenuOutcome::Verb(v.to_lowercase())
            }
            _ => {
                let owner = HWND(req.owner as *mut _);
                let info = CMINVOKECOMMANDINFOEX {
                    cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
                    fMask: CMIC_MASK_UNICODE | CMIC_MASK_PTINVOKE,
                    hwnd: owner,
                    // MAKEINTRESOURCE: the command's offset as the "verb".
                    lpVerb: PCSTR(id as usize as *const u8),
                    lpVerbW: PCWSTR(id as usize as *const u16),
                    nShow: SW_SHOWNORMAL.0,
                    ptInvoke: at,
                    ..Default::default()
                };
                // SAFETY: `info` is a sized CMINVOKECOMMANDINFOEX, which starts
                // with the CMINVOKECOMMANDINFO layout the method expects.
                let done =
                    unsafe { menu.InvokeCommand((&raw const info).cast::<CMINVOKECOMMANDINFO>()) };
                if done.is_ok() {
                    MenuOutcome::Shell
                } else {
                    unavailable("o comando do menu falhou")
                }
            }
        }
    };
    // SAFETY: `popup` was created above and is destroyed once; the owner
    // window gets the foreground back.
    unsafe {
        let _ = DestroyMenu(popup);
        let _ = SetForegroundWindow(HWND(req.owner as *mut _));
    }
    outcome
}

fn init() {
    // SAFETY: first COM/OLE call of a fresh worker thread (OLE, not just COM:
    // some menu commands use the clipboard or drag and drop).
    let _ = unsafe { OleInitialize(None) };
}

fn service() -> Option<&'static MenuService> {
    static SERVICE: OnceLock<Option<MenuService>> = OnceLock::new();
    SERVICE
        .get_or_init(|| {
            let shower: MenuShower = Arc::new(show);
            MenuService::with_shower(shower, init, MENU_PREPARE_LIMIT).ok()
        })
        .as_ref()
}

/// Shows the shell context menu at the mouse pointer; the reply arrives once
/// the user chose (or the menu could not be built in time).
pub fn show_context_menu(request: MenuRequest) -> std::sync::mpsc::Receiver<MenuReply> {
    match service() {
        Some(s) => s.show(request),
        None => {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = tx.send(MenuReply {
                outcome: MenuOutcome::Unavailable("menu indisponível".into()),
                ready_after: None,
                built_in: None,
            });
            rx
        }
    }
}
