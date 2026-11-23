//! Running a console program in the background (T6.12: `git status`):
//! no console window flashes, the output is read in full, and a program
//! that takes too long is stopped.

use std::io::{self, Read};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// `CREATE_NO_WINDOW`: no console for the child.
const NO_WINDOW: u32 = 0x0800_0000;

/// Runs `program` with `args` in `dir` and returns what it wrote to its
/// standard output, if it ends well within `timeout`.
pub fn run_hidden(
    program: &str,
    args: &[&str],
    dir: &Path,
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .creation_flags(NO_WINDOW)
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("no output"))?;
    // Read on a thread: a full pipe would otherwise block the child.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            let out = reader.join().unwrap_or_default();
            return if status.success() {
                Ok(out)
            } else {
                Err(io::Error::other(format!("{program} ended with {status}")))
            };
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{program} took too long"),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_errors_and_timeouts() {
        let dir = std::env::temp_dir();
        let out = run_hidden(
            "cmd.exe",
            &["/C", "echo fx-ok"],
            &dir,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(String::from_utf8_lossy(&out).contains("fx-ok"));
        assert!(run_hidden("cmd.exe", &["/C", "exit 3"], &dir, Duration::from_secs(5)).is_err());
        let slow = run_hidden(
            "ping.exe",
            &["-n", "5", "127.0.0.1"],
            &dir,
            Duration::from_millis(300),
        );
        assert_eq!(slow.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(run_hidden("no-such-program-fx.exe", &[], &dir, Duration::from_secs(1)).is_err());
    }
}
