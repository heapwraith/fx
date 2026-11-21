//! Folder watcher (task T1.8): `ReadDirectoryChangesW` (overlapped) on its own
//! thread, for one folder (not recursive). Notifications become
//! [`Change`]s with fresh metadata and are sent in batches, one per
//! completion. Dropping the [`Watcher`] cancels the pending read and joins the
//! thread.
//!
//! When the system's buffer overflows (too many changes at once) the read
//! completes with zero bytes or `ERROR_NOTIFY_ENUM_DIR`; that becomes
//! [`Change::Overflow`] and the view reads the folder again.

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{CloseHandle, ERROR_NOTIFY_ENUM_DIR, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OVERLAPPED, FILE_LIST_DIRECTORY,
    FILE_NOTIFY_CHANGE, FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadDirectoryChangesW,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, ResetEvent, SetEvent, WaitForMultipleObjects,
};
use windows::core::PCWSTR;

use super::{filetime_to_system_time, map_attributes, to_io, verbatim_wide};
use crate::{Change, Entry};

/// System buffer for notifications (64 KB is the limit over the network).
const BUFFER_BYTES: usize = 64 * 1024;

const ADDED: u32 = 1;
const REMOVED: u32 = 2;
const MODIFIED: u32 = 3;
const RENAMED_OLD: u32 = 4;
const RENAMED_NEW: u32 = 5;

/// An owned kernel handle.
struct Owned(HANDLE);

// SAFETY: kernel handles are process-wide numbers; each `Owned` is used by one
// thread at a time (moved into the watcher thread, or the stop event, which
// only gets SetEvent from Drop while the thread waits on it).
unsafe impl Send for Owned {}
// SAFETY: see above; SetEvent and waits on an event are thread-safe.
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this struct and closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Watches one folder until dropped.
pub struct Watcher {
    stop: std::sync::Arc<Owned>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Watcher")
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        // SAFETY: `stop` is a valid event handle owned by us.
        let _ = unsafe { SetEvent(self.stop.0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts watching `dir`; batches of changes arrive on `tx`. Fails right away
/// if the folder cannot be opened.
pub fn watch(dir: &Path, tx: Sender<Vec<Change>>) -> io::Result<Watcher> {
    watch_with_buffer(dir, tx, BUFFER_BYTES)
}

fn watch_with_buffer(dir: &Path, tx: Sender<Vec<Change>>, buffer: usize) -> io::Result<Watcher> {
    // `verbatim_wide` does not terminate the string: add the NUL here.
    let mut wide = verbatim_wide(dir)?;
    wide.push(0);
    // SAFETY: `wide` is NUL-terminated and outlives the call. BACKUP_SEMANTICS
    // is required to open a folder; sharing everything keeps the folder free to
    // be renamed or deleted by others.
    let folder = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_LIST_DIRECTORY.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED,
            None,
        )
    }
    .map_err(to_io)?;
    let folder = Owned(folder);
    // SAFETY: plain calls creating unnamed manual-reset events.
    let io_event =
        Owned(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(to_io)?);
    let stop = std::sync::Arc::new(Owned(
        // SAFETY: as above.
        unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(to_io)?,
    ));
    let thread_stop = stop.clone();
    let dir = dir.to_path_buf();
    let (armed_tx, armed_rx) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("fx-watch".into())
        .spawn(move || {
            run(
                &folder,
                &io_event,
                &thread_stop,
                &dir,
                &tx,
                buffer,
                armed_tx,
            )
        })?;
    // Return only once the first read is pending: changes made right after
    // `watch` returns are never missed.
    let _ = armed_rx.recv_timeout(std::time::Duration::from_secs(2));
    Ok(Watcher {
        stop,
        thread: Some(thread),
    })
}

fn run(
    folder: &Owned,
    io_event: &Owned,
    stop: &Owned,
    dir: &Path,
    tx: &Sender<Vec<Change>>,
    buffer: usize,
    armed: std::sync::mpsc::SyncSender<()>,
) {
    // DWORD-aligned, as ReadDirectoryChangesW requires.
    let mut buf = vec![0u32; buffer.div_ceil(4)];
    let filter = FILE_NOTIFY_CHANGE(
        FILE_NOTIFY_CHANGE_FILE_NAME.0
            | FILE_NOTIFY_CHANGE_DIR_NAME.0
            | FILE_NOTIFY_CHANGE_ATTRIBUTES.0
            | FILE_NOTIFY_CHANGE_SIZE.0
            | FILE_NOTIFY_CHANGE_LAST_WRITE.0,
    );
    loop {
        let mut ov = OVERLAPPED {
            hEvent: io_event.0,
            ..Default::default()
        };
        // SAFETY: `io_event` is a valid event handle.
        let _ = unsafe { ResetEvent(io_event.0) };
        // SAFETY: `buf` and `ov` stay alive and unmoved until the read completes
        // or is cancelled and waited for below.
        let started = unsafe {
            ReadDirectoryChangesW(
                folder.0,
                buf.as_mut_ptr().cast(),
                (buf.len() * 4) as u32,
                false,
                filter,
                None,
                Some(&mut ov),
                None,
            )
        };
        let _ = armed.try_send(());
        if started.is_err() {
            let _ = tx.send(vec![Change::Overflow]);
            return;
        }
        // SAFETY: both handles are valid events.
        let woke = unsafe { WaitForMultipleObjects(&[io_event.0, stop.0], false, INFINITE) };
        let mut bytes = 0u32;
        if woke != WAIT_OBJECT_0 {
            // Stop (or a wait failure): cancel and wait until the system no
            // longer touches `buf` and `ov`.
            // SAFETY: `ov` is the pending operation on `folder`.
            unsafe {
                let _ = CancelIoEx(folder.0, Some(&ov));
                let _ = GetOverlappedResult(folder.0, &ov, &mut bytes, true);
            }
            return;
        }
        // SAFETY: the operation completed (its event is set); `ov` is valid.
        let done = unsafe { GetOverlappedResult(folder.0, &ov, &mut bytes, false) };
        let changes = match done {
            Ok(()) if bytes > 0 => {
                // SAFETY: the system wrote `bytes` bytes into `buf`; viewing its
                // u32s as bytes is always valid.
                let raw = unsafe {
                    std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), bytes as usize)
                };
                to_changes(dir, &parse(raw))
            }
            Ok(()) => vec![Change::Overflow],
            Err(e) if e.code() == ERROR_NOTIFY_ENUM_DIR.to_hresult() => vec![Change::Overflow],
            Err(_) => {
                // The folder went away or became unreadable: let the view re-read.
                let _ = tx.send(vec![Change::Overflow]);
                return;
            }
        };
        if !changes.is_empty() && tx.send(changes).is_err() {
            return;
        }
    }
}

/// `(action, name)` pairs of a FILE_NOTIFY_INFORMATION chain.
fn parse(raw: &[u8]) -> Vec<(u32, OsString)> {
    let u32_at = |at: usize| -> Option<u32> {
        raw.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let mut out = Vec::new();
    let mut at = 0usize;
    while let (Some(next), Some(action), Some(len)) = (u32_at(at), u32_at(at + 4), u32_at(at + 8)) {
        let start = at + 12;
        let Some(name) = raw.get(start..start + len as usize) else {
            break;
        };
        let (units, _) = name.as_chunks::<2>();
        let wide: Vec<u16> = units.iter().map(|&u| u16::from_le_bytes(u)).collect();
        out.push((action, OsString::from_wide(&wide)));
        if next == 0 {
            break;
        }
        at += next as usize;
    }
    out
}

/// Current metadata of `dir\name`, or `None` if it is gone already.
fn stat(dir: &Path, name: &OsString) -> Option<Entry> {
    let path: PathBuf = dir.join(name);
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(Entry {
        name: name.clone(),
        size: meta.file_size(),
        modified: filetime_to_system_time(meta.last_write_time()),
        attributes: map_attributes(meta.file_attributes()),
    })
}

fn to_changes(dir: &Path, events: &[(u32, OsString)]) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    let mut pending_old: Option<OsString> = None;
    for (action, name) in events {
        if *action != RENAMED_NEW
            && let Some(old) = pending_old.take()
        {
            out.push(Change::Removed(old)); // moved out of the folder
        }
        match *action {
            ADDED => {
                if let Some(e) = stat(dir, name) {
                    out.push(Change::Added(e));
                }
            }
            REMOVED => out.push(Change::Removed(name.clone())),
            MODIFIED => {
                let repeat = matches!(out.last(), Some(Change::Modified(e)) if e.name == *name);
                if !repeat && let Some(e) = stat(dir, name) {
                    out.push(Change::Modified(e));
                }
            }
            RENAMED_OLD => pending_old = Some(name.clone()),
            RENAMED_NEW => match (pending_old.take(), stat(dir, name)) {
                (Some(from), Some(to)) => out.push(Change::Renamed { from, to }),
                (Some(from), None) => out.push(Change::Removed(from)),
                (None, Some(to)) => out.push(Change::Added(to)),
                (None, None) => {}
            },
            _ => {}
        }
    }
    if let Some(old) = pending_old {
        out.push(Change::Removed(old));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};

    /// Waits up to `limit` for a change matching `want`; returns how long it took.
    fn wait_for(
        rx: &Receiver<Vec<Change>>,
        limit: Duration,
        want: impl Fn(&Change) -> bool,
    ) -> Option<Duration> {
        let started = Instant::now();
        while let Some(left) = limit.checked_sub(started.elapsed()) {
            match rx.recv_timeout(left) {
                Ok(batch) if batch.iter().any(&want) => return Some(started.elapsed()),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        None
    }

    #[test]
    fn create_rename_modify_delete_arrive_within_100_ms() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let _w = watch(dir.path(), tx).unwrap();
        let limit = Duration::from_millis(100);
        let a = dir.path().join("relatório.txt");

        std::fs::write(&a, b"x").unwrap();
        let t = wait_for(
            &rx,
            limit,
            |c| matches!(c, Change::Added(e) if e.name == "relatório.txt"),
        );
        assert!(t.is_some(), "created");

        std::fs::write(&a, b"bigger content").unwrap();
        let t = wait_for(
            &rx,
            limit,
            |c| matches!(c, Change::Modified(e) if e.name == "relatório.txt" && e.size == 14),
        );
        assert!(t.is_some(), "modified");

        let b = dir.path().join("final.txt");
        std::fs::rename(&a, &b).unwrap();
        let t = wait_for(
            &rx,
            limit,
            |c| matches!(c, Change::Renamed { from, to } if from == "relatório.txt" && to.name == "final.txt"),
        );
        assert!(t.is_some(), "renamed");

        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let t = wait_for(
            &rx,
            limit,
            |c| matches!(c, Change::Added(e) if e.name == "sub" && e.attributes.is_dir()),
        );
        assert!(t.is_some(), "folder created");

        std::fs::remove_file(&b).unwrap();
        let t = wait_for(
            &rx,
            limit,
            |c| matches!(c, Change::Removed(n) if n == "final.txt"),
        );
        assert!(t.is_some(), "deleted");
    }

    /// A buffer too small for the burst overflows; the view must re-read.
    #[test]
    fn buffer_overflow_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let _w = watch_with_buffer(dir.path(), tx, 64).unwrap();
        for i in 0..200 {
            std::fs::write(
                dir.path().join(format!("file-with-a-long-name-{i:04}.txt")),
                b"x",
            )
            .unwrap();
        }
        let t = wait_for(&rx, Duration::from_secs(5), |c| *c == Change::Overflow);
        assert!(t.is_some(), "overflow reported");
    }

    #[test]
    fn dropping_the_watcher_stops_it() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let w = watch(dir.path(), tx).unwrap();
        let started = Instant::now();
        drop(w);
        assert!(started.elapsed() < Duration::from_secs(1));
        // The thread is gone, so the sender is dropped.
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        assert!(watch(&dir.path().join("missing"), channel().0).is_err());
    }

    #[test]
    fn parses_notify_records() {
        let mut raw = Vec::new();
        for (next, action, name) in [(20u32, ADDED, "ab"), (0, REMOVED, "c")] {
            let wide: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
            raw.extend_from_slice(&next.to_le_bytes());
            raw.extend_from_slice(&action.to_le_bytes());
            raw.extend_from_slice(&(wide.len() as u32).to_le_bytes());
            raw.extend_from_slice(&wide);
            while raw.len() % 4 != 0 || (next != 0 && raw.len() < next as usize) {
                raw.push(0);
            }
        }
        let got = parse(&raw);
        assert_eq!(got, [(ADDED, "ab".into()), (REMOVED, "c".into())]);
    }
}
