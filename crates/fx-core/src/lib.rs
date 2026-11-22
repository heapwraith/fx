//! fx-core: domain model, sorting, filtering, commands and undo history.
//!
//! Rules (see docs/ARCHITECTURE.md): no Windows APIs, no I/O, no GPUI.
//! Everything here must be unit-testable on any machine.
#![forbid(unsafe_code)]

pub mod complete;
mod entry;
pub mod format;
pub mod fuzzy;
mod history;
mod list;
pub mod natural;
mod pinned;
pub mod preview;
pub mod search_filter;
mod selection;
mod tabs;
mod undo;

pub use entry::{Attributes, Change, Entry};
pub use history::History;
pub use list::{Applied, Direction, FileList, SortKey, next_sort, type_label};
pub use pinned::Pinned;
pub use selection::{MoveMode, Selection, Step};
pub use tabs::Tabs;
pub use undo::UndoStack;
