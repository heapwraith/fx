//! Git status of the items of a folder (T6.12, D-068), from
//! `git status --porcelain=v2 -z` run at the repository's root: each name in
//! the folder gets the "most urgent" state of itself or of anything below it
//! (a folder with a changed file inside shows as changed).

use std::collections::HashMap;
use std::path::Path;

/// What git says about an item, most urgent last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitState {
    Untracked,
    Added,
    Modified,
    Renamed,
    Deleted,
    Conflicted,
}

impl GitState {
    /// One letter for the list (shape, not color, carries the meaning).
    pub fn letter(self) -> &'static str {
        match self {
            GitState::Untracked => "?",
            GitState::Added => "A",
            GitState::Modified => "M",
            GitState::Renamed => "R",
            GitState::Deleted => "D",
            GitState::Conflicted => "!",
        }
    }
}

/// The states of the items of `folder` (names, as in the folder), from the
/// porcelain v2 output of the repository at `repo_root`.
pub fn states_in(output: &[u8], repo_root: &Path, folder: &Path) -> HashMap<String, GitState> {
    let mut out: HashMap<String, GitState> = HashMap::new();
    let Ok(rel) = folder.strip_prefix(repo_root) else {
        return out;
    };
    let prefix: String = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let mut fields = output.split(|&b| b == 0).peekable();
    while let Some(record) = fields.next() {
        let Ok(line) = std::str::from_utf8(record) else {
            continue;
        };
        let (state, path) = match line.as_bytes().first() {
            Some(b'1') => match line.splitn(9, ' ').collect::<Vec<_>>().as_slice() {
                [_, xy, .., path] if line.splitn(9, ' ').count() == 9 => (from_xy(xy), *path),
                _ => continue,
            },
            Some(b'2') => {
                // Renamed: the original path follows as its own field.
                fields.next();
                match line.splitn(10, ' ').collect::<Vec<_>>().as_slice() {
                    [_, _xy, .., path] if line.splitn(10, ' ').count() == 10 => {
                        (Some(GitState::Renamed), *path)
                    }
                    _ => continue,
                }
            }
            Some(b'u') => match line.splitn(11, ' ').last() {
                Some(path) => (Some(GitState::Conflicted), path),
                None => continue,
            },
            Some(b'?') => (Some(GitState::Untracked), line.get(2..).unwrap_or("")),
            _ => continue,
        };
        let Some(state) = state else { continue };
        // The part of the path inside `folder`; its first segment is the item.
        let inside = if prefix.is_empty() {
            Some(path)
        } else {
            path.strip_prefix(prefix.as_str())
                .and_then(|r| r.strip_prefix('/'))
        };
        let Some(inside) = inside.filter(|r| !r.is_empty()) else {
            continue;
        };
        let name = inside.split('/').next().unwrap_or(inside).to_owned();
        let entry = out.entry(name).or_insert(state);
        if state > *entry {
            *entry = state;
        }
    }
    out
}

/// The state of an ordinary changed entry from its two-letter code.
fn from_xy(xy: &str) -> Option<GitState> {
    let b = xy.as_bytes();
    let (x, y) = (*b.first()?, *b.get(1)?);
    Some(if x == b'D' || y == b'D' {
        GitState::Deleted
    } else if x == b'A' {
        GitState::Added
    } else if x == b'R' || y == b'R' {
        GitState::Renamed
    } else {
        GitState::Modified
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(records: &[&str]) -> Vec<u8> {
        let mut v = Vec::new();
        for r in records {
            v.extend_from_slice(r.as_bytes());
            v.push(0);
        }
        v
    }

    #[test]
    fn items_take_the_most_urgent_state_below_them() {
        let o = out(&[
            "1 .M N... 100644 100644 100644 abc abc src/main.rs",
            "1 A. N... 000000 100644 100644 000 abc src/new.rs",
            "? notes.txt",
            "? src/scratch/x.txt",
            "u UU N... 100644 100644 100644 100644 a b c docs/a.md",
            "2 R. N... 100644 100644 100644 abc abc R100 lib/b.rs",
            "lib/a.rs",
        ]);
        let root = Path::new(r"C:\repo");
        let top = states_in(&o, root, root);
        assert_eq!(top["src"], GitState::Added.max(GitState::Modified));
        assert_eq!(top["notes.txt"], GitState::Untracked);
        assert_eq!(top["docs"], GitState::Conflicted);
        assert_eq!(top["lib"], GitState::Renamed);
        assert!(!top.contains_key("lib/a.rs"));
        let src = states_in(&o, root, &root.join("src"));
        assert_eq!(src["main.rs"], GitState::Modified);
        assert_eq!(src["new.rs"], GitState::Added);
        assert_eq!(src["scratch"], GitState::Untracked);
        assert!(states_in(&o, root, Path::new(r"D:\other")).is_empty());
        assert_eq!(GitState::Modified.letter(), "M");
    }

    #[test]
    fn garbage_is_ignored() {
        let root = Path::new(r"C:\repo");
        assert!(states_in(b"nonsense\0\xff\xfe\0\x31 short\0", root, root).is_empty());
    }
}
