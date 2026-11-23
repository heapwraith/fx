//! The undo history across sessions (T6.6, D-068): the saved rows wait here
//! until the operations engine takes them in, and the engine's current rows
//! wait here to be saved with the session. Until the saved rows are taken in,
//! nothing is saved, so an earlier history is never lost.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::store::UndoRow;

enum Saved {
    /// Still being read.
    Loading,
    Ready(Vec<UndoRow>),
    /// Taken in, or nothing to read (`FX_NO_STATE=1`, unreadable).
    Done,
}

struct State {
    saved: Saved,
    /// This session's history, once the saved one is in it.
    current: Option<Vec<UndoRow>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    saved: Saved::Loading,
    current: None,
});

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The saved rows were read (startup thread).
pub fn set_loaded(rows: Vec<UndoRow>) {
    lock().saved = Saved::Ready(rows);
}

/// Nothing will arrive (no saved state this session).
pub fn set_none() {
    lock().saved = Saved::Done;
}

/// `Some(rows)` once, when they arrived; `None` while loading or after.
/// The second value says whether to keep waiting.
pub fn take_loaded() -> (Option<Vec<UndoRow>>, bool) {
    let mut s = lock();
    match std::mem::replace(&mut s.saved, Saved::Done) {
        Saved::Ready(rows) => (Some(rows), false),
        Saved::Loading => {
            s.saved = Saved::Loading;
            (None, true)
        }
        Saved::Done => (None, false),
    }
}

/// This session's history (after the saved one was taken in).
pub fn set_current(rows: Vec<UndoRow>) {
    lock().current = Some(rows);
}

/// What to save with the session; `None` until the saved history is in.
pub fn saved() -> Option<Vec<UndoRow>> {
    lock().current.clone()
}
