//! P10 (D-064): copying a large file with fx's operations engine (the same
//! job the UI submits: temporary name, size check, rename) against the
//! system copy (`std::fs::copy`, which is `CopyFileExW`, the copy routine
//! Explorer's engine is built on). Explorer's own window is not timed: its
//! copy dialog cannot be driven silently. Files live in
//! `%TEMP%\fx-fixtures\copy` and are removed afterwards.

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant, SystemTime};

use crate::report::{commit, json_string, machine, results_dir, utc_date};
use crate::rng::Rng;

const SIZE: usize = 1 << 30;
const ROUNDS: usize = 3;
/// At least this fraction of the system copy's speed.
const TARGET: f64 = 0.9;

fn make_source(path: &Path) -> std::io::Result<()> {
    use std::io::Write;
    let mut rng = Rng::derive(10, 0);
    let mut f = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(path)?);
    let mut block = vec![0u8; 1 << 20];
    for _ in 0..SIZE / block.len() {
        for chunk in block.as_chunks_mut::<8>().0 {
            *chunk = rng.next_u64().to_le_bytes();
        }
        f.write_all(&block)?;
    }
    f.into_inner()
        .map_err(std::io::IntoInnerError::into_error)?
        .sync_all()
}

fn engine_copy(src: &Path, dest_dir: &Path) -> std::io::Result<Duration> {
    let (tx, rx) = channel();
    let engine = fx_ops::Engine::start(tx)?;
    let started = Instant::now();
    engine.submit(fx_ops::Job::copy(
        vec![src.to_path_buf()],
        dest_dir.to_path_buf(),
    ));
    for event in rx {
        if let fx_ops::Event::Finished { outcome, .. } = event {
            return match outcome {
                fx_ops::Outcome::Done => Ok(started.elapsed()),
                other => Err(std::io::Error::other(format!("copy ended: {other:?}"))),
            };
        }
    }
    Err(std::io::Error::other("the engine stopped"))
}

fn system_copy(src: &Path, dest: &Path) -> std::io::Result<Duration> {
    let started = Instant::now();
    std::fs::copy(src, dest)?;
    Ok(started.elapsed())
}

/// P10: 1 GiB, fx engine against the system copy, alternated, 3 rounds.
pub fn run(fixtures: &Path) -> std::io::Result<()> {
    let dir = fixtures.join("copy");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    let src = dir.join("grande.bin");
    make_source(&src)?;
    let (mut fx, mut sys) = (Vec::new(), Vec::new());
    let result = (|| -> std::io::Result<()> {
        for k in 0..ROUNDS {
            let out: PathBuf = dir.join(format!("fx-{k}"));
            std::fs::create_dir(&out)?;
            fx.push(engine_copy(&src, &out)?);
            std::fs::remove_dir_all(&out)?;
            let target = dir.join(format!("sys-{k}.bin"));
            sys.push(system_copy(&src, &target)?);
            std::fs::remove_file(&target)?;
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result?;
    fx.sort();
    sys.sort();
    let gib = SIZE as f64 / (1u64 << 30) as f64;
    let speed = |d: Duration| gib * 1024.0 / d.as_secs_f64();
    let (fx_mib_s, sys_mib_s) = (speed(fx[ROUNDS / 2]), speed(sys[ROUNDS / 2]));
    let ratio = fx_mib_s / sys_mib_s;
    let pass = ratio >= TARGET;
    println!("P10: copying 1 GiB on the same disk, {ROUNDS} rounds each, medians (release)");
    println!(
        "  fx engine {fx_mib_s:.0} MiB/s · system copy (CopyFileExW) {sys_mib_s:.0} MiB/s · {:.0}% (target ≥ {:.0}%)  {}",
        ratio * 100.0,
        TARGET * 100.0,
        if pass { "pass" } else { "FAIL" }
    );
    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"P10\",\n  \"task\": \"T5.7\",\n  \"bytes\": {b},\n",
            "  \"fx_mib_s\": {f:.1},\n  \"system_mib_s\": {s:.1},\n  \"ratio\": {r:.3},\n  \"pass\": {pass},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        b = SIZE,
        f = fx_mib_s,
        s = sys_mib_s,
        r = ratio,
        pass = pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-T5.7-P10.json")), json)?;
    Ok(())
}
