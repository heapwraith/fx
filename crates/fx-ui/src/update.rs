//! The updater (tasks T4.1 and T4.4, in the manner of Zed). Once a day, on a
//! worker thread, fx asks GitHub for its releases; a newer version is
//! downloaded by itself, checked against the release's `SHA256SUMS.txt`
//! (and its signature, when this fx is signed) and kept ready in
//! `%LOCALAPPDATA%\fx\update`. When fx closes, the new fx-setup (T4.3)
//! waits for it to end and replaces the files; "Reiniciar para atualizar"
//! does the same and opens fx again. `verificar_atualizacoes = false` turns
//! all of it off; the palette's "Atualizar o fx" still works on request.

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

/// Asks GitHub for a newer version (blocking; worker thread) and records
/// the time of the check.
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
    Ok(found)
}

fn update_dir() -> Option<PathBuf> {
    crate::store::state_path().map(|p| p.with_file_name("update"))
}

const READY: &str = "ready.txt";

/// Downloads and checks the installer of `update` and keeps it ready
/// (blocking; worker thread). It is applied when fx closes (T4.4).
pub fn prepare(update: &Update) -> io::Result<()> {
    // Only a plain file name from the release: no folders.
    if !fx_core::package::is_plain_name(&update.setup_name) {
        return Err(io::Error::other("unexpected installer name"));
    }
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
    let dir = update_dir().ok_or_else(|| io::Error::other("no data folder"))?;
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
    std::fs::write(
        dir.join(READY),
        format!("{}\n{}\n", update.version, update.setup_name),
    )
}

/// The version ready to apply and its installer. A leftover from an update
/// already applied (or a damaged note) is cleaned up.
pub fn ready() -> Option<(Version, PathBuf)> {
    let dir = update_dir()?;
    let note = std::fs::read_to_string(dir.join(READY)).ok();
    let parsed = note.as_deref().and_then(|text| {
        let mut lines = text.lines();
        let version = Version::parse(lines.next()?)?;
        let name = lines.next()?.trim().to_owned();
        fx_core::package::is_plain_name(&name).then(|| (version, dir.join(name)))
    });
    match parsed {
        Some((v, path)) if current().is_some_and(|c| v > c) && path.exists() => Some((v, path)),
        _ => {
            // Nothing (valid) to apply: an old installer or note goes.
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            None
        }
    }
}

/// Starts the update helper for the version ready, if any: the new
/// fx-setup waits for this process to end, replaces the files and, with
/// `relaunch`, opens fx again. Returns whether it started. Called when fx
/// quits (and by "Reiniciar para atualizar").
pub fn apply(relaunch: bool) -> bool {
    let Some((_, setup)) = ready() else {
        return false;
    };
    let pid = std::process::id().to_string();
    let mut cmd = std::process::Command::new(&setup);
    cmd.args(["--silent", "--wait", &pid]);
    if relaunch {
        cmd.arg("--relaunch");
    }
    let started = cmd.spawn().is_ok();
    if started && let Some(dir) = update_dir() {
        // Applied once; the installer itself is removed on the next start.
        let _ = std::fs::remove_file(dir.join(READY));
    }
    started
}

/// Whether to download updates by themselves (`verificar_atualizacoes`).
pub fn automatic() -> bool {
    enabled()
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
