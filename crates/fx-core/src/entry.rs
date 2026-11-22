//! A single folder entry, as delivered by folder enumeration (fx-fs).

use std::ffi::OsString;
use std::time::SystemTime;

/// Platform-neutral file attributes (docs/DECISIONS.md D-012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attributes(u16);

impl Attributes {
    pub const DIRECTORY: Attributes = Attributes(1 << 0);
    pub const HIDDEN: Attributes = Attributes(1 << 1);
    pub const SYSTEM: Attributes = Attributes(1 << 2);
    pub const READONLY: Attributes = Attributes(1 << 3);
    /// Symbolic link, junction or other reparse point.
    pub const LINK: Attributes = Attributes(1 << 4);
    /// Cloud files (T3.9): the data is not on this device.
    pub const OFFLINE: Attributes = Attributes(1 << 5);
    /// Opening or reading downloads the data (recall on open/data access).
    pub const RECALL: Attributes = Attributes(1 << 6);
    /// "Always keep on this device".
    pub const PINNED: Attributes = Attributes(1 << 7);
    /// "Free up space" asked: the data may be removed.
    pub const UNPINNED: Attributes = Attributes(1 << 8);

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

/// Sync state of an item in a cloud folder (OneDrive and other providers of
/// the Windows cloud files API), from its attributes alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudState {
    /// Not a cloud item (or a provider that sets none of the flags).
    None,
    /// Only in the cloud: reading it downloads it.
    CloudOnly,
    /// On this device, may be freed.
    Available,
    /// Always kept on this device.
    AlwaysKept,
}

impl CloudState {
    pub fn of(a: Attributes) -> CloudState {
        if a.contains(Attributes::PINNED) {
            CloudState::AlwaysKept
        } else if a.contains(Attributes::RECALL) || a.contains(Attributes::OFFLINE) {
            CloudState::CloudOnly
        } else if a.contains(Attributes::UNPINNED) {
            CloudState::Available
        } else {
            CloudState::None
        }
    }

    /// The item's content must not be read without the user asking
    /// (thumbnails, per-file icons): it would download it.
    pub fn is_cloud_only(self) -> bool {
        self == CloudState::CloudOnly
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
    fn cloud_state_from_attributes() {
        let a = Attributes::empty;
        assert_eq!(CloudState::of(a()), CloudState::None);
        assert_eq!(
            CloudState::of(a().with(Attributes::RECALL)),
            CloudState::CloudOnly
        );
        assert_eq!(
            CloudState::of(a().with(Attributes::OFFLINE)),
            CloudState::CloudOnly
        );
        assert_eq!(
            CloudState::of(a().with(Attributes::UNPINNED).with(Attributes::RECALL)),
            CloudState::CloudOnly,
            "freed: back in the cloud"
        );
        assert_eq!(
            CloudState::of(a().with(Attributes::UNPINNED)),
            CloudState::Available
        );
        assert_eq!(
            CloudState::of(a().with(Attributes::PINNED).with(Attributes::LINK)),
            CloudState::AlwaysKept
        );
        assert!(CloudState::of(a().with(Attributes::RECALL)).is_cloud_only());
        assert!(!CloudState::of(a().with(Attributes::PINNED)).is_cloud_only());
    }

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
