//! Saved searches (T6.3, D-068): a search kept with a name (its query), in
//! the sidebar and the palette; opened, it fills the search panel and, on the
//! whole disk, refreshes by itself. Kept in memory, read from `state.db` at
//! startup off the UI thread, saved with the session.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::store::SearchState;

struct State {
    list: Vec<SearchState>,
    loaded: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    list: Vec::new(),
    loaded: false,
});

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The saved ones arrived (startup); ones saved meanwhile stay first.
pub fn set_loaded(saved: Vec<SearchState>) {
    let mut s = lock();
    for q in saved {
        if !s.list.iter().any(|x| x.query == q.query) {
            s.list.push(q);
        }
    }
    s.loaded = true;
}

pub fn list() -> Vec<SearchState> {
    lock().list.clone()
}

/// Saves `q` (the same query replaces the old one). Returns whether it was
/// already saved.
pub fn save(q: SearchState) -> bool {
    let mut s = lock();
    match s.list.iter_mut().find(|x| x.query == q.query) {
        Some(old) => {
            *old = q;
            true
        }
        None => {
            s.list.push(q);
            false
        }
    }
}

pub fn remove(query: &str) {
    lock().list.retain(|q| q.query != query);
}

/// What to save with the session; `None` while loading.
pub fn saved() -> Option<Vec<SearchState>> {
    let s = lock();
    s.loaded.then(|| s.list.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn save_replace_remove_and_load() {
        let q = |text: &str| SearchState {
            query: text.into(),
            disk: true,
            root: PathBuf::from(r"C:\"),
        };
        assert_eq!(saved(), None);
        assert!(!save(q("tipo:pdf")));
        assert!(save(q("tipo:pdf")));
        set_loaded(vec![q("tipo:pdf"), q("relatório")]);
        assert_eq!(list().len(), 2);
        remove("relatório");
        assert_eq!(saved().map(|v| v.len()), Some(1));
    }
}
