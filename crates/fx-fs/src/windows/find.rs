//! Strategy `FindFirstFileEx`: `FindFirstFileExW(FindExInfoBasic, LARGE_FETCH)`.

use std::io;
use std::path::Path;

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW,
    FindNextFileW, WIN32_FIND_DATAW,
};
use windows::core::PCWSTR;

use super::{make_entry, to_io, verbatim_wide};
use crate::Batcher;

/// Closes the search handle on drop.
struct FindHandle(HANDLE);

impl Drop for FindHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid search handle returned by FindFirstFileExW and
        // is closed exactly once, here.
        let _ = unsafe { FindClose(self.0) };
    }
}

pub(super) fn read(dir: &Path, batcher: &mut Batcher<'_>) -> io::Result<()> {
    let mut pattern = verbatim_wide(dir)?;
    pattern.extend("\\*".encode_utf16());
    pattern.push(0);

    let mut data = WIN32_FIND_DATAW::default();
    // SAFETY: `pattern` is NUL-terminated and outlives the call; `data` is a valid,
    // writable WIN32_FIND_DATAW, which is what FindExInfoBasic fills.
    let first = unsafe {
        FindFirstFileExW(
            PCWSTR(pattern.as_ptr()),
            FindExInfoBasic,
            (&raw mut data).cast(),
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    let handle = match first {
        Ok(h) => FindHandle(h),
        // A folder with no entries at all (e.g. some volume roots).
        Err(e) if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() && dir.is_dir() => {
            return Ok(());
        }
        Err(e) => return Err(to_io(e)),
    };

    loop {
        if !push(&data, batcher) {
            return Ok(());
        }
        // SAFETY: `handle.0` is a live search handle and `data` is writable.
        match unsafe { FindNextFileW(handle.0, &mut data) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => return Ok(()),
            Err(e) => return Err(to_io(e)),
        }
    }
}

fn push(data: &WIN32_FIND_DATAW, batcher: &mut Batcher<'_>) -> bool {
    let len = data
        .cFileName
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(data.cFileName.len());
    let size = (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow);
    let ft = data.ftLastWriteTime;
    let modified = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    match make_entry(
        &data.cFileName[..len],
        size,
        modified,
        data.dwFileAttributes,
        // For a reparse point, the listing puts its tag here.
        data.dwReserved0,
    ) {
        Some(entry) => batcher.push(entry),
        None => true,
    }
}
