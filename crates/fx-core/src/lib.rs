//! fx-core: domain model, sorting, filtering, commands and undo history.
//!
//! Rules (see docs/ARCHITECTURE.md): no Windows APIs, no I/O, no GPUI.
//! Everything here must be unit-testable on any machine.
#![forbid(unsafe_code)]

mod entry;
pub mod format;
mod list;
pub mod natural;

pub use entry::{Attributes, Entry};
pub use list::{Direction, FileList, SortKey};
