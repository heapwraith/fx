//! The folders opened, for "ir para" que aprende (T6.5, D-068): kept in memory
//! for the palette (a lock and a sort of at most 500 entries), loaded from
//! `state.db` on a thread at startup and saved with the session. With
//! `FX_NO_STATE=1`, memory only.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use fx_core::frecency::{Frecency, Visit};

struct State {
    frecency: Frecency,
    /// The saved visits were read: from then on, saving replaces them.
    loaded: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    frecency: Frecency::new(),
    loaded: false,
});

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The saved visits arrived (startup): what was opened meanwhile counts on
/// top of them.
pub fn set_loaded(saved: Vec<Visit>) {
    let mut s = lock();
    let mut merged = Frecency::from_visits(saved);
    for v in s.frecency.visits() {
        merged.visit(&v.path, v.last);
    }
    s.frecency = merged;
    s.loaded = true;
}

/// `path` was opened.
pub fn visit(path: &Path) {
    lock().frecency.visit(path, now());
}

/// What to save with the session; `None` while loading.
pub fn saved() -> Option<Vec<Visit>> {
    let s = lock();
    s.loaded.then(|| s.frecency.visits().to_vec())
}

/// The most used folders right now (at most `n`).
pub fn top(n: usize) -> Vec<PathBuf> {
    let now = now();
    lock()
        .frecency
        .top(now, n)
        .into_iter()
        .map(|v| v.path.clone())
        .collect()
}
