//! Journal of temporary files (task T2.4, docs/SAFETY.md §2.5): survives a
//! killed process.
//!
//! A copy writes `.<name>.fx-<pid>-<job>.tmp` next to its destination and
//! renames it at the end; cancelling or failing deletes it. If the process
//! dies in between, nothing deletes it, so each temporary path is appended
//! to this process's journal before the copy starts. The journal stays open
//! without sharing for the whole life of the process; at the next start,
//! [`recover`] opens the journals it can (so their process is gone), deletes
//! the temporary files they list that still exist, and then the journal.
//!
//! Only names that look like our temporary files are ever deleted: a journal
//! can never make fx delete a user's file.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The journal of this process.
#[derive(Debug)]
pub(crate) struct Journal {
    file: Mutex<File>,
    path: PathBuf,
}

impl Journal {
    /// Creates this process's journal in `dir` (created if missing).
    pub(crate) fn create(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = dir.join(format!("pending-{}-{nanos}.log", std::process::id()));
        let file = exclusive(OpenOptions::new().create_new(true).write(true)).open(&path)?;
        Ok(Self {
            file: Mutex::new(file),
            path,
        })
    }

    /// Records `temp` before it is created. One line per path, as UTF-16 code
    /// units in hex, so any Windows name (even invalid UTF-16) round-trips.
    pub(crate) fn add(&self, temp: &Path) -> io::Result<()> {
        let line = encode(temp);
        let mut f = self.file.lock().unwrap_or_else(|e| e.into_inner());
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.flush()
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        // Every temporary file was renamed or deleted by then. The handle
        // closes after this, so the deletion is only scheduled: harmless.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Cleans up after processes that died in the middle of a copy: returns how
/// many temporary files were deleted. Journals still open (their process is
/// alive) are left alone.
pub fn recover(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_journal = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("pending-") && n.ends_with(".log"));
        if !is_journal {
            continue;
        }
        // Fails with a sharing violation while its process lives.
        let Ok(file) = exclusive(OpenOptions::new().read(true)).open(&path) else {
            continue;
        };
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if let Some(temp) = decode(&line)
                && is_temp_name(&temp)
                && std::fs::remove_file(&temp).is_ok()
            {
                removed += 1;
            }
        }
        let _ = std::fs::remove_file(&path);
    }
    removed
}

/// `.<name>.fx-<digits>-<digits>.tmp`: the only names [`recover`] deletes.
pub(crate) fn is_temp_name(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        // Our temporary names are built from the destination name; one that
        // is not valid Unicode is not checked here and simply not deleted.
        return false;
    };
    let Some(rest) = name.strip_prefix('.').and_then(|r| r.strip_suffix(".tmp")) else {
        return false;
    };
    let Some(at) = rest.rfind(".fx-") else {
        return false;
    };
    let (stem, ids) = (&rest[..at], &rest[at + 4..]);
    let mut parts = ids.split('-');
    let digits =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    !stem.is_empty() && digits(parts.next()) && digits(parts.next()) && parts.next().is_none()
}

#[cfg(windows)]
fn exclusive(options: &mut OpenOptions) -> &mut OpenOptions {
    use std::os::windows::fs::OpenOptionsExt;
    options.share_mode(0)
}

#[cfg(windows)]
fn encode(path: &Path) -> String {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .map(|u| format!("{u:04x}"))
        .collect()
}

#[cfg(windows)]
fn decode(line: &str) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    let line = line.trim();
    if line.is_empty() || !line.len().is_multiple_of(4) || !line.is_ascii() {
        return None;
    }
    let units: Option<Vec<u16>> = (0..line.len())
        .step_by(4)
        .map(|i| u16::from_str_radix(&line[i..i + 4], 16).ok())
        .collect();
    Some(PathBuf::from(std::ffi::OsString::from_wide(&units?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_temporary_names_qualify() {
        for ok in [
            ".foto.jpg.fx-1234-5.tmp",
            ".a.fx-1-1.tmp",
            ".nome com espaço.txt.fx-99-12.tmp",
        ] {
            assert!(is_temp_name(Path::new(ok)), "{ok}");
        }
        for bad in [
            "foto.jpg",
            "foto.jpg.fx-1-2.tmp",
            ".foto.jpg.fx-1-2.txt",
            ".foto.jpg.fx-a-2.tmp",
            ".foto.jpg.fx-1-2-3.tmp",
            ".fx-1-2.tmp",
            ".foto.fx--2.tmp",
        ] {
            assert!(!is_temp_name(Path::new(bad)), "{bad}");
        }
    }

    #[test]
    fn paths_round_trip_through_the_journal_encoding() {
        let p = PathBuf::from(r"C:\Dados\日本語 café\.x.fx-1-2.tmp");
        assert_eq!(decode(&encode(&p)), Some(p));
        assert_eq!(decode("zz"), None);
    }

    #[test]
    fn recover_skips_live_journals_and_deletes_only_listed_temporaries() {
        let t = tempfile::tempdir().unwrap();
        let dir = t.path().join("ops");
        let temp = t.path().join(".dados.bin.fx-777-3.tmp");
        let user = t.path().join("relatório.docx");
        std::fs::write(&temp, b"partial").unwrap();
        std::fs::write(&user, b"precious").unwrap();

        let live = Journal::create(&dir).unwrap();
        live.add(&temp).unwrap();
        assert_eq!(recover(&dir), 0, "a live journal is left alone");
        assert!(temp.exists());

        // A journal left by a dead process (not open by anyone).
        let dead = dir.join("pending-777-1.log");
        let lines = format!("{}\n{}\n", encode(&temp), encode(&user));
        std::fs::write(&dead, lines).unwrap();
        assert_eq!(recover(&dir), 1);
        assert!(!temp.exists(), "the temporary file is gone");
        assert_eq!(
            std::fs::read(&user).unwrap(),
            b"precious",
            "user file untouched"
        );
        assert!(live.path.exists(), "the live journal stays");
        assert!(!dead.exists(), "the dead journal is gone");
    }
}
