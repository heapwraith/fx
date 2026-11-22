//! The updater (task T4.1). Once a day, on a worker thread, fx asks GitHub
//! for its releases; a newer one is announced in the status bar and the
//! palette's "Atualizar o fx" installs it: the installer for this
//! architecture is downloaded to `%TEMP%`, checked against the release's
//! `SHA256SUMS.txt` (and its signature, when this fx is signed), then run
//! silently (`fx-setup --silent`, T4.3): the files are replaced next to the
//! running fx and the new version starts with the next fx. Nothing is downloaded or installed without the user
//! asking; `verificar_atualizacoes = false` turns the daily check off.

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use fx_core::update::{Update, Version};

const RELEASES: &str = "https://api.github.com/repos/heapwraith/fx/releases?per_page=20";
const MAX_JSON: usize = 4 << 20;
const MAX_SUMS: usize = 64 << 10;
const MAX_SETUP: usize = 200 << 20;

static ENABLED: AtomicBool = AtomicBool::new(true);
static CHOSEN: Mutex<Option<bool>> = Mutex::new(None);
static AVAILABLE: Mutex<Option<Update>> = Mutex::new(None);

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Palette: daily check on/off. Returns the new state.
pub fn switch() -> bool {
    let on = !enabled();
    set_enabled(on);
    if let Ok(mut c) = CHOSEN.lock() {
        *c = Some(on);
    }
    on
}

/// What to save in `verificar_atualizacoes`, if switched this session.
pub fn chosen() -> Option<bool> {
    CHOSEN.lock().ok().and_then(|c| *c)
}

/// The update found by the last check, if any.
pub fn available() -> Option<Update> {
    AVAILABLE.lock().ok().and_then(|a| a.clone())
}

fn current() -> Option<Version> {
    Version::parse(crate::VERSION)
}

fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn stamp_file() -> Option<PathBuf> {
    crate::store::state_path().map(|p| p.with_file_name("update-check.txt"))
}

/// Whether the daily check should run now (reads a tiny file: call it on a
/// worker thread).
pub fn due() -> bool {
    let last = stamp_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| t.trim().parse().ok());
    enabled() && fx_core::update::due(last, now_secs())
}

/// Asks GitHub for a newer version (blocking; worker thread). Remembers it
/// for the palette and records the time of the check.
pub fn check() -> io::Result<Option<Update>> {
    let current = current().ok_or_else(|| io::Error::other("unknown version"))?;
    let body = fx_shell::net::http_get(RELEASES, MAX_JSON)?;
    if let Some(f) = stamp_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, now_secs().to_string());
    }
    let text = String::from_utf8_lossy(&body);
    let found = fx_core::update::pick(&text, &current, arch());
    if let Ok(mut a) = AVAILABLE.lock() {
        a.clone_from(&found);
    }
    Ok(found)
}

/// Downloads and checks the installer, then starts it (blocking; worker
/// thread). On success the installer is running and will close fx.
pub fn install(update: &Update) -> io::Result<()> {
    let sums =
        String::from_utf8_lossy(&fx_shell::net::http_get(&update.sums_url, MAX_SUMS)?).into_owned();
    let setup = fx_shell::net::http_get(&update.setup_url, MAX_SETUP)?;
    let hash = fx_shell::net::sha256(&setup)?;
    if !fx_core::update::verify(&sums, &update.setup_name, &hash) {
        return Err(io::Error::other(fx_core::tr!(
            "o instalador baixado não confere com o SHA256SUMS do release",
            "the downloaded installer does not match the release's SHA256SUMS"
        )));
    }
    // Only a plain file name from the release: no folders.
    if update.setup_name.contains(['/', '\\']) || update.setup_name.contains("..") {
        return Err(io::Error::other("unexpected installer name"));
    }
    let dir = std::env::temp_dir().join("fx-update");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&update.setup_name);
    std::fs::write(&path, &setup)?;
    // A signed fx only installs a signed installer.
    let signed_now = std::env::current_exe()
        .map(|exe| fx_shell::net::is_signed(&exe))
        .unwrap_or(false);
    if signed_now && !fx_shell::net::is_signed(&path) {
        let _ = std::fs::remove_file(&path);
        return Err(io::Error::other(fx_core::tr!(
            "o instalador baixado não está assinado",
            "the downloaded installer is not signed"
        )));
    }
    // fx-setup (T4.3) installs next to the running fx (its exe is renamed
    // aside, not deleted): the new version starts with the next fx.
    let status = std::process::Command::new(&path).arg("--silent").status()?;
    let _ = std::fs::remove_file(&path);
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(fx_core::tr!(
            "o instalador não terminou",
            "the installer did not finish"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turned_off_never_checks() {
        set_enabled(false);
        assert!(!due(), "off: the daily check never runs");
        set_enabled(true);
        assert_eq!(arch(), "x64");
        assert!(
            current().is_some(),
            "the build version parses: {}",
            crate::VERSION
        );
    }
}
