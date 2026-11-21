//! fx-shell: Windows Shell (COM) bridge: context menu, thumbnails, drag and drop.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]` (D-012).

pub mod icons;
pub mod launch;
pub mod places;
pub mod thumbs;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    A11y, Instance, LatestTree, claim_instance, drives, known_folder, known_folders, launch,
    process_start_time, start_icon_service, start_thumb_service, supports_mica, watch_drives,
    windows_build,
};
