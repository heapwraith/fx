//! Reading an NTFS volume into an [`Index`] (MFT) and keeping it current (USN
//! journal), with `ntfs-reader` (docs/DECISIONS.md D-003). Requires
//! Administrator: without it every entry point fails with
//! `ErrorKind::PermissionDenied` and a clear message, never a panic.

use std::io;
use std::path::Path;
use std::thread;

use ntfs_reader::api::{FIRST_NORMAL_RECORD, FileId as NtfsFileId, NtfsAttributeType, ROOT_RECORD};
use ntfs_reader::errors::NtfsReaderError;
use ntfs_reader::file::NtfsFile;
use ntfs_reader::journal::{Journal, JournalOptions, NextUsn};
use ntfs_reader::mft::Mft;
use ntfs_reader::volume::Volume;

use crate::{FileId, Index, Record};

/// Message shown when the process is not elevated.
pub const NEEDS_ADMIN: &str =
    "reading the NTFS MFT requires Administrator rights: execute como Administrador";

const RECORD_NUMBER_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
const USN_REASON_FILE_CREATE: u32 = 0x0000_0100;
const USN_REASON_FILE_DELETE: u32 = 0x0000_0200;
const USN_REASON_RENAME_NEW_NAME: u32 = 0x0000_2000;

/// `\\.\C:` from `C`, `C:` or `C:\`.
fn device_path(volume: &str) -> io::Result<String> {
    let letter = volume.trim_end_matches(['\\', '/']).trim_end_matches(':');
    if letter.len() == 1 && letter.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(format!(r"\\.\{}:", letter.to_ascii_uppercase()))
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a volume letter: {volume:?} (expected e.g. \"C:\")"),
        ))
    }
}

fn to_io(e: NtfsReaderError) -> io::Error {
    match e {
        NtfsReaderError::ElevationError => {
            io::Error::new(io::ErrorKind::PermissionDenied, NEEDS_ADMIN)
        }
        NtfsReaderError::IOError(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            io::Error::new(io::ErrorKind::PermissionDenied, NEEDS_ADMIN)
        }
        NtfsReaderError::IOError(e) => e,
        other => io::Error::other(other.to_string()),
    }
}

fn open_volume(volume: &str) -> io::Result<Volume> {
    Volume::new(device_path(volume)?).map_err(to_io)
}

/// Reads the whole MFT of `volume` (e.g. `"C:"`) into an index, using all cores.
pub fn read_volume(volume: &str) -> io::Result<Index> {
    let mft = Mft::new(open_volume(volume)?).map_err(to_io)?;
    let first = FIRST_NORMAL_RECORD;
    let last = mft.max_record;
    let threads = thread::available_parallelism()
        .map_or(4, |n| n.get())
        .max(1) as u64;
    let chunk = (last.saturating_sub(first)).div_ceil(threads).max(1);

    let parts: Vec<io::Result<Index>> = thread::scope(|s| {
        let mft = &mft;
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let start = first + t * chunk;
                let end = (start + chunk).min(last);
                s.spawn(move || read_range(mft, start, end))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(io::Error::other("MFT reader thread panicked")))
            })
            .collect()
    });

    let mut index = Index::new(ROOT_RECORD);
    for part in parts {
        index.append(part?).map_err(io::Error::other)?;
    }
    index.shrink_to_fit();
    Ok(index)
}

fn read_range(mft: &Mft, start: u64, end: u64) -> io::Result<Index> {
    let mut index = Index::with_capacity(ROOT_RECORD, (end.saturating_sub(start)) as usize);
    for number in start..end {
        if !mft.record_exists(number) {
            continue;
        }
        let Some(file) = mft.get_record(number) else {
            continue;
        };
        if !file.is_used() || file.is_extension() {
            continue;
        }
        if let Some(record) = record_of(mft, &file) {
            index.insert(&record).map_err(io::Error::other)?;
        }
    }
    Ok(index)
}

/// Name, parent, size and last write time of one file (base record plus its
/// extension records). `None` for records without a usable name.
fn record_of(mft: &Mft, file: &NtfsFile<'_>) -> Option<Record> {
    let name = file.get_best_file_name(mft)?;
    // Copy out of the packed struct before slicing.
    let data = name.data;
    let len = usize::from(name.header.name_length).min(data.len());
    let mut size = 0u64;
    let mut modified = 0u64;
    for part in mft.file_records(file) {
        part.attributes(|att| {
            let type_id = att.header.type_id;
            if type_id == NtfsAttributeType::StandardInformation as u32 {
                if let Some(info) = att.as_standard_info() {
                    modified = info.modification_time;
                }
            } else if type_id == NtfsAttributeType::Data as u32 && att.header.name_length == 0 {
                // Unnamed stream only; for split streams the first extent has the size.
                if att.header.is_non_resident == 0 {
                    if let Some(h) = att.resident_header() {
                        size = u64::from(h.value_length);
                    }
                } else if let Some(h) = att.nonresident_header()
                    && h.lowest_vcn == 0
                {
                    size = h.data_size;
                }
            }
        });
    }
    Some(Record {
        id: file.number(),
        parent: name.parent(),
        name: data[..len].to_vec(),
        is_dir: file.is_directory(),
        size,
        modified,
    })
}

fn record_number(id: NtfsFileId) -> FileId {
    match id {
        NtfsFileId::Normal(n) => n & RECORD_NUMBER_MASK,
        NtfsFileId::Extended(id) => {
            let mut low = [0u8; 8];
            low.copy_from_slice(&id.Identifier[..8]);
            u64::from_le_bytes(low) & RECORD_NUMBER_MASK
        }
    }
}

/// Follows the USN journal of a volume from the moment it is created and applies
/// creations, renames/moves and deletions to an index.
pub struct JournalWatcher {
    journal: Journal,
}

impl std::fmt::Debug for JournalWatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JournalWatcher").finish_non_exhaustive()
    }
}

impl JournalWatcher {
    /// Starts at the current end of the journal (changes from now on).
    pub fn start(volume: &str) -> io::Result<Self> {
        let options = JournalOptions {
            next_usn: NextUsn::Next,
            ..Default::default()
        };
        let journal = Journal::new(open_volume(volume)?, options).map_err(to_io)?;
        Ok(Self { journal })
    }

    /// Reads pending journal records and applies them. Returns how many records
    /// changed the index.
    pub fn apply(&mut self, index: &mut Index) -> io::Result<usize> {
        let mut changed = 0;
        loop {
            let records = self.journal.read().map_err(to_io)?;
            if records.is_empty() {
                return Ok(changed);
            }
            for r in records {
                let id = record_number(r.file_id);
                if r.reason & USN_REASON_FILE_DELETE != 0 {
                    if index.remove(id) {
                        changed += 1;
                    }
                } else if r.reason & (USN_REASON_FILE_CREATE | USN_REASON_RENAME_NEW_NAME) != 0
                    && let Some(record) = record_from_disk(id, record_number(r.parent_id), &r.path)
                {
                    index.insert(&record).map_err(io::Error::other)?;
                    changed += 1;
                }
            }
        }
    }
}

/// Builds a record for a file reported by the journal, reading its current
/// metadata. `None` if it is already gone.
fn record_from_disk(id: FileId, parent: FileId, path: &Path) -> Option<Record> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path).ok()?;
    Some(Record {
        id,
        parent,
        name: path.file_name()?.encode_wide().collect(),
        is_dir: meta.is_dir(),
        size: meta.file_size(),
        modified: meta.last_write_time(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_paths() {
        assert_eq!(device_path("C:").unwrap(), r"\\.\C:");
        assert_eq!(device_path("d").unwrap(), r"\\.\D:");
        assert_eq!(device_path(r"e:\").unwrap(), r"\\.\E:");
        assert!(device_path("CD:").is_err());
        assert!(device_path(r"C:\Windows").is_err());
        assert!(device_path("").is_err());
    }

    #[test]
    fn without_admin_reading_fails_clearly() {
        // Normal test runs are not elevated; an elevated run reads the MFT instead.
        match read_volume("C:") {
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{e}");
                assert!(e.to_string().contains("Administrador"), "{e}");
            }
            Ok(index) => assert!(index.len() > 1000),
        }
        if let Err(e) = JournalWatcher::start("C:") {
            assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{e}");
        }
    }

    fn find(index: &Index, name: &str) -> Option<u32> {
        (0..index.slots() as u32).find(|&s| index.is_live(s) && index.name(s) == name)
    }

    /// Needs an elevated terminal: `cargo xwin test -p fx-index -- --ignored`.
    #[test]
    #[ignore = "needs Administrator (reads the MFT and USN journal of C:)"]
    fn index_c_and_follow_create_rename_delete() {
        let mut index = read_volume("C:").unwrap();
        assert!(index.len() > 10_000, "only {} files", index.len());
        let mut watcher = JournalWatcher::start("C:").unwrap();

        let dir = tempfile::tempdir().unwrap();
        let unique = format!("fx-usn-{}-{}", std::process::id(), index.len());
        let created = dir.path().join(format!("{unique}.txt"));
        std::fs::write(&created, b"fx").unwrap();
        watcher.apply(&mut index).unwrap();
        let slot = find(&index, &format!("{unique}.txt")).expect("created file not indexed");
        let path = index.path(slot).unwrap();
        assert!(created.ends_with(&path) || path.ends_with(created.file_name().unwrap()));

        let renamed = dir.path().join(format!("{unique}-renamed.txt"));
        std::fs::rename(&created, &renamed).unwrap();
        watcher.apply(&mut index).unwrap();
        assert!(
            find(&index, &format!("{unique}.txt")).is_none(),
            "old name still indexed"
        );
        assert!(
            find(&index, &format!("{unique}-renamed.txt")).is_some(),
            "new name missing"
        );

        std::fs::remove_file(&renamed).unwrap();
        watcher.apply(&mut index).unwrap();
        assert!(
            find(&index, &format!("{unique}-renamed.txt")).is_none(),
            "deleted file still indexed"
        );
    }
}
