//! Searching a folder tree by name (task T1.23, the non-elevated path of
//! Ctrl+F): a breadth-first walk on the caller's (background) thread with
//! the same enumeration as folder reads, case- and accent-insensitive
//! "contains" matching, results in batches as they are found, and a cancel
//! flag checked between folders and entries (the next key cancels).
//!
//! Links and junctions are listed but not entered (no cycles, no walking
//! into other volumes); folders that cannot be read are skipped and counted.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::{Instant, SystemTime};

use fx_core::natural::fold;

use crate::{Attributes, BATCH_MAX_DELAY, Strategy};

/// One match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: SystemTime,
}

/// How a walk ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchSummary {
    pub folders: usize,
    pub hits: usize,
    /// Folders that could not be read (no permission, gone meanwhile).
    pub skipped: usize,
    pub cancelled: bool,
    /// Stopped at `limit` hits.
    pub capped: bool,
}

/// Walks `root` and its subfolders for names containing `query` (folded:
/// case and accents ignored). Hits go to `tx` in batches (at most every
/// [`BATCH_MAX_DELAY`]); at most `limit` hits. Returns early when `cancel`
/// is set or the receiver is gone.
pub fn search_tree(
    root: &Path,
    query: &str,
    limit: usize,
    cancel: &AtomicBool,
    tx: &Sender<Vec<SearchHit>>,
) -> SearchSummary {
    let mut summary = SearchSummary::default();
    let needle = fold(query);
    if needle.is_empty() {
        return summary;
    }
    let mut pending: Vec<SearchHit> = Vec::new();
    let mut last_flush = Instant::now();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            summary.cancelled = true;
            break;
        }
        let (etx, erx) = channel();
        if crate::enumerate(Strategy::FindFirstFileEx, &dir, &etx).is_err() {
            summary.skipped += 1;
            continue;
        }
        drop(etx);
        summary.folders += 1;
        for entry in erx.into_iter().flatten() {
            let is_dir = entry.attributes.is_dir();
            let path = dir.join(&entry.name);
            if is_dir && !entry.attributes.contains(Attributes::LINK) {
                queue.push_back(path.clone());
            }
            if fold(&entry.name.to_string_lossy()).contains(&needle) {
                pending.push(SearchHit {
                    path,
                    is_dir,
                    size: entry.size,
                    modified: entry.modified,
                });
                summary.hits += 1;
                if summary.hits >= limit {
                    summary.capped = true;
                    break;
                }
            }
        }
        if summary.capped {
            break;
        }
        if !pending.is_empty() && last_flush.elapsed() >= BATCH_MAX_DELAY {
            last_flush = Instant::now();
            if tx.send(std::mem::take(&mut pending)).is_err() {
                summary.cancelled = true;
                return summary;
            }
        }
    }
    if !pending.is_empty() && tx.send(pending).is_err() {
        summary.cancelled = true;
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Fotos/2026/Praia")).unwrap();
        fs::create_dir_all(root.join("Documentos")).unwrap();
        fs::write(root.join("Fotos/2026/Praia/praia-sol.jpg"), b"x").unwrap();
        fs::write(root.join("Fotos/2026/RELATÓRIO de viagem.txt"), b"x").unwrap();
        fs::write(root.join("Documentos/relatorio-final.docx"), b"xy").unwrap();
        fs::write(root.join("outro.txt"), b"").unwrap();
        dir
    }

    fn run(root: &Path, query: &str, limit: usize) -> (Vec<String>, SearchSummary) {
        let (tx, rx) = channel();
        let cancel = AtomicBool::new(false);
        let summary = search_tree(root, query, limit, &cancel, &tx);
        drop(tx);
        let mut names: Vec<String> = rx
            .into_iter()
            .flatten()
            .map(|h| h.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        (names, summary)
    }

    #[test]
    fn finds_in_subfolders_ignoring_case_and_accents() {
        let dir = tree();
        let (names, summary) = run(dir.path(), "relatorio", 100);
        assert_eq!(names, ["RELATÓRIO de viagem.txt", "relatorio-final.docx"]);
        assert_eq!(summary.hits, 2);
        assert_eq!(summary.folders, 5, "root, Fotos, 2026, Praia, Documentos");
        assert!(!summary.cancelled && !summary.capped);
        // Folders match too.
        assert_eq!(run(dir.path(), "praia", 100).0, ["Praia", "praia-sol.jpg"]);
        assert!(run(dir.path(), "", 100).0.is_empty());
    }

    #[test]
    fn stops_at_the_limit_and_on_cancel() {
        let dir = tree();
        let (names, summary) = run(dir.path(), ".", 2);
        assert_eq!(names.len(), 2);
        assert!(summary.capped);

        let (tx, rx) = channel();
        let cancel = AtomicBool::new(true);
        let s = search_tree(dir.path(), "a", 100, &cancel, &tx);
        assert!(s.cancelled);
        drop(tx);
        assert_eq!(rx.into_iter().count(), 0);
    }

    /// The benchmark fixtures (`fx-bench fixtures`): the walk finds exactly
    /// what an independent `std::fs` walk finds.
    #[test]
    fn fixtures_match_an_independent_walk() {
        let root = std::env::temp_dir()
            .join("fx-fixtures")
            .join("unicode-long");
        if !root.is_dir() {
            eprintln!("skipped: run `fx-bench fixtures` first");
            return;
        }
        fn walk(dir: &Path, needle: &str, out: &mut Vec<PathBuf>) {
            for e in fs::read_dir(dir).unwrap().flatten() {
                let t = e.file_type().unwrap();
                if fold(&e.file_name().to_string_lossy()).contains(needle) {
                    out.push(e.path());
                }
                if t.is_dir() && !t.is_symlink() {
                    walk(&e.path(), needle, out);
                }
            }
        }
        for query in ["relatório", "AÇÃO", ".md"] {
            let mut expected = Vec::new();
            walk(&root, &fold(query), &mut expected);
            let (tx, rx) = channel();
            let s = search_tree(&root, query, usize::MAX, &AtomicBool::new(false), &tx);
            drop(tx);
            let mut got: Vec<PathBuf> = rx.into_iter().flatten().map(|h| h.path).collect();
            got.sort();
            expected.sort();
            assert!(!expected.is_empty(), "{query}");
            assert_eq!(got, expected, "{query}");
            assert_eq!(s.skipped, 0);
        }
    }

    /// Timing on all fixtures (informative): `--ignored --nocapture`, release.
    #[test]
    #[ignore]
    fn timing_on_all_fixtures() {
        let root = std::env::temp_dir().join("fx-fixtures");
        for query in ["relat", "000123", "zzz-nada"] {
            let (tx, rx) = channel();
            let started = Instant::now();
            let s = search_tree(&root, query, usize::MAX, &AtomicBool::new(false), &tx);
            let took = started.elapsed();
            drop(tx);
            let first = rx.into_iter().count();
            println!(
                "{query:>10}: {took:?}, {} hits, {} folders, {first} batches",
                s.hits, s.folders
            );
        }
    }

    #[test]
    fn unreadable_root_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let (names, summary) = run(&dir.path().join("sumiu"), "a", 10);
        assert!(names.is_empty());
        assert_eq!(summary.skipped, 1);
    }
}
