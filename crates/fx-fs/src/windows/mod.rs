//! Windows implementation of folder enumeration (three strategies, task T0.3).

mod find;
mod handle_info;

use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::{Duration, SystemTime};

use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_READONLY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};

use crate::{Attributes, Batcher, Entry, Strategy};

/// Reads `dir` with `strategy`, sending entries in batches over `tx`.
/// `.` and `..` are never included. Returns the number of entries sent; stops early
/// (still `Ok`) if the receiver is dropped.
pub fn enumerate(strategy: Strategy, dir: &Path, tx: &Sender<Vec<Entry>>) -> io::Result<usize> {
    let mut batcher = Batcher::new(tx);
    match strategy {
        Strategy::StdReadDir => std_read_dir(dir, &mut batcher)?,
        Strategy::FindFirstFileEx => find::read(dir, &mut batcher)?,
        Strategy::FileIdBothDirInfo => handle_info::read(dir, &mut batcher)?,
    }
    Ok(batcher.finish())
}

fn std_read_dir(dir: &Path, batcher: &mut Batcher<'_>) -> io::Result<()> {
    use std::os::windows::fs::MetadataExt;
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        // On Windows this metadata comes from the directory listing itself
        // (no extra system call per file).
        let meta = item.metadata()?;
        let entry = Entry {
            name: item.file_name(),
            size: meta.file_size(),
            modified: filetime_to_system_time(meta.last_write_time()),
            attributes: map_attributes(meta.file_attributes()),
        };
        if !batcher.push(entry) {
            break;
        }
    }
    Ok(())
}

/// Builds an [`Entry`] from raw Win32 fields. Returns `None` for `.` and `..`.
fn make_entry(name: &[u16], size: u64, last_write: u64, attributes: u32) -> Option<Entry> {
    const DOT: u16 = b'.' as u16;
    if name == [DOT] || name == [DOT, DOT] {
        return None;
    }
    Some(Entry {
        name: OsString::from_wide(name),
        size,
        modified: filetime_to_system_time(last_write),
        attributes: map_attributes(attributes),
    })
}

fn map_attributes(raw: u32) -> Attributes {
    let mut a = Attributes::empty();
    for (win, neutral) in [
        (FILE_ATTRIBUTE_DIRECTORY.0, Attributes::DIRECTORY),
        (FILE_ATTRIBUTE_HIDDEN.0, Attributes::HIDDEN),
        (FILE_ATTRIBUTE_SYSTEM.0, Attributes::SYSTEM),
        (FILE_ATTRIBUTE_READONLY.0, Attributes::READONLY),
        (FILE_ATTRIBUTE_REPARSE_POINT.0, Attributes::LINK),
    ] {
        if raw & win != 0 {
            a = a.with(neutral);
        }
    }
    a
}

/// FILETIME (100 ns ticks since 1601-01-01 UTC) to `SystemTime`.
fn filetime_to_system_time(ticks: u64) -> SystemTime {
    const TICKS_1601_TO_1970: u64 = 116_444_736_000_000_000;
    let to_duration = |t: u64| Duration::new(t / 10_000_000, ((t % 10_000_000) * 100) as u32);
    if ticks >= TICKS_1601_TO_1970 {
        SystemTime::UNIX_EPOCH + to_duration(ticks - TICKS_1601_TO_1970)
    } else {
        SystemTime::UNIX_EPOCH
            .checked_sub(to_duration(TICKS_1601_TO_1970 - ticks))
            .unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// Absolute `\\?\` form of `dir` as UTF-16 without a trailing NUL or separator,
/// so paths longer than MAX_PATH (260) work with the raw Win32 APIs.
fn verbatim_wide(dir: &Path) -> io::Result<Vec<u16>> {
    let abs = std::path::absolute(dir)?;
    let wide: Vec<u16> = abs.as_os_str().encode_wide().collect();
    let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
    let mut out = if wide.starts_with(&w(r"\\?\")) || wide.starts_with(&w(r"\\.\")) {
        wide
    } else if wide.starts_with(&w(r"\\")) {
        let mut v = w(r"\\?\UNC\");
        v.extend_from_slice(&wide[2..]);
        v
    } else {
        let mut v = w(r"\\?\");
        v.extend_from_slice(&wide);
        v
    };
    while out.last() == Some(&(b'\\' as u16)) {
        out.pop();
    }
    Ok(out)
}

/// Converts a `windows-rs` error into `io::Error`, keeping the Win32 code so
/// `kind()` works (e.g. `PermissionDenied`, `NotFound`).
fn to_io(e: windows::core::Error) -> io::Error {
    let code = e.code().0 as u32;
    if code & 0xFFFF_0000 == 0x8007_0000 {
        io::Error::from_raw_os_error((code & 0xFFFF) as i32)
    } else {
        io::Error::from_raw_os_error(code as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn verbatim_forms() {
        assert_eq!(
            verbatim_wide(Path::new(r"C:\a\b\")).unwrap(),
            wide(r"\\?\C:\a\b")
        );
        assert_eq!(
            verbatim_wide(Path::new(r"\\server\share\x")).unwrap(),
            wide(r"\\?\UNC\server\share\x")
        );
        assert_eq!(
            verbatim_wide(Path::new(r"\\?\C:\x")).unwrap(),
            wide(r"\\?\C:\x")
        );
    }

    #[test]
    fn filetime_conversion() {
        assert_eq!(
            filetime_to_system_time(116_444_736_000_000_000),
            SystemTime::UNIX_EPOCH
        );
        assert_eq!(
            filetime_to_system_time(116_444_736_000_000_000 + 15_000_000),
            SystemTime::UNIX_EPOCH + Duration::from_millis(1500)
        );
        assert!(filetime_to_system_time(0) < SystemTime::UNIX_EPOCH);
    }

    #[test]
    fn dots_are_skipped_and_attributes_mapped() {
        assert!(make_entry(&wide("."), 0, 0, 16).is_none());
        assert!(make_entry(&wide(".."), 0, 0, 16).is_none());
        let e = make_entry(&wide("..a"), 7, 0, 16 | 2).unwrap();
        assert_eq!(e.size, 7);
        assert!(e.attributes.is_dir());
        assert!(e.attributes.contains(Attributes::HIDDEN));
        assert!(!e.attributes.contains(Attributes::READONLY));
    }

    // ---- behaviour shared by every strategy, on real folders (tempfile) ----

    fn list(strategy: Strategy, dir: &Path) -> io::Result<Vec<Entry>> {
        let (tx, rx) = channel();
        let sent = enumerate(strategy, dir, &tx)?;
        drop(tx);
        let mut all: Vec<Entry> = rx.iter().flatten().collect();
        assert_eq!(all.len(), sent, "{strategy:?}: count mismatch");
        all.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(all)
    }

    #[test]
    fn empty_folder() {
        let tmp = tempfile::tempdir().unwrap();
        for s in Strategy::ALL {
            assert!(list(s, tmp.path()).unwrap().is_empty(), "{s:?}");
        }
    }

    #[test]
    fn missing_folder_is_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nope");
        for s in Strategy::ALL {
            let err = list(s, &missing).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::NotFound, "{s:?}: {err}");
        }
    }

    #[test]
    fn unicode_names_sizes_times_and_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let names = [
            "relatório.txt",
            "日本語.md",
            "😀 emoji",
            "👨‍👩‍👧.zip",
            "Ελληνικά",
        ];
        for (i, n) in names.iter().enumerate() {
            std::fs::write(tmp.path().join(n), vec![b'x'; i * 10]).unwrap();
        }
        std::fs::create_dir(tmp.path().join("pasta-çã")).unwrap();
        let expected_mtime = std::fs::metadata(tmp.path().join(names[1]))
            .unwrap()
            .modified()
            .unwrap();

        let reference = list(Strategy::StdReadDir, tmp.path()).unwrap();
        assert_eq!(reference.len(), names.len() + 1);
        for s in Strategy::ALL {
            let got = list(s, tmp.path()).unwrap();
            assert_eq!(got, reference, "{s:?} differs from std");
            for (i, n) in names.iter().enumerate() {
                let e = got.iter().find(|e| e.name == *n).unwrap();
                assert_eq!(e.size, (i * 10) as u64, "{s:?} {n}");
                assert!(!e.attributes.is_dir());
            }
            let md = got.iter().find(|e| e.name == names[1]).unwrap();
            assert_eq!(md.modified, expected_mtime, "{s:?}");
            assert!(
                got.iter()
                    .find(|e| e.name == "pasta-çã")
                    .unwrap()
                    .attributes
                    .is_dir()
            );
        }
    }

    #[test]
    fn path_longer_than_260_characters() {
        let tmp = tempfile::tempdir().unwrap();
        let mut deep = tmp.path().to_path_buf();
        for i in 0..4 {
            deep.push(format!("{i}-{}", "ç".repeat(90)));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("arquivo-longo.txt"), b"abc").unwrap();
        assert!(deep.as_os_str().encode_wide().count() > 260);
        for s in Strategy::ALL {
            let got = list(s, &deep).unwrap();
            assert_eq!(got.len(), 1, "{s:?}");
            assert_eq!(got[0].name, "arquivo-longo.txt");
            assert_eq!(got[0].size, 3);
        }
    }

    #[test]
    fn many_entries_arrive_in_several_batches() {
        let tmp = tempfile::tempdir().unwrap();
        let n = crate::BATCH_MAX_ENTRIES + 500;
        for i in 0..n {
            std::fs::File::create(tmp.path().join(format!("{i:05}"))).unwrap();
        }
        for s in Strategy::ALL {
            let (tx, rx) = channel();
            assert_eq!(enumerate(s, tmp.path(), &tx).unwrap(), n, "{s:?}");
            drop(tx);
            let batches: Vec<usize> = rx.iter().map(|b| b.len()).collect();
            assert!(batches.len() >= 2, "{s:?}: {batches:?}");
            assert_eq!(batches.iter().sum::<usize>(), n);
        }
    }

    /// Runs `icacls` (part of Windows) on a path inside a tempfile folder.
    fn icacls(path: &Path, args: &[&str]) {
        let status = std::process::Command::new("icacls")
            .arg(path)
            .args(args)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "icacls {args:?} failed");
    }

    #[test]
    fn file_without_read_permission_is_still_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = tmp.path().join("secret.bin");
        std::fs::write(&secret, b"12345").unwrap();
        // Deny read to Everyone (S-1-1-0); listing needs only the folder's permission.
        icacls(&secret, &["/deny", "*S-1-1-0:(R)"]);
        assert!(std::fs::read(&secret).is_err(), "deny ACL not effective");

        for s in Strategy::ALL {
            let got = list(s, tmp.path()).unwrap();
            assert_eq!(got.len(), 1, "{s:?}");
            assert_eq!(got[0].name, "secret.bin");
            assert_eq!(got[0].size, 5, "{s:?}");
        }
        icacls(&secret, &["/remove:d", "*S-1-1-0"]);
    }

    #[test]
    fn folder_without_list_permission_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let locked = tmp.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("x"), b"").unwrap();
        // (RD) = list folder / read data.
        icacls(&locked, &["/deny", "*S-1-1-0:(RD)"]);
        for s in Strategy::ALL {
            let err = list(s, &locked).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::PermissionDenied, "{s:?}: {err}");
        }
        icacls(&locked, &["/remove:d", "*S-1-1-0"]);
    }
}
