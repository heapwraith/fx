//! Folder listing stored by columns (docs/ARCHITECTURE.md, "Desenho da lista"):
//! one name buffer with offsets, separate size/date/attribute vectors, and an
//! index vector that sorting and filtering rearrange. Batches are appended; the
//! columns are never rebuilt.
//!
//! Changes from the folder watcher (task T1.8) are applied in place: a new
//! entry is appended to the columns and inserted at its sorted position; a
//! removed entry leaves the display order but keeps its slot, so the indices
//! of every other entry (held by the selection) stay valid.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ffi::OsString;
use std::time::SystemTime;

use crate::natural::{fold_into, natural_cmp};
use crate::{Attributes, Change, Entry};

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

impl SortKey {
    /// Stable identifier, for saved state.
    pub const fn id(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "modified",
            SortKey::Type => "type",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            SortKey::Name,
            SortKey::Size,
            SortKey::Modified,
            SortKey::Type,
        ]
        .into_iter()
        .find(|k| k.id() == id)
    }

    /// Direction of the first click on a column header: newest and largest
    /// first for dates and sizes (what one usually looks for), A→Z otherwise.
    pub const fn first_direction(self) -> Direction {
        match self {
            SortKey::Size | SortKey::Modified => Direction::Descending,
            SortKey::Name | SortKey::Type => Direction::Ascending,
        }
    }
}

/// Header click: the same column flips the direction, another column starts
/// at its first direction.
pub fn next_sort(current: (SortKey, Direction), clicked: SortKey) -> (SortKey, Direction) {
    match current {
        (key, Direction::Ascending) if key == clicked => (key, Direction::Descending),
        (key, Direction::Descending) if key == clicked => (key, Direction::Ascending),
        _ => (clicked, clicked.first_direction()),
    }
}

/// "Type" column text: folders, then the extension ("Arquivo JPG"), or just
/// "Arquivo" without one (a leading dot is not an extension: ".gitignore").
pub fn type_label(name: &str, is_dir: bool) -> String {
    if is_dir {
        return "Pasta de arquivos".to_owned();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 && dot + 1 < name.len() => {
            format!("Arquivo {}", name[dot + 1..].to_uppercase())
        }
        _ => "Arquivo".to_owned(),
    }
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
    /// Slots of entries removed by the watcher (not in `order`).
    removed: Vec<bool>,
    /// The order's sort, once sorted (new entries are inserted by it).
    sorted_by: Option<(SortKey, Direction)>,
    /// Live entries by lowercase name, built on the first change.
    by_name: Option<HashMap<String, u32>>,
}

/// What [`FileList::apply`] did, for the view to follow.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The watcher lost events: read the folder again.
    pub reload: bool,
    pub added: Vec<u32>,
    pub removed: Vec<u32>,
    /// `(old, new)` slots of renamed entries (to carry the selection over).
    pub renamed: Vec<(u32, u32)>,
    pub modified: Vec<u32>,
}

impl FileList {
    pub fn new() -> Self {
        Self::default()
    }

    /// Entries in the folder (removed ones excluded).
    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Number of slots: every index below it is valid for the accessors
    /// (including removed entries).
    pub fn slots(&self) -> usize {
        self.sizes.len()
    }

    pub fn is_removed(&self, i: u32) -> bool {
        self.removed.get(i as usize).copied().unwrap_or(false)
    }

    /// Appends a batch. New entries go to the end of the current order; call
    /// [`FileList::sort`] to place them.
    pub fn extend(&mut self, batch: impl IntoIterator<Item = Entry>) {
        for entry in batch {
            let index = self.push_slot(entry);
            self.order.push(index);
        }
    }

    /// Appends an entry to the columns (not to the order); returns its slot.
    fn push_slot(&mut self, entry: Entry) -> u32 {
        {
            let index = self.slots() as u32;
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
            self.removed.push(false);
            if self.by_name.is_some() {
                let key = self.name(index).to_lowercase();
                if let Some(map) = &mut self.by_name {
                    map.insert(key, index);
                }
            }
            index
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
        self.sorted_by = Some((key, direction));
    }

    /// Like [`FileList::set_order`], remembering the sort for new entries.
    pub fn set_sorted_order(
        &mut self,
        order: Vec<u32>,
        key: SortKey,
        direction: Direction,
    ) -> bool {
        let ok = self.set_order(order);
        if ok {
            self.sorted_by = Some((key, direction));
        }
        ok
    }

    /// The comparison [`FileList::sorted_order`] sorts by.
    fn compare(&self, a: u32, b: u32, key: SortKey, direction: Direction) -> Ordering {
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
    }

    /// The order [`FileList::sort`] would produce, without changing the list, so
    /// it can be computed off the UI thread and applied with [`FileList::set_order`].
    pub fn sorted_order(&self, key: SortKey, direction: Direction) -> Vec<u32> {
        let mut order: Vec<u32> = (0..self.slots() as u32)
            .filter(|&i| !self.is_removed(i))
            .collect();
        order.sort_unstable_by(|&a, &b| self.compare(a, b, key, direction));
        order
    }

    /// Replaces the display order. Rejected (returns `false`) unless `order` is a
    /// permutation of all entries, e.g. when batches arrived after it was computed.
    pub fn set_order(&mut self, order: Vec<u32>) -> bool {
        if order.len() != self.len() {
            return false;
        }
        let mut seen = vec![false; self.slots()];
        for &i in &order {
            match seen.get_mut(i as usize) {
                Some(s) if !*s && !self.is_removed(i) => *s = true,
                _ => return false,
            }
        }
        self.order = order;
        true
    }

    /// Hidden or system entry (shown only when the user asks, T1.21).
    pub fn is_hidden(&self, i: u32) -> bool {
        let a = self.attributes(i);
        a.contains(Attributes::HIDDEN) || a.contains(Attributes::SYSTEM)
    }

    /// The rows to show: `filtered` (a name filter's result, `None` for all
    /// entries in display order), without hidden and system entries unless
    /// `show_hidden`. `None` means "every entry, in display order" (nothing
    /// to leave out), so the common case costs no copy.
    pub fn visible(&self, filtered: Option<Vec<u32>>, show_hidden: bool) -> Option<Vec<u32>> {
        if show_hidden {
            return filtered;
        }
        match filtered {
            Some(mut rows) => {
                rows.retain(|&i| !self.is_hidden(i));
                Some(rows)
            }
            None => {
                if !self.order.iter().any(|&i| self.is_hidden(i)) {
                    return None;
                }
                Some(
                    self.order
                        .iter()
                        .copied()
                        .filter(|&i| !self.is_hidden(i))
                        .collect(),
                )
            }
        }
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
        let mut hit = vec![false; self.slots()];
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

impl FileList {
    /// Applies watcher changes in place (see the module comment). Stops at an
    /// [`Change::Overflow`]: the caller must read the folder again.
    pub fn apply(&mut self, changes: Vec<Change>) -> Applied {
        let mut done = Applied::default();
        if self.by_name.is_none() {
            let map = self
                .order
                .iter()
                .map(|&i| (self.name(i).to_lowercase(), i))
                .collect();
            self.by_name = Some(map);
        }
        for change in changes {
            match change {
                Change::Overflow => {
                    done.reload = true;
                    return done;
                }
                Change::Added(entry) | Change::Modified(entry) => match self.find(&entry.name) {
                    Some(i) => {
                        self.update(i, &entry);
                        done.modified.push(i);
                    }
                    None => done.added.push(self.insert(entry)),
                },
                Change::Removed(name) => {
                    if let Some(i) = self.find(&name) {
                        self.remove(i);
                        done.removed.push(i);
                    }
                }
                Change::Renamed { from, to } => {
                    let old = self.find(&from);
                    if let Some(i) = old {
                        self.remove(i);
                    }
                    let new = match self.find(&to.name) {
                        Some(i) => {
                            self.update(i, &to);
                            i
                        }
                        None => self.insert(to),
                    };
                    match old {
                        Some(i) => done.renamed.push((i, new)),
                        None => done.added.push(new),
                    }
                }
            }
        }
        done
    }

    fn find(&self, name: &std::ffi::OsStr) -> Option<u32> {
        let key = name.to_string_lossy().to_lowercase();
        let i = *self.by_name.as_ref()?.get(&key)?;
        (!self.is_removed(i)).then_some(i)
    }

    fn update(&mut self, i: u32, entry: &Entry) {
        let i = i as usize;
        self.sizes[i] = entry.size;
        self.modified[i] = entry.modified;
        self.attributes[i] = entry.attributes;
    }

    /// Adds an entry at its sorted position (at the end when not sorted yet).
    fn insert(&mut self, entry: Entry) -> u32 {
        let i = self.push_slot(entry);
        let at = match self.sorted_by {
            Some((key, dir)) => self
                .order
                .partition_point(|&o| self.compare(o, i, key, dir) == Ordering::Less),
            None => self.order.len(),
        };
        self.order.insert(at, i);
        i
    }

    fn remove(&mut self, i: u32) {
        if let Some(at) = self.order.iter().position(|&o| o == i) {
            self.order.remove(at);
        }
        self.removed[i as usize] = true;
        let key = self.name(i).to_lowercase();
        if let Some(map) = &mut self.by_name
            && map.get(&key) == Some(&i)
        {
            map.remove(&key);
        }
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
    fn hidden_and_system_entries_are_left_out() {
        let mut list = FileList::new();
        let mut hidden = file("desktop.ini", 1, 0);
        hidden.attributes = Attributes::HIDDEN.with(Attributes::SYSTEM);
        let mut sys = file("pagefile.sys", 1, 0);
        sys.attributes = Attributes::SYSTEM;
        let mut hidden_dir = dir(".git");
        hidden_dir.attributes = Attributes::DIRECTORY.with(Attributes::HIDDEN);
        list.extend(vec![
            file("b.txt", 1, 0),
            hidden,
            sys,
            hidden_dir,
            file("a.txt", 1, 0),
        ]);
        list.sort(SortKey::Name, Direction::Ascending);
        let shown = |v: Option<Vec<u32>>| names(&list, v.as_deref().unwrap_or(list.order()));
        assert_eq!(shown(list.visible(None, false)), ["a.txt", "b.txt"]);
        assert_eq!(shown(list.visible(None, true)).len(), 5);
        // Combined with a name filter.
        let txt = list.filter("t");
        assert_eq!(
            shown(list.visible(Some(txt.clone()), false)),
            ["a.txt", "b.txt"]
        );
        assert_eq!(list.visible(Some(txt.clone()), true), Some(txt));
        // Nothing hidden: no copy.
        let mut plain = FileList::new();
        plain.extend(vec![file("x", 1, 0)]);
        assert_eq!(plain.visible(None, false), None);
    }

    #[test]
    fn header_clicks_and_ids() {
        use Direction::*;
        assert_eq!(
            next_sort((SortKey::Name, Ascending), SortKey::Name),
            (SortKey::Name, Descending)
        );
        assert_eq!(
            next_sort((SortKey::Name, Descending), SortKey::Name),
            (SortKey::Name, Ascending)
        );
        assert_eq!(
            next_sort((SortKey::Name, Ascending), SortKey::Modified),
            (SortKey::Modified, Descending)
        );
        assert_eq!(
            next_sort((SortKey::Size, Descending), SortKey::Type),
            (SortKey::Type, Ascending)
        );
        for k in [
            SortKey::Name,
            SortKey::Size,
            SortKey::Modified,
            SortKey::Type,
        ] {
            assert_eq!(SortKey::from_id(k.id()), Some(k));
        }
        assert_eq!(SortKey::from_id("x"), None);
    }

    #[test]
    fn type_labels() {
        assert_eq!(type_label("foto.jpg", false), "Arquivo JPG");
        assert_eq!(type_label("a.tar.gz", false), "Arquivo GZ");
        assert_eq!(type_label("README", false), "Arquivo");
        assert_eq!(type_label(".gitignore", false), "Arquivo");
        assert_eq!(type_label("estranho.", false), "Arquivo");
        assert_eq!(type_label("fotos.2024", true), "Pasta de arquivos");
    }

    #[test]
    fn reordering_keeps_selection_and_cursor() {
        use crate::{MoveMode, Selection, Step};
        let mut list = FileList::new();
        list.extend(vec![file("b", 30, 1), file("a", 10, 3), file("c", 20, 2)]);
        list.sort(SortKey::Name, Direction::Ascending);
        let mut sel = Selection::default();
        // A cursor and a two-entry selection.
        sel.move_cursor(list.order(), Step::Down(1), MoveMode::Select);
        sel.move_cursor(list.order(), Step::Down(1), MoveMode::Extend);
        let chosen: Vec<String> = names(&list, list.order())
            .into_iter()
            .zip(list.order())
            .filter(|(_, i)| sel.is_selected(**i))
            .map(|(n, _)| n)
            .collect();
        let cursor = sel.cursor().map(|i| list.name(i).to_owned());
        // Re-sort off the UI thread's way: compute, then apply.
        let order = list.sorted_order(SortKey::Size, Direction::Descending);
        assert!(list.set_sorted_order(order, SortKey::Size, Direction::Descending));
        assert_eq!(names(&list, list.order()), ["b", "c", "a"]);
        let after: Vec<String> = list
            .order()
            .iter()
            .filter(|i| sel.is_selected(**i))
            .map(|&i| list.name(i).to_owned())
            .collect();
        let mut before = chosen.clone();
        before.sort();
        let mut now = after.clone();
        now.sort();
        assert_eq!(before, now, "same entries selected");
        assert_eq!(sel.cursor().map(|i| list.name(i).to_owned()), cursor);
        // The cursor's display row follows the entry.
        let row = names(&list, list.order())
            .iter()
            .position(|n| Some(n) == cursor.as_ref());
        assert!(row.is_some());
        assert_eq!(sel.position(list.order(), sel.cursor().unwrap()), row);
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

    fn entry(name: &str, size: u64) -> Entry {
        Entry {
            name: name.into(),
            size,
            modified: SystemTime::UNIX_EPOCH,
            attributes: Attributes::default(),
        }
    }

    fn shown(list: &FileList) -> Vec<&str> {
        list.order().iter().map(|&i| list.name(i)).collect()
    }

    #[test]
    fn changes_insert_sorted_remove_and_keep_other_indices() {
        let mut list = FileList::new();
        list.extend([entry("b.txt", 1), entry("d.txt", 1), entry("f.txt", 1)]);
        list.sort(SortKey::Name, Direction::Ascending);
        let d = list.order()[1];
        let done = list.apply(vec![
            Change::Added(entry("c.txt", 5)),
            Change::Added(entry("a.txt", 5)),
            Change::Removed("F.TXT".into()), // Windows names ignore case
        ]);
        assert_eq!(shown(&list), ["a.txt", "b.txt", "c.txt", "d.txt"]);
        assert_eq!((done.added.len(), done.removed.len()), (2, 1));
        assert_eq!(list.name(d), "d.txt", "existing indices are stable");
        assert_eq!((list.len(), list.slots()), (4, 5));
        assert!(list.is_removed(done.removed[0]));
        // Re-sorting and filtering skip the removed slot.
        assert_eq!(
            list.sorted_order(SortKey::Name, Direction::Ascending).len(),
            4
        );
        assert!(list.filter("f.txt").is_empty());
    }

    #[test]
    fn duplicates_and_unknown_names_are_harmless() {
        let mut list = FileList::new();
        list.extend([entry("a.txt", 1)]);
        list.sort(SortKey::Name, Direction::Ascending);
        // Already listed (the read and the watcher overlap): an update.
        let done = list.apply(vec![
            Change::Added(entry("a.txt", 9)),
            Change::Removed("missing".into()),
            Change::Modified(entry("new.txt", 3)),
        ]);
        assert_eq!(list.size(list.order()[0]), 9);
        assert_eq!(done.modified, [0]);
        assert_eq!(
            shown(&list),
            ["a.txt", "new.txt"],
            "unknown modified = added"
        );
    }

    #[test]
    fn rename_moves_to_the_new_sorted_place() {
        let mut list = FileList::new();
        list.extend([entry("a", 1), entry("m", 1), entry("z", 1)]);
        list.sort(SortKey::Name, Direction::Ascending);
        let done = list.apply(vec![Change::Renamed {
            from: "a".into(),
            to: entry("y", 1),
        }]);
        assert_eq!(shown(&list), ["m", "y", "z"]);
        let (old, new) = done.renamed[0];
        assert!(list.is_removed(old) && list.name(new) == "y");
        // A file can come back under its old name.
        list.apply(vec![Change::Added(entry("a", 2))]);
        assert_eq!(shown(&list), ["a", "m", "y", "z"]);
    }

    #[test]
    fn overflow_asks_for_a_reload() {
        let mut list = FileList::new();
        list.extend([entry("a", 1)]);
        let done = list.apply(vec![
            Change::Overflow,
            Change::Added(entry("never-applied", 1)),
        ]);
        assert!(done.reload);
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn unsorted_list_appends_new_entries() {
        let mut list = FileList::new();
        list.extend([entry("b", 1)]);
        list.apply(vec![Change::Added(entry("a", 1))]);
        assert_eq!(shown(&list), ["b", "a"], "sorted later with the rest");
    }
}
