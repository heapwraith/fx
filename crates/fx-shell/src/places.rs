//! Places shown in the sidebar (task T1.5): known user folders and drives.
//! Platform-neutral types; the Windows implementation is in
//! `windows/places.rs`.

use std::path::PathBuf;

/// A user folder the system knows about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnownFolder {
    Home,
    Desktop,
    Downloads,
    Documents,
    Pictures,
    Music,
    Videos,
}

impl KnownFolder {
    pub const ALL: [KnownFolder; 7] = [
        KnownFolder::Home,
        KnownFolder::Desktop,
        KnownFolder::Downloads,
        KnownFolder::Documents,
        KnownFolder::Pictures,
        KnownFolder::Music,
        KnownFolder::Videos,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// `C:\`.
    pub root: PathBuf,
    /// Volume label; empty when the volume has none or cannot be read.
    pub label: String,
    pub kind: DriveKind,
    /// Free and total bytes, when the volume can be read.
    pub space: Option<(u64, u64)>,
}
