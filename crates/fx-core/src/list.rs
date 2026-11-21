//! Folder listing stored by columns (docs/ARCHITECTURE.md, "Desenho da lista"):
//! one name buffer with offsets, separate size/date/attribute vectors, and an
//! index vector that sorting and filtering rearrange. Batches are appended; the
//! columns are never rebuilt.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsString;
use std::time::SystemTime;

use crate::natural::{fold_into, natural_cmp};
use crate::{Attributes, Entry};

/// Separator between folded keys in the filter buffer. Folded keys never contain
/// it (Windows forbids NUL in names) and a needle containing it matches nothing.
const SEP: char = '\0';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
    /// Extension (case/accent-insensitive), then name.
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Ascending,
    Descending,
}

#[derive(Debug, Default, Clone)]
pub struct FileList {
    /// Display names (UTF-8), concatenated; `name_ends[i]` is the end of name `i`.
    names: String,
    name_ends: Vec<u32>,
    /// Folded keys, each followed by `SEP`; `folded_ends[i]` is the end of key `i`
    /// (before its separator).
    folded: String,
    folded_ends: Vec<u32>,
    sizes: Vec<u64>,
    modified: Vec<SystemTime>,
    attributes: Vec<Attributes>,
    /// Original names that are not valid Unicode (rare on Windows: unpaired
    /// surrogates). Their display name in `names` is the lossy form.
    raw_names: HashMap<u32, OsString>,
    /// Current display order (indices into the columns).
    order: Vec<u32>,
}

impl FileList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.sizes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    /// Appends a batch. New entries go to the end of the current order; call
    /// [`FileList::sort`] to place them.
    pub fn extend(&mut self, batch: impl IntoIterator<Item = Entry>) {
        for entry in batch {
            let index = self.len() as u32;
            let name = match entry.name.into_string() {
                Ok(s) => s,
                Err(raw) => {
                    let lossy = raw.to_string_lossy().into_owned();
                    self.raw_names.insert(index, raw);
                    lossy
                }
            };
            self.names.push_str(&name);
            self.name_ends.push(self.names.len() as u32);
            fold_into(&name, &mut self.folded);
            self.folded_ends.push(self.folded.len() as u32);
            self.folded.push(SEP);
            self.sizes.push(entry.size);
            self.modified.push(entry.modified);
            self.attributes.push(entry.attributes);
            self.order.push(index);
        }
    }

    /// Display name of entry `i` (lossy only for names that are not valid Unicode).
    pub fn name(&self, i: u32) -> &str {
        let i = i as usize;
        let start = if i == 0 {
            0
        } else {
            self.name_ends[i - 1] as usize
        };
        &self.names[start..self.name_ends[i] as usize]
    }

    /// Exact original name of entry `i`, for file system calls.
    pub fn os_name(&self, i: u32) -> OsString {
        self.raw_names
            .get(&i)
            .cloned()
            .unwrap_or_else(|| OsString::from(self.name(i)))
    }

    pub fn size(&self, i: u32) -> u64 {
        self.sizes[i as usize]
    }

    pub fn modified(&self, i: u32) -> SystemTime {
        self.modified[i as usize]
    }

    pub fn attributes(&self, i: u32) -> Attributes {
        self.attributes[i as usize]
    }

    /// Current display order.
    pub fn order(&self) -> &[u32] {
        &self.order
    }

    fn folded(&self, i: u32) -> &str {
        let i = i as usize;
        // Each key is followed by one separator byte.
        let start = if i == 0 {
            0
        } else {
            self.folded_ends[i - 1] as usize + 1
        };
        &self.folded[start..self.folded_ends[i] as usize]
    }

    /// Sorts the display order. Folders always come first; ties fall back to the
    /// name, then to the exact name, then to arrival order (stable).
    pub fn sort(&mut self, key: SortKey, direction: Direction) {
        self.order = self.sorted_order(key, direction);
    }

    /// The order [`FileList::sort`] would produce, without changing the list, so
    /// it can be computed off the UI thread and applied with [`FileList::set_order`].
    pub fn sorted_order(&self, key: SortKey, direction: Direction) -> Vec<u32> {
        let mut order: Vec<u32> = (0..self.len() as u32).collect();
        let by_name = |a: u32, b: u32| {
            natural_cmp(self.folded(a), self.folded(b)).then_with(|| self.name(a).cmp(self.name(b)))
        };
        let primary = |a: u32, b: u32| -> Ordering {
            match key {
                SortKey::Name => by_name(a, b),
                SortKey::Size => self.size(a).cmp(&self.size(b)).then_with(|| by_name(a, b)),
                SortKey::Modified => self
                    .modified(a)
                    .cmp(&self.modified(b))
                    .then_with(|| by_name(a, b)),
                SortKey::Type => natural_cmp(extension(self.folded(a)), extension(self.folded(b)))
                    .then_with(|| by_name(a, b)),
            }
        };
        order.sort_unstable_by(|&a, &b| {
            let dirs_first = self
                .attributes(b)
                .is_dir()
                .cmp(&self.attributes(a).is_dir());
            dirs_first
                .then_with(|| match direction {
                    Direction::Ascending => primary(a, b),
                    Direction::Descending => primary(b, a),
                })
                // Arrival order breaks exact ties in both directions (stability).
                .then_with(|| a.cmp(&b))
        });
        order
    }

    /// Replaces the display order. Rejected (returns `false`) unless `order` is a
    /// permutation of all entries, e.g. when batches arrived after it was computed.
    pub fn set_order(&mut self, order: Vec<u32>) -> bool {
        if order.len() != self.len() {
            return false;
        }
        let mut seen = vec![false; order.len()];
        for &i in &order {
            match seen.get_mut(i as usize) {
                Some(s) if !*s => *s = true,
                _ => return false,
            }
        }
        self.order = order;
        true
    }

    /// Entries whose name contains `needle` (case/accent-insensitive), in the
    /// current display order. An empty needle returns everything.
    pub fn filter(&self, needle: &str) -> Vec<u32> {
        let needle = crate::natural::fold(needle);
        if needle.is_empty() {
            return self.order.clone();
        }
        if needle.contains(SEP) {
            return Vec::new();
        }
        // One pass over the folded buffer; map each match to its entry by walking
        // the (sorted) key ends forward.
        let mut hit = vec![false; self.len()];
        let mut entry = 0usize;
        for (at, _) in self.folded.match_indices(needle.as_str()) {
            while (self.folded_ends[entry] as usize) < at + needle.len() {
                entry += 1;
            }
            hit[entry] = true;
        }
        self.order
            .iter()
            .copied()
            .filter(|&i| hit[i as usize])
            .collect()
    }

    /// Like [`FileList::filter`], but only among `candidates` (keeping their
    /// order). When the user extends the text (`"rep"` -> `"repo"`), every match of
    /// the new text is among the matches of the old one, so filtering the previous
    /// result is equivalent and much cheaper than a full pass.
    pub fn filter_within(&self, needle: &str, candidates: &[u32]) -> Vec<u32> {
        let needle = crate::natural::fold(needle);
        if needle.is_empty() {
            return candidates.to_vec();
        }
        candidates
            .iter()
            .copied()
            .filter(|&i| self.folded(i).contains(needle.as_str()))
            .collect()
    }
}

/// Extension of a folded name (`""` when none; a leading dot is not an extension).
fn extension(folded: &str) -> &str {
    match folded.rfind('.') {
        Some(0) | None => "",
        Some(dot) => &folded[dot + 1..],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn file(name: &str, size: u64, secs: u64) -> Entry {
        Entry {
            name: name.into(),
            size,
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            attributes: Attributes::empty(),
        }
    }

    fn dir(name: &str) -> Entry {
        Entry {
            attributes: Attributes::DIRECTORY,
            ..file(name, 0, 0)
        }
    }

    fn names(list: &FileList, order: &[u32]) -> Vec<String> {
        order.iter().map(|&i| list.name(i).to_owned()).collect()
    }

    fn sorted(entries: Vec<Entry>, key: SortKey, dir: Direction) -> Vec<String> {
        let mut list = FileList::new();
        list.extend(entries);
        list.sort(key, dir);
        names(&list, list.order())
    }

    #[test]
    fn natural_order_a2_before_a10() {
        let got = sorted(
            vec![file("a10", 0, 0), file("a2", 0, 0), file("a1", 0, 0)],
            SortKey::Name,
            Direction::Ascending,
        );
        assert_eq!(got, ["a1", "a2", "a10"]);
    }

    #[test]
    fn accents_and_case_are_ignored_for_order() {
        let got = sorted(
            vec![
                file("Zebra", 0, 0),
                file("école", 0, 0),
                file("Eagle", 0, 0),
                file("ação", 0, 0),
                file("Banana", 0, 0),
                file("fable", 0, 0),
            ],
            SortKey::Name,
            Direction::Ascending,
        );
        assert_eq!(got, ["ação", "Banana", "Eagle", "école", "fable", "Zebra"]);
    }

    #[test]
    fn exact_ties_keep_arrival_order_in_both_directions() {
        // Same size, same folded name differs only by case: exact-name tiebreak,
        // then arrival order for identical everything.
        let entries = || {
            vec![
                file("b", 5, 0),
                file("a", 5, 0),
                file("A", 5, 0),
                file("dup", 5, 0),
                file("dup", 5, 0),
            ]
        };
        let mut list = FileList::new();
        list.extend(entries());
        list.sort(SortKey::Size, Direction::Ascending);
        assert_eq!(list.order(), [2, 1, 0, 3, 4]);
        list.sort(SortKey::Size, Direction::Descending);
        assert_eq!(list.order(), [3, 4, 0, 1, 2]);
    }

    #[test]
    fn folders_first_for_every_key_and_direction() {
        for key in [
            SortKey::Name,
            SortKey::Size,
            SortKey::Modified,
            SortKey::Type,
        ] {
            for d in [Direction::Ascending, Direction::Descending] {
                let got = sorted(
                    vec![
                        file("a.txt", 9, 9),
                        dir("zeta"),
                        file("b.txt", 1, 1),
                        dir("alfa"),
                    ],
                    key,
                    d,
                );
                let first_two: Vec<&str> = got[..2].iter().map(String::as_str).collect();
                assert!(
                    first_two.contains(&"alfa") && first_two.contains(&"zeta"),
                    "{key:?} {d:?}: {got:?}"
                );
            }
        }
    }

    #[test]
    fn sort_by_size_date_and_type() {
        let entries = || {
            vec![
                file("c.zip", 300, 1),
                file("a.txt", 100, 3),
                file("b.md", 200, 2),
                file("d.TXT", 50, 4),
                file("noext", 10, 5),
            ]
        };
        assert_eq!(
            sorted(entries(), SortKey::Size, Direction::Ascending),
            ["noext", "d.TXT", "a.txt", "b.md", "c.zip"]
        );
        assert_eq!(
            sorted(entries(), SortKey::Modified, Direction::Descending),
            ["noext", "d.TXT", "a.txt", "b.md", "c.zip"]
        );
        assert_eq!(
            sorted(entries(), SortKey::Type, Direction::Ascending),
            ["noext", "b.md", "a.txt", "d.TXT", "c.zip"]
        );
    }

    #[test]
    fn filter_contains_ignoring_accents_and_case() {
        let mut list = FileList::new();
        list.extend(vec![
            file("Relatório Final.pdf", 0, 0),
            file("relatorio-2.txt", 0, 0),
            file("foto.jpg", 0, 0),
            file("RELATÓRIO.md", 0, 0),
        ]);
        list.sort(SortKey::Name, Direction::Ascending);
        assert_eq!(
            names(&list, &list.filter("RELATO")),
            ["Relatório Final.pdf", "relatorio-2.txt", "RELATÓRIO.md"]
        );
        assert_eq!(names(&list, &list.filter("ó f")), ["Relatório Final.pdf"]);
        assert_eq!(list.filter("").len(), 4);
        assert!(list.filter("xyz").is_empty());
        // A match never spans two names.
        assert!(list.filter("pdfrel").is_empty());
        assert!(list.filter("\0").is_empty());
    }

    #[test]
    fn filter_within_matches_full_filter_when_refining() {
        let mut list = FileList::new();
        list.extend(
            [
                "Relatório.pdf",
                "relat.txt",
                "report-1",
                "Report-12",
                "foto",
                "repórter",
            ]
            .map(|n| file(n, 0, 0)),
        );
        list.sort(SortKey::Name, Direction::Ascending);
        let mut previous = list.filter("");
        let mut typed = String::new();
        for c in "repo".chars() {
            typed.push(c);
            let refined = list.filter_within(&typed, &previous);
            assert_eq!(refined, list.filter(&typed), "after typing {typed:?}");
            previous = refined;
        }
        assert_eq!(
            names(&list, &previous),
            ["report-1", "Report-12", "repórter"]
        );
        assert_eq!(list.filter_within("", &previous), previous);
    }

    #[test]
    fn batches_append_and_resort() {
        let mut list = FileList::new();
        list.extend(vec![file("b", 0, 0), file("d", 0, 0)]);
        list.sort(SortKey::Name, Direction::Ascending);
        list.extend(vec![file("a", 0, 0), file("c", 0, 0)]);
        assert_eq!(names(&list, list.order()), ["b", "d", "a", "c"]);
        list.sort(SortKey::Name, Direction::Ascending);
        assert_eq!(names(&list, list.order()), ["a", "b", "c", "d"]);
        assert_eq!(list.len(), 4);
        assert_eq!(list.size(2), 0);
    }

    #[test]
    fn sorted_order_is_computed_aside_and_validated() {
        let mut list = FileList::new();
        list.extend(vec![file("b", 0, 0), file("a", 0, 0), file("c", 0, 0)]);
        let order = list.sorted_order(SortKey::Name, Direction::Ascending);
        assert_eq!(order, [1, 0, 2]);
        assert_eq!(list.order(), [0, 1, 2], "sorted_order must not mutate");
        assert!(!list.set_order(vec![0, 1]), "wrong length");
        assert!(!list.set_order(vec![0, 0, 1]), "duplicate");
        assert!(!list.set_order(vec![0, 1, 9]), "out of range");
        assert!(list.set_order(order.clone()));
        assert_eq!(list.order(), order.as_slice());
        // A batch arriving after the order was computed makes it stale.
        list.extend(vec![file("d", 0, 0)]);
        assert!(!list.set_order(order));
    }

    #[test]
    fn names_round_trip_exactly() {
        let mut list = FileList::new();
        list.extend(vec![file("日本語 😀.txt", 1, 0)]);
        assert_eq!(list.os_name(0), OsString::from("日本語 😀.txt"));
        assert_eq!(list.attributes(0), Attributes::empty());
        assert_eq!(list.modified(0), SystemTime::UNIX_EPOCH);
    }
}
