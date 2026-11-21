//! fx-fs: Folder enumeration and change watching.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]`; the public API
//! uses only neutral types (docs/DECISIONS.md D-012).

use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

pub use fx_core::{Attributes, Change, Entry};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{Watcher, enumerate, watch};

/// How a folder is read. All strategies deliver the same entries; they differ
/// only in speed (task T0.3, docs/DECISIONS.md D-009).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// `std::fs::read_dir` (baseline).
    StdReadDir,
    /// `FindFirstFileExW` with `FindExInfoBasic` and `FIND_FIRST_EX_LARGE_FETCH`.
    FindFirstFileEx,
    /// `GetFileInformationByHandleEx(FileIdBothDirectoryInfo)` with a large buffer.
    FileIdBothDirInfo,
}

impl Strategy {
    pub const ALL: [Strategy; 3] = [
        Strategy::StdReadDir,
        Strategy::FindFirstFileEx,
        Strategy::FileIdBothDirInfo,
    ];

    /// Stable identifier, used in benchmark result files.
    pub const fn id(self) -> &'static str {
        match self {
            Strategy::StdReadDir => "std-read-dir",
            Strategy::FindFirstFileEx => "find-first-file-ex",
            Strategy::FileIdBothDirInfo => "file-id-both-dir-info",
        }
    }
}

/// A batch is sent when it reaches this many entries...
pub const BATCH_MAX_ENTRIES: usize = 2_000;
/// ...or when this much time has passed since the previous batch, whichever comes
/// first (docs/ARCHITECTURE.md, "Modelo de threads").
pub const BATCH_MAX_DELAY: Duration = Duration::from_millis(8);

/// Groups entries into batches and sends them over a channel.
#[derive(Debug)]
pub struct Batcher<'a> {
    tx: &'a Sender<Vec<Entry>>,
    batch: Vec<Entry>,
    last_flush: Instant,
    sent: usize,
}

impl<'a> Batcher<'a> {
    pub fn new(tx: &'a Sender<Vec<Entry>>) -> Self {
        Self {
            tx,
            batch: Vec::with_capacity(BATCH_MAX_ENTRIES),
            last_flush: Instant::now(),
            sent: 0,
        }
    }

    /// Adds an entry. Returns `false` when the receiver is gone, meaning the
    /// enumeration should stop (e.g. the user navigated away).
    pub fn push(&mut self, entry: Entry) -> bool {
        self.batch.push(entry);
        if self.batch.len() >= BATCH_MAX_ENTRIES || self.last_flush.elapsed() >= BATCH_MAX_DELAY {
            return self.flush();
        }
        true
    }

    /// Sends what is pending. Returns `false` when the receiver is gone.
    pub fn flush(&mut self) -> bool {
        self.last_flush = Instant::now();
        if self.batch.is_empty() {
            return true;
        }
        let batch = std::mem::replace(&mut self.batch, Vec::with_capacity(BATCH_MAX_ENTRIES));
        self.sent += batch.len();
        self.tx.send(batch).is_ok()
    }

    /// Flushes and returns the number of entries sent.
    pub fn finish(mut self) -> usize {
        self.flush();
        self.sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    fn entry(i: usize) -> Entry {
        Entry {
            name: OsString::from(format!("f{i}")),
            size: 0,
            modified: SystemTime::UNIX_EPOCH,
            attributes: Attributes::empty(),
        }
    }

    #[test]
    fn batches_by_size_and_keeps_order() {
        let (tx, rx) = channel();
        let mut b = Batcher::new(&tx);
        let n = BATCH_MAX_ENTRIES * 2 + 5;
        for i in 0..n {
            assert!(b.push(entry(i)));
        }
        assert_eq!(b.finish(), n);
        drop(tx);
        let batches: Vec<Vec<Entry>> = rx.iter().collect();
        assert!(batches.iter().all(|b| b.len() <= BATCH_MAX_ENTRIES));
        let names: Vec<OsString> = batches.into_iter().flatten().map(|e| e.name).collect();
        let expected: Vec<OsString> = (0..n).map(|i| entry(i).name).collect();
        assert_eq!(names, expected);
    }

    #[test]
    fn flushes_after_delay() {
        let (tx, rx) = channel();
        let mut b = Batcher::new(&tx);
        assert!(b.push(entry(0)));
        std::thread::sleep(BATCH_MAX_DELAY + Duration::from_millis(2));
        assert!(b.push(entry(1)));
        assert_eq!(rx.try_recv().map(|v| v.len()), Ok(2));
    }

    #[test]
    fn empty_finish_sends_nothing_and_dropped_receiver_stops() {
        let (tx, rx) = channel();
        assert_eq!(Batcher::new(&tx).finish(), 0);
        assert!(rx.try_recv().is_err());

        drop(rx);
        let mut b = Batcher::new(&tx);
        b.batch.push(entry(0));
        assert!(!b.flush());
    }
}
