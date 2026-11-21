//! fx-ops: file operations engine (task T1.11): a queue of copy and move jobs
//! that follows docs/SAFETY.md §2.
//!
//! - Jobs run on worker threads, one worker per destination volume: jobs to
//!   the same disk run one after the other, jobs to different disks in
//!   parallel.
//! - Each file is copied (`CopyFile2`) to a temporary file next to its
//!   destination, its size is checked, and only then is it renamed to the
//!   final name. Cancelling or failing deletes the temporary file; the source
//!   is never touched by a copy.
//! - A move on the same volume is a rename. Across volumes it is a verified
//!   copy, and the source is deleted only after its whole copy is in place.
//! - A name conflict never overwrites by default: it is reported as an event
//!   and resolved by the job's policy, "keep both" (`nome (2).ext`) unless
//!   the user chose otherwise. The final rename itself refuses to replace.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.

use std::collections::HashMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

#[cfg(windows)]
mod windows;

pub type JobId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Copy,
    Move,
}

/// What to do when the destination name exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Resolution {
    /// Keep the existing item and give the new one a free name (`a (2).txt`).
    #[default]
    KeepBoth,
    /// Replace an existing file (folders are merged). Only on explicit choice.
    Replace,
    /// Leave this item out.
    Skip,
}

/// A copy or move of some items into a folder.
#[derive(Debug, Clone)]
pub struct Job {
    kind: OpKind,
    sources: Vec<PathBuf>,
    dest: PathBuf,
    on_conflict: Resolution,
    /// Tests: treat a move as cross-volume (copy, verify, delete).
    force_copy: bool,
}

impl Job {
    pub fn copy(sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self::new(OpKind::Copy, sources, dest)
    }

    pub fn move_to(sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self::new(OpKind::Move, sources, dest)
    }

    fn new(kind: OpKind, sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self {
            kind,
            sources,
            dest,
            on_conflict: Resolution::KeepBoth,
            force_copy: false,
        }
    }

    pub fn on_conflict(mut self, resolution: Resolution) -> Self {
        self.on_conflict = resolution;
        self
    }

    pub fn kind(&self) -> OpKind {
        self.kind
    }
}

/// What a job did, for the undo history (task T1.12).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// `(source, destination)` of every top-level item copied.
    pub copied: Vec<(PathBuf, PathBuf)>,
    /// `(old, new)` of every top-level item moved.
    pub moved: Vec<(PathBuf, PathBuf)>,
    /// Items left out (skipped conflicts, links to folders).
    pub skipped: Vec<PathBuf>,
    /// Existing files that were replaced (only with `Resolution::Replace`).
    pub replaced: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Started {
        job: JobId,
        files: u64,
        bytes: u64,
    },
    /// Sent at most every `PROGRESS_EVERY`, and after each file.
    Progress {
        job: JobId,
        done_bytes: u64,
        total_bytes: u64,
        current: PathBuf,
    },
    Conflict {
        job: JobId,
        source: PathBuf,
        existing: PathBuf,
        resolution: Resolution,
        /// Where the item went (`None` when skipped).
        placed_at: Option<PathBuf>,
    },
    Finished {
        job: JobId,
        outcome: Outcome,
        report: Report,
    },
}

const PROGRESS_EVERY: Duration = Duration::from_millis(50);

struct Work {
    id: JobId,
    job: Job,
    cancel: Arc<AtomicI32>,
}

/// The queue. Dropping it lets running jobs finish and stops the workers.
pub struct Engine {
    tx: Mutex<Sender<Work>>,
    cancels: Arc<Mutex<HashMap<JobId, Arc<AtomicI32>>>>,
    next: AtomicU64,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Engine")
    }
}

impl Engine {
    /// Starts the queue; events of every job arrive on `events`.
    pub fn start(events: Sender<Event>) -> io::Result<Self> {
        let (tx, rx) = channel::<Work>();
        let cancels: Arc<Mutex<HashMap<JobId, Arc<AtomicI32>>>> = Arc::default();
        let done = cancels.clone();
        std::thread::Builder::new()
            .name("fx-ops".into())
            .spawn(move || schedule(&rx, &events, &done))?;
        Ok(Self {
            tx: Mutex::new(tx),
            cancels,
            next: AtomicU64::new(1),
        })
    }

    /// Queues a job; returns its id (used in its events).
    pub fn submit(&self, job: Job) -> JobId {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicI32::new(0));
        lock(&self.cancels).insert(id, cancel.clone());
        let _ = lock(&self.tx).send(Work { id, job, cancel });
        id
    }

    /// Cancels a queued or running job (the current file is abandoned and its
    /// temporary file removed; finished files stay).
    pub fn cancel(&self, id: JobId) {
        if let Some(flag) = lock(&self.cancels).get(&id) {
            flag.store(1, Ordering::SeqCst);
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Volume of a path: its drive or UNC share (`C:`, `\\server\share`).
pub fn volume_key(path: &Path) -> String {
    match std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .components()
        .next()
    {
        Some(Component::Prefix(p)) => p.as_os_str().to_string_lossy().to_uppercase(),
        _ => String::new(),
    }
}

/// Hands each job to the worker of its destination volume.
fn schedule(
    rx: &Receiver<Work>,
    events: &Sender<Event>,
    cancels: &Arc<Mutex<HashMap<JobId, Arc<AtomicI32>>>>,
) {
    let mut workers: HashMap<String, Sender<Work>> = HashMap::new();
    for work in rx {
        let key = volume_key(&work.job.dest);
        let send = |tx: &Sender<Work>, w: Work| tx.send(w).map_err(|e| e.0);
        let pending = match workers.get(&key) {
            Some(tx) => send(tx, work).err(),
            None => Some(work),
        };
        if let Some(work) = pending {
            let (tx, wrx) = channel::<Work>();
            let events = events.clone();
            let cancels = cancels.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("fx-ops {key}"))
                .spawn(move || {
                    for w in wrx {
                        let id = w.id;
                        run_job(&w, &events);
                        lock(&cancels).remove(&id);
                    }
                });
            if spawned.is_ok() {
                let _ = tx.send(work);
                workers.insert(key, tx);
            }
        }
    }
}

/// Free name in `dir` for `name`: `a.txt`, `a (2).txt`, `a (3).txt`...
pub fn keep_both_name(dir: &Path, name: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !exists(p))
        .unwrap_or_else(|| dir.join(name))
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

/// Files and bytes under `path` (links to folders are not followed).
fn measure(path: &Path) -> (u64, u64) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (0, 0);
    };
    if meta.is_dir() && !is_link(&meta) {
        let mut total = (0, 0);
        if let Ok(rd) = std::fs::read_dir(path) {
            for e in rd.flatten() {
                let (f, b) = measure(&e.path());
                total.0 += f;
                total.1 += b;
            }
        }
        total
    } else {
        (1, meta.len())
    }
}

struct Run<'a> {
    id: JobId,
    job: &'a Job,
    cancel: &'a AtomicI32,
    events: &'a Sender<Event>,
    total: u64,
    done: u64,
    last_progress: Instant,
    report: Report,
}

#[derive(Debug)]
enum Stop {
    Cancelled,
    Failed(io::Error),
}

impl From<io::Error> for Stop {
    fn from(e: io::Error) -> Self {
        #[cfg(windows)]
        if e.raw_os_error() == Some(windows::REQUEST_ABORTED) {
            return Stop::Cancelled;
        }
        Stop::Failed(e)
    }
}

fn run_job(work: &Work, events: &Sender<Event>) {
    let job = &work.job;
    let (files, bytes) = job.sources.iter().fold((0, 0), |acc, s| {
        let (f, b) = measure(s);
        (acc.0 + f, acc.1 + b)
    });
    let _ = events.send(Event::Started {
        job: work.id,
        files,
        bytes,
    });
    let mut run = Run {
        id: work.id,
        job,
        cancel: &work.cancel,
        events,
        total: bytes,
        done: 0,
        last_progress: Instant::now(),
        report: Report::default(),
    };
    let result = job.sources.iter().try_for_each(|s| run.item(s));
    let outcome = match result {
        Ok(()) => Outcome::Done,
        Err(Stop::Cancelled) => Outcome::Cancelled,
        Err(Stop::Failed(e)) => Outcome::Failed(e.to_string()),
    };
    let _ = events.send(Event::Finished {
        job: work.id,
        outcome,
        report: run.report,
    });
}

impl Run<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst) != 0
    }

    /// One top-level source.
    fn item(&mut self, src: &Path) -> Result<(), Stop> {
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        let src = std::path::absolute(src)?;
        let meta = std::fs::symlink_metadata(&src)?;
        let name = src
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name"))?
            .to_string_lossy()
            .into_owned();
        let dest_dir = std::path::absolute(&self.job.dest)?;
        if meta.is_dir() && dest_dir.starts_with(&src) {
            return Err(Stop::Failed(io::Error::new(
                io::ErrorKind::InvalidInput,
                "não é possível copiar ou mover uma pasta para dentro dela mesma",
            )));
        }
        if self.job.kind == OpKind::Move && src.parent() == Some(dest_dir.as_path()) {
            return Ok(()); // moving into its own folder: nothing to do
        }
        let mut target = dest_dir.join(&name);
        let mut replace = false;
        if exists(&target) {
            let resolution = self.job.on_conflict;
            let placed = match resolution {
                Resolution::KeepBoth => Some(keep_both_name(&dest_dir, &name, exists)),
                Resolution::Replace => Some(target.clone()),
                Resolution::Skip => None,
            };
            let _ = self.events.send(Event::Conflict {
                job: self.id,
                source: src.clone(),
                existing: target.clone(),
                resolution,
                placed_at: placed.clone(),
            });
            match placed {
                Some(p) => {
                    replace = resolution == Resolution::Replace;
                    target = p;
                }
                None => {
                    self.report.skipped.push(src);
                    return Ok(());
                }
            }
        }

        if self.job.kind == OpKind::Move && !self.job.force_copy && !replace {
            match rename(&src, &target) {
                Ok(()) => {
                    self.done += measure(&target).1;
                    self.report.moved.push((src, target));
                    return Ok(());
                }
                Err(e) if is_not_same_device(&e) => {} // copy, verify, delete
                Err(e) => return Err(e.into()),
            }
        }
        self.copy_tree(&src, &target, replace)?;
        if self.job.kind == OpKind::Move {
            // Every file of `src` is now verified at `target`: only now may
            // the source go.
            if meta.is_dir() && !is_link(&meta) {
                std::fs::remove_dir_all(&src)?;
            } else {
                std::fs::remove_file(&src)?;
            }
            self.report.moved.push((src, target));
        } else {
            self.report.copied.push((src, target));
        }
        Ok(())
    }

    fn copy_tree(&mut self, src: &Path, target: &Path, replace: bool) -> Result<(), Stop> {
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        let meta = std::fs::symlink_metadata(src)?;
        if meta.is_dir() {
            if is_link(&meta) {
                // A link to a folder could loop or point anywhere: not followed.
                self.report.skipped.push(src.to_path_buf());
                return Ok(());
            }
            match std::fs::create_dir(target) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists && replace => {} // merge
                Err(e) => return Err(e.into()),
            }
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let child = target.join(entry.file_name());
                let replace_child = replace && exists(&child);
                if exists(&child) && !replace_child {
                    // Only possible when merging: keep both.
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let placed = keep_both_name(target, &name, exists);
                    self.copy_tree(&entry.path(), &placed, false)?;
                } else {
                    self.copy_tree(&entry.path(), &child, replace_child)?;
                }
            }
            Ok(())
        } else {
            self.copy_file(src, target, replace, meta.len())
        }
    }

    /// Temporary file, size check, then rename (or replace on request).
    fn copy_file(
        &mut self,
        src: &Path,
        target: &Path,
        replace: bool,
        size: u64,
    ) -> Result<(), Stop> {
        let dir = target.parent().unwrap_or(Path::new("."));
        let name = target
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let temp = dir.join(format!(".{name}.fx-{}-{}.tmp", std::process::id(), self.id));
        let base = self.done;
        let copied = {
            let (events, id, total) = (self.events, self.id, self.total);
            let last = &mut self.last_progress;
            let mut report = |bytes: u64| {
                if last.elapsed() >= PROGRESS_EVERY {
                    *last = Instant::now();
                    let _ = events.send(Event::Progress {
                        job: id,
                        done_bytes: base + bytes,
                        total_bytes: total,
                        current: src.to_path_buf(),
                    });
                }
            };
            copy_file(src, &temp, self.cancel, &mut report)
        };
        let finish = copied.map_err(Stop::from).and_then(|()| {
            let written = std::fs::metadata(&temp)?.len();
            if written != size {
                return Err(Stop::Failed(io::Error::other(format!(
                    "tamanho diferente depois da cópia: {written} de {size} bytes"
                ))));
            }
            if replace && exists(target) {
                // Explicit "replace": swap in one step (atomic on NTFS).
                std::fs::rename(&temp, target)?;
                self.report.replaced.push(target.to_path_buf());
                Ok(())
            } else {
                place(&temp, target).map_err(Stop::from)
            }
        });
        if finish.is_err() {
            let _ = std::fs::remove_file(&temp);
            return finish;
        }
        self.done = base + size;
        let _ = self.events.send(Event::Progress {
            job: self.id,
            done_bytes: self.done,
            total_bytes: self.total,
            current: src.to_path_buf(),
        });
        Ok(())
    }
}

/// Final rename of a verified temporary file. If the name was taken while
/// copying, the file gets a "keep both" name instead: nothing is replaced.
fn place(temp: &Path, target: &Path) -> io::Result<()> {
    match rename(temp, target) {
        Err(e) if exists(target) => {
            let dir = target.parent().unwrap_or(Path::new("."));
            let name = target
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let other = keep_both_name(dir, &name, exists);
            rename(temp, &other).map_err(|_| e)
        }
        r => r,
    }
}

#[cfg(windows)]
fn rename(from: &Path, to: &Path) -> io::Result<()> {
    windows::rename_no_replace(from, to)
}

#[cfg(windows)]
fn is_not_same_device(e: &io::Error) -> bool {
    e.raw_os_error() == Some(windows::NOT_SAME_DEVICE)
}

#[cfg(windows)]
fn copy_file(
    src: &Path,
    dst: &Path,
    cancel: &AtomicI32,
    report: &mut dyn FnMut(u64),
) -> io::Result<()> {
    windows::copy_file(src, dst, cancel, report)
}

#[cfg(test)]
mod tests;
