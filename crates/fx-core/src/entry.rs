//! A single folder entry, as delivered by folder enumeration (fx-fs).

use std::ffi::OsString;
use std::time::SystemTime;

/// Platform-neutral file attributes (docs/DECISIONS.md D-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attributes(u8);

impl Attributes {
    pub const DIRECTORY: Attributes = Attributes(1 << 0);
    pub const HIDDEN: Attributes = Attributes(1 << 1);
    pub const SYSTEM: Attributes = Attributes(1 << 2);
    pub const READONLY: Attributes = Attributes(1 << 3);
    /// Symbolic link, junction or other reparse point.
    pub const LINK: Attributes = Attributes(1 << 4);

    pub const fn empty() -> Self {
        Attributes(0)
    }

    pub const fn contains(self, other: Attributes) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Attributes) -> Self {
        Attributes(self.0 | other.0)
    }

    pub const fn is_dir(self) -> bool {
        self.contains(Attributes::DIRECTORY)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File name only (no parent path). Never converted lossily.
    pub name: OsString,
    /// Size in bytes (0 for folders).
    pub size: u64,
    /// Last write time.
    pub modified: SystemTime,
    pub attributes: Attributes,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_flags_combine() {
        let a = Attributes::empty()
            .with(Attributes::DIRECTORY)
            .with(Attributes::HIDDEN);
        assert!(a.is_dir());
        assert!(a.contains(Attributes::HIDDEN));
        assert!(!a.contains(Attributes::SYSTEM));
        assert!(a.contains(Attributes::DIRECTORY.with(Attributes::HIDDEN)));
        assert!(!Attributes::default().is_dir());
    }
}

/// A change in a folder reported by the watcher (task T1.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(Entry),
    Removed(std::ffi::OsString),
    Renamed {
        from: std::ffi::OsString,
        to: Entry,
    },
    /// Size, date or attributes changed.
    Modified(Entry),
    /// Events were lost (the watcher's buffer overflowed): read the folder again.
    Overflow,
}
