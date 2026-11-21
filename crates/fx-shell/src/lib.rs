//! fx-shell: Windows Shell (COM) bridge: context menu, thumbnails, drag and drop.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]` (D-012).

pub mod clipboard;
pub mod icons;
pub mod launch;
pub mod menu;
pub mod places;
pub mod thumbs;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    A11y, DragResult, Instance, LatestTree, claim_instance, clear_clipboard, clipboard_files,
    drag_files, drives, drop_keys, known_folder, known_folders, launch, preview_png,
    process_start_time, set_clipboard_files, show_context_menu, start_icon_service,
    start_thumb_service, supports_mica, utc_offset_at, watch_drives, window_id, windows_build,
};
