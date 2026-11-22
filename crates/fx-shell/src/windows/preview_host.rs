//! The Windows side of preview handlers (task T2.9): which handler a file
//! type has, and hosting it — run only inside the `fx-preview` process.
//!
//! The host window is a borderless popup placed over the Quick Look panel.
//! It is deliberately *not* owned by (nor a child of) fx's window: a window
//! relationship across processes attaches the two input queues, and a slow
//! handler would then freeze fx's keyboard. fx keeps it in place instead:
//! `raise` while fx is active, `hide` when it is not, `move` on layout
//! changes. The handler (`IPreviewHandler`, usually run by Windows in its
//! `prevhost.exe` surrogate) draws into it.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::io::{self, BufRead, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::Mutex;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, HBRUSH};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, CLSIDFromString, COINIT_APARTMENTTHREADED,
    CoCreateInstance, CoInitializeEx, STGM_READ, STGM_SHARE_DENY_NONE,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithFile, IInitializeWithStream};
use windows::Win32::UI::Shell::{
    ASSOCF_INIT_DEFAULTTOSTAR, ASSOCSTR_SHELLEXTENSION, AssocQueryStringW, IInitializeWithItem,
    IPreviewHandler, IShellItem, SHCreateItemFromParsingName, SHCreateStreamOnFileEx,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    HWND_TOP, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_HIDE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetWindowPos, ShowWindow,
    TranslateMessage, WM_APP, WM_CLOSE, WM_DESTROY, WNDCLASSW, WS_CLIPCHILDREN, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};
use windows::core::{GUID, HSTRING, PCWSTR, PWSTR, w};

use crate::preview::ScreenRect;

/// Shell extension key of preview handlers.
const PREVIEW_HANDLER: &str = "{8895b1c6-b41f-4c1c-a562-0d564250836f}";
/// Moves the window to the rectangle stored in `PENDING_MOVE`.
const WM_FX_MOVE: u32 = WM_APP + 1;
/// Brings it to the top (without activating) and shows it.
const WM_FX_RAISE: u32 = WM_APP + 2;
const WM_FX_HIDE: u32 = WM_APP + 3;

/// `(x, y)` of a window's client area converted to screen pixels.
pub fn client_to_screen(window: isize, x: i32, y: i32) -> (i32, i32) {
    let mut p = windows::Win32::Foundation::POINT { x, y };
    // SAFETY: plain call; a stale handle only makes it fail.
    let _ =
        unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(HWND(window as *mut _), &mut p) };
    (p.x, p.y)
}

/// The preview handler CLSID registered for `path`'s type, if any.
pub fn handler_for(path: &Path) -> Option<GUID> {
    let ext = path.extension()?;
    let mut dotted = std::ffi::OsString::from(".");
    dotted.push(ext);
    let ext: Vec<u16> = dotted.encode_wide().chain([0]).collect();
    let mut buf = [0u16; 64];
    let mut len = buf.len() as u32;
    // SAFETY: NUL-terminated strings alive for the call; `buf` is writable
    // for `len` units.
    let hr = unsafe {
        AssocQueryStringW(
            ASSOCF_INIT_DEFAULTTOSTAR,
            ASSOCSTR_SHELLEXTENSION,
            PCWSTR(ext.as_ptr()),
            &HSTRING::from(PREVIEW_HANDLER),
            Some(PWSTR(buf.as_mut_ptr())),
            &mut len,
        )
    };
    if hr.is_err() {
        return None;
    }
    // SAFETY: `buf` holds a NUL-terminated CLSID string written above.
    unsafe { CLSIDFromString(PCWSTR(buf.as_ptr())) }.ok()
}

thread_local! {
    static HANDLER: RefCell<Option<IPreviewHandler>> = const { RefCell::new(None) };
}
static PENDING_MOVE: Mutex<Option<ScreenRect>> = Mutex::new(None);

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_FX_MOVE => {
            let rect = PENDING_MOVE.lock().ok().and_then(|mut m| m.take());
            if let Some(r) = rect {
                // SAFETY: our own window, on its thread.
                let _ = unsafe {
                    SetWindowPos(
                        hwnd,
                        None,
                        r.x,
                        r.y,
                        r.width,
                        r.height,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
                let client = client_rect(hwnd);
                HANDLER.with(|h| {
                    if let Some(h) = h.borrow().as_ref() {
                        // SAFETY: the handler lives on this thread; `client` is valid.
                        let _ = unsafe { h.SetRect(&client) };
                    }
                });
            }
            LRESULT(0)
        }
        WM_FX_RAISE => {
            // SAFETY: our own window, on its thread.
            unsafe {
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            LRESULT(0)
        }
        WM_FX_HIDE => {
            // SAFETY: our own window, on its thread.
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
            LRESULT(0)
        }
        WM_CLOSE => {
            HANDLER.with(|h| {
                if let Some(h) = h.borrow_mut().take() {
                    // SAFETY: the handler lives on this thread.
                    let _ = unsafe { h.Unload() };
                }
            });
            // SAFETY: our own window.
            let _ = unsafe { DestroyWindow(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: plain call on this thread.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: default handling with the arguments we were given.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn client_rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: valid window; `r` is writable.
    let _ = unsafe { GetClientRect(hwnd, &mut r) };
    r
}

/// Hosts the preview of `path` in a popup owned by `owner` at `rect`, until
/// told to quit (or its input closes). Prints `ready` or `error …`.
pub fn run_host(path: &Path, _owner: isize, rect: ScreenRect) -> io::Result<()> {
    let say = |line: &str| {
        let mut out = io::stdout();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
    };
    // SAFETY: first COM call of this process's main thread.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let Some(clsid) = handler_for(path) else {
        say(fx_core::tr!(
            "error este tipo não tem visualizador do Windows",
            "error this type has no Windows viewer"
        ));
        return Ok(());
    };
    // SAFETY: the class name and procedure are static; module of this exe.
    let hwnd = unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("fxPreviewHost"),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _),
            ..Default::default()
        };
        RegisterClassW(&class);
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("fxPreviewHost"),
            w!("fx-preview"),
            WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            None,
            None,
            Some(instance.into()),
            None,
        )?
    };
    let handler = match create_handler(&clsid, path) {
        Ok(h) => h,
        Err(e) => {
            say(&format!("error {e}"));
            return Ok(());
        }
    };
    let client = client_rect(hwnd);
    // SAFETY: valid window and handler of this thread; `client` is valid.
    let shown = unsafe {
        handler
            .SetWindow(hwnd, &client)
            .and_then(|()| handler.DoPreview())
    };
    if let Err(e) = shown {
        say(&format!("error {e}"));
        return Ok(());
    }
    HANDLER.with(|h| *h.borrow_mut() = Some(handler));
    say("ready");

    // Commands from fx arrive on standard input.
    let target = hwnd.0 as isize;
    std::thread::spawn(move || {
        let post = |msg: u32| {
            // SAFETY: posting to a window handle is safe even if it is gone.
            let _ =
                unsafe { PostMessageW(Some(HWND(target as *mut _)), msg, WPARAM(0), LPARAM(0)) };
        };
        for line in io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.as_slice() {
                ["move", x, y, w, h] => {
                    if let (Ok(x), Ok(y), Ok(w), Ok(h)) =
                        (x.parse(), y.parse(), w.parse(), h.parse())
                    {
                        if let Ok(mut m) = PENDING_MOVE.lock() {
                            *m = Some(ScreenRect {
                                x,
                                y,
                                width: w,
                                height: h,
                            });
                        }
                        post(WM_FX_MOVE);
                    }
                }
                ["raise"] => post(WM_FX_RAISE),
                ["hide"] => post(WM_FX_HIDE),
                ["quit"] => break,
                _ => {}
            }
        }
        post(WM_CLOSE);
    });

    let mut msg = MSG::default();
    // SAFETY: standard message loop of this thread.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
        // SAFETY: `msg` came from GetMessageW.
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

/// The handler, initialized with the file: a stream first (what most
/// handlers support), then the path, then a shell item.
fn create_handler(clsid: &GUID, path: &Path) -> windows::core::Result<IPreviewHandler> {
    // SAFETY: COM is initialized on this thread; `clsid` is valid.
    let handler: IPreviewHandler = unsafe {
        CoCreateInstance(clsid, None, CLSCTX_LOCAL_SERVER)
            .or_else(|_| CoCreateInstance(clsid, None, CLSCTX_INPROC_SERVER))?
    };
    let wide = HSTRING::from(OsStr::new(path));
    if let Ok(init) = windows::core::Interface::cast::<IInitializeWithStream>(&handler) {
        // SAFETY: valid path string; read-only stream handed to the handler.
        let stream = unsafe {
            SHCreateStreamOnFileEx(&wide, (STGM_READ | STGM_SHARE_DENY_NONE).0, 0, false, None)
        }?;
        // SAFETY: valid interfaces of this thread.
        if unsafe { init.Initialize(&stream, STGM_READ.0) }.is_ok() {
            return Ok(handler);
        }
    }
    if let Ok(init) = windows::core::Interface::cast::<IInitializeWithFile>(&handler) {
        // SAFETY: valid path string.
        if unsafe { init.Initialize(&wide, STGM_READ.0) }.is_ok() {
            return Ok(handler);
        }
    }
    let init: IInitializeWithItem = windows::core::Interface::cast(&handler)?;
    // SAFETY: valid path string; no bind context.
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(&wide, None) }?;
    // SAFETY: valid interfaces of this thread.
    unsafe { init.Initialize(&item, STGM_READ.0) }?;
    Ok(handler)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints the handlers registered on this machine (informative).
    #[test]
    #[ignore]
    fn list_handlers() {
        for ext in [
            "pdf", "docx", "xlsx", "pptx", "mp4", "mp3", "wav", "txt", "rtf", "html", "svg",
        ] {
            let p = std::path::PathBuf::from(format!("x.{ext}"));
            println!("{ext}: {:?}", handler_for(&p));
        }
    }

    #[test]
    fn no_handler_for_unknown_types() {
        assert!(handler_for(Path::new("x.fx-no-such-type")).is_none());
        assert!(handler_for(Path::new("sem-extensao")).is_none());
    }
}
