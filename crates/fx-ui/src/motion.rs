//! Transitions (task T4.2): overlays (palette, menus, search, Quick Look,
//! dialogs) fade in over 120 ms, never more than 150 ms. A transition never
//! delays a frame or a key: the overlay is there, interactive, from the
//! first frame, only its opacity eases in. With "Animation effects" off in
//! Windows nothing animates.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, IntoElement, ParentElement, SharedString, Styled, div, px,
};

/// Every fx transition is at most this long (fx-design §1).
#[cfg(test)]
const MAX: Duration = Duration::from_millis(150);
pub const ENTER: Duration = Duration::from_millis(120);

static ON: AtomicBool = AtomicBool::new(true);

/// Reads the Windows setting (at startup).
pub fn init() {
    ON.store(fx_shell::animations_enabled(), Ordering::Relaxed);
}

/// The entry duration, or none when animations are off.
fn entry(on: bool) -> Option<Duration> {
    on.then_some(ENTER)
}

/// `element` fading in when it first appears (`id` keeps the animation
/// state while it stays on screen, so typing in it does not restart it).
pub fn enter<E: IntoElement + Styled + 'static>(id: &'static str, element: E) -> AnyElement {
    match entry(ON.load(Ordering::Relaxed)) {
        Some(d) => element
            .with_animation(
                id,
                Animation::new(d).with_easing(gpui::ease_out_quint()),
                // Starts visible enough to read at once.
                |e, t| e.opacity(0.35 + 0.65 * t),
            )
            .into_any_element(),
        None => element.into_any_element(),
    }
}

/// How long a switch takes to slide (Apple's switches: about 0.2 s).
pub const TOGGLE: Duration = Duration::from_millis(200);
const TRACK_W: f32 = 40.0;
const TRACK_H: f32 = 22.0;
const KNOB: f32 = 18.0;

/// Colors of a switch: the track when on and off, and the knob.
#[derive(Clone, Copy)]
pub struct SwitchColors {
    pub on: u32,
    pub off: u32,
    pub knob: u32,
    /// High contrast: an outlined track instead of a gray fill.
    pub outline: Option<u32>,
}

/// `a` to `b` by `t` (0..1), channel by channel (`0xRRGGBBAA`).
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, shift: u32| ((c >> shift) & 0xFF) as f32;
    [24, 16, 8, 0].iter().fold(0u32, |acc, &sh| {
        let v = ch(a, sh) + (ch(b, sh) - ch(a, sh)) * t;
        acc | ((v.round().clamp(0.0, 255.0) as u32) << sh)
    })
}

/// The switch drawn at `t` (0: off, 1: on): the knob's place and the
/// track's color follow it.
fn switch_at(c: SwitchColors, t: f32) -> gpui::Div {
    let travel = TRACK_W - KNOB - 4.0;
    let mut track = div()
        .relative()
        .w(px(TRACK_W))
        .h(px(TRACK_H))
        .rounded_full()
        .bg(crate::theme::rgba(mix(c.off, c.on, t)));
    if let Some(line) = c.outline {
        track = track.border_1().border_color(crate::theme::rgba(line));
    }
    track.child(
        div()
            .absolute()
            .top(px(
                (TRACK_H - KNOB) / 2.0 - if c.outline.is_some() { 1.0 } else { 0.0 }
            ))
            .left(px(2.0 + travel * t))
            .size(px(KNOB))
            .rounded_full()
            .bg(crate::theme::rgba(c.knob))
            .shadow_sm(),
    )
}

/// An on/off switch. `changed`: it was just flipped, so it slides there
/// (the id carries the state: a flip starts a new animation, a redraw
/// does not); otherwise, and with Windows animations off, it just is.
pub fn switch(id: SharedString, on: bool, changed: bool, c: SwitchColors) -> AnyElement {
    let end = if on { 1.0 } else { 0.0 };
    if !changed || !ON.load(Ordering::Relaxed) {
        return switch_at(c, end).into_any_element();
    }
    div()
        .w(px(TRACK_W))
        .h(px(TRACK_H))
        .with_animation(
            SharedString::from(format!("{id}-{on}")),
            Animation::new(TOGGLE).with_easing(gpui::ease_out_quint()),
            move |el, t| {
                let at = if on { t } else { 1.0 - t };
                el.child(switch_at(c, at))
            },
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_blend_channel_by_channel() {
        assert_eq!(mix(0x000000FF, 0xFFFFFFFF, 0.0), 0x000000FF);
        assert_eq!(mix(0x000000FF, 0xFFFFFFFF, 1.0), 0xFFFFFFFF);
        assert_eq!(mix(0x00000000, 0xC8640AFF, 0.5), 0x64320580);
    }

    #[test]
    fn short_and_off_when_windows_says_so() {
        const { assert!(ENTER.as_millis() <= MAX.as_millis()) };
        assert_eq!(entry(true), Some(ENTER));
        assert_eq!(
            entry(false),
            None,
            "animation effects off: nothing animates"
        );
    }
}
