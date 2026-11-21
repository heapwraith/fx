//! Selection model of a list (task T1.2): a set of selected entries, a cursor
//! (the focused entry) and an anchor (where a Shift range starts).
//!
//! Everything is stored by *entry index* (the index into [`crate::FileList`]),
//! never by display position, so the selection survives sorting and filtering.
//! Operations that depend on the display take `rows`: the entry indices shown,
//! in display order (`FileList::order()` or a filter result).
//!
//! Entries hidden by a filter stay selected; use [`Selection::selected_in`] to
//! get only the visible ones.

/// A cursor movement, in display rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Up(usize),
    Down(usize),
    First,
    Last,
}

/// What a cursor movement does to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveMode {
    /// Plain arrow: the new cursor entry becomes the only selected one.
    Select,
    /// Shift+arrow: selects the range from the anchor to the new cursor.
    Extend,
    /// Ctrl+arrow: moves the cursor only.
    CursorOnly,
}

#[derive(Debug, Clone, Default)]
pub struct Selection {
    bits: Vec<u64>,
    count: usize,
    cursor: Option<u32>,
    anchor: Option<u32>,
    /// Last known display position of the cursor (checked before searching).
    hint: usize,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of selected entries (visible or not).
    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn cursor(&self) -> Option<u32> {
        self.cursor
    }

    pub fn anchor(&self) -> Option<u32> {
        self.anchor
    }

    pub fn is_selected(&self, i: u32) -> bool {
        let (word, bit) = split(i);
        self.bits.get(word).is_some_and(|w| w & bit != 0)
    }

    /// Selected entries, in entry-index order.
    pub fn selected(&self) -> impl Iterator<Item = u32> + '_ {
        self.bits.iter().enumerate().flat_map(|(w, &bits)| {
            (0..64)
                .filter(move |b| bits & (1 << b) != 0)
                .map(move |b| (w * 64 + b) as u32)
        })
    }

    /// Selected entries among `rows`, in display order.
    pub fn selected_in<'a>(&'a self, rows: &'a [u32]) -> impl Iterator<Item = u32> + 'a {
        rows.iter().copied().filter(|&i| self.is_selected(i))
    }

    /// Unselects everything and forgets cursor and anchor.
    pub fn clear(&mut self) {
        self.clear_bits();
        self.cursor = None;
        self.anchor = None;
    }

    /// Click: `i` becomes the only selected entry, the cursor and the anchor.
    pub fn select_only(&mut self, i: u32) {
        self.clear_bits();
        self.set(i, true);
        self.cursor = Some(i);
        self.anchor = Some(i);
    }

    /// Ctrl+click: flips `i` and moves the cursor and the anchor to it.
    pub fn toggle(&mut self, i: u32) {
        let on = !self.is_selected(i);
        self.set(i, on);
        self.cursor = Some(i);
        self.anchor = Some(i);
    }

    /// Ctrl+Space: flips the cursor entry.
    pub fn toggle_cursor(&mut self) {
        if let Some(c) = self.cursor {
            self.toggle(c);
        }
    }

    /// Shift+click: selects exactly the rows between the anchor and `i`
    /// (inclusive) and moves the cursor to `i`. Without a visible anchor, `i`
    /// becomes the anchor.
    pub fn extend_to(&mut self, rows: &[u32], i: u32) {
        let Some(to) = self.position(rows, i) else {
            return;
        };
        let from = self
            .anchor
            .and_then(|a| rows.iter().position(|&r| r == a))
            .unwrap_or(to);
        self.anchor = Some(rows[from]);
        self.clear_bits();
        for &r in &rows[from.min(to)..=from.max(to)] {
            self.set(r, true);
        }
        self.cursor = Some(i);
        self.hint = to;
    }

    /// Replaces the selection (e.g. when coming back to a folder through the
    /// history); the cursor becomes the anchor too.
    pub fn restore(&mut self, selected: impl IntoIterator<Item = u32>, cursor: Option<u32>) {
        self.clear_bits();
        for i in selected {
            self.set(i, true);
        }
        self.cursor = cursor;
        self.anchor = cursor;
    }

    /// Entry `i` left the folder: unselect it (and drop it as cursor/anchor).
    pub fn forget(&mut self, i: u32) {
        self.set(i, false);
        if self.cursor == Some(i) {
            self.cursor = None;
        }
        if self.anchor == Some(i) {
            self.anchor = None;
        }
    }

    /// Entry `old` was renamed and is now entry `new`: it keeps its selection
    /// and cursor/anchor role.
    pub fn moved(&mut self, old: u32, new: u32) {
        let was = self.is_selected(old);
        self.set(old, false);
        if was {
            self.set(new, true);
        }
        if self.cursor == Some(old) {
            self.cursor = Some(new);
        }
        if self.anchor == Some(old) {
            self.anchor = Some(new);
        }
    }

    /// Ctrl+A: selects every row; the cursor stays (or goes to the first row).
    pub fn select_all(&mut self, rows: &[u32]) {
        self.clear_bits();
        for &r in rows {
            self.set(r, true);
        }
        let cursor = self
            .cursor
            .filter(|&c| self.is_selected(c))
            .or_else(|| rows.first().copied());
        self.cursor = cursor;
        self.anchor = cursor;
    }

    /// Moves the cursor and returns its new display position (to scroll to).
    /// Without a visible cursor, any step lands on the first row except
    /// [`Step::Last`]. Returns `None` when `rows` is empty.
    pub fn move_cursor(&mut self, rows: &[u32], step: Step, mode: MoveMode) -> Option<usize> {
        let last = rows.len().checked_sub(1)?;
        let current = self.cursor.and_then(|c| self.position(rows, c));
        let target = match (step, current) {
            (Step::Last, _) => last,
            (Step::First, _) | (_, None) => 0,
            (Step::Up(n), Some(p)) => p.saturating_sub(n),
            (Step::Down(n), Some(p)) => p.saturating_add(n).min(last),
        };
        let entry = rows[target];
        match mode {
            MoveMode::Select => self.select_only(entry),
            MoveMode::Extend => self.extend_to(rows, entry),
            MoveMode::CursorOnly => self.cursor = Some(entry),
        }
        self.hint = target;
        Some(target)
    }

    /// Display position of entry `i` in `rows`, if shown.
    pub fn position(&mut self, rows: &[u32], i: u32) -> Option<usize> {
        if rows.get(self.hint) == Some(&i) {
            return Some(self.hint);
        }
        let p = rows.iter().position(|&r| r == i)?;
        self.hint = p;
        Some(p)
    }

    fn set(&mut self, i: u32, on: bool) {
        let (word, bit) = split(i);
        if word >= self.bits.len() {
            if !on {
                return;
            }
            self.bits.resize(word + 1, 0);
        }
        let w = &mut self.bits[word];
        let was = *w & bit != 0;
        if on && !was {
            *w |= bit;
            self.count += 1;
        } else if !on && was {
            *w &= !bit;
            self.count -= 1;
        }
    }

    fn clear_bits(&mut self) {
        self.bits.fill(0);
        self.count = 0;
    }
}

fn split(i: u32) -> (usize, u64) {
    ((i / 64) as usize, 1 << (i % 64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: u32) -> Vec<u32> {
        (0..n).collect()
    }

    fn sel(s: &Selection) -> Vec<u32> {
        s.selected().collect()
    }

    #[test]
    fn click_toggle_and_clear() {
        let mut s = Selection::new();
        assert!(s.is_empty());
        s.select_only(3);
        s.toggle(70);
        assert_eq!(sel(&s), [3, 70]);
        assert_eq!((s.cursor(), s.anchor()), (Some(70), Some(70)));
        s.toggle(3);
        assert_eq!(sel(&s), [70]);
        s.select_only(5);
        assert_eq!((sel(&s), s.len()), (vec![5], 1));
        s.toggle_cursor();
        assert!(s.is_empty());
        s.clear();
        assert_eq!(s.cursor(), None);
    }

    #[test]
    fn shift_range_follows_the_anchor_and_inverts() {
        let r = rows(20);
        let mut s = Selection::new();
        s.select_only(5);
        s.extend_to(&r, 8);
        assert_eq!(sel(&s), [5, 6, 7, 8]);
        // Crossing the anchor inverts the range: 5..8 becomes 2..5.
        s.extend_to(&r, 2);
        assert_eq!(sel(&s), [2, 3, 4, 5]);
        assert_eq!((s.cursor(), s.anchor()), (Some(2), Some(5)));
        // Shift+arrows do the same, one row at a time.
        s.move_cursor(&r, Step::Down(1), MoveMode::Extend);
        assert_eq!(sel(&s), [3, 4, 5]);
        s.move_cursor(&r, Step::Down(3), MoveMode::Extend);
        assert_eq!(sel(&s), [5, 6]);
        // A plain arrow collapses the selection and moves the anchor.
        s.move_cursor(&r, Step::Down(1), MoveMode::Select);
        assert_eq!((sel(&s), s.anchor()), (vec![7], Some(7)));
    }

    #[test]
    fn ctrl_click_then_shift_starts_from_the_toggled_entry() {
        let r = rows(10);
        let mut s = Selection::new();
        s.select_only(1);
        s.toggle(6);
        s.extend_to(&r, 4);
        assert_eq!(sel(&s), [4, 5, 6]);
    }

    #[test]
    fn cursor_movement_clamps_at_both_ends() {
        let r = rows(100);
        let mut s = Selection::new();
        // No cursor yet: any step lands on the first row, End on the last.
        assert_eq!(s.move_cursor(&r, Step::Up(1), MoveMode::Select), Some(0));
        assert_eq!(s.move_cursor(&r, Step::Up(1), MoveMode::Select), Some(0));
        assert_eq!(
            s.move_cursor(&r, Step::Down(30), MoveMode::Select),
            Some(30)
        );
        assert_eq!(
            s.move_cursor(&r, Step::Down(500), MoveMode::Select),
            Some(99)
        );
        assert_eq!(s.move_cursor(&r, Step::First, MoveMode::Select), Some(0));
        assert_eq!(s.move_cursor(&r, Step::Last, MoveMode::Select), Some(99));
        assert_eq!(sel(&s), [99]);
        assert_eq!(s.move_cursor(&[], Step::Down(1), MoveMode::Select), None);
    }

    #[test]
    fn ctrl_arrows_move_only_the_cursor() {
        let r = rows(10);
        let mut s = Selection::new();
        s.select_only(2);
        s.move_cursor(&r, Step::Down(3), MoveMode::CursorOnly);
        s.toggle_cursor();
        assert_eq!(sel(&s), [2, 5]);
        assert_eq!(s.cursor(), Some(5));
    }

    #[test]
    fn select_all_selects_the_visible_rows() {
        let mut s = Selection::new();
        s.select_only(9);
        s.select_all(&[4, 2, 7]);
        assert_eq!(sel(&s), [2, 4, 7]);
        // The old cursor (9) is not selected any more: the first row takes over.
        assert_eq!((s.cursor(), s.anchor()), (Some(4), Some(4)));
        s.select_all(&[]);
        assert!(s.is_empty());
    }

    /// Sorting changes display positions, not the selected entries; movement
    /// continues from the cursor's new position.
    #[test]
    fn selection_survives_reordering() {
        let by_name = vec![0, 1, 2, 3, 4];
        let by_size = vec![4, 2, 0, 3, 1];
        let mut s = Selection::new();
        s.select_only(2);
        s.extend_to(&by_name, 3);
        assert_eq!(sel(&s), [2, 3]);
        assert_eq!(sel(&s), [2, 3], "re-sorting keeps the same entries");
        assert_eq!(s.selected_in(&by_size).collect::<Vec<_>>(), [2, 3]);
        // Cursor 3 sits at position 3 in the new order; Down goes to entry 1.
        s.move_cursor(&by_size, Step::Down(1), MoveMode::Select);
        assert_eq!(sel(&s), [1]);
        // Shift ranges use the current order.
        s.extend_to(&by_size, 2);
        assert_eq!(sel(&s), [0, 1, 2, 3]);
    }

    /// Filtering hides rows; hidden entries stay selected but are not
    /// "visible selected", and a hidden cursor restarts at the first row.
    #[test]
    fn selection_survives_filtering() {
        let all = rows(10);
        let filtered = vec![1, 4, 8];
        let mut s = Selection::new();
        s.select_only(3);
        s.extend_to(&all, 5);
        assert_eq!(sel(&s), [3, 4, 5]);
        assert_eq!(s.selected_in(&filtered).collect::<Vec<_>>(), [4]);
        // Cursor 5 is hidden by the filter: Down lands on the first visible row.
        assert_eq!(
            s.move_cursor(&filtered, Step::Down(1), MoveMode::Select),
            Some(0)
        );
        assert_eq!(sel(&s), [1]);
        s.move_cursor(&filtered, Step::Down(1), MoveMode::Extend);
        assert_eq!(sel(&s), [1, 4]);
        // Clearing the filter keeps the selection; ranges now span hidden rows.
        s.move_cursor(&all, Step::Down(2), MoveMode::Extend);
        assert_eq!(sel(&s), [1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn shift_with_hidden_anchor_starts_at_the_target() {
        let mut s = Selection::new();
        s.select_only(9);
        s.extend_to(&[1, 2, 3], 2);
        assert_eq!((sel(&s), s.anchor()), (vec![2], Some(2)));
        // Extending to an entry that is not shown does nothing.
        s.extend_to(&[1, 2, 3], 7);
        assert_eq!(sel(&s), [2]);
    }

    #[test]
    fn restore_replaces_selection_and_cursor() {
        let mut s = Selection::new();
        s.select_only(1);
        s.restore([4, 9, 4], Some(9));
        assert_eq!((sel(&s), s.len()), (vec![4, 9], 2));
        assert_eq!((s.cursor(), s.anchor()), (Some(9), Some(9)));
        s.extend_to(&rows(12), 11);
        assert_eq!(sel(&s), [9, 10, 11]);
    }

    #[test]
    fn removed_and_renamed_entries() {
        let mut s = Selection::new();
        s.select_only(2);
        s.toggle(5); // cursor and anchor on 5
        s.moved(5, 9);
        assert_eq!(
            (sel(&s), s.cursor(), s.anchor()),
            (vec![2, 9], Some(9), Some(9))
        );
        s.forget(9);
        assert_eq!((sel(&s), s.cursor(), s.anchor()), (vec![2], None, None));
        s.moved(7, 8); // not selected: nothing to carry
        assert_eq!(sel(&s), [2]);
    }

    #[test]
    fn large_ranges_and_positions() {
        let r: Vec<u32> = (0..100_000).rev().collect();
        let mut s = Selection::new();
        s.select_only(99_999);
        s.move_cursor(&r, Step::Last, MoveMode::Extend);
        assert_eq!(s.len(), 100_000);
        assert_eq!(s.position(&r, 0), Some(99_999));
        assert_eq!(s.position(&r, 50_000), Some(49_999));
        assert_eq!(s.position(&r, 100_000), None);
    }
}
