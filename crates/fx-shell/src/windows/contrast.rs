//! Windows high contrast (task T3.8): whether it is on and its colors.

use windows::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT,
    COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

pub use crate::contrast::SystemColors;

fn color(index: SYS_COLOR_INDEX) -> u32 {
    // SAFETY: plain call.
    let c = unsafe { GetSysColor(index) }; // 0x00BBGGRR
    let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
    (r << 24) | (g << 16) | (b << 8) | 0xFF
}

/// The high contrast colors, when high contrast is on.
pub fn high_contrast() -> Option<SystemColors> {
    let mut hc = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: `hc` is a writable HIGHCONTRASTW whose size is given.
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some((&raw mut hc).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    if hc.dwFlags.0 & HCF_HIGHCONTRASTON.0 == 0 {
        return None;
    }
    Some(SystemColors {
        window: color(COLOR_WINDOW),
        window_text: color(COLOR_WINDOWTEXT),
        highlight: color(COLOR_HIGHLIGHT),
        highlight_text: color(COLOR_HIGHLIGHTTEXT),
        gray_text: color(COLOR_GRAYTEXT),
        hot_light: color(COLOR_HOTLIGHT),
        button_face: color(COLOR_BTNFACE),
    })
}
