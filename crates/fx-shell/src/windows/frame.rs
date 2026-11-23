//! The window frame (title bar and border drawn by Windows) in the app's
//! theme, not only the system one: the app can be dark while Windows is
//! light (D-074).

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::core::BOOL;

/// Dark or light frame for the window `hwnd` (from [`super::window_id`]).
/// Best effort: older Windows without the attribute keep their frame.
pub fn set_dark_frame(hwnd: isize, dark: bool) {
    let value = BOOL::from(dark);
    // SAFETY: `hwnd` is a live top-level window of this process (taken from
    // GPUI's window handle on the UI thread); the attribute reads a BOOL
    // from `value`, which outlives the call, with its exact size.
    let _ = unsafe {
        DwmSetWindowAttribute(
            HWND(hwnd as *mut core::ffi::c_void),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const value).cast(),
            size_of::<BOOL>() as u32,
        )
    };
}
