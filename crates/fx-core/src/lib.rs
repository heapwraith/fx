//! fx-core: domain model, sorting, filtering, commands and undo history.
//!
//! Rules (see docs/ARCHITECTURE.md): no Windows APIs, no I/O, no GPUI.
//! Everything here must be unit-testable on any machine.
#![forbid(unsafe_code)]

pub mod batch_rename;
pub mod complete;
pub mod crash;
mod entry;
pub mod format;
pub mod fuzzy;
pub mod highlight;
mod history;
mod list;
pub mod locale;
pub mod markdown;
pub mod miller;
pub mod natural;
pub mod package;
mod pinned;
pub mod preview;
pub mod search_filter;
mod selection;
pub mod shell_arg;
mod tabs;
pub mod tags;
pub mod temp_name;
mod undo;
pub mod update;
pub mod vt;
pub mod zip_list;

pub use entry::{Attributes, Change, CloudState, Entry};
pub use history::History;
pub use list::{Applied, Direction, FileList, SortKey, next_sort, type_label};
pub use pinned::Pinned;
pub use selection::{MoveMode, Selection, Step};
pub use tabs::Tabs;
pub use undo::UndoStack;
