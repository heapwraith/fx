//! fx-preview: hosts one Windows preview handler (PDF, Office, video,
//! audio...) for fx's Quick Look, in its own process (task T2.9): a handler
//! is third-party code and may hang or crash; here that only ends this
//! process. Started by fx as
//!
//! ```text
//! fx-preview <owner window> <x> <y> <width> <height> <file>
//! ```
//!
//! and driven over its standard streams (see `fx_shell::preview`). A GUI
//! program: as a console one, Windows would open a terminal window for it,
//! which takes the focus from fx.
#![windows_subsystem = "windows"]

use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args.as_slice() {
        [owner, x, y, w, h, file] => (|| {
            Some((
                owner.parse::<isize>().ok()?,
                fx_shell::preview::ScreenRect {
                    x: x.parse().ok()?,
                    y: y.parse().ok()?,
                    width: w.parse().ok()?,
                    height: h.parse().ok()?,
                },
                PathBuf::from(file),
            ))
        })(),
        _ => None,
    };
    let Some((owner, rect, file)) = parsed else {
        eprintln!("uso: fx-preview <janela> <x> <y> <largura> <altura> <arquivo>");
        return ExitCode::from(2);
    };
    match fx_shell::preview_host::run_host(&file, owner, rect) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            println!("error {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    ExitCode::FAILURE
}
