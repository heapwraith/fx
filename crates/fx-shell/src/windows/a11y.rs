//! Accessibility (task T1.10, D-008): UI Automation for a GPUI window through
//! `accesskit_windows::SubclassingAdapter`, which answers `WM_GETOBJECT` by
//! subclassing the window (GPUI itself has no UIA support).
//!
//! The adapter must be attached while the window is still hidden. Screen
//! readers ask for the tree from inside the window procedure, where the app's
//! state cannot be borrowed, so the UI keeps the latest full tree in a shared
//! cell and the activation handler hands out a copy of it.

use std::cell::RefCell;
use std::num::NonZeroIsize;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, TreeUpdate};
use accesskit_windows::{HWND, SubclassingAdapter};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The latest full tree of a window, kept by the UI.
pub type LatestTree = Rc<RefCell<Option<TreeUpdate>>>;

struct Activation(LatestTree);

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.try_borrow().ok().and_then(|t| t.clone())
    }
}

struct Actions(Sender<ActionRequest>);

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        let _ = self.0.send(request);
    }
}

/// UI Automation for one window.
pub struct A11y {
    adapter: SubclassingAdapter,
}

impl std::fmt::Debug for A11y {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("A11y")
    }
}

impl A11y {
    /// Attaches to a window that is not shown yet. Returns the adapter and
    /// the channel of actions requested by assistive technology (focus, click).
    /// `None` if the handle is not a Win32 window or the window is visible.
    pub fn attach(
        window: &impl HasWindowHandle,
        latest: LatestTree,
    ) -> Option<(Self, Receiver<ActionRequest>)> {
        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(win32) = handle.as_raw() else {
            return None;
        };
        let hwnd = HWND(NonZeroIsize::get(win32.hwnd) as *mut _);
        // SubclassingAdapter::new panics on a visible window: check first.
        // SAFETY: `hwnd` is the live window GPUI just created on this thread.
        let visible = unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd) };
        if visible.as_bool() {
            return None;
        }
        let (tx, rx) = channel();
        let adapter = SubclassingAdapter::new(hwnd, Activation(latest), Actions(tx));
        Some((Self { adapter }, rx))
    }

    /// Sends a tree update if a screen reader is listening; `make` is not
    /// even called otherwise, so this costs nothing without one.
    pub fn update(&mut self, make: impl FnOnce() -> TreeUpdate) {
        if let Some(events) = self.adapter.update_if_active(make) {
            events.raise();
        }
    }
}
