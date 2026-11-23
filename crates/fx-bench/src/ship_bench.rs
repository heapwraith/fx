//! What ships (T5.7, D-064): S1–S3, the sizes of the programs and of the
//! installer, and P13, installing and applying an update with the real
//! installer, into a test folder (`--test-root`: its own registry key, never
//! the user's install).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use crate::report::{commit, json_string, machine, results_dir, utc_date};

/// Decimal megabytes, as in docs/PERFORMANCE.md.
const MB: f64 = 1_000_000.0;
/// S1 (D-069), S2, S3 in MB.
const S1_FX: f64 = 20.0;
const S2_SETUP: f64 = 10.0;
const S3_INDEXD: f64 = 1.0;
const S3_PREVIEW: f64 = 0.5;
/// P13: install, and apply an update when fx closes (seconds).
const P13_INSTALL_S: f64 = 3.0;
const P13_UPDATE_S: f64 = 2.0;

fn release_dir() -> std::io::Result<PathBuf> {
    Ok(std::env::current_exe()?
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default())
}

fn size_mb(path: &Path) -> std::io::Result<f64> {
    Ok(std::fs::metadata(path)?.len() as f64 / MB)
}

/// The installer packed from this build (fx-setup --pack, as the CI does).
fn pack_installer(rel: &Path) -> std::io::Result<PathBuf> {
    let out = std::env::temp_dir().join("fx-bench-setup.exe");
    let docs = std::env::current_dir()?;
    let status = Command::new(rel.join("fx-setup.exe"))
        .arg("--pack")
        .arg(rel.join("fx-setup-boot.exe"))
        .arg(rel)
        .arg(&docs)
        .arg(&out)
        .status()?;
    if !status.success() || !out.is_file() {
        return Err(std::io::Error::other(
            "fx-setup --pack failed (build fx-setup and fx-setup-boot in release; run from the repository root)",
        ));
    }
    Ok(out)
}

/// One silent run of the installer into `root`; its time.
fn silent_install(setup: &Path, root: &Path) -> std::io::Result<Duration> {
    let started = Instant::now();
    let status = Command::new(setup)
        .args(["--silent", "--test-root"])
        .arg(root)
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other("silent install failed"));
    }
    Ok(started.elapsed())
}

/// S1–S3 and P13.
pub fn run() -> std::io::Result<()> {
    let rel = release_dir()?;
    let setup = pack_installer(&rel)?;
    let sizes = [
        ("S1 fx.exe", size_mb(&rel.join("fx.exe"))?, S1_FX),
        ("S2 installer", size_mb(&setup)?, S2_SETUP),
        (
            "S3 fx-indexd.exe",
            size_mb(&rel.join("fx-indexd.exe"))?,
            S3_INDEXD,
        ),
        (
            "S3 fx-preview.exe",
            size_mb(&rel.join("fx-preview.exe"))?,
            S3_PREVIEW,
        ),
    ];
    println!("S1-S3: sizes of what ships (release)");
    let mut all_pass = true;
    for (name, mb, limit) in sizes {
        let pass = mb <= limit;
        all_pass &= pass;
        println!(
            "  {name:<18} {mb:>6.2} MB   target ≤ {limit} MB  {}",
            if pass { "pass" } else { "FAIL" }
        );
    }
    // P13: a fresh install, then the same installer over it (an update).
    let root = std::env::temp_dir().join("fx-bench-install");
    let _ = std::fs::remove_dir_all(&root);
    let mut installs = Vec::new();
    let mut updates = Vec::new();
    for _ in 0..3 {
        let _ = std::fs::remove_dir_all(&root);
        installs.push(silent_install(&setup, &root)?);
        updates.push(silent_install(&setup, &root)?);
    }
    // Uninstall with the installed copy (test key and folder only).
    let _ = Command::new(root.join("Programs").join("fx").join("fx-setup.exe"))
        .args(["--uninstall", "--quiet", "--test-root"])
        .arg(&root)
        .status();
    std::thread::sleep(Duration::from_secs(3));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&setup);
    let median = |v: &mut Vec<Duration>| {
        v.sort();
        v[v.len() / 2].as_secs_f64()
    };
    let (install, update) = (median(&mut installs), median(&mut updates));
    let p13 = install <= P13_INSTALL_S && update <= P13_UPDATE_S;
    all_pass &= p13;
    println!("P13: silent installer into a test folder, 3 rounds (release)");
    println!(
        "  install {install:.2} s (target ≤ {P13_INSTALL_S} s)   update over it {update:.2} s (target ≤ {P13_UPDATE_S} s)  {}",
        if p13 { "pass" } else { "FAIL" }
    );
    let date = utc_date(SystemTime::now());
    let json = format!(
        concat!(
            "{{\n  \"id\": \"S1-S3,P13\",\n  \"task\": \"T5.7\",\n",
            "  \"fx_mb\": {s1:.3},\n  \"installer_mb\": {s2:.3},\n  \"indexd_mb\": {s3a:.3},\n  \"preview_mb\": {s3b:.3},\n",
            "  \"install_s\": {i:.3},\n  \"update_s\": {u:.3},\n  \"pass\": {pass},\n",
            "  \"machine\": {machine},\n  \"commit\": {commit},\n  \"date\": {date}\n}}\n"
        ),
        s1 = sizes[0].1,
        s2 = sizes[1].1,
        s3a = sizes[2].1,
        s3b = sizes[3].1,
        i = install,
        u = update,
        pass = all_pass,
        machine = json_string(&machine()),
        commit = json_string(commit()),
        date = json_string(&date),
    );
    std::fs::write(results_dir()?.join(format!("{date}-T5.7-ship.json")), json)?;
    Ok(())
}
