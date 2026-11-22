//! Workspaces (T6.13, D-068): named sets of tabs ("Trabalho", "Fotos"),
//! saved from the palette and opened in a window of their own. Kept in
//! memory, read from `state.db` at startup off the UI thread and saved with
//! the session. With `FX_NO_STATE=1`, memory only.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::store::WorkspaceState;

struct State {
    list: Vec<WorkspaceState>,
    /// The saved ones were read: from then on, saving replaces them.
    loaded: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    list: Vec::new(),
    loaded: false,
});

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The saved workspaces arrived (startup); ones made meanwhile win.
pub fn set_loaded(saved: Vec<WorkspaceState>) {
    let mut s = lock();
    for ws in saved {
        if !s.list.iter().any(|w| w.name.eq_ignore_ascii_case(&ws.name)) {
            s.list.push(ws);
        }
    }
    s.list.sort_by_key(|w| w.name.to_lowercase());
    s.loaded = true;
}

/// Every workspace, by name.
pub fn list() -> Vec<WorkspaceState> {
    lock().list.clone()
}

/// Saves `ws`, replacing one with the same name (case does not matter).
/// Returns whether it replaced one.
pub fn save(ws: WorkspaceState) -> bool {
    let mut s = lock();
    let replaced = match s
        .list
        .iter_mut()
        .find(|w| w.name.eq_ignore_ascii_case(&ws.name))
    {
        Some(old) => {
            *old = ws;
            true
        }
        None => {
            s.list.push(ws);
            false
        }
    };
    s.list.sort_by_key(|w| w.name.to_lowercase());
    replaced
}

/// Removes the workspace called `name`.
pub fn remove(name: &str) {
    lock().list.retain(|w| w.name != name);
}

/// What to save with the session; `None` while loading.
pub fn saved() -> Option<Vec<WorkspaceState>> {
    let s = lock();
    s.loaded.then(|| s.list.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws(name: &str, tabs: usize) -> WorkspaceState {
        WorkspaceState {
            name: name.into(),
            tabs: (0..tabs)
                .map(|k| PathBuf::from(format!(r"C:\p{k}")))
                .collect(),
            active: 0,
        }
    }

    // The process-wide list: one test walks the whole life of it.
    #[test]
    fn save_update_remove_and_load() {
        assert_eq!(saved(), None, "nothing to save before loading");
        assert!(!save(ws("Trabalho", 2)), "a new one");
        assert!(
            save(ws("trabalho", 3)),
            "the same name in another case updates it"
        );
        assert_eq!(list().len(), 1);
        assert_eq!(list()[0].tabs.len(), 3);
        set_loaded(vec![ws("TRABALHO", 9), ws("Fotos", 1)]);
        let names: Vec<String> = list().into_iter().map(|w| w.name).collect();
        assert_eq!(
            names,
            ["Fotos", "trabalho"],
            "the one made meanwhile wins; by name"
        );
        assert_eq!(saved().map(|v| v.len()), Some(2));
        remove("Fotos");
        assert_eq!(list().len(), 1);
    }
}
