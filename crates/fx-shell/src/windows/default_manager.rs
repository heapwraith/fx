//! fx as the default file manager (task T3.1, docs/SAFETY.md §1): folders,
//! drives and Win+E open in fx, through keys in `HKCU\Software\Classes`
//! only (no administrator, nothing machine-wide), and "revert" puts back
//! exactly what was there before.
//!
//! What fx sets (each with an empty `DelegateExecute`, which makes Explorer
//! use the command instead of its own COM handler):
//! - `Folder`, `Directory` and `Drive` → `shell\open\command`
//! - the "new window" verb of the Computer folder's CLSID (what Win+E runs)
//!
//! Before writing, every touched value and every key that did not exist are
//! recorded in a backup key; reverting restores those values and deletes
//! only the keys fx created. All functions take the root, so the tests run
//! under `HKCU\Software\fx-test-<pid>` and never touch the real keys.

use std::io;

use super::registry::{delete_tree, delete_value, exists, get, set};

/// Where the real setting lives (under `HKCU`).
pub const CLASSES: &str = r"Software\Classes";
/// Where the backup of what fx replaced lives (under `HKCU`).
pub const BACKUP: &str = r"Software\fx\DefaultManager";

/// The keys fx writes (relative to the classes root).
const KEYS: [&str; 4] = [
    r"Folder\shell\open\command",
    r"Directory\shell\open\command",
    r"Drive\shell\open\command",
    r"CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}\shell\opennewwindow\command",
];
/// The values fx writes in each key: the command and `DelegateExecute`.
const VALUES: [&str; 2] = ["", "DelegateExecute"];
/// Marks a value that did not exist before (in the backup).
const ABSENT: &str = "\u{1}absent";

/// The command fx registers for `key`: with the folder for the open verbs,
/// alone for Win+E (it opens no particular folder).
fn command(key: &str, exe: &str) -> String {
    if key.starts_with("CLSID") {
        format!("\"{exe}\"")
    } else {
        format!("\"{exe}\" \"%1\"")
    }
}

fn backup_name(key: &str, value: &str) -> String {
    format!("{key}|{value}")
}

/// Whether fx (at `exe`) is the default under `classes`.
pub fn is_default(classes: &str, exe: &str) -> bool {
    KEYS.iter()
        .all(|k| get(&format!(r"{classes}\{k}"), "").as_deref() == Some(command(k, exe).as_str()))
}

/// Makes fx (at `exe`) the default. `backup` keeps what was replaced; a
/// second call keeps the first backup (so revert always goes back to the
/// state before fx).
pub fn enable(classes: &str, backup: &str, exe: &str) -> io::Result<()> {
    if exe.contains('"') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            fx_core::tr!("caminho inválido", "invalid path"),
        ));
    }
    if !exists(backup) {
        for k in KEYS {
            let path = format!(r"{classes}\{k}");
            // The topmost component of the path that does not exist yet:
            // deleting it on revert removes exactly what fx created.
            let mut created = String::new();
            let mut current = classes.to_owned();
            for part in k.split('\\') {
                current = format!(r"{current}\{part}");
                if !exists(&current) {
                    created = current.clone();
                    break;
                }
            }
            set(backup, &format!("{k}|created"), &created)?;
            for v in VALUES {
                let old = get(&path, v).unwrap_or_else(|| ABSENT.to_owned());
                set(backup, &backup_name(k, v), &old)?;
            }
        }
    }
    for k in KEYS {
        let path = format!(r"{classes}\{k}");
        set(&path, "", &command(k, exe))?;
        set(&path, "DelegateExecute", "")?;
    }
    Ok(())
}

/// Goes back to the state before [`enable`]: restores the replaced values,
/// deletes the keys fx created, then the backup. Without a backup, nothing
/// to do.
pub fn revert(classes: &str, backup: &str) -> io::Result<()> {
    if !exists(backup) {
        return Ok(());
    }
    for k in KEYS {
        let path = format!(r"{classes}\{k}");
        let created = get(backup, &format!("{k}|created")).unwrap_or_default();
        if !created.is_empty() {
            delete_tree(&created)?;
            continue;
        }
        for v in VALUES {
            match get(backup, &backup_name(k, v)) {
                Some(old) if old == ABSENT => delete_value(&path, v)?,
                Some(old) => set(&path, v, &old)?,
                None => {}
            }
        }
    }
    delete_tree(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private registry area for one test, deleted at the end.
    struct Root(String);

    impl Root {
        fn new(name: &str) -> Self {
            let root = format!(r"Software\fx-test-{}-{name}", std::process::id());
            let _ = delete_tree(&root);
            Self(root)
        }
        fn classes(&self) -> String {
            format!(r"{}\Classes", self.0)
        }
        fn backup(&self) -> String {
            format!(r"{}\Backup", self.0)
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = delete_tree(&self.0);
        }
    }

    const EXE: &str = r"C:\Program Files\fx\fx.exe";

    #[test]
    fn enable_then_revert_on_an_empty_root_leaves_nothing() {
        let r = Root::new("empty");
        std::mem::drop(super::super::registry::create(&r.classes()).unwrap());
        assert!(!is_default(&r.classes(), EXE));
        enable(&r.classes(), &r.backup(), EXE).unwrap();
        assert!(is_default(&r.classes(), EXE));
        let path = format!(r"{}\{}", r.classes(), KEYS[3]);
        assert_eq!(
            get(&path, "").unwrap(),
            format!("\"{EXE}\""),
            "Win+E: no folder"
        );
        let folder = format!(r"{}\{}", r.classes(), KEYS[0]);
        assert_eq!(get(&folder, "").unwrap(), format!("\"{EXE}\" \"%1\""));
        assert_eq!(get(&path, "DelegateExecute").unwrap(), "");
        revert(&r.classes(), &r.backup()).unwrap();
        for top in ["Folder", "Directory", "Drive", "CLSID"] {
            assert!(
                !exists(&format!(r"{}\{top}", r.classes())),
                "{top} left behind"
            );
        }
        assert!(!exists(&r.backup()));
        assert!(!is_default(&r.classes(), EXE));
    }

    #[test]
    fn revert_restores_values_that_were_there_before() {
        let r = Root::new("previous");
        let folder = format!(r"{}\{}", r.classes(), KEYS[0]);
        set(&folder, "", r#""C:\Outro\gerenciador.exe" "%1""#).unwrap();
        set(
            &folder,
            "DelegateExecute",
            "{11dbb47c-a525-400b-9e80-a54615a090c0}",
        )
        .unwrap();
        // Something else under Folder that fx must not delete.
        set(&format!(r"{}\Folder\shell\outro", r.classes()), "", "x").unwrap();
        enable(&r.classes(), &r.backup(), EXE).unwrap();
        // A second enable keeps the first backup.
        enable(&r.classes(), &r.backup(), r"D:\fx\fx.exe").unwrap();
        revert(&r.classes(), &r.backup()).unwrap();
        assert_eq!(
            get(&folder, "").unwrap(),
            r#""C:\Outro\gerenciador.exe" "%1""#
        );
        assert_eq!(
            get(&folder, "DelegateExecute").unwrap(),
            "{11dbb47c-a525-400b-9e80-a54615a090c0}"
        );
        assert_eq!(
            get(&format!(r"{}\Folder\shell\outro", r.classes()), "").unwrap(),
            "x"
        );
        assert!(
            !exists(&format!(r"{}\Directory", r.classes())),
            "created keys are removed"
        );
        // Reverting again is harmless.
        revert(&r.classes(), &r.backup()).unwrap();
    }

    #[test]
    fn the_real_keys_are_named_only_by_the_app() {
        // Guard: production paths are constants used by the app alone; tests
        // above only use their own root.
        assert_eq!(CLASSES, r"Software\Classes");
        assert!(BACKUP.starts_with(r"Software\fx\"));
        assert!(
            enable("x", "y", r#"C:\a"b.exe"#).is_err(),
            "quotes are refused"
        );
    }
}
