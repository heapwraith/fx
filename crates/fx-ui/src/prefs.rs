//! Small on/off preferences read on the UI thread: set at startup from
//! `settings.toml`, switched from the palette or Settings, and saved with the
//! session only when switched. One type for all of them (review, D-066: the
//! same atomics and lock were copied four times).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// One preference.
pub struct Toggle {
    on: AtomicBool,
    chosen: Mutex<Option<bool>>,
}

impl Toggle {
    pub const fn new(default: bool) -> Self {
        Self {
            on: AtomicBool::new(default),
            chosen: Mutex::new(None),
        }
    }

    pub fn get(&self) -> bool {
        self.on.load(Ordering::Relaxed)
    }

    /// From `settings.toml`, at startup (not saved again).
    pub fn set(&self, on: bool) {
        self.on.store(on, Ordering::Relaxed);
    }

    /// On/off by the user. Returns the new state.
    pub fn switch(&self) -> bool {
        let on = !self.get();
        self.set(on);
        if let Ok(mut c) = self.chosen.lock() {
            *c = Some(on);
        }
        on
    }

    /// What to save, if the user switched it this session.
    pub fn chosen(&self) -> Option<bool> {
        self.chosen.lock().ok().and_then(|c| *c)
    }
}

/// Ask before sending items to the Recycle Bin (`confirmar_lixeira`).
pub static CONFIRM_RECYCLE: Toggle = Toggle::new(true);
/// Reopen the last windows and tabs when fx starts (`restaurar_sessao`).
pub static RESTORE_SESSION: Toggle = Toggle::new(true);
/// Check for updates once a day and download them (`verificar_atualizacoes`).
pub static UPDATES: Toggle = Toggle::new(true);
/// Write crash reports (`relatorio_falhas`).
pub static CRASH_REPORTS: Toggle = Toggle::new(true);

#[cfg(test)]
mod tests {
    use super::Toggle;

    #[test]
    fn only_a_switch_is_saved() {
        let t = Toggle::new(true);
        t.set(false);
        assert!(!t.get());
        assert_eq!(t.chosen(), None, "from settings: nothing to save");
        assert!(t.switch());
        assert_eq!(t.chosen(), Some(true));
    }
}
