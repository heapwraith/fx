//! Previews by Windows preview handlers (task T2.9), hosted by the
//! `fx-preview` process so that a slow or hanging handler (third-party
//! code) never blocks fx. Platform-neutral part: starting the process,
//! waiting for it with a time limit, moving it, and killing it.
//!
//! Protocol on the child's standard streams: it prints `ready` once the
//! preview is drawn (or `error <text>`); it reads lines `move x y w h`
//! (screen pixels), `raise`, `hide` and `quit`. Closing its input also ends it.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::channel;
use std::time::Duration;

/// A rectangle in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A running preview host. Dropping it ends the process.
#[derive(Debug)]
pub struct PreviewProcess {
    child: Child,
    stdin: ChildStdin,
    /// The host's window, as it announced it (`ready <hwnd>`).
    pub window: Option<isize>,
}

impl PreviewProcess {
    /// Starts `command` (with piped standard streams) and waits up to
    /// `timeout` for `ready`. On a timeout, an `error` line or an early exit
    /// the process is killed and an error returned. Blocks: call it off the
    /// UI thread.
    pub fn start(mut command: Command, timeout: Duration) -> io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("no stdout"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("no stdin"))?;
        let (tx, rx) = channel();
        std::thread::Builder::new()
            .name("fx-preview-wait".into())
            .spawn(move || {
                let mut first = String::new();
                let _ = BufReader::new(stdout).read_line(&mut first);
                let _ = tx.send(first);
            })?;
        let mut process = Self {
            child,
            stdin,
            window: None,
        };
        match rx.recv_timeout(timeout) {
            Ok(line) if line.trim() == "ready" || line.starts_with("ready ") => {
                process.window = line
                    .trim()
                    .strip_prefix("ready ")
                    .and_then(|h| h.parse().ok());
                Ok(process)
            }
            Ok(line) if line.starts_with("error") => {
                process.kill();
                Err(io::Error::other(
                    line.trim_start_matches("error").trim().to_owned(),
                ))
            }
            Ok(_) => {
                process.kill();
                Err(io::Error::other(fx_core::tr!(
                    "a prévia terminou sem abrir",
                    "the preview ended without opening"
                )))
            }
            Err(_) => {
                process.kill();
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    fx_core::tr!(
                        "o visualizador do Windows não respondeu",
                        "the Windows viewer did not respond"
                    ),
                ))
            }
        }
    }

    /// Moves the preview window (screen pixels).
    pub fn move_to(&mut self, r: ScreenRect) {
        let _ = writeln!(self.stdin, "move {} {} {} {}", r.x, r.y, r.width, r.height);
        let _ = self.stdin.flush();
    }

    /// Shows the preview above everything else of the desktop's normal
    /// windows (fx is active), or hides it (fx is not).
    pub fn set_visible(&mut self, visible: bool) {
        let _ = writeln!(self.stdin, "{}", if visible { "raise" } else { "hide" });
        let _ = self.stdin.flush();
    }

    fn kill(&mut self) {
        let _ = writeln!(self.stdin, "quit");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for PreviewProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::time::Instant;

    fn powershell(script: &str) -> Command {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-Command", script]);
        c
    }

    /// A hanging host (a fake handler that never answers) costs fx only the
    /// time limit, and the process is gone afterwards.
    #[test]
    fn a_hanging_host_is_killed_after_the_time_limit() {
        let started = Instant::now();
        let r = PreviewProcess::start(
            powershell("Start-Sleep -Seconds 30"),
            Duration::from_millis(1500),
        );
        let err = r.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn ready_errors_and_early_exits() {
        let mut p = PreviewProcess::start(
            powershell("[Console]::Out.WriteLine('ready'); [Console]::Out.Flush(); Start-Sleep -Seconds 30"),
            Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(p.window, None, "an old host without its window handle");
        p.move_to(ScreenRect {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        });
        drop(p);
        let p = PreviewProcess::start(
            powershell("[Console]::Out.WriteLine('ready 4242'); [Console]::Out.Flush(); Start-Sleep -Seconds 30"),
            Duration::from_secs(20),
        )
        .unwrap();
        assert_eq!(p.window, Some(4242));
        drop(p);
        let e = PreviewProcess::start(
            powershell("Write-Output 'error sem visualizador'"),
            Duration::from_secs(20),
        )
        .unwrap_err();
        assert_eq!(e.to_string(), "sem visualizador");
        assert!(PreviewProcess::start(powershell("exit 3"), Duration::from_secs(20)).is_err());
    }
}
