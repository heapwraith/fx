//! fx-indexd: the optional index service (task T2.5, docs/DECISIONS.md D-040).
//!
//! It reads the MFT of every fixed NTFS volume (`fx-index`), follows their
//! USN journals so the index stays current, and answers name searches over a
//! named pipe that only SYSTEM and the user who installed it can open.
//! Results inside other users' profile folders are left out.
//!
//! ```text
//! fx-indexd install            install and start (Administrator; UAC from the app)
//! fx-indexd uninstall          stop and remove, leaving nothing behind (Administrator)
//! fx-indexd status             installed? running? how many items?
//! fx-indexd query <text>       search through the running service
//! fx-indexd run [--user SID]   serve in the foreground (Administrator; for tests)
//! fx-indexd service --user SID entry point used by the Service Control Manager
//! ```

#[cfg(windows)]
mod engine;

use std::process::ExitCode;

#[cfg(windows)]
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let result = match args.as_slice() {
        ["install"] => install(),
        ["uninstall"] => fx_index::service::uninstall().map(|()| println!("fx-indexd removido")),
        ["status"] => {
            status();
            Ok(())
        }
        ["query", words @ ..] if !words.is_empty() => query(&words.join(" ")),
        ["run"] => {
            fx_index::system::current_user_sid().and_then(|sid| engine::run_foreground(&sid))
        }
        ["run", "--user", sid] => engine::run_foreground(sid),
        ["service", "--user", sid] => {
            let sid = (*sid).to_owned();
            fx_index::service::run_service(
                move |stop| engine::serve(&sid, stop),
                fx_index::pipe::wake,
            )
        }
        _ => {
            eprintln!(
                "uso: fx-indexd install | uninstall | status | query <texto> | run [--user SID]"
            );
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fx-indexd: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn install() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    // The elevated process runs as the same user who accepted the UAC
    // prompt (an administrator account): that user gets the pipe.
    let sid = fx_index::system::current_user_sid()?;
    let target = fx_index::service::install(&exe, &sid)?;
    println!("fx-indexd instalado em {}", target.display());
    Ok(())
}

#[cfg(windows)]
fn status() {
    use fx_index::service::{ServiceState, service_state};
    let state = service_state();
    println!(
        "serviço: {}",
        match state {
            ServiceState::NotInstalled => "não instalado",
            ServiceState::Stopped => "parado",
            ServiceState::Running => "rodando",
            ServiceState::Other => "iniciando ou parando",
        }
    );
    if state == ServiceState::Running
        && let Ok(r) = fx_index::pipe::query(
            &fx_index::protocol::Request {
                query: String::new(),
                limit: 0,
                ..Default::default()
            },
            2000,
        )
    {
        println!(
            "índice: {} ({} itens, {:.1} MB)",
            match r.status {
                fx_index::protocol::Status::Ready => "pronto",
                fx_index::protocol::Status::Indexing => "lendo os discos",
            },
            r.items,
            r.index_bytes as f64 / 1_048_576.0
        );
    }
}

#[cfg(windows)]
fn query(text: &str) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStringExt;
    let started = std::time::Instant::now();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    // Filters as in the app (`tipo:`, `tamanho:`...); relative dates in UTC.
    let filters = fx_core::search_filter::Filters::parse(text, now, 0);
    let r = fx_index::pipe::query(
        &fx_index::protocol::Request {
            query: filters.text.clone(),
            limit: 20,
            filter: fx_index::protocol::SearchFilter::from_filters(&filters),
        },
        2000,
    )?;
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "{:?}, {} itens no índice, {} resultados em {ms:.1} ms",
        r.status,
        r.items,
        r.hits.len()
    );
    for h in &r.hits {
        let path = std::ffi::OsString::from_wide(&h.path);
        println!(
            "  {}{}",
            path.to_string_lossy(),
            if h.is_dir { "\\" } else { "" }
        );
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("fx-indexd: Windows only");
    ExitCode::FAILURE
}
