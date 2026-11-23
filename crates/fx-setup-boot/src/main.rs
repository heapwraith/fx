//! fx-setup-boot: the program part of the downloaded installer (T5.7).
//!
//! The setup program (`fx-setup.exe`, with the GPUI interface) travels
//! compressed inside the package, like everything else; this launcher, a
//! few hundred kilobytes, takes it out to a fresh folder in %TEMP%, runs it
//! with the same arguments plus `--package <this file>` (so it reads the
//! package from here), waits, passes its exit code on and removes the
//! folder. Before this the setup program also went uncompressed as the
//! installer's program part, 7 MB of the 16.
#![forbid(unsafe_code)]
#![windows_subsystem = "windows"]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fx_core::package;

/// The setup program's name in the package (as `fx_setup::pack::SETUP_EXE`).
const SETUP_EXE: &str = "fx-setup.exe";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            fx_shell::install::error_box(
                "fx",
                &fx_core::trf!(
                    "O instalador não pôde começar ({e}). Baixe-o de novo.",
                    "The installer could not start ({e}). Download it again."
                ),
            );
            ExitCode::FAILURE
        }
    }
}

fn run() -> std::io::Result<ExitCode> {
    let me = std::env::current_exe()?;
    let program = setup_program(&std::fs::read(&me)?)?;
    let dir = fresh_dir()?;
    let exe = dir.join(SETUP_EXE);
    let result = std::fs::write(&exe, &program).and_then(|()| {
        std::process::Command::new(&exe)
            .args(std::env::args_os().skip(1))
            .arg("--package")
            .arg(&me)
            .status()
    });
    remove_dir(&dir);
    let status = result?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(c) => ExitCode::from(u8::try_from(c).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

/// The setup program out of this file's package, its hash checked.
fn setup_program(file: &[u8]) -> std::io::Result<Vec<u8>> {
    let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, what.to_owned());
    let (_, payload) = package::detach(file).ok_or_else(|| bad("no package"))?;
    let (manifest, blob) = package::decode(payload).map_err(|e| bad(&e.to_string()))?;
    let data = fx_shell::install::decompress(blob, manifest.total_size() as usize)?;
    let hash = |b: &[u8]| fx_shell::net::sha256(b).unwrap_or([0; 32]);
    let files = package::split(&manifest, &data, hash).map_err(|e| bad(&e.to_string()))?;
    files
        .into_iter()
        .find(|(name, _)| *name == SETUP_EXE)
        .map(|(_, bytes)| bytes.to_vec())
        .ok_or_else(|| bad("no setup program in the package"))
}

/// A new, empty folder under %TEMP% for the setup program.
fn fresh_dir() -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir();
    for k in 0..100u32 {
        let dir = base.join(format!("fx-setup-{}-{k}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("no free temporary folder"))
}

/// Removes the folder (the program just ended: a few tries while Windows
/// lets go of the file).
fn remove_dir(dir: &Path) {
    for _ in 0..20 {
        if std::fs::remove_dir_all(dir).is_ok() || !dir.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}
