//! High contrast colors from the system (task T3.8), platform-neutral type.

/// The system colors fx uses in high contrast, as RGBA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemColors {
    pub window: u32,
    pub window_text: u32,
    pub highlight: u32,
    pub highlight_text: u32,
    pub gray_text: u32,
    pub hot_light: u32,
    pub button_face: u32,
}
