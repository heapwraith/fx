//! Opening things (task T1.9): files with their default app, "Open with…",
//! a terminal in a folder. Platform-neutral part: every launch runs on its own
//! thread and reports back on a channel, so the UI never waits, even when the
//! associated app (or a shell handler) is slow to start.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

/// What to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// A file (or folder) with its default app.
    Open(PathBuf),
    /// The system "Open with…" dialog for a file.
    OpenWith(PathBuf),
    /// A terminal whose current folder is this one.
    Terminal(PathBuf),
}

/// Runs `job` on a new thread; the result arrives on the returned channel.
pub fn spawn_launch<F>(job: F) -> Receiver<std::io::Result<()>>
where
    F: FnOnce() -> std::io::Result<()> + Send + 'static,
{
    let (tx, rx) = channel();
    let spawned = std::thread::Builder::new()
        .name("fx-launch".into())
        .spawn(move || {
            let _ = tx.send(job());
        });
    if let Err(e) = spawned {
        let (tx, rx2) = channel();
        let _ = tx.send(Err(e));
        return rx2;
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// A slow app (2 s to start) does not hold the caller.
    #[test]
    fn a_slow_launch_returns_immediately() {
        let started = Instant::now();
        let rx = spawn_launch(|| {
            std::thread::sleep(Duration::from_secs(2));
            Ok(())
        });
        assert!(started.elapsed() < Duration::from_millis(50));
        assert!(rx.try_recv().is_err(), "not finished yet");
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().is_ok());
    }

    #[test]
    fn errors_come_back() {
        let rx = spawn_launch(|| Err(std::io::Error::other("no app")));
        let e = rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err();
        assert_eq!(e.to_string(), "no app");
    }
}
