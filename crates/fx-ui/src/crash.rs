//! Crash reports (task T3.10): a panic leaves a report with no personal data
//! (`fx_core::crash`) in `%LOCALAPPDATA%\fx\falhas`. On the next start fx
//! says so, and the palette's "Relatar a última falha" opens a GitHub issue
//! with the report filled in, in the browser, for the user to read and
//! send; nothing is sent by fx. `relatorio_falhas = false` in
//! `settings.toml` (or the palette) turns it off: nothing is written.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

const ISSUES: &str = "https://github.com/heapwraith/fx/issues/new";
/// Browsers and GitHub accept long URLs, but not unbounded ones.
const MAX_BODY: usize = 6000;

fn dir() -> Option<PathBuf> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        std::env::var_os("FX_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("fx")))
            .map(|d| d.join("falhas"))
    })
    .clone()
}

/// Installs the panic hook (first thing in `main`).
pub fn install() {
    let _ = dir(); // resolved now, not inside the hook
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = dir() {
            let message = info
                .payload()
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_default();
            let (file, line) = info.location().map_or(("?", 0), |l| (l.file(), l.line()));
            let trace = std::backtrace::Backtrace::force_capture().to_string();
            let user = std::env::var("USERNAME").ok();
            let report = fx_core::crash::report(&fx_core::crash::Panic {
                version: crate::VERSION,
                message: &message,
                file,
                line,
                backtrace: &trace,
                user: user.as_deref(),
            });
            let _ = write(&dir, crate::prefs::CRASH_REPORTS.get(), &report);
        }
        previous(info);
    }));
}

static NOTICE: AtomicBool = AtomicBool::new(false);

/// At startup: a report from the last run is waiting.
pub fn set_notice(on: bool) {
    NOTICE.store(on, Ordering::Relaxed);
}

/// The first window shows the notice; later ones don't.
pub fn take_notice() -> bool {
    NOTICE.swap(false, Ordering::Relaxed)
}

/// Writes `report` as a new file in `dir`, unless turned off.
fn write(dir: &Path, enabled: bool, report: &str) -> Option<PathBuf> {
    if !enabled {
        return None;
    }
    std::fs::create_dir_all(dir).ok()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // A second panic in the same second must not replace the first report.
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("falha-{stamp}-{}-{n}.txt", std::process::id()));
    std::fs::write(&path, report).ok()?;
    Some(path)
}

/// The newest report not yet looked at (read at startup, before the event
/// loop).
pub fn pending() -> Option<PathBuf> {
    newest(&dir()?)
}

fn newest(dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .max()
}

/// The issue URL for a report (title and body filled in).
pub fn issue_url(report: &str) -> String {
    let first = report.lines().nth(2).unwrap_or("falha");
    let place = first.trim_start_matches("location: ");
    let title = fx_core::trf!("Falha: {place}", "Crash: {place}");
    let mut body = fx_core::trf!(
        "O fx fechou com uma falha. Relatório gerado pelo próprio fx (sem caminhos nem nomes):\n\n```\n{report}```\n\nO que eu estava fazendo:\n",
        "fx closed with a crash. Report made by fx itself (no paths or names):\n\n```\n{report}```\n\nWhat I was doing:\n"
    );
    if body.len() > MAX_BODY {
        let mut cut = MAX_BODY;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
    }
    format!(
        "{ISSUES}?labels=bug&title={}&body={}",
        encode(&title),
        encode(&body)
    )
}

fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// After the user opened the issue page: the report is kept, renamed so it
/// is not offered again.
pub fn mark_seen(path: &Path) {
    let _ = std::fs::rename(path, path.with_extension("visto"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turned_off_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("falhas");
        assert_eq!(write(&dir, false, "x"), None);
        assert!(!dir.exists(), "not even the folder");
        let p = write(&dir, true, "relatório").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "relatório");
        assert_eq!(newest(&dir), Some(p.clone()));
        mark_seen(&p);
        assert_eq!(newest(&dir), None, "seen: not offered again");
    }

    #[test]
    fn issue_link_is_encoded_and_bounded() {
        let report = "fx 0.2.0\nmodule: fx_ui\nlocation: fx-ui/src/a.rs:3\nmessage: x & y\n";
        let url = issue_url(report);
        assert!(url.starts_with("https://github.com/heapwraith/fx/issues/new?labels=bug&title=Falha%3A%20fx-ui%2Fsrc%2Fa.rs%3A3&body="));
        assert!(!url.contains(' ') && !url.contains('&') || url.matches('&').count() == 2);
        let long = "a".repeat(20_000);
        assert!(issue_url(&long).len() < 3 * MAX_BODY + 300);
    }
}
