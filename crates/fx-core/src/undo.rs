//! Undo/redo history (task T1.12, SAFETY.md §2.4). Generic over the entry:
//! fx-ui stores each finished operation with the operation that reverts it.
//!
//! Recording a new operation clears the redo side, like every editor. Undoing
//! takes an entry from the undo side; once its reversal has run, the caller
//! puts it on the redo side (and the other way round), so a failed reversal
//! never loses the entry silently: the caller decides.

#[derive(Debug, Clone)]
pub struct UndoStack<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
}

impl<T> UndoStack<T> {
    pub const DEFAULT_LIMIT: usize = 100;

    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// A new operation finished: it can be undone; nothing can be redone.
    pub fn record(&mut self, entry: T) {
        self.redo.clear();
        self.push_undo(entry);
    }

    /// An entry whose redo just ran goes back to the undo side (redo kept).
    pub fn push_undo(&mut self, entry: T) {
        self.undo.push(entry);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
    }

    pub fn push_redo(&mut self, entry: T) {
        self.redo.push(entry);
    }

    /// The most recent operation, to revert.
    pub fn take_undo(&mut self) -> Option<T> {
        self.undo.pop()
    }

    /// The most recently undone operation, to apply again.
    pub fn take_redo(&mut self) -> Option<T> {
        self.redo.pop()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_round_trip() {
        let mut s = UndoStack::new(10);
        s.record("a");
        s.record("b");
        let b = s.take_undo().unwrap();
        s.push_redo(b);
        assert_eq!(s.take_redo(), Some("b"));
        s.push_undo("b");
        assert_eq!(s.take_undo(), Some("b"));
        assert_eq!(s.take_undo(), Some("a"));
        assert_eq!(s.take_undo(), None);
    }

    #[test]
    fn recording_clears_redo() {
        let mut s = UndoStack::new(10);
        s.record(1);
        let e = s.take_undo().unwrap();
        s.push_redo(e);
        assert!(s.can_redo());
        s.record(2);
        assert!(!s.can_redo());
        assert!(s.can_undo());
    }

    #[test]
    fn oldest_entries_fall_off() {
        let mut s = UndoStack::new(2);
        for i in 0..5 {
            s.record(i);
        }
        assert_eq!(
            (s.take_undo(), s.take_undo(), s.take_undo()),
            (Some(4), Some(3), None)
        );
    }
}
