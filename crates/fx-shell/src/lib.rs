//! fx-shell: Windows Shell (COM) bridge: context menu, thumbnails, drag and drop.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]` (D-012).

pub mod icons;
pub mod places;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    Instance, claim_instance, drives, known_folder, known_folders, process_start_time,
    start_icon_service, supports_mica, watch_drives, windows_build,
};
