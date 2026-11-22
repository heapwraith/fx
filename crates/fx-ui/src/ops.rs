//! File operations of the app (task T1.12): the fx-ops queue, the undo/redo
//! history and the status messages, shared by every window.
//!
//! Each finished job is recorded with the job that reverts it (computed by
//! `fx_ops::undo_of` from what the job actually did). Ctrl+Z runs the
//! reversal and moves the entry to the redo side once it succeeded; Ctrl+Y
//! runs the original job again and records its new reversal. A reversal that
//! fails keeps its entry where it was, so nothing is silently lost.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use fx_core::UndoStack;
use fx_ops::{
    Engine, Event, Job, JobId, OpKind, Outcome, Report, Resolution, system_dirs, system_guard,
    undo_of,
};
use gpui::Context;

/// How long a status message stays.
const NOTICE_FOR: Duration = Duration::from_secs(6);

#[derive(Clone)]
struct Entry {
    forward: Job,
    backward: Job,
    /// For the history panel: what was done, and when.
    label: String,
    at: std::time::SystemTime,
}

impl Entry {
    fn new(forward: Job, backward: Job, report: &Report) -> Self {
        Self {
            label: history_label(&forward, report),
            forward,
            backward,
            at: std::time::SystemTime::now(),
        }
    }
}

/// One line of the history panel.
#[derive(Debug, Clone)]
pub struct DoneOp {
    /// Index for [`Ops::undo_at`].
    pub index: usize,
    pub label: String,
    pub at: std::time::SystemTime,
}

enum Pending {
    Do(Job),
    Undo(Entry),
    Redo(Job),
}

/// A running copy or move, for the queue panel (task T1.13).
#[derive(Debug, Clone)]
pub struct JobView {
    pub id: JobId,
    pub label: String,
    pub bytes: u64,
    pub done: u64,
    pub paused: bool,
}

/// A conflict waiting for the user's decision.
#[derive(Debug, Clone)]
pub struct Ask {
    pub job: JobId,
    pub name: String,
    pub folder: String,
    /// The item being copied/moved, and the one already there.
    pub incoming: Option<fx_ops::ItemInfo>,
    pub existing: Option<fx_ops::ItemInfo>,
}

pub struct Ops {
    engine: Option<Engine>,
    active: Vec<JobView>,
    asks: VecDeque<Ask>,
    pending: HashMap<JobId, Pending>,
    history: UndoStack<Entry>,
    notice: Option<(String, Instant)>,
    /// Folders made by "new folder" jobs, for the tab that asked to rename it.
    created: HashMap<JobId, PathBuf>,
    /// Pastes of cut files: the clipboard is emptied once they succeed.
    clear_after: std::collections::HashSet<JobId>,
    /// "Started" events that arrived before `submit` registered the job (the
    /// engine can start it right away): byte totals by job.
    early_starts: HashMap<JobId, u64>,
}

impl Ops {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            engine: Self::start(cx),
            active: Vec::new(),
            asks: VecDeque::new(),
            pending: HashMap::new(),
            history: UndoStack::new(UndoStack::<Entry>::DEFAULT_LIMIT),
            notice: None,
            created: HashMap::new(),
            clear_after: std::collections::HashSet::new(),
            early_starts: HashMap::new(),
        }
    }

    fn start(cx: &mut Context<Self>) -> Option<Engine> {
        let (tx, rx) = channel();
        // With the journal, temporary files left by a killed fx are deleted
        // now; without a data folder, the engine still runs without it.
        let engine = match crate::store::ops_journal_dir() {
            Some(dir) => Engine::start_with_journal(tx.clone(), &dir)
                .or_else(|_| Engine::start(tx))
                .ok()?,
            None => Engine::start(tx).ok()?,
        };
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                // Wait on a background thread, never on the UI thread.
                let (back, event) = background
                    .spawn(async move {
                        let e = rx.recv();
                        (rx, e)
                    })
                    .await;
                rx = back;
                let Ok(event) = event else { break };
                let alive = this.update(cx, |ops, cx| ops.on_event(event, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Some(engine)
    }

    /// Queues a user operation. `Err(reason)` when it touches a drive root or
    /// a system folder: ask the user, then submit `job.confirm_system()`.
    pub fn submit(&mut self, job: Job) -> Result<JobId, String> {
        if let Some(reason) = system_guard(&job, &system_dirs())
            && !job.is_confirmed()
        {
            return Err(reason);
        }
        let engine = self
            .engine
            .as_ref()
            .ok_or("fila de operações indisponível")?;
        let id = engine.submit(job.clone());
        self.pending.insert(id, Pending::Do(job));
        self.show_if_started(id);
        Ok(id)
    }

    /// Operations that can be undone, newest first (history panel).
    pub fn done(&self) -> Vec<DoneOp> {
        self.history
            .undo_entries()
            .iter()
            .enumerate()
            .rev()
            .map(|(index, e)| DoneOp {
                index,
                label: e.label.clone(),
                at: e.at,
            })
            .collect()
    }

    /// Undoes one operation from the history panel, not necessarily the last
    /// one. Its reversal goes through the engine like any job: nothing is
    /// overwritten, and items changed since are reported, not forced.
    pub fn undo_at(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(engine) = &self.engine else { return };
        if let Some(entry) = self.history.take_undo_at(index) {
            let id = engine.submit(entry.backward.clone());
            self.pending.insert(id, Pending::Undo(entry));
            self.show_if_started(id);
        }
        cx.notify();
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = &self.engine else { return };
        match self.history.take_undo() {
            Some(entry) => {
                let id = engine.submit(entry.backward.clone());
                self.pending.insert(id, Pending::Undo(entry));
                self.show_if_started(id);
            }
            None => self.say("nada para desfazer"),
        }
        cx.notify();
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = &self.engine else { return };
        match self.history.take_redo() {
            Some(entry) => {
                let id = engine.submit(entry.forward.clone());
                self.pending.insert(id, Pending::Redo(entry.forward));
                self.show_if_started(id);
            }
            None => self.say("nada para refazer"),
        }
        cx.notify();
    }

    /// Empties the clipboard when job `id` succeeds (paste of cut files).
    pub fn clear_clipboard_after(&mut self, id: JobId) {
        self.clear_after.insert(id);
    }

    /// Adds a started copy/move to the panel once both its "Started" event
    /// and its registration are in (they can arrive in either order).
    fn show_if_started(&mut self, id: JobId) {
        let Some(label) = self.pending.get(&id).and_then(start_label) else {
            return;
        };
        let Some(bytes) = self.early_starts.remove(&id) else {
            return;
        };
        self.active.push(JobView {
            id,
            label,
            bytes,
            done: 0,
            paused: false,
        });
    }

    /// Running copies and moves.
    pub fn active(&self) -> &[JobView] {
        &self.active
    }

    /// The conflict to show now, if any.
    pub fn current_ask(&self) -> Option<&Ask> {
        self.asks.front()
    }

    /// The user decided the current conflict.
    pub fn answer(&mut self, resolution: Resolution, apply_to_all: bool, cx: &mut Context<Self>) {
        if let (Some(ask), Some(engine)) = (self.asks.pop_front(), &self.engine) {
            engine.answer(ask.job, resolution, apply_to_all);
        }
        cx.notify();
    }

    pub fn set_paused(&mut self, id: JobId, paused: bool, cx: &mut Context<Self>) {
        if let Some(engine) = &self.engine {
            if paused {
                engine.pause(id);
            } else {
                engine.resume(id);
            }
        }
        if let Some(j) = self.active.iter_mut().find(|j| j.id == id) {
            j.paused = paused;
        }
        cx.notify();
    }

    pub fn cancel(&mut self, id: JobId, cx: &mut Context<Self>) {
        if let Some(engine) = &self.engine {
            engine.resume(id);
            engine.cancel(id);
        }
        self.asks.retain(|a| a.job != id);
        cx.notify();
    }

    /// The folder made by "new folder" job `id`, once it finished.
    pub fn created_by(&mut self, id: JobId) -> Option<PathBuf> {
        self.created.remove(&id)
    }

    /// The current status message, if recent.
    pub fn notice(&self) -> Option<&str> {
        self.notice
            .as_ref()
            .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
            .map(|(text, _)| text.as_str())
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), Instant::now()));
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        let (id, outcome, report) = match event {
            Event::Started { job, bytes, .. } => {
                self.early_starts.insert(job, bytes);
                self.show_if_started(job);
                cx.notify();
                return;
            }
            Event::Progress {
                job, done_bytes, ..
            } => {
                if let Some(j) = self.active.iter_mut().find(|j| j.id == job) {
                    j.done = done_bytes;
                    cx.notify();
                }
                return;
            }
            Event::Ask {
                job,
                source,
                existing,
                source_info,
                existing_info,
            } => {
                self.asks.push_back(Ask {
                    job,
                    name: source
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                    folder: existing
                        .parent()
                        .map_or_else(String::new, |p| p.display().to_string()),
                    incoming: source_info,
                    existing: existing_info,
                });
                cx.notify();
                return;
            }
            Event::Conflict { .. } => return,
            Event::Finished {
                job,
                outcome,
                report,
            } => (job, outcome, report),
        };
        self.active.retain(|j| j.id != id);
        self.asks.retain(|a| a.job != id);
        self.early_starts.remove(&id);
        if self.clear_after.remove(&id) && outcome == Outcome::Done {
            std::thread::spawn(|| {
                let _ = fx_shell::clear_clipboard();
            });
        }
        // Tags follow what fx renamed or moved (T3.4).
        for (old, new) in report
            .moved
            .iter()
            .chain(report.renamed.iter())
            .chain(report.batch_renamed.iter())
        {
            crate::tags::moved(old, new);
        }
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        if let Some(c) = &report.created {
            self.created.insert(id, c.clone());
        }
        let worked = report != Report::default();
        match pending {
            Pending::Do(job) => {
                // Even a cancelled or failed job may have done part of its work.
                if worked && let Some(backward) = undo_of(&job, &report) {
                    self.history
                        .record(Entry::new(job.clone(), backward, &report));
                }
                self.say(describe(&job, &outcome, &report, ""));
            }
            Pending::Undo(entry) => {
                if outcome == Outcome::Done {
                    self.say(describe(&entry.backward, &outcome, &report, "desfeito: "));
                    self.history.push_redo(entry);
                } else {
                    self.say(describe(&entry.backward, &outcome, &report, "desfazer: "));
                    self.history.push_undo(entry);
                }
            }
            Pending::Redo(job) => {
                if worked && let Some(backward) = undo_of(&job, &report) {
                    self.history
                        .push_undo(Entry::new(job.clone(), backward, &report));
                }
                self.say(describe(&job, &outcome, &report, "refeito: "));
            }
        }
        cx.notify();
    }
}

/// Panel label of a long job (copies and moves only).
fn start_label(pending: &Pending) -> Option<String> {
    let job = match pending {
        Pending::Do(j) | Pending::Redo(j) => j,
        Pending::Undo(e) => &e.backward,
    };
    let verb = match job.kind() {
        OpKind::Copy => "Copiando",
        OpKind::Move | OpKind::MoveBack => "Movendo",
        _ => return None,
    };
    Some(format!(
        "{verb} {} para {}",
        count(job.source_count()),
        job.dest_name()
    ))
}

fn count(n: usize) -> String {
    fx_core::format::items(n)
}

/// History panel line: what the operation did, in a sentence.
fn history_label(job: &Job, report: &Report) -> String {
    let (first, renamed) = job.first_names();
    let what = match (job.source_count(), first) {
        (1, Some(name)) => format!("\"{name}\""),
        (n, _) => count(n),
    };
    match job.kind() {
        OpKind::Copy => format!("Copiou {what} para {}", job.dest_name()),
        OpKind::Move | OpKind::MoveBack => format!("Moveu {what} para {}", job.dest_name()),
        OpKind::Recycle => format!("Enviou {what} para a Lixeira"),
        OpKind::Rename => match renamed {
            Some(new) => format!("Renomeou {what} para \"{new}\""),
            None => format!("Renomeou {what}"),
        },
        OpKind::BatchRename => format!("Renomeou {} em lote", count(job.source_count())),
        OpKind::NewFolder => match report.created.as_ref().and_then(|p| p.file_name()) {
            Some(n) => format!("Criou a pasta \"{}\"", n.to_string_lossy()),
            None => "Criou uma pasta".to_owned(),
        },
        OpKind::Restore => format!("Restaurou {what} da Lixeira"),
        OpKind::DeletePermanent => format!("Excluiu {what} permanentemente"),
    }
}

/// Status message for a finished job.
fn describe(job: &Job, outcome: &Outcome, report: &Report, prefix: &str) -> String {
    let what = match job.kind() {
        OpKind::Copy => format!("copiado: {}", count(report.copied.len())),
        OpKind::Move | OpKind::MoveBack => format!("movido: {}", count(report.moved.len())),
        OpKind::Recycle => {
            let sent = report.recycled.len() + report.untracked.len();
            let mut s = format!("enviado para a Lixeira: {}", count(sent));
            if !report.untracked.is_empty() {
                s.push_str(&format!(
                    " ({} não localizados na Lixeira: o Windows pode tê-los excluído de vez)",
                    report.untracked.len()
                ));
            }
            s
        }
        OpKind::DeletePermanent => {
            format!("excluído permanentemente: {}", count(report.deleted.len()))
        }
        OpKind::Rename => "renomeado".into(),
        OpKind::BatchRename => format!("renomeado: {}", count(report.batch_renamed.len())),
        OpKind::NewFolder => "pasta criada".into(),
        OpKind::Restore => format!("restaurado da Lixeira: {}", count(report.restored.len())),
    };
    match outcome {
        Outcome::Done => format!("{prefix}{what}"),
        Outcome::Cancelled => format!("{prefix}cancelado ({what})"),
        Outcome::Failed(why) => format!("{prefix}erro: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use super::history_label;
    use fx_ops::{Job, Report};
    use std::path::PathBuf;

    #[test]
    fn history_labels_say_what_was_done() {
        let r = Report::default();
        let one = Job::copy(
            vec![PathBuf::from(r"C:\a\foto.jpg")],
            PathBuf::from(r"C:\b"),
        );
        assert_eq!(history_label(&one, &r), "Copiou \"foto.jpg\" para b");
        let many = Job::recycle(vec![PathBuf::from(r"C:\a\x"), PathBuf::from(r"C:\a\y")]);
        assert_eq!(history_label(&many, &r), "Enviou 2 itens para a Lixeira");
        let ren = Job::rename(PathBuf::from(r"C:\a\x.txt"), "y.txt".into());
        assert_eq!(history_label(&ren, &r), "Renomeou \"x.txt\" para \"y.txt\"");
    }
}
