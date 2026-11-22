//! Windows primitives of the operations engine (task T1.11): `CopyFile2`
//! with progress and cancellation, and a rename that never replaces. They
//! live here, not in fx-ops, so `unsafe` stays in the crates allowed to have
//! it (CLAUDE.md; review, D-066).

use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::AtomicI32;

use windows::Win32::Storage::FileSystem::{
    COPY_FILE_FAIL_IF_EXISTS, COPYFILE2_CALLBACK_CHUNK_FINISHED, COPYFILE2_EXTENDED_PARAMETERS,
    COPYFILE2_MESSAGE, COPYFILE2_MESSAGE_ACTION, COPYFILE2_PROGRESS_CONTINUE, CopyFile2,
    MOVE_FILE_FLAGS, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::{BOOL, PCWSTR};

/// `\\?\` form of an absolute path, NUL-terminated (no MAX_PATH limit).
fn verbatim(path: &Path) -> io::Result<Vec<u16>> {
    let abs = std::path::absolute(path)?;
    let wide: Vec<u16> = abs.as_os_str().encode_wide().collect();
    let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let mut out = if wide.starts_with(&w(r"\\?\")) || wide.starts_with(&w(r"\\.\")) {
        wide
    } else if wide.starts_with(&w(r"\\")) {
        let mut v = w(r"\\?\UNC\");
        v.extend_from_slice(&wide[2..]);
        v
    } else {
        let mut v = w(r"\\?\");
        v.extend_from_slice(&wide);
        v
    };
    out.push(0);
    Ok(out)
}

/// COM for a worker thread (`IFileOperation` needs an STA).
pub fn com_init() {
    // SAFETY: first COM call of a fresh worker thread, never uninitialized.
    let _ = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };
}

pub(crate) fn to_io(e: windows::core::Error) -> io::Error {
    let code = e.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        io::Error::from_raw_os_error(code as i32)
    }
}

struct Progress<'a> {
    report: &'a mut dyn FnMut(u64),
}

unsafe extern "system" fn on_progress(
    message: *const COPYFILE2_MESSAGE,
    context: *const c_void,
) -> COPYFILE2_MESSAGE_ACTION {
    // SAFETY: CopyFile2 passes a valid message for the duration of the call,
    // and `context` is the `Progress` that `copy_file` keeps alive until
    // CopyFile2 returns; callbacks run on the copying thread, one at a time.
    unsafe {
        if (*message).Type == COPYFILE2_CALLBACK_CHUNK_FINISHED {
            let done = (*message).Info.ChunkFinished.uliTotalBytesTransferred;
            let progress = &mut *(context as *mut Progress<'_>);
            (progress.report)(done);
        }
    }
    COPYFILE2_PROGRESS_CONTINUE
}

/// Copies `src` to the new file `dst` (fails if it exists). `cancel` is read
/// by the system while copying: set it to 1 to abort, which also deletes the
/// partial `dst`. `report` gets the bytes copied so far.
pub fn copy_file(
    src: &Path,
    dst: &Path,
    cancel: &AtomicI32,
    report: &mut dyn FnMut(u64),
) -> io::Result<()> {
    let from = verbatim(src)?;
    let to = verbatim(dst)?;
    let mut progress = Progress { report };
    let params = COPYFILE2_EXTENDED_PARAMETERS {
        dwSize: std::mem::size_of::<COPYFILE2_EXTENDED_PARAMETERS>() as u32,
        dwCopyFlags: COPY_FILE_FAIL_IF_EXISTS,
        // BOOL is a transparent i32, so the atomic can be read in its place.
        pfCancel: cancel.as_ptr().cast::<BOOL>(),
        pProgressRoutine: Some(on_progress),
        pvCallbackContext: (&raw mut progress).cast(),
    };
    // SAFETY: both paths are NUL-terminated and outlive the call; `params`,
    // the cancel flag and `progress` stay alive until CopyFile2 returns.
    unsafe { CopyFile2(PCWSTR(from.as_ptr()), PCWSTR(to.as_ptr()), Some(&params)) }.map_err(to_io)
}

/// Renames `from` to `to` on the same volume; fails (never replaces) if `to`
/// exists, and with `ERROR_NOT_SAME_DEVICE` across volumes.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    let a = verbatim(from)?;
    let b = verbatim(to)?;
    // SAFETY: both paths are NUL-terminated and outlive the call. Without
    // MOVEFILE_REPLACE_EXISTING or MOVEFILE_COPY_ALLOWED this is a plain
    // same-volume rename that refuses to overwrite.
    unsafe {
        MoveFileExW(
            PCWSTR(a.as_ptr()),
            PCWSTR(b.as_ptr()),
            MOVE_FILE_FLAGS(MOVEFILE_WRITE_THROUGH.0),
        )
    }
    .map_err(to_io)
}

/// Win32 error code for "different volume" (rename impossible).
const NOT_SAME_DEVICE: i32 = 17;
/// Win32 error code of a copy aborted through the cancel flag.
const REQUEST_ABORTED: i32 = 1235;

/// A move across volumes, which a rename cannot do (copy, then delete).
pub fn is_cross_device(e: &io::Error) -> bool {
    e.raw_os_error() == Some(NOT_SAME_DEVICE)
}

/// A copy stopped by its cancel flag.
pub fn is_aborted(e: &io::Error) -> bool {
    e.raw_os_error() == Some(REQUEST_ABORTED)
}
