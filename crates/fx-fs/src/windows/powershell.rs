//! A hidden PowerShell for the .NET and COM parts of Windows (archives,
//! Windows Search): no window (`CREATE_NO_WINDOW`), no prompt
//! (`-NonInteractive`), the script passed as UTF-16 (`-EncodedCommand`) and
//! paths through a UTF-8 list file, never on the command line.

use std::io::{self, BufRead, Read};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// CREATE_NO_WINDOW: no console window for the helper process.
const NO_WINDOW: u32 = 0x0800_0000;

/// A PowerShell string literal (single quotes doubled).
pub(crate) fn ps_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                out.push(T[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Runs `script` in a hidden PowerShell, calling `line` for each line it
/// prints (progress). Fails with what it wrote on its error stream.
pub(crate) fn run_powershell(script: &str, line: &mut dyn FnMut(&str)) -> io::Result<()> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let exe = Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut child = Command::new(exe)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ])
        .arg(base64(&utf16))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(NO_WINDOW)
        .spawn()?;
    let mut err = child.stderr.take();
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    if let Some(out) = child.stdout.take() {
        for l in io::BufReader::new(out).lines() {
            line(l?.trim());
        }
    }
    let status = child.wait()?;
    let errors = errors.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        let first = errors
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("PowerShell failed")
            .to_owned();
        Err(io::Error::other(first))
    }
}

/// Writes `lines` to a new UTF-8 list file in `%TEMP%`.
pub(crate) fn list_file(lines: &[String]) -> io::Result<tempfile_path::TempList> {
    tempfile_path::TempList::new(lines)
}

/// A list file removed when dropped.
mod tempfile_path {
    use std::io::{self, Write};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    pub struct TempList(pub PathBuf);

    impl TempList {
        pub fn new(lines: &[String]) -> io::Result<Self> {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("fx-list-{}-{n}.txt", std::process::id()));
            let mut f = std::fs::File::create(&path)?;
            for l in lines {
                f.write_all(l.as_bytes())?;
                f.write_all(b"\n")?;
            }
            Ok(TempList(path))
        }
    }

    impl Drop for TempList {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
