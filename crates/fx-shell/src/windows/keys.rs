//! Physical digit keys (T1.7 shortcuts): with Shift held, GPUI reports the
//! shifted character of a digit ("@" for 2 on US and ABNT2 layouts, a dead
//! key for 6 on ABNT2) and drops Shift, so Ctrl+Shift+2 cannot be matched
//! from the keystroke. The key state of the UI thread still says which
//! digit is down.

use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_SHIFT};

/// The digit (top row) held down together with Shift, if any. Reads only
/// this thread's key state (no wait).
pub fn shifted_digit() -> Option<u8> {
    // SAFETY: plain call reading this thread's keyboard state.
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    if !down(VK_SHIFT.0) {
        return None;
    }
    (0..=9u8).find(|d| down(u16::from(b'0' + d)))
}
