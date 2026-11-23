//! fx-shell: Windows Shell (COM) bridge: context menu, thumbnails, drag and drop.
//!
//! See docs/ARCHITECTURE.md. Every `unsafe` block needs a `// SAFETY:` comment.
//! Platform code lives in `src/windows/` behind `#[cfg(windows)]` (D-012).

pub mod clipboard;
pub mod contrast;
pub mod icons;
pub mod launch;
pub mod menu;
pub mod places;
pub mod preview;
pub mod thumbs;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    A11y, DragResult, Instance, LatestTree, animations_enabled, claim_instance, clear_clipboard,
    clipboard_files, drag_files, drives, drop_keys, is_elevated, item_properties, known_folder,
    known_folders, launch, preview_image, process_start_time, set_clipboard_files, shifted_digit,
    show_context_menu, start_icon_service, start_thumb_service, supports_mica,
    system_language_is_portuguese, utc_offset_at, watch_drives, window_id, windows_build,
};
#[cfg(windows)]
pub use windows::{conpty, contrast::high_contrast, default_manager, install, net, preview_host};
