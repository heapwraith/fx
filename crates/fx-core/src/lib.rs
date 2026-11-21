//! fx-core: domain model, sorting, filtering, commands and undo history.
//!
//! Rules (see docs/ARCHITECTURE.md): no Windows APIs, no I/O, no GPUI.
//! Everything here must be unit-testable on any machine.
#![forbid(unsafe_code)]

pub mod complete;
mod entry;
pub mod format;
mod history;
mod list;
pub mod natural;
mod pinned;
mod selection;
mod tabs;

pub use entry::{Attributes, Entry};
pub use history::History;
pub use list::{Direction, FileList, SortKey};
pub use pinned::Pinned;
pub use selection::{MoveMode, Selection, Step};
pub use tabs::Tabs;
