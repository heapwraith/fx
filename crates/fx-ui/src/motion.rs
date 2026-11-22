//! Transitions (task T4.2): overlays (palette, menus, search, Quick Look,
//! dialogs) fade in over 120 ms, never more than 150 ms. A transition never
//! delays a frame or a key: the overlay is there, interactive, from the
//! first frame, only its opacity eases in. With "Animation effects" off in
//! Windows nothing animates.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Animation, AnimationExt, AnyElement, IntoElement, Styled};

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

#[cfg(test)]
mod tests {
    use super::*;

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
