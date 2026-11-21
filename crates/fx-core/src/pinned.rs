//! Quick access (task T1.5): folders pinned to the sidebar, in the user's
//! order. Kept in memory until settings persistence (task T1.17).
//!
//! Paths are compared case-insensitively and ignoring a trailing separator,
//! like Windows does, so `C:\Fotos` and `c:\fotos\` are the same pin.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Pinned {
    paths: Vec<PathBuf>,
}

/// Comparison key of a path: lowercase, without trailing separators (a root
/// such as `C:\` keeps its separator).
fn key(p: &Path) -> String {
    let s = p.to_string_lossy().to_lowercase();
    let trimmed = s.trim_end_matches(['\\', '/']);
    if trimmed.ends_with(':') || trimmed.is_empty() {
        s
    } else {
        trimmed.to_owned()
    }
}

impl Pinned {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn contains(&self, p: &Path) -> bool {
        let k = key(p);
        self.paths.iter().any(|q| key(q) == k)
    }

    /// Pins `p` at the end; returns false if it was already pinned.
    pub fn pin(&mut self, p: PathBuf) -> bool {
        if self.contains(&p) {
            return false;
        }
        self.paths.push(p);
        true
    }

    /// Returns false if `p` was not pinned.
    pub fn unpin(&mut self, p: &Path) -> bool {
        let k = key(p);
        let before = self.paths.len();
        self.paths.retain(|q| key(q) != k);
        self.paths.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_keeps_order_and_rejects_duplicates() {
        let mut p = Pinned::new();
        assert!(p.pin(PathBuf::from(r"C:\Fotos")));
        assert!(p.pin(PathBuf::from(r"D:\Projetos")));
        assert!(
            !p.pin(PathBuf::from(r"c:\fotos\")),
            "same folder, other spelling"
        );
        assert_eq!(
            p.paths(),
            [PathBuf::from(r"C:\Fotos"), PathBuf::from(r"D:\Projetos")]
        );
        assert!(p.contains(Path::new(r"D:\PROJETOS")));
    }

    #[test]
    fn unpin_removes_by_folder() {
        let mut p = Pinned::new();
        p.pin(PathBuf::from(r"C:\"));
        p.pin(PathBuf::from(r"C:\Dados"));
        assert!(p.unpin(Path::new(r"c:\dados\")));
        assert!(!p.unpin(Path::new(r"C:\Dados")));
        assert!(p.contains(Path::new("c:\\")), "a root keeps its separator");
        assert!(
            !p.contains(Path::new("c:")),
            "C: (current dir of C) is not C:\\"
        );
        assert_eq!(p.len(), 1);
    }
}
