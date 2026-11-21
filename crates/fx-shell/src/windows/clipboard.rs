//! Windows clipboard for files (task T1.14). Callers run these off the UI
//! thread: another app may hold the clipboard open for a moment, and
//! `open` retries briefly.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_HDROP;
use windows::core::w;

use super::to_io;
use crate::clipboard::{ClipFiles, EFFECT_COPY, EFFECT_MOVE, decode_drop_files, encode_drop_files};

/// Closes the clipboard when dropped.
struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: only created after a successful OpenClipboard on this thread.
        let _ = unsafe { CloseClipboard() };
    }
}

fn open() -> io::Result<Open> {
    let mut last = None;
    for _ in 0..10 {
        // SAFETY: no owner window; the clipboard is closed by `Open`'s drop.
        match unsafe { OpenClipboard(None) } {
            Ok(()) => return Ok(Open),
            Err(e) => last = Some(e),
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(last.map_or_else(|| io::Error::other("clipboard busy"), to_io))
}

fn drop_effect_format() -> u32 {
    // SAFETY: a static NUL-terminated name; registering twice is harmless.
    unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) }
}

/// Copies `bytes` into a movable global block (owned by the clipboard once set).
fn global(bytes: &[u8]) -> io::Result<HGLOBAL> {
    // SAFETY: plain allocation of `len` bytes.
    let mem = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.map_err(to_io)?;
    // SAFETY: `mem` is a fresh block of at least `bytes.len()` bytes; it is
    // locked only for the copy.
    unsafe {
        let ptr = GlobalLock(mem).cast::<u8>();
        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        let _ = GlobalUnlock(mem);
    }
    Ok(mem)
}

/// Bytes of a clipboard block.
fn read_global(handle: HANDLE) -> Option<Vec<u8>> {
    let mem = HGLOBAL(handle.0);
    // SAFETY: `mem` is the clipboard's block, valid while the clipboard is
    // open; it is locked only for the copy and its size bounds the read.
    unsafe {
        let size = GlobalSize(mem);
        let ptr = GlobalLock(mem).cast::<u8>();
        if ptr.is_null() {
            return None;
        }
        let bytes = std::slice::from_raw_parts(ptr, size).to_vec();
        let _ = GlobalUnlock(mem);
        Some(bytes)
    }
}

/// Puts files on the clipboard (Ctrl+C, or Ctrl+X with `cut`).
pub fn set_clipboard_files(paths: &[PathBuf], cut: bool) -> io::Result<()> {
    let drop = global(&encode_drop_files(paths))?;
    let effect = global(&(if cut { EFFECT_MOVE } else { EFFECT_COPY }).to_le_bytes())?;
    let _open = open()?;
    // SAFETY: the clipboard is open on this thread; on success the system
    // owns both blocks.
    unsafe {
        EmptyClipboard().map_err(to_io)?;
        SetClipboardData(u32::from(CF_HDROP.0), Some(HANDLE(drop.0))).map_err(to_io)?;
        SetClipboardData(drop_effect_format(), Some(HANDLE(effect.0))).map_err(to_io)?;
    }
    Ok(())
}

/// Files on the clipboard, if any (from fx, the Explorer or any app).
pub fn clipboard_files() -> io::Result<Option<ClipFiles>> {
    // SAFETY: plain query.
    if unsafe { IsClipboardFormatAvailable(u32::from(CF_HDROP.0)) }.is_err() {
        return Ok(None);
    }
    let _open = open()?;
    // SAFETY: the clipboard is open on this thread.
    let Ok(handle) = (unsafe { GetClipboardData(u32::from(CF_HDROP.0)) }) else {
        return Ok(None);
    };
    let Some(paths) = read_global(handle).and_then(|b| decode_drop_files(&b)) else {
        return Ok(None);
    };
    // SAFETY: the clipboard is still open on this thread.
    let cut = unsafe { GetClipboardData(drop_effect_format()) }
        .ok()
        .and_then(read_global)
        .and_then(|b| {
            b.get(..4)
                .map(|d| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
        })
        .is_some_and(|effect| effect & EFFECT_MOVE != 0);
    Ok(Some(ClipFiles { paths, cut }))
}

/// Empties the clipboard (after pasting a cut, like the Explorer).
pub fn clear_clipboard() -> io::Result<()> {
    let _open = open()?;
    // SAFETY: the clipboard is open on this thread.
    unsafe { EmptyClipboard() }.map_err(to_io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overwrites the user's clipboard: run only on request
    /// (`cargo xwin test -p fx-shell -- --ignored`).
    #[test]
    #[ignore]
    fn real_clipboard_round_trip() {
        let paths = vec![PathBuf::from(r"C:\Windows\notepad.exe")];
        set_clipboard_files(&paths, true).unwrap();
        let got = clipboard_files().unwrap().unwrap();
        assert_eq!(got, ClipFiles { paths, cut: true });
        clear_clipboard().unwrap();
    }
}
