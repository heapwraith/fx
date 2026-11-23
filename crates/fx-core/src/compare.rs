//! Comparing two folders (T6.11, D-068): what exists on one side only, and
//! which files differ (size, or a modification time more than 2 s apart:
//! FAT keeps 2 s). A folder on one side only is one difference, not one per
//! file inside it. Equal items are not listed.

use std::collections::HashMap;
use std::path::PathBuf;

/// One item of a tree, by its path relative to the tree's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItem {
    pub rel: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds.
    pub modified: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Diff {
    OnlyLeft,
    OnlyRight,
    LeftNewer,
    RightNewer,
    /// Same time, different size.
    SizeDiffers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub rel: PathBuf,
    pub is_dir: bool,
    pub diff: Diff,
}

/// The differences between two trees, by path.
pub fn compare(left: &[TreeItem], right: &[TreeItem]) -> Vec<Difference> {
    let l: HashMap<&PathBuf, &TreeItem> = left.iter().map(|i| (&i.rel, i)).collect();
    let r: HashMap<&PathBuf, &TreeItem> = right.iter().map(|i| (&i.rel, i)).collect();
    let mut out = Vec::new();
    // One side only: skip what is inside a folder already reported.
    let mut one_side = |items: &[TreeItem], other: &HashMap<&PathBuf, &TreeItem>, diff: Diff| {
        let mut reported: Vec<&PathBuf> = Vec::new();
        let mut sorted: Vec<&TreeItem> = items.iter().collect();
        sorted.sort_by(|a, b| a.rel.cmp(&b.rel));
        for item in sorted {
            if other.contains_key(&item.rel) || reported.iter().any(|d| item.rel.starts_with(d)) {
                continue;
            }
            if item.is_dir {
                reported.push(&item.rel);
            }
            out.push(Difference {
                rel: item.rel.clone(),
                is_dir: item.is_dir,
                diff,
            });
        }
    };
    one_side(left, &r, Diff::OnlyLeft);
    one_side(right, &l, Diff::OnlyRight);
    for item in left {
        let Some(other) = r.get(&item.rel) else {
            continue;
        };
        if item.is_dir || other.is_dir {
            continue;
        }
        let diff = if item.modified > other.modified + 2 {
            Some(Diff::LeftNewer)
        } else if other.modified > item.modified + 2 {
            Some(Diff::RightNewer)
        } else if item.size != other.size {
            Some(Diff::SizeDiffers)
        } else {
            None
        };
        if let Some(diff) = diff {
            out.push(Difference {
                rel: item.rel.clone(),
                is_dir: false,
                diff,
            });
        }
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, size: u64, modified: i64) -> TreeItem {
        TreeItem {
            rel: PathBuf::from(rel),
            is_dir: false,
            size,
            modified,
        }
    }

    fn dir(rel: &str) -> TreeItem {
        TreeItem {
            rel: PathBuf::from(rel),
            is_dir: true,
            size: 0,
            modified: 0,
        }
    }

    #[test]
    fn differences_by_side_time_and_size() {
        let left = vec![
            file("a.txt", 10, 100),
            file("b.txt", 10, 200),
            file("c.txt", 10, 100),
            file("d.txt", 10, 100),
            dir("so-esquerda"),
            file(r"so-esquerda\x.txt", 1, 1),
            file("igual.txt", 5, 50),
        ];
        let right = vec![
            file("a.txt", 10, 101),
            file("b.txt", 10, 100),
            file("c.txt", 10, 300),
            file("d.txt", 12, 100),
            file("so-direita.txt", 1, 1),
            file("igual.txt", 5, 51),
        ];
        let got: Vec<(String, Diff)> = compare(&left, &right)
            .into_iter()
            .map(|d| (d.rel.display().to_string(), d.diff))
            .collect();
        assert_eq!(
            got,
            vec![
                ("b.txt".into(), Diff::LeftNewer),
                ("c.txt".into(), Diff::RightNewer),
                ("d.txt".into(), Diff::SizeDiffers),
                ("so-direita.txt".into(), Diff::OnlyRight),
                ("so-esquerda".into(), Diff::OnlyLeft),
            ]
        );
    }
}
