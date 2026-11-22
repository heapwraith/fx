//! Small preferences read on the UI thread (atomics, set at startup from
//! `settings.toml` and switched from the palette; saved with the session).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Ask before sending items to the Recycle Bin (Delete).
static CONFIRM_RECYCLE: AtomicBool = AtomicBool::new(true);
static CHOSEN_RECYCLE: Mutex<Option<bool>> = Mutex::new(None);

pub fn set_confirm_recycle(on: bool) {
    CONFIRM_RECYCLE.store(on, Ordering::Relaxed);
}

pub fn confirm_recycle() -> bool {
    CONFIRM_RECYCLE.load(Ordering::Relaxed)
}

/// Palette: on/off. Returns the new state.
pub fn switch_confirm_recycle() -> bool {
    let on = !confirm_recycle();
    set_confirm_recycle(on);
    if let Ok(mut c) = CHOSEN_RECYCLE.lock() {
        *c = Some(on);
    }
    on
}

/// What to save in `confirmar_lixeira`, if switched this session.
pub fn chosen_confirm_recycle() -> Option<bool> {
    CHOSEN_RECYCLE.lock().ok().and_then(|c| *c)
}
