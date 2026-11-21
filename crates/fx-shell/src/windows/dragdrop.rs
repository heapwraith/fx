//! Drag and drop with other apps (task T1.15). Dropping into fx is handled by
//! GPUI's own `IDropTarget` (it delivers the paths); this module adds what
//! GPUI lacks: the real modifier keys at drop time (GPUI's window sees no key
//! events while another app drives the drag), and dragging files out with
//! the shell's own data object (`SHDoDragDrop`), so the Explorer, the
//! desktop or a browser receive exactly what the Explorer would give them.

use std::io;
use std::num::NonZeroIsize;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoTaskMemFree, IDataObject};
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, IDropSource,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    BHID_DataObject, IShellItemArray, SHCreateShellItemArrayFromIDLists, SHDoDragDrop,
    SHParseDisplayName,
};
use windows::core::PCWSTR;

use super::to_io;

/// What the drop target did with dragged files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragResult {
    None,
    Copied,
    Moved,
    Linked,
}

/// Window id of a GPUI window, for [`drag_files`].
pub fn window_id(window: &impl HasWindowHandle) -> Option<isize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(w) => Some(NonZeroIsize::get(w.hwnd)),
        _ => None,
    }
}

/// Shift and Ctrl as they are now (read at drop time).
pub fn drop_keys() -> (bool, bool) {
    // SAFETY: plain queries of the keyboard state.
    let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0;
    (down(VK_SHIFT.0), down(VK_CONTROL.0))
}

/// Owned absolute item ids (freed on drop).
struct Pidls(Vec<*mut ITEMIDLIST>);

impl Drop for Pidls {
    fn drop(&mut self) {
        for p in &self.0 {
            // SAFETY: each id came from SHParseDisplayName and is freed once.
            unsafe { CoTaskMemFree(Some(p.cast_const().cast())) };
        }
    }
}

/// Drags `paths` out of the window `window` until the user drops them
/// (modal: runs its own message loop). Must be called on the window's thread
/// while no app state is borrowed.
pub fn drag_files(window: isize, paths: &[PathBuf]) -> io::Result<DragResult> {
    let data = data_object(paths)?;
    let allowed = DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_MOVE.0 | DROPEFFECT_LINK.0);
    // SAFETY: `window` is our live window on this thread; no custom drop
    // source (the shell's default one follows the mouse and Esc).
    let effect = unsafe {
        SHDoDragDrop(
            Some(HWND(window as *mut _)),
            &data,
            None::<&IDropSource>,
            allowed,
        )
    }
    .map_err(to_io)?;
    Ok(if effect.0 & DROPEFFECT_MOVE.0 != 0 {
        DragResult::Moved
    } else if effect.0 & DROPEFFECT_COPY.0 != 0 {
        DragResult::Copied
    } else if effect.0 & DROPEFFECT_LINK.0 != 0 {
        DragResult::Linked
    } else {
        DragResult::None
    })
}

/// The shell's standard data object for `paths` (drag out, multi-item
/// Properties). Needs COM on the calling thread.
pub(super) fn data_object(paths: &[PathBuf]) -> io::Result<IDataObject> {
    let mut pidls = Pidls(Vec::with_capacity(paths.len()));
    for p in paths {
        let wide: Vec<u16> = p.as_os_str().encode_wide().chain([0]).collect();
        let mut pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        // SAFETY: `wide` is NUL-terminated; `pidl` receives a new id we own.
        unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) }
            .map_err(to_io)?;
        pidls.0.push(pidl);
    }
    let ids: Vec<*const ITEMIDLIST> = pidls.0.iter().map(|p| p.cast_const()).collect();
    // SAFETY: `ids` are valid absolute item ids, alive until `pidls` drops.
    let items: IShellItemArray =
        unsafe { SHCreateShellItemArrayFromIDLists(&ids) }.map_err(to_io)?;
    // SAFETY: valid array; asks the shell for its standard data object.
    unsafe { items.BindToHandler(None, &BHID_DataObject) }.map_err(to_io)
}
