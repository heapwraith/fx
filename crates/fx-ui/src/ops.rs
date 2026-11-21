//! File operations of the app (task T1.12): the fx-ops queue, the undo/redo
//! history and the status messages, shared by every window.
//!
//! Each finished job is recorded with the job that reverts it (computed by
//! `fx_ops::undo_of` from what the job actually did). Ctrl+Z runs the
//! reversal and moves the entry to the redo side once it succeeded; Ctrl+Y
//! runs the original job again and records its new reversal. A reversal that
//! fails keeps its entry where it was, so nothing is silently lost.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use fx_core::UndoStack;
use fx_ops::{
    Engine, Event, Job, JobId, OpKind, Outcome, Report, system_dirs, system_guard, undo_of,
};
use gpui::Context;

/// How long a status message stays.
const NOTICE_FOR: Duration = Duration::from_secs(6);

#[derive(Clone)]
struct Entry {
    forward: Job,
    backward: Job,
}

enum Pending {
    Do(Job),
    Undo(Entry),
    Redo(Job),
}

pub struct Ops {
    engine: Option<Engine>,
    pending: HashMap<JobId, Pending>,
    history: UndoStack<Entry>,
    notice: Option<(String, Instant)>,
    /// Folders made by "new folder" jobs, for the tab that asked to rename it.
    created: HashMap<JobId, PathBuf>,
}

impl Ops {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            engine: Self::start(cx),
            pending: HashMap::new(),
            history: UndoStack::new(UndoStack::<Entry>::DEFAULT_LIMIT),
            notice: None,
            created: HashMap::new(),
        }
    }

    fn start(cx: &mut Context<Self>) -> Option<Engine> {
        let (tx, rx) = channel();
        let engine = Engine::start(tx).ok()?;
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
        Ok(id)
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        let Some(engine) = &self.engine else { return };
        match self.history.take_undo() {
            Some(entry) => {
                let id = engine.submit(entry.backward.clone());
                self.pending.insert(id, Pending::Undo(entry));
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
            }
            None => self.say("nada para refazer"),
        }
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
        let Event::Finished {
            job: id,
            outcome,
            report,
        } = event
        else {
            return;
        };
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
                    self.history.record(Entry {
                        forward: job.clone(),
                        backward,
                    });
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
                    self.history.push_undo(Entry {
                        forward: job.clone(),
                        backward,
                    });
                }
                self.say(describe(&job, &outcome, &report, "refeito: "));
            }
        }
        cx.notify();
    }
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 item".into()
    } else {
        format!("{n} itens")
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
        OpKind::NewFolder => "pasta criada".into(),
        OpKind::Restore => format!("restaurado da Lixeira: {}", count(report.restored.len())),
    };
    match outcome {
        Outcome::Done => format!("{prefix}{what}"),
        Outcome::Cancelled => format!("{prefix}cancelado ({what})"),
        Outcome::Failed(why) => format!("{prefix}erro: {why}"),
    }
}
