//! Fuzzy name search over an [`Index`] (task T0.8): subsequence matching with an
//! fzf-style score, case- and accent-insensitive, on all cores, cancellable, with
//! partial results sent before the search ends.

use std::cmp::Ordering;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
use std::sync::mpsc::Sender;
use std::thread;

use fx_core::natural::fold;

use crate::Index;

/// How many results a search keeps (and sends as the first partial batch).
pub const MAX_RESULTS: usize = 200;
/// Slots per work unit; threads take units from a shared counter.
const UNIT: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub slot: u32,
    pub score: i32,
}

/// Best first; ties: shorter name first (a closer match), then lower slot.
fn rank(a: &(Hit, u32), b: &(Hit, u32)) -> Ordering {
    b.0.score
        .cmp(&a.0.score)
        .then(a.1.cmp(&b.1))
        .then(a.0.slot.cmp(&b.0.slot))
}

/// Keeps the best `MAX_RESULTS` hits (with their name length for tie-breaks).
#[derive(Debug, Default)]
struct Top {
    hits: Vec<(Hit, u32)>,
}

impl Top {
    fn push(&mut self, hit: Hit, len: u32) {
        self.hits.push((hit, len));
        if self.hits.len() >= MAX_RESULTS * 2 {
            self.trim();
        }
    }

    fn trim(&mut self) {
        self.hits.sort_unstable_by(rank);
        self.hits.truncate(MAX_RESULTS);
    }

    fn merge(&mut self, other: Top) {
        self.hits.extend(other.hits);
        self.trim();
    }

    fn sorted(mut self) -> Vec<Hit> {
        self.trim();
        self.hits.into_iter().map(|(h, _)| h).collect()
    }
}

/// Searches `index` for `query`. Returns the best [`MAX_RESULTS`] hits, best
/// first, or `None` if `cancel` was set (e.g. the next key arrived). If `partial`
/// is given, the first [`MAX_RESULTS`] matches found are sent on it as soon as
/// they exist, before the search finishes.
pub fn search(
    index: &Index,
    query: &str,
    cancel: &AtomicBool,
    partial: Option<&Sender<Vec<Hit>>>,
) -> Option<Vec<Hit>> {
    let q = fold(query);
    if q.is_empty() {
        return Some(Vec::new());
    }
    let pattern = Pattern::new(&q);
    let slots = index.slots();
    let next_unit = AtomicUsize::new(0);
    let merged = Mutex::new(Top::default());
    let partial_sent = AtomicBool::new(partial.is_none());
    let threads = thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(slots.div_ceil(UNIT).max(1));

    thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    if cancel.load(AtomicOrdering::Relaxed) {
                        return;
                    }
                    let unit = next_unit.fetch_add(1, AtomicOrdering::Relaxed);
                    let start = unit * UNIT;
                    if start >= slots {
                        return;
                    }
                    let mut top = Top::default();
                    for slot in start as u32..(start + UNIT).min(slots) as u32 {
                        if !index.is_live(slot) {
                            continue;
                        }
                        let name = index.folded(slot);
                        if let Some(score) = pattern.score(name) {
                            top.push(Hit { slot, score }, name.len() as u32);
                        }
                    }
                    let Ok(mut all) = merged.lock() else { return };
                    all.merge(top);
                    if !partial_sent.load(AtomicOrdering::Relaxed)
                        && all.hits.len() >= MAX_RESULTS
                        && let Some(tx) = partial
                    {
                        partial_sent.store(true, AtomicOrdering::Relaxed);
                        let _ = tx.send(all.hits.iter().map(|(h, _)| *h).collect());
                    }
                }
            });
        }
    });

    if cancel.load(AtomicOrdering::Relaxed) {
        return None;
    }
    merged.into_inner().ok().map(Top::sorted)
}

/// A folded query, matched on bytes when ASCII (the common case) and on chars
/// otherwise.
enum Pattern {
    Ascii(Vec<u8>),
    /// Chars, plus the query's UTF-8 bytes for a cheap pre-filter.
    Chars(Vec<char>, Vec<u8>),
}

impl Pattern {
    fn new(folded: &str) -> Self {
        if folded.is_ascii() {
            Pattern::Ascii(folded.as_bytes().to_vec())
        } else {
            Pattern::Chars(folded.chars().collect(), folded.as_bytes().to_vec())
        }
    }

    fn score(&self, name: &str) -> Option<i32> {
        match self {
            Pattern::Ascii(q) => score_seq(name.as_bytes(), q, |&b| is_boundary(b as char)),
            Pattern::Chars(q, bytes) => {
                // If the chars are a subsequence, so are their UTF-8 bytes: reject
                // most names without decoding them.
                if !is_byte_subsequence(name.as_bytes(), bytes) {
                    return None;
                }
                let name: Vec<char> = name.chars().collect();
                score_seq(&name, q, |&c| is_boundary(c))
            }
        }
    }
}

fn is_byte_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
    let mut it = needle.iter().peekable();
    for b in haystack {
        if it.peek() == Some(&b) {
            it.next();
        }
    }
    it.peek().is_none()
}

fn is_boundary(c: char) -> bool {
    matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '[' | ']' | ',')
}

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 12;
const BOUNDARY: i32 = 8;
const GAP: i32 = 3;

/// fzf-v1-style score: find the leftmost end of a subsequence match, walk back to
/// the latest start (shortest window), then score that window. `None` if `q` is
/// not a subsequence of `name`.
fn score_seq<T: PartialEq>(
    name: &[T],
    q: &[T],
    boundary_before: impl Fn(&T) -> bool,
) -> Option<i32> {
    // Forward: leftmost position where the whole query has been matched.
    let mut qi = 0;
    let mut end = None;
    for (i, c) in name.iter().enumerate() {
        if *c == q[qi] {
            qi += 1;
            if qi == q.len() {
                end = Some(i);
                break;
            }
        }
    }
    let end = end?;
    // Backward: latest start that still matches the whole query.
    let mut qi = q.len();
    let mut start = end;
    for i in (0..=end).rev() {
        if name[i] == q[qi - 1] {
            qi -= 1;
            if qi == 0 {
                start = i;
                break;
            }
        }
    }
    // Score the window [start, end].
    let mut score = 0;
    let mut qi = 0;
    let mut prev: Option<usize> = None;
    for i in start..=end {
        if qi < q.len() && name[i] == q[qi] {
            score += MATCH;
            if prev == Some(i.wrapping_sub(1)) {
                score += CONSECUTIVE;
            }
            if i == 0 || boundary_before(&name[i - 1]) {
                score += BOUNDARY;
            }
            if let Some(p) = prev {
                score -= GAP * (i - p - 1) as i32;
            }
            prev = Some(i);
            qi += 1;
        }
    }
    // Shorter names rank higher for the same match.
    Some(score - (name.len() as i32) / 8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileId, Record};
    use std::sync::mpsc::channel;

    fn index_of(names: &[&str]) -> Index {
        let mut index = Index::new(5);
        for (i, n) in names.iter().enumerate() {
            index
                .insert(&Record {
                    id: 24 + i as FileId,
                    parent: 5,
                    name: n.encode_utf16().collect(),
                    is_dir: false,
                    size: 0,
                    modified: 0,
                })
                .unwrap();
        }
        index
    }

    fn names(index: &Index, hits: &[Hit]) -> Vec<String> {
        hits.iter().map(|h| index.name(h.slot).to_owned()).collect()
    }

    fn run(index: &Index, q: &str) -> Vec<String> {
        names(
            index,
            &search(index, q, &AtomicBool::new(false), None).unwrap(),
        )
    }

    #[test]
    fn subsequence_case_and_accent_insensitive() {
        let index = index_of(&["Relatório Final.pdf", "readme.md", "notes.txt", "RLTR.bin"]);
        let got = run(&index, "rltr");
        assert!(got.contains(&"Relatório Final.pdf".to_owned()));
        assert!(got.contains(&"RLTR.bin".to_owned()));
        assert!(!got.contains(&"notes.txt".to_owned()));
        assert_eq!(run(&index, "RELATORIO"), ["Relatório Final.pdf"]);
        assert_eq!(run(&index, "relatório"), ["Relatório Final.pdf"]);
        assert!(run(&index, "zzz").is_empty());
        assert!(run(&index, "").is_empty());
    }

    #[test]
    fn better_matches_rank_first() {
        let index = index_of(&[
            "a_b_c_kernel_extra_long_name.dll",
            "my-kernel32-copy.dll",
            "kernel32.dll",
            "k_e_r_n_e_l.txt",
        ]);
        let got = run(&index, "kernel");
        assert_eq!(got[0], "kernel32.dll", "{got:?}");
        assert_eq!(got.last().unwrap(), "k_e_r_n_e_l.txt", "{got:?}");
        // Word-start matches beat scattered ones.
        let index = index_of(&["xsetupx.exe", "setup.exe"]);
        assert_eq!(run(&index, "setup")[0], "setup.exe");
    }

    #[test]
    fn non_ascii_queries_match_by_char() {
        let index = index_of(&["日本語 メモ.txt", "😀 feliz.png", "plain.txt"]);
        assert_eq!(run(&index, "日語"), ["日本語 メモ.txt"]);
        assert_eq!(run(&index, "😀f"), ["😀 feliz.png"]);
    }

    #[test]
    fn byte_prefilter_is_a_necessary_condition() {
        assert!(is_byte_subsequence("😀 feliz".as_bytes(), "😀f".as_bytes()));
        assert!(!is_byte_subsequence(b"abc", b"abd"));
        assert!(is_byte_subsequence(b"abc", b""));
    }

    #[test]
    fn deleted_entries_are_not_found() {
        let mut index = index_of(&["keep.txt", "gone.txt"]);
        index.remove(25);
        assert_eq!(run(&index, "txt"), ["keep.txt"]);
    }

    #[test]
    fn keeps_best_200_and_sends_partial_results_first() {
        let names: Vec<String> = (0..20_000).map(|i| format!("file-{i:05}.txt")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let index = index_of(&refs);
        let (tx, rx) = channel();
        let hits = search(&index, "file", &AtomicBool::new(false), Some(&tx)).unwrap();
        assert_eq!(hits.len(), MAX_RESULTS);
        assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
        let first = rx.try_recv().expect("partial results were not sent");
        assert!(first.len() >= MAX_RESULTS);
        assert!(rx.try_recv().is_err(), "partial results are sent once");
    }

    #[test]
    fn cancelled_search_returns_none() {
        let index = index_of(&["a.txt"; 100]);
        assert_eq!(search(&index, "a", &AtomicBool::new(true), None), None);
    }
}
