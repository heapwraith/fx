//! Back/forward history of a tab (task T1.3). Generic over what is stored per
//! visit (fx-ui keeps the folder, scroll position and selection).
//!
//! Navigating to a new place after going back drops the forward entries (the
//! history branches, like a browser). The back list keeps at most `limit`
//! entries; the oldest are dropped first.

#[derive(Debug, Clone)]
pub struct History<T> {
    back: Vec<T>,
    current: T,
    forward: Vec<T>,
    limit: usize,
}

impl<T> History<T> {
    /// Default number of entries kept behind the current one.
    pub const DEFAULT_LIMIT: usize = 100;

    pub fn new(current: T, limit: usize) -> Self {
        Self {
            back: Vec::new(),
            current,
            forward: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// Rebuilds a history (from saved state): `back` oldest first, `forward`
    /// as returned by [`History::parts`].
    pub fn from_parts(back: Vec<T>, current: T, forward: Vec<T>, limit: usize) -> Self {
        let limit = limit.max(1);
        let mut back = back;
        if back.len() > limit {
            back.drain(..back.len() - limit);
        }
        Self {
            back,
            current,
            forward,
            limit,
        }
    }

    /// Back entries (oldest first), the current one, and the forward entries
    /// (the next one last), for saving.
    pub fn parts(&self) -> (&[T], &T, &[T]) {
        (&self.back, &self.current, &self.forward)
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    /// To update the current visit (e.g. its scroll position) before leaving it.
    pub fn current_mut(&mut self) -> &mut T {
        &mut self.current
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// A new visit: the current one goes to the back list and the forward list
    /// is dropped.
    pub fn push(&mut self, next: T) {
        let previous = std::mem::replace(&mut self.current, next);
        self.back.push(previous);
        if self.back.len() > self.limit {
            let excess = self.back.len() - self.limit;
            self.back.drain(..excess);
        }
        self.forward.clear();
    }

    /// Steps back; returns the visit to show, or `None` at the start.
    pub fn go_back(&mut self) -> Option<&T> {
        let previous = self.back.pop()?;
        let current = std::mem::replace(&mut self.current, previous);
        self.forward.push(current);
        Some(&self.current)
    }

    /// Steps forward; returns the visit to show, or `None` at the end.
    pub fn go_forward(&mut self) -> Option<&T> {
        let next = self.forward.pop()?;
        let current = std::mem::replace(&mut self.current, next);
        self.back.push(current);
        Some(&self.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_and_forward() {
        let mut h = History::new("a", 10);
        assert!(!h.can_go_back() && !h.can_go_forward());
        assert_eq!(h.go_back(), None);
        h.push("b");
        h.push("c");
        assert_eq!(h.go_back(), Some(&"b"));
        assert_eq!(h.go_back(), Some(&"a"));
        assert_eq!(h.go_back(), None);
        assert_eq!(*h.current(), "a");
        assert_eq!(h.go_forward(), Some(&"b"));
        assert_eq!(h.go_forward(), Some(&"c"));
        assert_eq!(h.go_forward(), None);
        assert!(h.can_go_back() && !h.can_go_forward());
    }

    #[test]
    fn navigating_after_going_back_branches() {
        let mut h = History::new("a", 10);
        h.push("b");
        h.push("c");
        h.go_back();
        h.go_back();
        assert!(h.can_go_forward());
        h.push("x");
        assert!(!h.can_go_forward(), "b and c are dropped");
        assert_eq!(h.go_back(), Some(&"a"));
        assert_eq!(h.go_forward(), Some(&"x"));
        assert_eq!(h.go_forward(), None);
    }

    #[test]
    fn back_list_is_limited() {
        let mut h = History::new(0, 3);
        for i in 1..=10 {
            h.push(i);
        }
        let mut seen = Vec::new();
        while let Some(&v) = h.go_back() {
            seen.push(v);
        }
        assert_eq!(seen, [9, 8, 7], "only the 3 most recent are kept");
        // Forward keeps everything that was walked back over.
        let mut ahead = 0;
        while h.go_forward().is_some() {
            ahead += 1;
        }
        assert_eq!((ahead, *h.current()), (3, 10));
    }

    #[test]
    fn parts_round_trip() {
        let mut h = History::new("a", 10);
        h.push("b");
        h.push("c");
        h.go_back();
        let (back, cur, fwd) = h.parts();
        let mut copy = History::from_parts(back.to_vec(), *cur, fwd.to_vec(), 10);
        assert_eq!(copy.go_forward(), Some(&"c"));
        assert_eq!(copy.go_back(), Some(&"b"));
        assert_eq!(copy.go_back(), Some(&"a"));
        let trimmed = History::from_parts(vec![1, 2, 3, 4], 5, Vec::new(), 2);
        assert_eq!(trimmed.parts().0, [3, 4]);
    }

    #[test]
    fn current_visit_can_be_updated_before_leaving() {
        let mut h = History::new(("a", 0), 10);
        h.current_mut().1 = 240;
        h.push(("b", 0));
        assert_eq!(h.go_back(), Some(&("a", 240)));
    }
}
