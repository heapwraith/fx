//! A folder tree as a list (T6.11): every item below `root`, with its path
//! relative to it, size and modification time. Links are not followed; at
//! most `MAX_ITEMS` are listed (`truncated` says so).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use fx_core::compare::TreeItem;

pub const MAX_ITEMS: usize = 200_000;

/// The items under `root` and whether the list stopped at `MAX_ITEMS`.
pub fn tree_items(root: &Path, cancel: &AtomicBool) -> (Vec<TreeItem>, bool) {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            if out.len() >= MAX_ITEMS {
                return (out, true);
            }
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            let path = e.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let meta = e.metadata().ok();
            out.push(TreeItem {
                rel: rel.to_path_buf(),
                is_dir: kind.is_dir(),
                size: meta
                    .as_ref()
                    .map_or(0, |m| if kind.is_dir() { 0 } else { m.len() }),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64),
            });
            if kind.is_dir() {
                stack.push(path);
            }
        }
    }
    (out, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_everything_below_with_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("a.txt"), b"abc").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"x").unwrap();
        let (mut items, cut) = tree_items(dir.path(), &AtomicBool::new(false));
        items.sort_by(|a, b| a.rel.cmp(&b.rel));
        assert!(!cut);
        let rels: Vec<String> = items.iter().map(|i| i.rel.display().to_string()).collect();
        assert_eq!(rels, ["b.txt", "sub", r"sub\a.txt"]);
        assert_eq!(items[2].size, 3);
        assert!(items[1].is_dir);
    }
}
