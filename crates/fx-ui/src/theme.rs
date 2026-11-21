//! Visual foundation (task T1.1): Fluent 2 tokens for light and dark themes,
//! spacing in multiples of 4 px, Segoe UI Variable, and list densities
//! (docs/PRODUCT.md, "Visual").

use gpui::{Rgba, WindowAppearance};

/// A color as `0xRRGGBBAA`.
pub type Color = u32;

pub fn rgba(c: Color) -> Rgba {
    gpui::rgba(c)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn from_appearance(a: WindowAppearance) -> Self {
        match a {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Mode::Dark,
            WindowAppearance::Light | WindowAppearance::VibrantLight => Mode::Light,
        }
    }

    /// `FX_THEME=light|dark` forces a theme (used for screenshots).
    pub fn from_env() -> Option<Self> {
        match std::env::var("FX_THEME").ok()?.as_str() {
            "light" => Some(Mode::Light),
            "dark" => Some(Mode::Dark),
            _ => None,
        }
    }
}

/// Row height of the list (docs/PRODUCT.md: 24 / 32 / 40 px).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Compact,
    Standard,
    Comfortable,
}

impl Density {
    pub const fn row_height(self) -> f32 {
        match self {
            Density::Compact => 24.0,
            Density::Standard => 32.0,
            Density::Comfortable => 40.0,
        }
    }

    /// `FX_DENSITY=compact|standard|comfortable` (used for screenshots).
    pub fn from_env() -> Option<Self> {
        match std::env::var("FX_DENSITY").ok()?.as_str() {
            "compact" => Some(Density::Compact),
            "standard" => Some(Density::Standard),
            "comfortable" => Some(Density::Comfortable),
            _ => None,
        }
    }
}

/// Spacing unit: every margin, padding and gap is a multiple of 4 px.
pub const fn space(units: u32) -> f32 {
    (units * 4) as f32
}

pub const BODY_SIZE: f32 = 14.0;
pub const CAPTION_SIZE: f32 = 12.0;
pub const RADIUS: f32 = 4.0;

/// Font families in order of preference (Segoe UI Variable ships with
/// Windows 11; Windows 10 has Segoe UI).
pub const FONT_FAMILIES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];

/// Icon fonts (Windows 11, then Windows 10). Glyphs below exist in both.
pub const ICON_FAMILIES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
pub const ICON_BACK: &str = "\u{E72B}";
pub const ICON_FORWARD: &str = "\u{E72A}";
pub const ICON_UP: &str = "\u{E74A}";
pub const ICON_CHEVRON: &str = "\u{E76C}";
pub const ICON_ADD: &str = "\u{E710}";
pub const ICON_CLOSE: &str = "\u{E711}";
pub const ICON_MENU: &str = "\u{E700}";
pub const ICON_PIN: &str = "\u{E718}";
pub const ICON_UNPIN: &str = "\u{E77A}";
pub const ICON_HOME: &str = "\u{E80F}";
pub const ICON_DESKTOP: &str = "\u{E7F4}";
pub const ICON_DOWNLOADS: &str = "\u{E896}";
pub const ICON_DOCUMENTS: &str = "\u{E8A5}";
pub const ICON_PICTURES: &str = "\u{EB9F}";
pub const ICON_MUSIC: &str = "\u{E8D6}";
pub const ICON_VIDEOS: &str = "\u{E714}";
pub const ICON_FOLDER: &str = "\u{E8B7}";
pub const ICON_DRIVE: &str = "\u{EDA2}";
pub const ICON_USB: &str = "\u{E88E}";
pub const ICON_NETWORK_DRIVE: &str = "\u{E8CE}";
pub const ICON_DISC: &str = "\u{E958}";
pub const ICON_GRID: &str = "\u{E80A}";
pub const ICON_LIST: &str = "\u{E8FD}";
pub const ICON_PAUSE: &str = "\u{E769}";
pub const ICON_PLAY: &str = "\u{E768}";

pub fn pick_icon_font(available: &[String]) -> &'static str {
    ICON_FAMILIES
        .iter()
        .find(|f| available.iter().any(|a| a == *f))
        .copied()
        .unwrap_or(ICON_FAMILIES[1])
}

pub fn pick_font(available: &[String]) -> &'static str {
    FONT_FAMILIES
        .iter()
        .find(|f| available.iter().any(|a| a == *f))
        .copied()
        .unwrap_or(FONT_FAMILIES[1])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub mode: Mode,
    /// Approximate Mica tint; the window background where Mica is unavailable
    /// and the reference for contrast checks.
    pub base: Color,
    /// Fill of the content area (list) on top of the backdrop.
    pub layer: Color,
    pub text: Color,
    pub text_secondary: Color,
    pub accent: Color,
    pub hover: Color,
    pub selected: Color,
    pub divider: Color,
    pub focus: Color,
    pub thumb: Color,
    pub thumb_hover: Color,
}

impl Theme {
    pub const LIGHT: Theme = Theme {
        mode: Mode::Light,
        base: 0xF3F3F3FF,
        layer: 0xFFFFFF80,
        text: 0x1A1A1AFF,
        text_secondary: 0x5C5C5CFF,
        accent: 0x005FB8FF,
        hover: 0x0000000A,
        selected: 0x005FB81F,
        divider: 0x00000014,
        focus: 0x1A1A1AFF,
        thumb: 0x0000005C,
        thumb_hover: 0x0000008A,
    };

    pub const DARK: Theme = Theme {
        mode: Mode::Dark,
        base: 0x202020FF,
        layer: 0x3A3A3A4D,
        text: 0xFFFFFFFF,
        text_secondary: 0xC8C8C8FF,
        accent: 0x60CDFFFF,
        hover: 0xFFFFFF0F,
        selected: 0x60CDFF26,
        divider: 0xFFFFFF14,
        focus: 0xFFFFFFFF,
        thumb: 0xFFFFFF5C,
        thumb_hover: 0xFFFFFF8A,
    };

    pub fn for_mode(mode: Mode) -> Theme {
        match mode {
            Mode::Light => Theme::LIGHT,
            Mode::Dark => Theme::DARK,
        }
    }
}

/// `top` composited over the opaque color `bottom` (alpha blending).
#[cfg(test)]
fn over(top: Color, bottom: Color) -> Color {
    let a = f64::from(top & 0xFF) / 255.0;
    let ch = |shift: u32| {
        let t = f64::from((top >> shift) & 0xFF);
        let b = f64::from((bottom >> shift) & 0xFF);
        ((t * a + b * (1.0 - a)).round() as u32) << shift
    };
    ch(24) | ch(16) | ch(8) | 0xFF
}

/// WCAG 2.x relative luminance of an opaque color.
#[cfg(test)]
fn luminance(c: Color) -> f64 {
    let lin = |shift: u32| {
        let v = f64::from((c >> shift) & 0xFF) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(24) + 0.7152 * lin(16) + 0.0722 * lin(8)
}

/// WCAG contrast ratio between two opaque colors (1.0 to 21.0).
#[cfg(test)]
fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_reference_values() {
        assert!((contrast(0x000000FF, 0xFFFFFFFF) - 21.0).abs() < 0.01);
        assert!((contrast(0x777777FF, 0xFFFFFFFF) - 4.48).abs() < 0.01);
        assert_eq!(over(0x00000000, 0x123456FF), 0x123456FF);
        assert_eq!(over(0xFF0000FF, 0x123456FF), 0xFF0000FF);
        assert_eq!(over(0xFFFFFF80, 0x000000FF), 0x808080FF);
    }

    /// Every text color on every background it can appear on (backdrop, list
    /// layer, hovered row, selected row) meets WCAG AA (4.5:1).
    #[test]
    fn text_contrast_is_at_least_4_5_in_both_themes() {
        for t in [Theme::LIGHT, Theme::DARK] {
            let layer = over(t.layer, t.base);
            let backgrounds = [t.base, layer, over(t.hover, layer), over(t.selected, layer)];
            for bg in backgrounds {
                for (name, fg) in [("text", t.text), ("secondary", t.text_secondary)] {
                    let c = contrast(fg, bg);
                    assert!(c >= 4.5, "{:?} {name} on {bg:08X}: {c:.2}", t.mode);
                }
            }
            // The focus ring and the selection indicator must stand out (3:1, WCAG 1.4.11).
            assert!(contrast(t.focus, layer) >= 3.0, "{:?} focus", t.mode);
            assert!(contrast(t.accent, layer) >= 3.0, "{:?} accent", t.mode);
        }
    }

    #[test]
    fn densities_and_spacing() {
        assert_eq!(Density::Compact.row_height(), 24.0);
        assert_eq!(Density::Standard.row_height(), 32.0);
        assert_eq!(Density::Comfortable.row_height(), 40.0);
        assert_eq!(space(3), 12.0);
        for d in [Density::Compact, Density::Standard, Density::Comfortable] {
            assert_eq!(d.row_height() % 4.0, 0.0);
        }
    }

    #[test]
    fn font_fallback() {
        let win11 = vec![
            "Arial".to_owned(),
            "Segoe UI Variable Text".to_owned(),
            "Segoe UI".to_owned(),
        ];
        assert_eq!(pick_font(&win11), "Segoe UI Variable Text");
        assert_eq!(pick_font(&["Segoe UI".to_owned()]), "Segoe UI");
        assert_eq!(pick_font(&[]), "Segoe UI");
    }
}
