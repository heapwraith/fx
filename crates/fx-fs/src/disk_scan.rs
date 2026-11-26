//! Reading a folder tree for the disk map (T6.1, D-078): a few threads list
//! folders in parallel (breadth first, so the map shows every level early)
//! and one thread puts what they read into the shared `DiskTree`, holding its
//! lock only for short batches so the UI can read it without waiting
//! (`try_read`). Links and junctions are not entered; folders that cannot be
//! read count as read. A folder that an earlier scan already mapped is
//! grafted from it instead of being read again.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use fx_core::disk_tree::{DiskTree, Listed, ROOT};

use crate::{Attributes, Strategy};

/// Totals another source knows (the index): subfolder name to bytes.
pub type Totals = Box<dyn Fn(&Path) -> Option<HashMap<OsString, u64>> + Send>;

/// Folders this close to the root get their totals from `Totals` first, so
/// the map has its final shape before they are read.
const EXPECTED_DEPTH: usize = 2;
/// Longest the tree stays locked for one batch.
const BATCH: Duration = Duration::from_millis(8);
/// The UI hears about changes at most this often.
const NOTIFY_EVERY: Duration = Duration::from_millis(120);

struct Queue {
    jobs: VecDeque<(u32, PathBuf)>,
    finished: bool,
}

/// Reads everything below the tree's root into `tree` (which must be new),
/// on threads of its own. `notify` gets a message while it progresses and one
/// at the end; `done` is set when it ends (finished or cancelled).
pub fn scan_tree(
    tree: Arc<RwLock<DiskTree>>,
    reuse: Option<Arc<RwLock<DiskTree>>>,
    totals: Option<Totals>,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    notify: Sender<()>,
) -> std::io::Result<()> {
    let root = tree
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .root_path()
        .to_path_buf();
    let queue = Arc::new((
        Mutex::new(Queue {
            jobs: VecDeque::from([(ROOT, root)]),
            finished: false,
        }),
        Condvar::new(),
    ));
    let (tx, rx) = channel::<(u32, PathBuf, Option<Vec<Listed>>)>();
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(2, 6);
    for k in 0..workers {
        let queue = queue.clone();
        let tx = tx.clone();
        let cancel = cancel.clone();
        std::thread::Builder::new()
            .name(format!("fx-map-read-{k}"))
            .spawn(move || worker(&queue, &tx, &cancel))?;
    }
    drop(tx);
    std::thread::Builder::new()
        .name("fx-map-tree".into())
        .spawn(move || {
            let mut outstanding = 1usize;
            let mut last_notify = Instant::now();
            while outstanding > 0 && !cancel.load(Ordering::Relaxed) {
                let Ok(first) = rx.recv() else { break };
                let mut new_jobs = Vec::new();
                let mut expected = Vec::new();
                {
                    let mut t = tree.write().unwrap_or_else(PoisonError::into_inner);
                    let start = Instant::now();
                    let mut next = Some(first);
                    while let Some((dir, path, listed)) = next.take() {
                        outstanding -= 1;
                        match listed {
                            None => t.listing_failed(dir),
                            Some(entries) => {
                                for (id, name) in t.add_listing(dir, entries) {
                                    let sub = path.join(&name);
                                    let old = reuse
                                        .as_ref()
                                        .map(|r| r.read().unwrap_or_else(PoisonError::into_inner))
                                        .filter(|old| same_path(old.root_path(), &sub));
                                    match old {
                                        Some(old) => t.graft(id, &old),
                                        None => new_jobs.push((id, sub)),
                                    }
                                }
                                if totals.is_some() && t.depth(dir) < EXPECTED_DEPTH {
                                    expected.push((dir, path));
                                }
                            }
                        }
                        if start.elapsed() < BATCH {
                            next = rx.try_recv().ok();
                        }
                    }
                }
                // The index answers outside the lock.
                if let Some(totals) = totals.as_ref() {
                    for (dir, path) in expected {
                        let Some(known) = totals(&path) else { continue };
                        let mut t = tree.write().unwrap_or_else(PoisonError::into_inner);
                        let subs: Vec<u32> = t.children(dir).filter(|&c| t.is_dir(c)).collect();
                        for c in subs {
                            if let Some(&bytes) = known.get(&t.os_name(c)) {
                                t.set_expected(c, bytes);
                            }
                        }
                    }
                }
                outstanding += new_jobs.len();
                if !new_jobs.is_empty() {
                    let (lock, wake) = &*queue;
                    lock.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .jobs
                        .extend(new_jobs);
                    wake.notify_all();
                }
                if last_notify.elapsed() >= NOTIFY_EVERY {
                    last_notify = Instant::now();
                    if notify.send(()).is_err() {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            let (lock, wake) = &*queue;
            lock.lock().unwrap_or_else(PoisonError::into_inner).finished = true;
            wake.notify_all();
            done.store(true, Ordering::Relaxed);
            let _ = notify.send(());
        })?;
    Ok(())
}

fn worker(
    queue: &(Mutex<Queue>, Condvar),
    tx: &Sender<(u32, PathBuf, Option<Vec<Listed>>)>,
    cancel: &AtomicBool,
) {
    let (lock, wake) = queue;
    loop {
        let job = {
            let mut q = lock.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if q.finished || cancel.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(job) = q.jobs.pop_front() {
                    break job;
                }
                q = wake
                    .wait_timeout(q, Duration::from_millis(200))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        };
        let (dir, path) = job;
        let listed = read(&path);
        if tx.send((dir, path, listed)).is_err() {
            return;
        }
    }
}

/// The entries of one folder; links to folders are left out (not entered,
/// not counted twice).
pub fn read(path: &Path) -> Option<Vec<Listed>> {
    let (tx, rx) = channel();
    crate::enumerate(Strategy::FindFirstFileEx, path, &tx).ok()?;
    drop(tx);
    Some(
        rx.into_iter()
            .flatten()
            .filter(|e| !(e.attributes.is_dir() && e.attributes.contains(Attributes::LINK)))
            .map(|e| Listed {
                is_dir: e.attributes.is_dir(),
                size: if e.attributes.is_dir() { 0 } else { e.size },
                name: e.name,
            })
            .collect(),
    )
}

fn same_path(a: &Path, b: &Path) -> bool {
    a == b
        || a.as_os_str()
            .to_str()
            .zip(b.as_os_str().to_str())
            .is_some_and(|(a, b)| a.to_lowercase() == b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(root: &Path, reuse: Option<Arc<RwLock<DiskTree>>>) -> DiskTree {
        let tree = Arc::new(RwLock::new(DiskTree::new(root.to_path_buf())));
        let done = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        scan_tree(
            tree.clone(),
            reuse,
            None,
            Arc::new(AtomicBool::new(false)),
            done.clone(),
            tx,
        )
        .unwrap();
        while !done.load(Ordering::Relaxed) {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
        tree.read().unwrap().clone()
    }

    #[test]
    fn reads_the_whole_tree() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir_all(sub.join("deep")).unwrap();
        std::fs::write(sub.join("deep").join("a.bin"), vec![0u8; 300_000]).unwrap();
        std::fs::write(sub.join("b.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("c.txt"), b"xy").unwrap();
        let t = run(dir.path(), None);
        assert!(t.complete(ROOT));
        assert_eq!(t.bytes(ROOT), 300_007);
        assert_eq!(t.files(ROOT), 3);
        let a = t.find(&sub.join("deep").join("a.bin")).unwrap();
        assert_eq!(t.bytes(a), 300_000);
        assert_eq!(t.path_of(a), sub.join("deep").join("a.bin"));
    }

    #[test]
    fn going_up_reuses_the_mapped_folder() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        std::fs::write(sub.join("a.bin"), vec![0u8; 400_000]).unwrap();
        let old = run(&sub, None);
        // Changed after the first scan: the graft keeps what was read.
        std::fs::write(sub.join("new.bin"), vec![0u8; 1000]).unwrap();
        let t = run(dir.path(), Some(Arc::new(RwLock::new(old))));
        assert_eq!(t.bytes(ROOT), 400_000);
        assert!(t.find(&sub.join("a.bin")).is_some());
    }
}
