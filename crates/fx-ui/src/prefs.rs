//! Small on/off preferences read on the UI thread: set at startup from
//! `settings.toml`, switched from the palette or Settings, and saved with the
//! session only when switched. One type for all of them (review, D-066: the
//! same atomics and lock were copied four times).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::theme::Mode;

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

/// The theme the user picked (`tema`): dark by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePick {
    Dark,
    Light,
    /// Follow the Windows light or dark setting.
    System,
}

impl ThemePick {
    /// From `settings.toml` ("escuro", "claro", "sistema"); anything else is dark.
    pub fn from_setting(s: &str) -> Self {
        match s {
            "claro" => ThemePick::Light,
            "sistema" => ThemePick::System,
            _ => ThemePick::Dark,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            ThemePick::Dark => "escuro",
            ThemePick::Light => "claro",
            ThemePick::System => "sistema",
        }
    }

    /// The mode to draw with, given the Windows one.
    pub fn mode(self, system: Mode) -> Mode {
        match self {
            ThemePick::Dark => Mode::Dark,
            ThemePick::Light => Mode::Light,
            ThemePick::System => system,
        }
    }
}

/// (current, picked by the user this session)
static THEME: Mutex<(ThemePick, Option<ThemePick>)> = Mutex::new((ThemePick::Dark, None));

pub fn theme() -> ThemePick {
    THEME.lock().map_or(ThemePick::Dark, |t| t.0)
}

/// From `settings.toml` or `FX_THEME`, at startup (not saved again).
pub fn set_theme(pick: ThemePick) {
    if let Ok(mut t) = THEME.lock() {
        t.0 = pick;
    }
}

/// Picked in Settings: applied now and saved with the session.
pub fn choose_theme(pick: ThemePick) {
    if let Ok(mut t) = THEME.lock() {
        *t = (pick, Some(pick));
    }
}

pub fn theme_chosen() -> Option<ThemePick> {
    THEME.lock().ok().and_then(|t| t.1)
}

#[cfg(test)]
mod tests {
    use super::{ThemePick, Toggle};
    use crate::theme::Mode;

    #[test]
    fn theme_pick_round_trips_and_defaults_to_dark() {
        for p in [ThemePick::Dark, ThemePick::Light, ThemePick::System] {
            assert_eq!(ThemePick::from_setting(p.setting()), p);
        }
        assert_eq!(ThemePick::from_setting(""), ThemePick::Dark);
        assert_eq!(ThemePick::Dark.mode(Mode::Light), Mode::Dark);
        assert_eq!(ThemePick::Light.mode(Mode::Dark), Mode::Light);
        assert_eq!(ThemePick::System.mode(Mode::Light), Mode::Light);
    }

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
