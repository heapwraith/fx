//! Tabs of a window (task T1.4): an ordered list with one active tab. Generic
//! over the tab's contents (fx-ui keeps a folder view per tab, each with its
//! own history).
//!
//! Rules (like browsers and Explorer): a new tab opens right after the active
//! one and becomes active; closing the active tab activates its right
//! neighbour (or the left one when it was the last); Ctrl+Tab wraps around;
//! moving a tab keeps the same tab active.

#[derive(Debug, Clone)]
pub struct Tabs<T> {
    items: Vec<T>,
    active: usize,
}

impl<T> Tabs<T> {
    /// A window always starts with one tab.
    pub fn new(first: T) -> Self {
        Self {
            items: vec![first],
            active: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Never true while the window is open; closing the last tab returns it.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &T {
        &self.items[self.active]
    }

    pub fn get(&self, i: usize) -> Option<&T> {
        self.items.get(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    /// Opens `tab` right after the active one and activates it; returns its index.
    pub fn open(&mut self, tab: T) -> usize {
        let at = self.active + 1;
        self.items.insert(at, tab);
        self.active = at;
        at
    }

    /// Closes tab `i` and returns it. Closing the last tab leaves the list
    /// empty (the caller closes the window).
    pub fn close(&mut self, i: usize) -> Option<T> {
        if i >= self.items.len() {
            return None;
        }
        let tab = self.items.remove(i);
        if i < self.active || (i == self.active && self.active == self.items.len()) {
            self.active = self.active.saturating_sub(1);
        }
        Some(tab)
    }

    pub fn activate(&mut self, i: usize) -> bool {
        if i < self.items.len() {
            self.active = i;
            true
        } else {
            false
        }
    }

    /// Ctrl+Tab.
    pub fn next(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + 1) % self.items.len();
        }
    }

    /// Ctrl+Shift+Tab.
    pub fn previous(&mut self) {
        if !self.items.is_empty() {
            self.active = (self.active + self.items.len() - 1) % self.items.len();
        }
    }

    /// Drag to reorder: moves tab `from` to position `to`; the active tab
    /// stays the same tab.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        let n = self.items.len();
        if from >= n || to >= n {
            return false;
        }
        let tab = self.items.remove(from);
        self.items.insert(to, tab);
        self.active = if self.active == from {
            to
        } else if from < self.active && to >= self.active {
            self.active - 1
        } else if from > self.active && to <= self.active {
            self.active + 1
        } else {
            self.active
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(t: &Tabs<char>) -> String {
        t.iter().collect()
    }

    #[test]
    fn open_inserts_after_the_active_tab() {
        let mut t = Tabs::new('a');
        assert_eq!(t.open('b'), 1);
        t.activate(0);
        assert_eq!(t.open('c'), 1);
        assert_eq!((order(&t), *t.active()), ("acb".into(), 'c'));
    }

    #[test]
    fn closing_picks_the_right_neighbour_then_the_left() {
        let mut t = Tabs::new('a');
        t.open('b');
        t.open('c');
        t.open('d'); // a b c [d]
        t.activate(1); // a [b] c d
        assert_eq!(t.close(1), Some('b'));
        assert_eq!(*t.active(), 'c', "right neighbour");
        t.activate(2); // a c [d]
        t.close(2);
        assert_eq!(*t.active(), 'c', "last tab: left neighbour");
        // Closing a tab left of the active one keeps the same tab active.
        t.close(0);
        assert_eq!((order(&t), *t.active()), ("c".into(), 'c'));
        assert_eq!(t.close(5), None);
        assert_eq!(t.close(0), Some('c'));
        assert!(t.is_empty());
    }

    #[test]
    fn closing_a_tab_right_of_the_active_one_keeps_it() {
        let mut t = Tabs::new('a');
        t.open('b');
        t.open('c');
        t.activate(0);
        t.close(2);
        assert_eq!((order(&t), t.active_index()), ("ab".into(), 0));
    }

    #[test]
    fn next_and_previous_wrap_around() {
        let mut t = Tabs::new('a');
        t.open('b');
        t.open('c'); // active c
        t.next();
        assert_eq!(*t.active(), 'a');
        t.previous();
        assert_eq!(*t.active(), 'c');
        t.previous();
        assert_eq!(*t.active(), 'b');
        let mut one = Tabs::new('x');
        one.next();
        one.previous();
        assert_eq!(*one.active(), 'x');
    }

    #[test]
    fn moving_keeps_the_same_tab_active() {
        let mut t = Tabs::new('a');
        for c in ['b', 'c', 'd'] {
            t.open(c);
        }
        t.activate(1); // a [b] c d
        assert!(t.move_tab(1, 3)); // dragged the active tab
        assert_eq!((order(&t), *t.active()), ("acdb".into(), 'b'));
        t.move_tab(0, 2); // a moves past c and d; b stays active
        assert_eq!((order(&t), *t.active()), ("cdab".into(), 'b'));
        t.move_tab(3, 0);
        assert_eq!((order(&t), *t.active()), ("bcda".into(), 'b'));
        t.activate(2); // b c [d] a
        t.move_tab(0, 3); // b goes after the active tab
        assert_eq!((order(&t), *t.active()), ("cdab".into(), 'd'));
        t.move_tab(3, 0); // b comes back before it
        assert_eq!((order(&t), *t.active()), ("bcda".into(), 'd'));
        assert!(!t.move_tab(0, 9));
    }
}
