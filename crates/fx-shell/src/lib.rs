//! fx-shell: Windows Shell (COM) bridge: context menu, thumbnails, drag and drop.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]` (D-012).

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::process_start_time;
