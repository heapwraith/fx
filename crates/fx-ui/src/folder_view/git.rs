//! Git status in the list (T6.12, D-068): in a folder inside a repository,
//! each item shows a letter (M changed, A added, ? new, R renamed, D deleted,
//! ! conflict) from `git status --porcelain=v2 -z`, run on a thread of its
//! own without a console window, with a time limit. Read after the folder
//! loads and again when it changes (never two at a time). Without git, or
//! outside a repository, nothing shows.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use fx_core::git_status::{GitState, states_in};
use gpui::Context;

use super::FolderView;

#[derive(Default)]
pub(super) struct Git {
    pub(super) states: HashMap<String, GitState>,
    running: bool,
    again: bool,
    ticket: u64,
}

impl FolderView {
    /// Reads the git status of this folder's items (again).
    pub(super) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        if self.git.running {
            self.git.again = true;
            return;
        }
        self.git.running = true;
        self.git.ticket += 1;
        let (ticket, folder) = (self.git.ticket, self.path.clone());
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-git".into())
            .spawn(move || {
                let _ = tx.send(read_states(folder));
            });
        cx.spawn(async move |this, cx| {
            let states = rx.await.unwrap_or_default();
            let _ = this.update(cx, |view, cx| {
                view.git.running = false;
                if view.git.ticket == ticket {
                    view.git.states = states;
                    cx.notify();
                }
                if std::mem::take(&mut view.git.again) {
                    view.refresh_git(cx);
                }
            });
        })
        .detach();
    }

    /// Leaving the folder: nothing from the old one stays.
    pub(super) fn forget_git(&mut self) {
        self.git.states.clear();
        self.git.ticket += 1;
    }
}

/// The states, or nothing (no repository, no git, too slow).
fn read_states(folder: PathBuf) -> HashMap<String, GitState> {
    let Some(root) = folder.ancestors().find(|a| a.join(".git").exists()) else {
        return HashMap::new();
    };
    let args = ["status", "--porcelain=v2", "-z", "--untracked-files=normal"];
    match fx_shell::run_hidden("git", &args, root, Duration::from_secs(3)) {
        Ok(out) => states_in(&out, root, &folder),
        Err(e) => {
            log::info!("git status unavailable: {e}");
            HashMap::new()
        }
    }
}

/// The state in words, for screen readers.
pub(super) fn words(state: GitState) -> &'static str {
    match state {
        GitState::Untracked => fx_core::tr!("git: novo", "git: new"),
        GitState::Added => fx_core::tr!("git: adicionado", "git: added"),
        GitState::Modified => fx_core::tr!("git: alterado", "git: changed"),
        GitState::Renamed => fx_core::tr!("git: renomeado", "git: renamed"),
        GitState::Deleted => fx_core::tr!("git: apagado", "git: deleted"),
        GitState::Conflicted => fx_core::tr!("git: em conflito", "git: in conflict"),
    }
}
