//! `FX_TRACE_MODULES=<file>` (T5.7): appends to `<file>` the modules (DLLs)
//! that appeared since the last checkpoint, with the checkpoint's name and
//! time, to find what loads them at startup (idle memory, P8).

use std::collections::HashSet;
use std::io::Write;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

fn state() -> &'static Mutex<(HashSet<String>, Instant)> {
    static S: OnceLock<Mutex<(HashSet<String>, Instant)>> = OnceLock::new();
    S.get_or_init(|| Mutex::new((HashSet::new(), Instant::now())))
}

/// Writes the modules new since the last call (nothing without the variable).
pub fn checkpoint(label: &str) {
    let Some(file) = std::env::var_os("FX_TRACE_MODULES") else {
        return;
    };
    let mut s = state().lock().unwrap_or_else(PoisonError::into_inner);
    let now: Vec<String> = fx_shell::loaded_modules();
    let new: Vec<&String> = now.iter().filter(|m| !s.0.contains(*m)).collect();
    let line = format!(
        "{:>7.1} ms {label}: {}\n",
        s.1.elapsed().as_secs_f64() * 1000.0,
        new.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(" ")
    );
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
    {
        let _ = f.write_all(line.as_bytes());
    }
    s.0.extend(now);
}
