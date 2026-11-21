//! Strategy `FileIdBothDirInfo`: open the folder and read it in large blocks with
//! `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)`.

use std::io;
use std::mem::{offset_of, size_of};
use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_BOTH_DIR_INFO, FILE_LIST_DIRECTORY,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdBothDirectoryInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING,
};
use windows::core::PCWSTR;

use super::{make_entry, to_io, verbatim_wide};
use crate::Batcher;

/// Bytes requested per system call.
const BUFFER_BYTES: usize = 256 * 1024;

/// Closes the folder handle on drop.
struct DirHandle(HANDLE);

impl Drop for DirHandle {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a valid handle returned by CreateFileW and is closed
        // exactly once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn read(dir: &Path, batcher: &mut Batcher<'_>) -> io::Result<()> {
    let mut path = verbatim_wide(dir)?;
    path.push(0);
    // SAFETY: `path` is NUL-terminated and outlives the call. FILE_FLAG_BACKUP_SEMANTICS
    // is required to open a folder; FILE_LIST_DIRECTORY is the only right we need.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(to_io)?;
    let handle = DirHandle(handle);

    // u64 elements: entries are 8-byte aligned inside the buffer.
    let mut buf = vec![0u64; BUFFER_BYTES / size_of::<u64>()];
    let bytes = buf.len() * size_of::<u64>();
    loop {
        // SAFETY: `buf` is writable for `bytes` bytes and suitably aligned for
        // FILE_ID_BOTH_DIR_INFO; `handle.0` is an open folder handle.
        let r = unsafe {
            GetFileInformationByHandleEx(
                handle.0,
                FileIdBothDirectoryInfo,
                buf.as_mut_ptr().cast(),
                bytes as u32,
            )
        };
        match r {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => return Ok(()),
            Err(e) => return Err(to_io(e)),
        }
        if !parse_block(as_bytes(&buf), batcher) {
            return Ok(());
        }
    }
}

fn as_bytes(buf: &[u64]) -> &[u8] {
    // SAFETY: any u64 slice is a valid byte slice of 8× its length; u8 has no
    // alignment or validity requirements.
    unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), size_of_val(buf)) }
}

/// Reads a little-endian u32 / u64 at `at` (bounds already checked by the caller).
fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}
fn u64_at(b: &[u8], at: usize) -> u64 {
    (u64::from(u32_at(b, at + 4)) << 32) | u64::from(u32_at(b, at))
}

/// Walks the chain of FILE_ID_BOTH_DIR_INFO records in `block` using only
/// bounds-checked byte reads. Returns `false` when the receiver is gone.
fn parse_block(block: &[u8], batcher: &mut Batcher<'_>) -> bool {
    let name_at = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    let mut offset = 0usize;
    loop {
        let Some(record) = block.get(offset..) else {
            return true;
        };
        if record.len() < name_at {
            return true;
        }
        let next = u32_at(record, offset_of!(FILE_ID_BOTH_DIR_INFO, NextEntryOffset)) as usize;
        let name_len = u32_at(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileNameLength)) as usize;
        let Some(name_bytes) = record.get(name_at..name_at + name_len) else {
            return true;
        };
        let name: Vec<u16> = name_bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&c| u16::from_le_bytes(c))
            .collect();
        let size = u64_at(record, offset_of!(FILE_ID_BOTH_DIR_INFO, EndOfFile));
        let modified = u64_at(record, offset_of!(FILE_ID_BOTH_DIR_INFO, LastWriteTime));
        let attributes = u32_at(record, offset_of!(FILE_ID_BOTH_DIR_INFO, FileAttributes));
        if let Some(entry) = make_entry(&name, size, modified, attributes)
            && !batcher.push(entry)
        {
            return false;
        }
        if next == 0 {
            return true;
        }
        offset += next;
    }
}
