//! Installing and uninstalling (task T4.3), as a transaction:
//!
//! 1. every file is written to a staging folder next to the install folder
//!    (same volume, so the swap is a rename) and checked;
//! 2. each installed file is renamed aside (`name.fx-old-<pid>`: works even
//!    for the running fx.exe, which Windows lets one rename but not delete)
//!    and the new one is renamed in; any failure puts everything back;
//! 3. shortcuts and the Settings › Apps entry are written;
//! 4. the files set aside are deleted when possible (a running one is left
//!    for the next install or start of Pathdash: `update::clean_set_aside`).
//!
//! Uninstalling deletes only the files fx installed, never a folder's
//! unknown contents.

use std::io;
use std::path::{Path, PathBuf};

use fx_shell::install::{self as win, UninstallEntry};

use crate::pack::{APP_EXE, DOCUMENTS, LEGACY_EXE, PROGRAMS, Package, SETUP_EXE};

/// Where things go. `default()` is the real per-user location; tests point
/// everything at temporary folders and a test registry key.
#[derive(Debug, Clone)]
pub struct Target {
    pub dir: PathBuf,
    /// The Start Menu shortcut.
    pub start_menu: Option<PathBuf>,
    /// The desktop shortcut (written only when asked).
    pub desktop: Option<PathBuf>,
    /// Under `HKCU`.
    pub uninstall_key: String,
    /// The old Inno Setup entry to adopt (removed after installing).
    pub legacy_key: Option<String>,
    /// The default-manager keys (classes root, backup) under `HKCU`: if
    /// they point at the old `fx.exe`, installing moves them to the new
    /// program, so Win+E keeps working (D-074). `None` in tests that must
    /// not touch them.
    pub default_manager: Option<(String, String)>,
    /// The real install of this user: only then may the uninstaller touch
    /// what belongs to the whole machine (the fx-indexd service). `false`
    /// for `--test-root` and in tests.
    pub real: bool,
}

impl Target {
    /// Where fx goes for this user. `test_root` (`--test-root <pasta>`, for
    /// end-to-end tests) puts everything under one folder and a test key; it
    /// is an explicit option, never read from the environment, so nothing
    /// inherited can redirect a real install (review, D-066).
    pub fn user(test_root: Option<&Path>) -> Option<Target> {
        if let Some(root) = test_root {
            return Some(Target {
                dir: root.join("Programs").join("fx"),
                start_menu: Some(root.join("Start Menu").join(SHORTCUT)),
                desktop: Some(root.join("Desktop").join(SHORTCUT)),
                uninstall_key: r"Software\fx-test-setup\Uninstall\fx".to_owned(),
                legacy_key: None,
                default_manager: None,
                real: false,
            });
        }
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
        Some(Target {
            dir: local.join("Programs").join("fx"),
            start_menu: win::start_menu_programs().map(|p| p.join(SHORTCUT)),
            desktop: fx_shell::known_folder(fx_shell::places::KnownFolder::Desktop)
                .map(|p| p.join(SHORTCUT)),
            uninstall_key: win::UNINSTALL_KEY.to_owned(),
            legacy_key: Some(win::INNO_KEY.to_owned()),
            default_manager: Some((
                fx_shell::default_manager::CLASSES.to_owned(),
                fx_shell::default_manager::BACKUP.to_owned(),
            )),
            real: true,
        })
    }

    pub fn exe(&self) -> PathBuf {
        self.dir.join(APP_EXE)
    }

    /// The program as it was named up to 0.3.0.
    pub fn legacy_exe(&self) -> PathBuf {
        self.dir.join(LEGACY_EXE)
    }

    /// The shortcut a 0.3.0 install left next to `lnk` (`fx.lnk`).
    fn legacy_shortcut(lnk: &Path) -> PathBuf {
        lnk.with_file_name(LEGACY_SHORTCUT)
    }
}

/// The app's name, as the shortcuts and Settings › Apps show it.
const APP_NAME: &str = "Pathdash";
const SHORTCUT: &str = "Pathdash.lnk";
const LEGACY_SHORTCUT: &str = "fx.lnk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    pub desktop_shortcut: bool,
}

/// Progress, for the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Writing files: done, total.
    Copying(usize, usize),
    Shortcuts,
    Registering,
    Done,
}

/// The version installed at `t`, if any (from the Settings › Apps entry).
pub fn installed_version(t: &Target) -> Option<String> {
    win::registered_version(&t.uninstall_key)
        .filter(|_| t.exe().exists() || t.legacy_exe().exists())
}

fn old_name(name: &str) -> String {
    format!("{name}.fx-old-{}", std::process::id())
}

/// Deletes the files a previous install or update set aside.
pub fn clean_old(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().contains(".fx-old-") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Moves the staged files into `dir`, all or nothing.
pub fn swap(staging: &Path, dir: &Path, names: &[String]) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    // (name, had an old file set aside)
    let mut done: Vec<(String, bool)> = Vec::new();
    let mut result = Ok(());
    for name in names {
        let target = dir.join(name);
        let had_old = target.exists();
        if had_old && let Err(e) = std::fs::rename(&target, dir.join(old_name(name))) {
            result = Err(e);
            break;
        }
        if let Err(e) = std::fs::rename(staging.join(name), &target) {
            if had_old {
                let _ = std::fs::rename(dir.join(old_name(name)), &target);
            }
            result = Err(e);
            break;
        }
        done.push((name.clone(), had_old));
    }
    if result.is_err() {
        // Back to what was there.
        for (name, had_old) in done.iter().rev() {
            let target = dir.join(name);
            let _ = std::fs::remove_file(&target);
            if *had_old {
                let _ = std::fs::rename(dir.join(old_name(name)), &target);
            }
        }
    }
    result
}

/// Why an install stopped and what it left behind, so the screen says exactly
/// that (status is always real; review, D-066).
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// Stopped before replacing anything: what is on disk is as before.
    #[error(transparent)]
    NothingChanged(io::Error),
    /// The new files are in place; `missing` (and what comes after it) was
    /// not done.
    #[error("{error}")]
    Incomplete { missing: Missing, error: io::Error },
}

/// The step that failed after the files were replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    /// The shortcuts (so the Settings › Apps entry is missing too).
    Shortcut,
    /// The Settings › Apps entry (and with it the uninstaller).
    AppsEntry,
}

/// Installs `pkg` at `t`.
pub fn install(
    pkg: &Package,
    t: &Target,
    opts: Options,
    progress: &mut dyn FnMut(Step),
) -> Result<(), InstallError> {
    let parent = t
        .dir
        .parent()
        .ok_or_else(|| io::Error::other("install folder has no parent"))
        .map_err(InstallError::NothingChanged)?;
    std::fs::create_dir_all(parent).map_err(InstallError::NothingChanged)?;
    clean_old(&t.dir);
    let staging = parent.join(format!(".fx-setup-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(InstallError::NothingChanged)?;
    let total = pkg.files.len();
    let staged = (|| {
        for (k, (name, bytes)) in pkg.files.iter().enumerate() {
            std::fs::write(staging.join(name), bytes)?;
            progress(Step::Copying(k + 1, total));
        }
        let names: Vec<String> = pkg.files.iter().map(|(n, _)| n.clone()).collect();
        swap(&staging, &t.dir, &names)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    // A failed swap put everything back.
    staged.map_err(InstallError::NothingChanged)?;

    let exe = t.exe();
    // Over a 0.3.0 install: the program was fx.exe. Set it aside (deleted
    // below, or at the next install if it is somehow still running) and
    // move the default-manager keys that point at it to the new program.
    let legacy = t.legacy_exe();
    if legacy.exists() {
        if let Some((classes, backup)) = &t.default_manager
            && fx_shell::default_manager::is_default(classes, &legacy.to_string_lossy())
        {
            let _ = fx_shell::default_manager::enable(classes, backup, &exe.to_string_lossy());
        }
        let _ = std::fs::rename(&legacy, t.dir.join(old_name(LEGACY_EXE)));
    }

    let incomplete = |missing| move |error| InstallError::Incomplete { missing, error };
    progress(Step::Shortcuts);
    if let Some(lnk) = &t.start_menu {
        let _ = std::fs::remove_file(Target::legacy_shortcut(lnk));
        win::create_shortcut(lnk, &exe, APP_NAME).map_err(incomplete(Missing::Shortcut))?;
    }
    if let Some(lnk) = &t.desktop {
        // Keep a desktop shortcut the user already had under the old name.
        let had = std::fs::remove_file(Target::legacy_shortcut(lnk)).is_ok() || lnk.exists();
        if opts.desktop_shortcut || had {
            win::create_shortcut(lnk, &exe, APP_NAME).map_err(incomplete(Missing::Shortcut))?;
        }
    }

    progress(Step::Registering);
    let setup = t.dir.join(SETUP_EXE);
    let size_kb = pkg.manifest.total_size().div_ceil(1024) as u32;
    win::register_uninstall(
        &t.uninstall_key,
        &UninstallEntry {
            name: APP_NAME.into(),
            version: display_version(pkg.version()),
            publisher: APP_NAME.into(),
            install_dir: t.dir.clone(),
            icon: exe.clone(),
            uninstall: format!("\"{}\" --uninstall", setup.display()),
            quiet_uninstall: format!("\"{}\" --uninstall --quiet", setup.display()),
            size_kb,
            url: "https://github.com/heapwraith/fx".into(),
        },
    )
    .map_err(incomplete(Missing::AppsEntry))?;
    // An install by the old Inno Setup installer in the same place.
    if let Some(legacy) = &t.legacy_key
        && win::is_registered(legacy)
    {
        let _ = win::unregister_uninstall(legacy);
        for f in ["unins000.exe", "unins000.dat"] {
            let _ = std::fs::remove_file(t.dir.join(f));
        }
    }
    clean_old(&t.dir);
    progress(Step::Done);
    Ok(())
}

/// "0.3.0-beta+abc123" → "0.3.0-beta" (what Settings › Apps shows).
pub fn display_version(v: &str) -> String {
    v.split('+').next().unwrap_or(v).to_owned()
}

/// Every file name fx installs.
fn installed_names() -> Vec<&'static str> {
    let mut v = vec![SETUP_EXE];
    v.extend(PROGRAMS);
    v.push(LEGACY_EXE);
    v.extend(DOCUMENTS);
    v.extend(["unins000.exe", "unins000.dat"]);
    v
}

/// Removes fx from `t`; with `data`, also these settings folders (each must
/// be named `fx`). Fails, leaving things as they are as far as possible,
/// if a program is still running.
pub fn uninstall(t: &Target, data: &[PathBuf], progress: &mut dyn FnMut(Step)) -> io::Result<()> {
    // Refuse anything that does not look like an fx install folder.
    // The program, under its current name or the one up to 0.3.0.
    let exe = [t.exe(), t.legacy_exe()].into_iter().find(|p| p.exists());
    let Some(exe) = exe.filter(|_| t.dir.file_name().is_some_and(|n| n == "fx")) else {
        return Err(io::Error::other("not a Pathdash install folder"));
    };
    // A running fx.exe cannot be deleted: check first, before touching
    // anything (renaming it to itself fails the same way deletion would
    // not, so try opening it for deletion through a rename round trip).
    let probe = t.dir.join(old_name("probe"));
    std::fs::rename(&exe, &probe)?;
    let delete = std::fs::remove_file(&probe);
    if let Err(e) = delete {
        let _ = std::fs::rename(&probe, &exe);
        return Err(e);
    }
    // Real progress: one step per installed file actually present.
    let present: Vec<&str> = installed_names()
        .into_iter()
        .filter(|n| t.dir.join(n).exists())
        .collect();
    for (k, name) in present.iter().enumerate() {
        let _ = std::fs::remove_file(t.dir.join(name));
        progress(Step::Copying(k + 1, present.len()));
    }
    clean_old(&t.dir);
    let _ = std::fs::remove_dir(&t.dir); // only if now empty
    for lnk in [&t.start_menu, &t.desktop].into_iter().flatten() {
        let _ = std::fs::remove_file(lnk);
        let _ = std::fs::remove_file(Target::legacy_shortcut(lnk));
    }
    progress(Step::Registering);
    win::unregister_uninstall(&t.uninstall_key)?;
    for d in data {
        if d.file_name().is_some_and(|n| n == "fx") && d.is_dir() {
            std::fs::remove_dir_all(d)?;
        }
    }
    Ok(())
}

/// The settings folders fx uses (`%APPDATA%\fx`, `%LOCALAPPDATA%\fx`).
pub fn data_dirs() -> Vec<PathBuf> {
    ["APPDATA", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|d| PathBuf::from(d).join("fx"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fx_core::package::Manifest;

    fn package(text: &str) -> Package {
        let mut files = vec![(SETUP_EXE.to_owned(), b"setup".to_vec())];
        files.extend(
            PROGRAMS
                .iter()
                .map(|n| ((*n).to_owned(), format!("{n} {text}").into_bytes())),
        );
        Package {
            manifest: Manifest {
                version: format!("{text}+abc"),
                files: Vec::new(),
            },
            files,
        }
    }

    fn target(root: &Path, tag: &str) -> Target {
        Target {
            dir: root.join("Programs").join("fx"),
            start_menu: Some(root.join("menu").join(SHORTCUT)),
            desktop: Some(root.join("desk").join(SHORTCUT)),
            uninstall_key: format!(
                r"Software\fx-test-{}-{tag}\Uninstall\fx",
                std::process::id()
            ),
            legacy_key: None,
            default_manager: None,
            real: false,
        }
    }

    /// Makes the installed fx.exe a real running program (a copy of
    /// ping.exe waiting a few seconds), like an open fx window.
    fn run_as_fx(t: &Target) -> std::process::Child {
        let ping = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32")
            .join("PING.EXE");
        std::fs::copy(ping, t.exe()).unwrap();
        let child = std::process::Command::new(t.exe())
            .args(["-n", "6", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        child
    }

    fn cleanup_key(tag: &str) {
        let _ =
            win::unregister_uninstall(&format!(r"Software\fx-test-{}-{tag}", std::process::id()));
    }

    /// A step that fails after the swap is reported as incomplete, never as
    /// "nothing changed": the new files are already there.
    #[test]
    fn a_failed_shortcut_is_reported_as_incomplete() {
        let root = tempfile::tempdir().unwrap();
        let mut t = target(root.path(), "incomplete");
        // A shortcut inside a file cannot be created.
        let blocker = root.path().join("not-a-folder");
        std::fs::write(&blocker, b"x").unwrap();
        t.start_menu = Some(blocker.join(SHORTCUT));
        let opts = Options {
            desktop_shortcut: false,
        };
        let e = install(&package("1.0.0"), &t, opts, &mut |_| {}).unwrap_err();
        assert!(
            matches!(
                e,
                InstallError::Incomplete {
                    missing: Missing::Shortcut,
                    ..
                }
            ),
            "{e:?}"
        );
        assert_eq!(
            std::fs::read_to_string(t.exe()).unwrap(),
            "pathdash.exe 1.0.0"
        );
        assert!(
            !win::is_registered(&t.uninstall_key),
            "registering comes after"
        );
    }

    #[test]
    fn install_update_and_uninstall() {
        let root = tempfile::tempdir().unwrap();
        let t = target(root.path(), "a");
        let mut steps = Vec::new();
        install(
            &package("1.0.0"),
            &t,
            Options {
                desktop_shortcut: true,
            },
            &mut |s| steps.push(s),
        )
        .unwrap();
        assert_eq!(steps.last(), Some(&Step::Done));
        assert!(steps.contains(&Step::Copying(4, 4)));
        assert_eq!(
            std::fs::read_to_string(t.exe()).unwrap(),
            "pathdash.exe 1.0.0"
        );
        assert!(t.start_menu.as_ref().unwrap().exists());
        assert!(t.desktop.as_ref().unwrap().exists());
        assert_eq!(installed_version(&t).as_deref(), Some("1.0.0"));

        // Update while fx is running: the running program is set aside.
        let mut running = run_as_fx(&t);
        install(
            &package("2.0.0"),
            &t,
            Options {
                desktop_shortcut: false,
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(t.exe()).unwrap(),
            "pathdash.exe 2.0.0"
        );
        let aside = std::fs::read_dir(&t.dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".fx-old-"))
            .count();
        assert_eq!(aside, 1, "the running fx.exe waits to be deleted");
        let _ = running.kill();
        let _ = running.wait();
        assert_eq!(installed_version(&t).as_deref(), Some("2.0.0"));

        // Uninstall leaves files fx did not install, and the folder with them.
        std::fs::write(t.dir.join("meu.txt"), b"mine").unwrap();
        let data = root.path().join("data").join("fx");
        std::fs::create_dir_all(&data).unwrap();
        uninstall(&t, std::slice::from_ref(&data), &mut |_| {}).unwrap();
        assert!(!t.exe().exists());
        assert!(t.dir.join("meu.txt").exists(), "unknown files are kept");
        assert!(!t.start_menu.as_ref().unwrap().exists());
        assert!(!data.exists());
        assert_eq!(installed_version(&t), None);
        cleanup_key("a");
    }

    /// Installing over 0.3.0 (program `fx.exe`, shortcut `fx.lnk`, fx as
    /// the default manager): the old program and shortcuts go, a desktop
    /// shortcut the user had comes back under the new name, and the default
    /// manager points at the new program (D-074).
    #[test]
    fn installing_over_0_3_0_moves_everything_to_the_new_name() {
        use fx_shell::default_manager::{enable, is_default};
        let root = tempfile::tempdir().unwrap();
        let mut t = target(root.path(), "legacy");
        let reg = format!(r"Software\fx-test-{}-legacy", std::process::id());
        let (classes, backup) = (format!(r"{reg}\Classes"), format!(r"{reg}\Backup"));
        t.default_manager = Some((classes.clone(), backup.clone()));
        std::fs::create_dir_all(&t.dir).unwrap();
        std::fs::write(t.legacy_exe(), b"fx.exe 0.3.0").unwrap();
        let (menu, desk) = (t.start_menu.clone().unwrap(), t.desktop.clone().unwrap());
        for lnk in [&menu, &desk] {
            std::fs::create_dir_all(lnk.parent().unwrap()).unwrap();
            std::fs::write(Target::legacy_shortcut(lnk), b"old").unwrap();
        }
        enable(&classes, &backup, &t.legacy_exe().to_string_lossy()).unwrap();

        let opts = Options {
            desktop_shortcut: false,
        };
        install(&package("1.0.0"), &t, opts, &mut |_| {}).unwrap();
        assert!(!t.legacy_exe().exists(), "fx.exe is gone");
        assert_eq!(
            std::fs::read_to_string(t.exe()).unwrap(),
            "pathdash.exe 1.0.0"
        );
        assert!(menu.exists() && desk.exists(), "both shortcuts, new name");
        assert!(!Target::legacy_shortcut(&menu).exists());
        assert!(!Target::legacy_shortcut(&desk).exists());
        assert!(is_default(&classes, &t.exe().to_string_lossy()));
        assert_eq!(installed_version(&t).as_deref(), Some("1.0.0"));

        uninstall(&t, &[], &mut |_| {}).unwrap();
        let _ = fx_shell::default_manager::revert(&classes, &backup);
        let _ = win::unregister_uninstall(&reg);
        cleanup_key("legacy");
    }

    #[test]
    fn a_failed_swap_restores_the_old_files() {
        let root = tempfile::tempdir().unwrap();
        let (staging, dir) = (root.path().join("new"), root.path().join("fx"));
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), b"old a").unwrap();
        std::fs::write(dir.join("b"), b"old b").unwrap();
        std::fs::write(staging.join("a"), b"new a").unwrap();
        // "b" is missing from staging: the swap fails at the second file.
        let names = vec!["a".to_owned(), "b".to_owned()];
        assert!(swap(&staging, &dir, &names).is_err());
        assert_eq!(std::fs::read(dir.join("a")).unwrap(), b"old a");
        assert_eq!(std::fs::read(dir.join("b")).unwrap(), b"old b");
        let leftovers: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(leftovers.len(), 2, "nothing set aside is left behind");
    }

    #[test]
    fn uninstall_refuses_a_running_fx_and_other_folders() {
        let root = tempfile::tempdir().unwrap();
        let t = target(root.path(), "b");
        install(
            &package("1.0.0"),
            &t,
            Options {
                desktop_shortcut: false,
            },
            &mut |_| {},
        )
        .unwrap();
        let mut running = run_as_fx(&t);
        let r = uninstall(&t, &[], &mut |_| {});
        let _ = running.kill();
        let _ = running.wait();
        assert!(r.is_err(), "running: refused");
        assert!(t.exe().exists(), "and nothing was removed");
        assert_eq!(installed_version(&t).as_deref(), Some("1.0.0"));
        let mut other = t.clone();
        other.dir = root.path().join("Documentos");
        std::fs::create_dir_all(&other.dir).unwrap();
        assert!(uninstall(&other, &[], &mut |_| {}).is_err());
        uninstall(&t, &[], &mut |_| {}).unwrap();
        cleanup_key("b");
        assert_eq!(display_version("0.3.0-beta+abc-dirty"), "0.3.0-beta");
    }
}
