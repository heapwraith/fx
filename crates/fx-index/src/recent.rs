//! The timeline of the disks (T6.2, D-068): the latest changes the USN
//! journal reported, kept by the service in a bounded ring (at most
//! `MAX_CHANGES`; places that change all the time and say nothing to the
//! user, like the system and temporary folders, are left out).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use crate::protocol::ChangeKind;

/// Most changes kept (a few MB at most).
pub const MAX_CHANGES: usize = 50_000;
/// A new change of an item seen among the last ones replaces it.
const RECENT_WINDOW: usize = 64;

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub kind: ChangeKind,
    pub is_dir: bool,
    /// Unix seconds.
    pub at: i64,
}

#[derive(Debug, Default)]
pub struct Recent {
    changes: VecDeque<Change>,
}

impl Recent {
    pub fn push(&mut self, mut change: Change) {
        if is_noise(&change.path) {
            return;
        }
        let len = self.changes.len();
        if let Some(back) = self
            .changes
            .iter()
            .rev()
            .take(RECENT_WINDOW)
            .position(|c| c.path == change.path)
        {
            let old = self.changes.remove(len - 1 - back);
            // Created and then written: still "created".
            if old.is_some_and(|o| o.kind == ChangeKind::Created)
                && change.kind == ChangeKind::Modified
            {
                change.kind = ChangeKind::Created;
            }
        }
        self.changes.push_back(change);
        if self.changes.len() > MAX_CHANGES {
            self.changes.pop_front();
        }
    }

    /// The changes since `since` (Unix seconds) that `keep` accepts, newest
    /// first, at most `limit`.
    pub fn since(&self, since: i64, limit: usize, keep: impl Fn(&Change) -> bool) -> Vec<&Change> {
        self.changes
            .iter()
            .rev()
            .take_while(|c| c.at >= since)
            .filter(|c| keep(c))
            .take(limit)
            .collect()
    }
}

/// System and temporary places that change all the time.
pub fn is_noise(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    let lower = text.to_ascii_lowercase();
    const MARKERS: [&str; 10] = [
        r"\windows\",
        r"\$recycle.bin\",
        r"\system volume information\",
        r"\programdata\",
        r"\$extend\",
        r"\appdata\local\temp\",
        r"\appdata\local\microsoft\",
        r"\appdata\local\packages\",
        r"\appdata\local\google\",
        r"\appdata\local\mozilla\",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
        || lower.ends_with(".tmp")
        || lower.ends_with("pagefile.sys")
        || lower.ends_with("swapfile.sys")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(path: &str, kind: ChangeKind, at: i64) -> Change {
        Change {
            path: PathBuf::from(path),
            kind,
            is_dir: false,
            at,
        }
    }

    #[test]
    fn newest_first_within_the_window_and_noise_left_out() {
        let mut r = Recent::default();
        r.push(change(
            r"C:\Users\ana\Downloads\a.zip",
            ChangeKind::Created,
            100,
        ));
        r.push(change(
            r"C:\Windows\System32\x.dll",
            ChangeKind::Modified,
            101,
        ));
        r.push(change(
            r"C:\Users\ana\AppData\Local\Temp\y",
            ChangeKind::Created,
            102,
        ));
        r.push(change(r"C:\Users\ana\notas.txt", ChangeKind::Modified, 200));
        let got: Vec<&str> = r
            .since(150, 10, |_| true)
            .iter()
            .map(|c| c.path.to_str().unwrap())
            .collect();
        assert_eq!(got, [r"C:\Users\ana\notas.txt"]);
        assert_eq!(r.since(0, 10, |_| true).len(), 2);
        assert_eq!(r.since(0, 1, |_| true).len(), 1, "limit");
        assert!(
            r.since(0, 10, |c| !c.path.starts_with(r"C:\Users\ana"))
                .is_empty()
        );
    }

    #[test]
    fn a_new_change_replaces_the_old_and_created_stays_created() {
        let mut r = Recent::default();
        r.push(change(r"D:\a.txt", ChangeKind::Created, 1));
        r.push(change(r"D:\a.txt", ChangeKind::Modified, 2));
        let all = r.since(0, 10, |_| true);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].kind, ChangeKind::Created);
        assert_eq!(all[0].at, 2);
        r.push(change(r"D:\a.txt", ChangeKind::Deleted, 3));
        assert_eq!(r.since(0, 10, |_| true)[0].kind, ChangeKind::Deleted);
    }

    #[test]
    fn bounded() {
        let mut r = Recent::default();
        for k in 0..(MAX_CHANGES + 10) {
            r.push(change(&format!(r"D:\f{k}"), ChangeKind::Created, k as i64));
        }
        assert_eq!(r.since(0, usize::MAX, |_| true).len(), MAX_CHANGES);
    }
}
