//! fx-index: NTFS MFT/USN reader, in-memory index and fuzzy search.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! The index itself is platform-neutral; reading NTFS lives in `src/windows/`
//! behind `#[cfg(windows)]` (D-012).

mod index;
mod search;

pub use index::{FileId, Index, IndexFull, Record};
pub use search::{Hit, MAX_RESULTS, search};

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{JournalWatcher, NEEDS_ADMIN, ReadStats, read_volume, read_volume_with_stats};
