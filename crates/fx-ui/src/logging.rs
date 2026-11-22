//! Log file (review, D-066): warnings and errors of fx and GPUI go to
//! `%LOCALAPPDATA%\fx\logs\fx.log`, so a problem leaves a trace instead of
//! vanishing. The caller never waits for the disk (the UI thread logs too):
//! lines go to a writer thread. Past 1 MB the file becomes `fx.old.log` and
//! a new one starts. Nothing with `FX_NO_STATE=1` (tests, benchmarks);
//! `FX_LOG=info|debug|trace` records more. Local only, like crash reports.

use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::SystemTime;

/// Size at which the file is rotated.
const MAX_BYTES: u64 = 1 << 20;

struct Line {
    at: SystemTime,
    level: log::Level,
    target: String,
    text: String,
}

struct FileLog {
    tx: Sender<Line>,
}

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::max_level()
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            let _ = self.tx.send(Line {
                at: SystemTime::now(),
                level: r.level(),
                target: r.target().to_owned(),
                text: r.args().to_string(),
            });
        }
    }

    fn flush(&self) {}
}

/// The level from `FX_LOG` (warnings and errors by default).
fn level_from(var: Option<&str>) -> log::LevelFilter {
    match var {
        Some("trace") => log::LevelFilter::Trace,
        Some("debug") => log::LevelFilter::Debug,
        Some("info") => log::LevelFilter::Info,
        _ => log::LevelFilter::Warn,
    }
}

/// Starts the log; call once, early in `main`.
pub fn init() {
    if std::env::var_os("FX_NO_STATE").is_some_and(|v| v == "1") {
        return;
    }
    let Some(dir) = crate::store::state_path().map(|p| p.with_file_name("logs")) else {
        return;
    };
    let (tx, rx) = channel();
    let writer = std::thread::Builder::new()
        .name("fx-log".into())
        .spawn(move || write_lines(&dir, rx));
    if writer.is_ok() && log::set_boxed_logger(Box::new(FileLog { tx })).is_ok() {
        log::set_max_level(level_from(std::env::var("FX_LOG").ok().as_deref()));
    }
}

fn write_lines(dir: &Path, rx: Receiver<Line>) {
    let path = dir.join("fx.log");
    let mut file: Option<std::fs::File> = None;
    for line in rx {
        if file.is_none() {
            let _ = std::fs::create_dir_all(dir);
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
                let _ = std::fs::rename(&path, dir.join("fx.old.log"));
            }
            file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok();
        }
        let Some(f) = file.as_mut() else {
            continue;
        };
        let _ = writeln!(
            f,
            "{} {:<5} {}: {}",
            stamp(line.at),
            line.level,
            line.target,
            line.text
        );
        // Past the limit: rotate before the next line.
        if f.metadata().is_ok_and(|m| m.len() > MAX_BYTES) {
            file = None;
        }
    }
}

/// `2026-11-22 14:03:12`, local time.
fn stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
        + fx_shell::utc_offset_at(t);
    let (y, m, d) = fx_core::format::civil_from_days(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        s / 3600,
        (s % 3600) / 60,
        s % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_and_stamps() {
        assert_eq!(level_from(None), log::LevelFilter::Warn);
        assert_eq!(level_from(Some("debug")), log::LevelFilter::Debug);
        assert_eq!(level_from(Some("x")), log::LevelFilter::Warn);
        let s = stamp(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000));
        assert_eq!(s.len(), 19, "{s}");
        assert!(s.starts_with("2023-11-1"), "{s}");
    }

    #[test]
    fn lines_go_to_the_file_and_rotate() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = channel();
        let line = |text: &str| Line {
            at: SystemTime::now(),
            level: log::Level::Warn,
            target: "fx".into(),
            text: text.into(),
        };
        tx.send(line("primeira")).unwrap();
        tx.send(line(&"x".repeat(MAX_BYTES as usize + 10))).unwrap();
        tx.send(line("depois")).unwrap();
        drop(tx);
        write_lines(dir.path(), rx);
        let now = std::fs::read_to_string(dir.path().join("fx.log")).unwrap();
        let old = std::fs::read_to_string(dir.path().join("fx.old.log")).unwrap();
        assert!(old.contains("primeira") && now.contains("depois") && !now.contains("primeira"));
    }
}
