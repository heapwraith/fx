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
//!
//! Task T1.12 adds the Recycle Bin (`IFileOperation`; restoring for undo),
//! permanent deletion (only on explicit request), renaming, new folders, and
//! [`undo_of`]: the job that reverts a finished job. Jobs touching a drive
//! root or a system folder are refused unless confirmed ([`system_guard`]).
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
    /// Moves exact `(from, to)` pairs (undo of a move).
    MoveBack,
    Recycle,
    /// Permanent deletion: only on explicit request with confirmation.
    DeletePermanent,
    Rename,
    /// Several renames in one folder, as one operation (T2.8).
    BatchRename,
    NewFolder,
    /// Puts recycled items back (undo of a deletion).
    Restore,
}

/// A recycled item: where it was and where the Recycle Bin keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recycled {
    pub original: PathBuf,
    /// The `$I` file (metadata).
    pub info: PathBuf,
    /// The `$R` file or folder (data).
    pub data: PathBuf,
    /// Deletion time (FILETIME ticks).
    pub deleted_at: u64,
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
    /// Ask the user ([`Event::Ask`], answered with [`Engine::answer`]).
    Ask,
}

/// A copy or move of some items into a folder.
#[derive(Debug, Clone)]
pub struct Job {
    kind: OpKind,
    sources: Vec<PathBuf>,
    dest: PathBuf,
    on_conflict: Resolution,
    /// New name (rename) or folder name (new folder).
    name: String,
    pairs: Vec<(PathBuf, PathBuf)>,
    restore: Vec<Recycled>,
    /// The user confirmed an operation on a system folder or drive root.
    confirmed: bool,
    /// Tests: treat a move as cross-volume (copy, verify, delete).
    force_copy: bool,
    /// Tests: each file fails like a full disk after this many bytes were
    /// written to its temporary file (T2.4).
    #[cfg(test)]
    fail_after: Option<u64>,
    /// Tests: copy slowly (so another process can be killed mid-copy).
    #[cfg(test)]
    throttle: bool,
}

impl Job {
    pub fn copy(sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self::new(OpKind::Copy, sources, dest)
    }

    pub fn move_to(sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self::new(OpKind::Move, sources, dest)
    }

    /// Sends items to the Recycle Bin.
    pub fn recycle(items: Vec<PathBuf>) -> Self {
        Self::new(OpKind::Recycle, items, PathBuf::new())
    }

    /// Deletes items for good (Shift+Delete, after the user confirmed).
    pub fn delete_permanently(items: Vec<PathBuf>) -> Self {
        Self::new(OpKind::DeletePermanent, items, PathBuf::new())
    }

    /// Renames each `(item, new name)` in place, as one operation that undo
    /// reverts at once (T2.8). Two phases (all to temporary names, then to
    /// the final ones), so swaps and cycles (a→b, b→a) work; if a step
    /// fails, what was done is put back.
    pub fn batch_rename(pairs: Vec<(PathBuf, String)>) -> Self {
        let mut job = Self::new(OpKind::BatchRename, Vec::new(), PathBuf::new());
        job.pairs = pairs
            .into_iter()
            .map(|(item, name)| {
                let to = item.with_file_name(name);
                (item, to)
            })
            .collect();
        job
    }

    /// Renames `item` to `new_name` in the same folder (never replacing).
    pub fn rename(item: PathBuf, new_name: String) -> Self {
        let mut job = Self::new(OpKind::Rename, vec![item], PathBuf::new());
        job.name = new_name;
        job
    }

    /// Creates folder `name` in `parent` ("keep both" if taken).
    pub fn new_folder(parent: PathBuf, name: String) -> Self {
        let mut job = Self::new(OpKind::NewFolder, Vec::new(), parent);
        job.name = name;
        job
    }

    /// Puts recycled items back where they were.
    pub fn restore(items: Vec<Recycled>) -> Self {
        let mut job = Self::new(OpKind::Restore, Vec::new(), PathBuf::new());
        job.restore = items;
        job
    }

    /// Moves each `from` to exactly `to` (undo of a move).
    pub fn move_back(pairs: Vec<(PathBuf, PathBuf)>) -> Self {
        let mut job = Self::new(OpKind::MoveBack, Vec::new(), PathBuf::new());
        job.pairs = pairs;
        job
    }

    /// The user confirmed touching a system folder or drive root.
    pub fn confirm_system(mut self) -> Self {
        self.confirmed = true;
        self
    }

    fn new(kind: OpKind, sources: Vec<PathBuf>, dest: PathBuf) -> Self {
        Self {
            kind,
            sources,
            dest,
            on_conflict: Resolution::KeepBoth,
            name: String::new(),
            pairs: Vec::new(),
            restore: Vec::new(),
            confirmed: false,
            force_copy: false,
            #[cfg(test)]
            fail_after: None,
            #[cfg(test)]
            throttle: false,
        }
    }

    pub fn on_conflict(mut self, resolution: Resolution) -> Self {
        self.on_conflict = resolution;
        self
    }

    pub fn kind(&self) -> OpKind {
        self.kind
    }

    /// Name of the first item the job works on, and for renames the new
    /// name (for messages).
    pub fn first_names(&self) -> (Option<String>, Option<String>) {
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
        match self.pairs.first() {
            Some((from, to)) => (name(from), name(to)),
            None => (
                self.sources.first().and_then(|p| name(p)),
                (self.kind == OpKind::Rename).then(|| self.name.clone()),
            ),
        }
    }

    /// Number of items the job works on.
    pub fn source_count(&self) -> usize {
        self.sources.len().max(self.pairs.len())
    }

    /// Name of the destination folder (for messages).
    pub fn dest_name(&self) -> String {
        let dest = match self.pairs.first() {
            Some((_, to)) => to.parent().unwrap_or(to),
            None => self.dest.as_path(),
        };
        dest.file_name().map_or_else(
            || dest.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Whether the user confirmed touching a system folder or drive root.
    pub fn is_confirmed(&self) -> bool {
        self.confirmed
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
    /// Items now in the Recycle Bin.
    pub recycled: Vec<Recycled>,
    /// Items deleted for good (permanent deletion).
    pub deleted: Vec<PathBuf>,
    /// Items sent to the Recycle Bin that could not be found there (so they
    /// cannot be restored by undo; the Recycle Bin itself still has them,
    /// unless Windows deleted them for good after asking).
    pub untracked: Vec<PathBuf>,
    /// `(old, new)` path of a rename.
    pub renamed: Option<(PathBuf, PathBuf)>,
    /// `(old, new)` paths of a batch rename.
    pub batch_renamed: Vec<(PathBuf, PathBuf)>,
    /// Folder created by a "new folder" job.
    pub created: Option<PathBuf>,
    /// Where restored items went.
    pub restored: Vec<PathBuf>,
}

/// The job that reverts a finished job, from what it actually did. `None`
/// when there is nothing to revert, or for permanent deletions.
pub fn undo_of(job: &Job, report: &Report) -> Option<Job> {
    let undo = match job.kind {
        OpKind::Copy => {
            let copies: Vec<PathBuf> = report.copied.iter().map(|(_, d)| d.clone()).collect();
            // Copies go to the Recycle Bin, never straight to nothing.
            (!copies.is_empty()).then(|| Job::recycle(copies))
        }
        OpKind::Move | OpKind::MoveBack => {
            let back: Vec<(PathBuf, PathBuf)> = report
                .moved
                .iter()
                .map(|(a, b)| (b.clone(), a.clone()))
                .collect();
            (!back.is_empty()).then(|| Job::move_back(back))
        }
        OpKind::Recycle => {
            (!report.recycled.is_empty()).then(|| Job::restore(report.recycled.clone()))
        }
        OpKind::Restore => {
            (!report.restored.is_empty()).then(|| Job::recycle(report.restored.clone()))
        }
        OpKind::Rename => report.renamed.as_ref().and_then(|(old, new)| {
            let name = old.file_name()?.to_string_lossy().into_owned();
            Some(Job::rename(new.clone(), name))
        }),
        OpKind::BatchRename => (!report.batch_renamed.is_empty()).then(|| {
            Job::batch_rename(
                report
                    .batch_renamed
                    .iter()
                    .filter_map(|(old, new)| {
                        Some((new.clone(), old.file_name()?.to_string_lossy().into_owned()))
                    })
                    .collect(),
            )
        }),
        OpKind::NewFolder => report.created.clone().map(|c| Job::recycle(vec![c])),
        OpKind::DeletePermanent => None,
    };
    // The reversal touches the same places: the same confirmation holds.
    undo.map(|mut u| {
        u.confirmed = job.confirmed;
        u
    })
}

/// What dropping `sources` on folder `dest` does, like the Explorer: Ctrl
/// copies, Shift moves (Ctrl+Shift would link: fx copies), and without keys
/// items on the same volume move and others copy.
pub fn drop_operation(sources: &[PathBuf], dest: &Path, shift: bool, ctrl: bool) -> OpKind {
    match (shift, ctrl) {
        (_, true) => OpKind::Copy,
        (true, false) => OpKind::Move,
        (false, false) => {
            let dest_volume = volume_key(dest);
            if sources.iter().all(|s| volume_key(s) == dest_volume) {
                OpKind::Move
            } else {
                OpKind::Copy
            }
        }
    }
}

/// Folders where a slip is dangerous: Windows and the program folders.
pub fn system_dirs() -> Vec<PathBuf> {
    [
        "SystemRoot",
        "windir",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .collect()
}

fn lower(p: &Path) -> PathBuf {
    PathBuf::from(p.as_os_str().to_string_lossy().to_lowercase())
}

/// Why `job` needs an extra confirmation, if it does (SAFETY.md §2.7): it
/// deletes, moves or renames a drive root, or touches a system folder.
pub fn system_guard(job: &Job, system: &[PathBuf]) -> Option<String> {
    let abs = |p: &Path| lower(&std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf()));
    let mut touched: Vec<PathBuf> = job.sources.iter().map(|p| abs(p)).collect();
    touched.extend(job.pairs.iter().map(|(from, _)| abs(from)));
    if touched.iter().any(|p| p.parent().is_none()) {
        return Some(
            fx_core::tr!(
                "a operação apaga, move ou renomeia a raiz de uma unidade",
                "the operation deletes, moves or renames the root of a drive"
            )
            .into(),
        );
    }
    touched.extend(job.pairs.iter().map(|(_, to)| abs(to)));
    if matches!(job.kind, OpKind::Copy | OpKind::Move | OpKind::NewFolder) {
        touched.push(abs(&job.dest));
    }
    touched.extend(job.restore.iter().map(|r| abs(&r.original)));
    let system: Vec<PathBuf> = system.iter().map(|s| abs(s)).collect();
    touched
        .iter()
        .find_map(|p| system.iter().find(|s| p.starts_with(s)))
        .map(|s| {
            fx_core::trf!(
                "a operação mexe numa pasta do sistema ({})",
                "the operation touches a system folder ({})",
                s.display()
            )
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    Failed(String),
}

// `Finished` carries the report and is much bigger than the other variants;
/// Size and modification date of one side of a conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemInfo {
    pub is_dir: bool,
    /// File size (0 for folders: their size would need a walk).
    pub bytes: u64,
    pub modified: Option<std::time::SystemTime>,
}

impl ItemInfo {
    /// Reads it from disk (runs on the job's worker thread).
    fn of(path: &Path) -> Option<Self> {
        let m = std::fs::symlink_metadata(path).ok()?;
        Some(Self {
            is_dir: m.is_dir(),
            bytes: if m.is_dir() { 0 } else { m.len() },
            modified: m.modified().ok(),
        })
    }
}

// there is one per job and progress is throttled, so boxing would buy nothing.
#[allow(clippy::large_enum_variant)]
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
    /// A conflict under [`Resolution::Ask`]: the job waits for
    /// [`Engine::answer`] (or a cancel).
    Ask {
        job: JobId,
        source: PathBuf,
        existing: PathBuf,
        /// Size and date of both sides, so the user can compare them
        /// (`None` when they could not be read).
        source_info: Option<ItemInfo>,
        existing_info: Option<ItemInfo>,
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

/// The user's handles on a job.
struct Control {
    cancel: Arc<AtomicI32>,
    paused: std::sync::atomic::AtomicBool,
    /// Answers to [`Event::Ask`]: the resolution and "apply to all".
    answers: Mutex<Sender<(Resolution, bool)>>,
}

type Controls = Arc<Mutex<HashMap<JobId, Arc<Control>>>>;

struct Work {
    id: JobId,
    job: Box<Job>,
    journal: Option<Arc<journal::Journal>>,
    control: Arc<Control>,
    answers: Receiver<(Resolution, bool)>,
}

/// The queue. Dropping it lets running jobs finish and stops the workers.
pub struct Engine {
    tx: Mutex<Sender<Work>>,
    journal: Option<Arc<journal::Journal>>,
    controls: Controls,
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
        Self::start_inner(events, None)
    }

    /// [`Engine::start`] with a journal of temporary files in `dir` (T2.4):
    /// first deletes the temporary files left by processes that died in the
    /// middle of a copy ([`recover`]), then records this process's.
    pub fn start_with_journal(events: Sender<Event>, dir: &Path) -> io::Result<Self> {
        recover(dir);
        let journal = journal::Journal::create(dir)?;
        Self::start_inner(events, Some(Arc::new(journal)))
    }

    fn start_inner(
        events: Sender<Event>,
        journal: Option<Arc<journal::Journal>>,
    ) -> io::Result<Self> {
        let (tx, rx) = channel::<Work>();
        let controls: Controls = Arc::default();
        let done = controls.clone();
        std::thread::Builder::new()
            .name("fx-ops".into())
            .spawn(move || schedule(&rx, &events, &done))?;
        Ok(Self {
            tx: Mutex::new(tx),
            journal,
            controls,
            next: AtomicU64::new(1),
        })
    }

    /// Queues a job; returns its id (used in its events).
    pub fn submit(&self, job: Job) -> JobId {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (answer_tx, answers) = channel();
        let control = Arc::new(Control {
            cancel: Arc::new(AtomicI32::new(0)),
            paused: std::sync::atomic::AtomicBool::new(false),
            answers: Mutex::new(answer_tx),
        });
        lock(&self.controls).insert(id, control.clone());
        let _ = lock(&self.tx).send(Work {
            id,
            job: Box::new(job),
            journal: self.journal.clone(),
            control,
            answers,
        });
        id
    }

    fn control(&self, id: JobId) -> Option<Arc<Control>> {
        lock(&self.controls).get(&id).cloned()
    }

    /// Cancels a queued or running job (the current file is abandoned and its
    /// temporary file removed; finished files stay).
    pub fn cancel(&self, id: JobId) {
        if let Some(c) = self.control(id) {
            c.cancel.store(1, Ordering::SeqCst);
        }
    }

    /// Pauses a job before its next file (the file being copied finishes).
    pub fn pause(&self, id: JobId) {
        if let Some(c) = self.control(id) {
            c.paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume(&self, id: JobId) {
        if let Some(c) = self.control(id) {
            c.paused.store(false, Ordering::SeqCst);
        }
    }

    /// Answers a job's [`Event::Ask`]; `apply_to_all` reuses the answer for
    /// the job's later conflicts.
    pub fn answer(&self, id: JobId, resolution: Resolution, apply_to_all: bool) {
        if let Some(c) = self.control(id) {
            let _ = lock(&c.answers).send((resolution, apply_to_all));
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
fn schedule(rx: &Receiver<Work>, events: &Sender<Event>, controls: &Controls) {
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
            let controls = controls.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("fx-ops {key}"))
                .spawn(move || {
                    #[cfg(windows)]
                    windows::com_init();
                    for w in wrx {
                        let id = w.id;
                        run_job(&w, &events);
                        lock(&controls).remove(&id);
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
    /// Where temporary files are recorded before they exist (T2.4).
    journal: Option<&'a journal::Journal>,
    cancel: &'a AtomicI32,
    control: &'a Control,
    answers: &'a Receiver<(Resolution, bool)>,
    /// "Apply to all" answer for later conflicts.
    sticky: Option<Resolution>,
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
    if !job.confirmed
        && let Some(reason) = system_guard(job, &system_dirs())
    {
        let _ = events.send(Event::Finished {
            job: work.id,
            outcome: Outcome::Failed(fx_core::trf!(
                "{reason}: é preciso confirmar",
                "{reason}: confirmation needed"
            )),
            report: Report::default(),
        });
        return;
    }
    let measured: Vec<&Path> = match job.kind {
        OpKind::Copy | OpKind::Move | OpKind::Recycle | OpKind::DeletePermanent => {
            job.sources.iter().map(PathBuf::as_path).collect()
        }
        OpKind::MoveBack => job.pairs.iter().map(|(from, _)| from.as_path()).collect(),
        _ => Vec::new(),
    };
    let (files, bytes) = measured.iter().fold((0, 0), |acc, s| {
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
        journal: work.journal.as_deref(),
        cancel: &work.control.cancel,
        control: &work.control,
        answers: &work.answers,
        sticky: None,
        events,
        total: bytes,
        done: 0,
        last_progress: Instant::now(),
        report: Report::default(),
    };
    let result = match job.kind {
        OpKind::Copy | OpKind::Move => job.sources.iter().try_for_each(|s| run.item(s)),
        OpKind::MoveBack => job
            .pairs
            .iter()
            .try_for_each(|(from, to)| run.move_exact(from, to)),
        OpKind::Recycle => run.recycle(),
        OpKind::DeletePermanent => run.delete_permanently(),
        OpKind::Rename => run.rename_item(),
        OpKind::BatchRename => run.batch_rename(),
        OpKind::NewFolder => run.new_folder(),
        OpKind::Restore => run.restore_items(),
    };
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
    /// Moves `from` to exactly `to` (a free "keep both" name if `to` exists).
    fn move_exact(&mut self, from: &Path, to: &Path) -> Result<(), Stop> {
        if self.cancelled() {
            return Err(Stop::Cancelled);
        }
        let mut target = to.to_path_buf();
        if exists(&target) {
            let dir = to.parent().unwrap_or(Path::new("."));
            let name = to
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            target = keep_both_name(dir, &name, exists);
        }
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        match rename(from, &target) {
            Ok(()) => {}
            Err(e) if is_not_same_device(&e) || self.job.force_copy => {
                let meta = std::fs::symlink_metadata(from)?;
                self.copy_tree(from, &target, false)?;
                if meta.is_dir() && !is_link(&meta) {
                    std::fs::remove_dir_all(from)?;
                } else {
                    std::fs::remove_file(from)?;
                }
            }
            Err(e) => return Err(e.into()),
        }
        self.report.moved.push((from.to_path_buf(), target));
        Ok(())
    }

    fn recycle(&mut self) -> Result<(), Stop> {
        let items: Vec<PathBuf> = self
            .job
            .sources
            .iter()
            .map(std::path::absolute)
            .collect::<io::Result<_>>()?;
        // The Recycle Bin records the real path (through subst drives and
        // junctions): resolve it while the item still exists.
        let real: Vec<PathBuf> = items.iter().map(|i| canonical_path(i)).collect();
        recycle_items(&items)?;
        for (item, real) in items.into_iter().zip(real) {
            match find_recycled(&item, &real) {
                Some(r) => self.report.recycled.push(r),
                None if !exists(&item) => self.report.untracked.push(item),
                None => {}
            }
        }
        Ok(())
    }

    fn delete_permanently(&mut self) -> Result<(), Stop> {
        for item in &self.job.sources {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            let meta = std::fs::symlink_metadata(item)?;
            if meta.is_dir() && !is_link(&meta) {
                std::fs::remove_dir_all(item)?;
            } else if meta.is_dir() {
                std::fs::remove_dir(item)?; // a link to a folder: only the link
            } else {
                std::fs::remove_file(item)?;
            }
            self.report.deleted.push(item.clone());
        }
        Ok(())
    }

    fn rename_item(&mut self) -> Result<(), Stop> {
        let from = std::path::absolute(&self.job.sources[0])?;
        let name = self.job.name.trim();
        if name.is_empty() || name.contains(['\\', '/']) || name == "." || name == ".." {
            return Err(Stop::Failed(io::Error::new(
                io::ErrorKind::InvalidInput,
                fx_core::tr!("nome inválido", "invalid name"),
            )));
        }
        let to = from.with_file_name(name);
        rename(&from, &to).map_err(|e| {
            if e.kind() == io::ErrorKind::AlreadyExists
                || exists(&to) && !same_file_name(&from, &to)
            {
                Stop::Failed(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    fx_core::tr!(
                        "já existe um item com esse nome",
                        "an item with that name already exists"
                    ),
                ))
            } else {
                Stop::from(e)
            }
        })?;
        self.report.renamed = Some((from, to));
        Ok(())
    }

    fn batch_rename(&mut self) -> Result<(), Stop> {
        let pairs: Vec<(PathBuf, PathBuf)> = self
            .job
            .pairs
            .iter()
            .filter(|(from, to)| from != to)
            .map(|(from, to)| Ok((std::path::absolute(from)?, std::path::absolute(to)?)))
            .collect::<io::Result<_>>()?;
        for (_, to) in &pairs {
            let name = to
                .file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default();
            if name.trim().is_empty() || name == "." || name == ".." {
                return Err(Stop::Failed(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    fx_core::tr!("nome inválido", "invalid name"),
                )));
            }
        }
        // Phase 1: every item to a temporary name next to it.
        let mut temps: Vec<PathBuf> = Vec::with_capacity(pairs.len());
        let mut failure = None;
        for (k, (from, _)) in pairs.iter().enumerate() {
            let temp = from.with_file_name(format!(
                ".fx-ren-{}-{}-{k}.tmp",
                std::process::id(),
                self.id
            ));
            match rename(from, &temp) {
                Ok(()) => temps.push(temp),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        // Phase 2: temporary names to the final ones (never replacing).
        let mut placed = 0;
        if failure.is_none() {
            for ((_, to), temp) in pairs.iter().zip(&temps) {
                match rename(temp, to) {
                    Ok(()) => placed += 1,
                    Err(e) => {
                        failure = Some(if exists(to) {
                            io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                fx_core::trf!(
                                    "já existe um item chamado {}",
                                    "an item named {} already exists",
                                    to.file_name()
                                        .map(|n| n.to_string_lossy())
                                        .unwrap_or_default()
                                ),
                            )
                        } else {
                            e
                        });
                        break;
                    }
                }
            }
        }
        if let Some(e) = failure {
            // Put everything back: final names first, then temporary ones.
            for (k, (from, to)) in pairs.iter().enumerate() {
                let current = if k < placed {
                    to
                } else if k < temps.len() {
                    &temps[k]
                } else {
                    continue;
                };
                let _ = rename(current, from);
            }
            return Err(Stop::Failed(e));
        }
        self.report.batch_renamed = pairs;
        Ok(())
    }

    fn new_folder(&mut self) -> Result<(), Stop> {
        let dir = std::path::absolute(&self.job.dest)?;
        let name = self.job.name.trim();
        let mut target = dir.join(name);
        if exists(&target) {
            target = keep_both_name(&dir, name, exists);
        }
        std::fs::create_dir(&target)?;
        self.report.created = Some(target);
        Ok(())
    }

    fn restore_items(&mut self) -> Result<(), Stop> {
        for item in &self.job.restore {
            let placed = restore_item(item)?;
            self.report.restored.push(placed);
        }
        Ok(())
    }

    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst) != 0
    }

    /// Between files: waits while paused; stops when cancelled.
    fn checkpoint(&self) -> Result<(), Stop> {
        while self.control.paused.load(Ordering::SeqCst) && !self.cancelled() {
            std::thread::sleep(Duration::from_millis(50));
        }
        if self.cancelled() {
            Err(Stop::Cancelled)
        } else {
            Ok(())
        }
    }

    /// The resolution of a conflict: the job's policy, a previous "apply to
    /// all" answer, or the user's answer (waiting for it, cancellable).
    fn resolve(&mut self, source: &Path, existing: &Path) -> Result<Resolution, Stop> {
        if self.job.on_conflict != Resolution::Ask {
            return Ok(self.job.on_conflict);
        }
        if let Some(r) = self.sticky {
            return Ok(r);
        }
        let _ = self.events.send(Event::Ask {
            job: self.id,
            source: source.to_path_buf(),
            existing: existing.to_path_buf(),
            source_info: ItemInfo::of(source),
            existing_info: ItemInfo::of(existing),
        });
        loop {
            if self.cancelled() {
                return Err(Stop::Cancelled);
            }
            match self.answers.recv_timeout(Duration::from_millis(100)) {
                Ok((resolution, all)) => {
                    // "Ask" is not an answer: fall back to the safe default.
                    let r = if resolution == Resolution::Ask {
                        Resolution::KeepBoth
                    } else {
                        resolution
                    };
                    if all {
                        self.sticky = Some(r);
                    }
                    return Ok(r);
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(Stop::Cancelled),
            }
        }
    }

    /// One top-level source.
    fn item(&mut self, src: &Path) -> Result<(), Stop> {
        self.checkpoint()?;
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
                fx_core::tr!(
                    "não é possível copiar ou mover uma pasta para dentro dela mesma",
                    "a folder cannot be copied or moved into itself"
                ),
            )));
        }
        if self.job.kind == OpKind::Move && src.parent() == Some(dest_dir.as_path()) {
            return Ok(()); // moving into its own folder: nothing to do
        }
        let mut target = dest_dir.join(&name);
        let mut replace = false;
        if exists(&target) {
            let resolution = self.resolve(&src, &target)?;
            let placed = match resolution {
                Resolution::KeepBoth | Resolution::Ask => {
                    Some(keep_both_name(&dest_dir, &name, exists))
                }
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
        self.checkpoint()?;
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
        if let Some(journal) = self.journal {
            // Before the file exists: if fx dies while copying, the next start
            // finds and deletes it. No journal entry, no copy.
            journal.add(&temp)?;
        }
        let base = self.done;
        let copied = {
            let (events, id, total) = (self.events, self.id, self.total);
            let last = &mut self.last_progress;
            #[cfg(test)]
            let throttle = self.job.throttle;
            let mut report = |bytes: u64| {
                #[cfg(test)]
                if throttle {
                    std::thread::sleep(Duration::from_millis(15));
                }
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
            #[cfg(test)]
            let copied = match self.job.fail_after {
                Some(limit) => disk_full_after(src, &temp, limit),
                None => copy_file(src, &temp, self.cancel, &mut report),
            };
            #[cfg(not(test))]
            let copied = copy_file(src, &temp, self.cancel, &mut report);
            copied
        };
        let finish = copied.map_err(Stop::from).and_then(|()| {
            let written = std::fs::metadata(&temp)?.len();
            if written != size {
                return Err(Stop::Failed(io::Error::other(fx_core::trf!(
                    "tamanho diferente depois da cópia: {written} de {size} bytes",
                    "size differs after the copy: {written} of {size} bytes"
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

/// Tests: a copy that runs out of disk space. Writes the first `limit`
/// bytes of `src` to `dst` (the partial file a real copy leaves behind) and
/// fails with `ERROR_DISK_FULL`, like `CopyFile2` on a full volume.
#[cfg(test)]
fn disk_full_after(src: &Path, dst: &Path, limit: u64) -> io::Result<()> {
    use std::io::Read;
    let mut partial = Vec::new();
    std::fs::File::open(src)?
        .take(limit)
        .read_to_end(&mut partial)?;
    std::fs::write(dst, partial)?;
    Err(io::Error::from_raw_os_error(112)) // ERROR_DISK_FULL
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

/// Same name except for case (a case-only rename is allowed).
fn same_file_name(a: &Path, b: &Path) -> bool {
    lower(a) == lower(b)
}

#[cfg(windows)]
fn recycle_items(items: &[PathBuf]) -> io::Result<()> {
    windows::recycle::recycle(items)
}

#[cfg(windows)]
fn find_recycled(item: &Path, real: &Path) -> Option<Recycled> {
    windows::recycle::find_recycled(item, real)
}

#[cfg(windows)]
fn canonical_path(item: &Path) -> PathBuf {
    windows::recycle::canonical(item)
}

#[cfg(windows)]
fn restore_item(item: &Recycled) -> io::Result<PathBuf> {
    windows::recycle::restore(item)
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

mod journal;
pub use journal::recover;

#[cfg(test)]
mod tests;
