//! The timeline without noise (T6.2 follow-up): programs rewrite their own
//! files all the time (browser and editor databases, logs, caches under
//! `AppData` and `ProgramData`), and those changes buried the ones the user
//! made. They are hidden by default; one key shows them again.

use std::path::{Component, Path};

/// Folders where only programs write.
const APP_FOLDERS: [&str; 12] = [
    "appdata",
    "programdata",
    "$recycle.bin",
    "logs",
    "log",
    "cache",
    "caches",
    "temp",
    "tmp",
    "crashdumps",
    "dxcache",
    "shadercache",
];

/// Endings of files programs keep rewriting.
const NOISE_ENDINGS: [&str; 16] = [
    "-journal",
    "-wal",
    "-shm",
    ".tmp",
    ".temp",
    ".lock",
    ".lck",
    ".log",
    ".etl",
    ".ldb",
    ".pf",
    ".crdownload",
    ".partial",
    ".part",
    ".db-wal",
    ".sqlite-wal",
];

/// The change of `path` is a program's own bookkeeping, not the user's.
pub fn is_noise(path: &Path) -> bool {
    let in_app_folder = path.components().any(|c| match c {
        Component::Normal(n) => n
            .to_str()
            .is_some_and(|n| APP_FOLDERS.iter().any(|a| n.eq_ignore_ascii_case(a))),
        _ => false,
    });
    if in_app_folder {
        return true;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_lowercase();
    // Office's "~$report.docx" owner files and editors' "~" backups.
    name.starts_with("~$") || NOISE_ENDINGS.iter().any(|e| name.ends_with(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_bookkeeping_is_noise() {
        for p in [
            r"C:\Users\ana\AppData\Local\Google\Chrome\User Data\Default\History-journal",
            r"C:\Users\ana\AppData\Roaming\Code\User\workspaceStorage\state.vscdb",
            r"C:\ProgramData\Microsoft\Windows\WER\report.wer",
            r"D:\projeto\banco.sqlite-shm",
            r"D:\projeto\build.log",
            r"C:\Users\ana\Documents\~$relatório.docx",
            r"C:\Users\ana\Downloads\filme.mkv.crdownload",
            r"C:\Program Files (x86)\Steam\logs\webhelper.txt",
        ] {
            assert!(is_noise(Path::new(p)), "{p}");
        }
    }

    #[test]
    fn the_users_own_changes_are_not() {
        for p in [
            r"C:\Users\ana\Documents\relatório.docx",
            r"C:\Users\ana\Downloads\filme.mkv",
            r"D:\Fotos\praia.jpg",
            r"C:\Users\ana\Desktop\appdata-notas.txt",
        ] {
            assert!(!is_noise(Path::new(p)), "{p}");
        }
    }
}
