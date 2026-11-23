//! Archives (T6.7, D-069): a ZIP is read from its central directory
//! (`fx_core::zip_list`, nothing decompressed to list it) and browsed as a
//! folder. Entries are extracted and ZIPs made with the `zip` crate; 7z and
//! tar (plain, gzip, bzip2, xz) are extracted whole with `sevenz-rust2`,
//! `tar`, `flate2`, `bzip2` and `lzma-rust2`. All of them are Rust crates,
//! the same on Windows and Linux; no helper process. Which entry goes where
//! is decided here with every name part checked
//! (`fx_core::archive::is_safe_name`); links are never extracted and a file
//! is never replaced.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use fx_core::zip_list::{self, ZipEntry};

use crate::{Attributes, Entry};

/// The most entries read from a ZIP.
const MAX_ENTRIES: usize = 2_000_000;

/// The archive file `path` is in (or is), if any: the nearest ancestor that
/// is a file with an archive name. Touches the disk: worker threads only.
pub fn archive_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(fx_core::archive::is_archive)
                && p.is_file()
        })
        .map(Path::to_path_buf)
}

/// The entries of a ZIP, in central directory order.
pub fn read_zip(zip: &Path) -> io::Result<Vec<ZipEntry>> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "not a readable ZIP");
    let mut f = File::open(zip)?;
    let len = f.metadata()?.len();
    let tail_len = len.min(66_000);
    let tail_off = len - tail_len;
    let mut tail = vec![0u8; tail_len as usize];
    f.seek(SeekFrom::Start(tail_off))?;
    f.read_exact(&mut tail)?;
    let mut read_at = |off: u64, n: usize| {
        let mut b = vec![0u8; n];
        f.seek(SeekFrom::Start(off)).ok()?;
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    let (dir_off, dir_len, total) =
        zip_list::central_directory(&tail, tail_off, &mut read_at).ok_or_else(bad)?;
    if dir_len > 1 << 30 {
        return Err(bad());
    }
    let dir = read_at(dir_off, dir_len as usize).ok_or_else(bad)?;
    Ok(zip_list::parse_central(&dir, total, MAX_ENTRIES)
        .ok_or_else(bad)?
        .entries)
}

/// The parts of `path` below the archive `zip`.
pub fn inner_parts(zip: &Path, path: &Path) -> Vec<String> {
    path.strip_prefix(zip)
        .map(|rel| {
            rel.components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The items of the folder `path` inside the ZIP `zip`, as entries.
/// `offset(local)` is the UTC offset (seconds) at that local time.
pub fn list_zip_folder(
    zip: &Path,
    path: &Path,
    offset: impl Fn(i64) -> i64,
) -> io::Result<Vec<Entry>> {
    let entries = read_zip(zip)?;
    let inner = inner_parts(zip, path);
    let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
    Ok(fx_core::archive::zip_children(&entries, &inner)
        .into_iter()
        .map(|item| {
            let local = fx_core::archive::dos_to_local_secs(item.dos_time);
            let utc = if local == 0 { 0 } else { local - offset(local) };
            Entry {
                name: item.name.into(),
                size: item.size,
                modified: SystemTime::UNIX_EPOCH + Duration::from_secs(utc.max(0) as u64),
                attributes: if item.is_dir {
                    Attributes::DIRECTORY
                } else {
                    Attributes::empty()
                },
            }
        })
        .collect())
}

/// A fresh, empty folder under `<temp>/fx-extract`.
pub fn staging_dir() -> io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join("fx-extract");
    std::fs::create_dir_all(&base)?;
    // Leftovers of earlier extractions (a day old) go first.
    if let Ok(old) = std::fs::read_dir(&base) {
        for e in old.flatten() {
            let stale = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(86_400));
            if stale {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    loop {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!("{}-{n}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

fn link_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        fx_core::tr!(
            "o arquivo compactado traz um link; a extração parou",
            "the archive holds a link; extraction stopped"
        ),
    )
}

fn invalid(e: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}

/// `dest` joined with the parts of an entry name (`/` or `\` separated),
/// if every part is safe; `None` for the archive root itself.
fn target_of(dest: &Path, name: &str) -> io::Result<Option<PathBuf>> {
    let parts: Vec<&str> = name
        .split(['/', '\\'])
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.is_empty() {
        return Ok(None);
    }
    if !parts.iter().all(|p| fx_core::archive::is_safe_name(p)) {
        return Err(invalid(format!("unsafe entry name {name:?}")));
    }
    let mut t = dest.to_path_buf();
    t.extend(parts);
    Ok(Some(t))
}

/// Writes `data` into the new file `target` (never replacing one).
fn write_new(target: &Path, data: &mut dyn Read) -> io::Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = BufWriter::new(File::create_new(target)?);
    io::copy(data, &mut out)?;
    out.into_inner().map_err(io::IntoInnerError::into_error)?;
    Ok(())
}

/// Extracts the ZIP entries of `plan` (`(entry index, path parts)`, from
/// `fx_core::archive::zip_plan`) into the empty folder `dest`, calling
/// `progress(done, total)` as they are written. Never replaces a file.
pub fn extract_zip(
    zip: &Path,
    plan: &[(usize, Vec<String>)],
    dest: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> io::Result<()> {
    let entries = read_zip(zip)?;
    let mut ar = zip::ZipArchive::new(BufReader::new(File::open(zip)?)).map_err(invalid)?;
    let total = plan.len();
    for (done, (k, parts)) in plan.iter().enumerate() {
        let expected = entries.get(*k).ok_or_else(|| invalid("entry index"))?;
        let mut f = ar.by_index(*k).map_err(invalid)?;
        // Our central directory reader and the crate's must agree.
        if f.name().map_err(invalid)?.trim_end_matches('/') != expected.name.trim_end_matches('/') {
            return Err(invalid("ZIP entries out of step"));
        }
        if f.is_symlink() {
            return Err(link_error());
        }
        if !parts.iter().all(|p| fx_core::archive::is_safe_name(p)) {
            return Err(invalid("unsafe entry name"));
        }
        let mut target = dest.to_path_buf();
        target.extend(parts);
        if f.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            write_new(&target, &mut f)?;
        }
        progress(done + 1, total);
    }
    Ok(())
}

/// The kind of a non-ZIP archive, by its name.
enum Other {
    SevenZip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
}

fn other_kind(name: &str) -> Option<Other> {
    let n = name.to_ascii_lowercase();
    let ends = |e: &[&str]| e.iter().any(|e| n.ends_with(e));
    Some(if ends(&[".7z"]) {
        Other::SevenZip
    } else if ends(&[".tar.gz", ".tgz"]) {
        Other::TarGz
    } else if ends(&[".tar.bz2", ".tbz2"]) {
        Other::TarBz2
    } else if ends(&[".tar.xz", ".txz"]) {
        Other::TarXz
    } else if ends(&[".tar"]) {
        Other::Tar
    } else {
        return None;
    })
}

/// Extracts a whole non-ZIP archive (7z; tar plain, gzip, bzip2 or xz)
/// into the empty folder `dest`, calling `progress(items)` as items are
/// written.
pub fn extract_all_other(
    archive: &Path,
    dest: &Path,
    progress: &mut dyn FnMut(usize),
) -> io::Result<()> {
    let name = archive
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let kind = other_kind(&name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            fx_core::tr!(
                "este tipo de arquivo compactado não é suportado",
                "this kind of archive is not supported"
            ),
        )
    })?;
    let file = BufReader::new(File::open(archive)?);
    let reader: Box<dyn Read> = match kind {
        Other::SevenZip => return extract_7z(archive, dest, progress),
        Other::Tar => Box::new(file),
        Other::TarGz => Box::new(flate2::bufread::MultiGzDecoder::new(file)),
        Other::TarBz2 => Box::new(bzip2::bufread::MultiBzDecoder::new(file)),
        Other::TarXz => Box::new(lzma_rust2::XzReader::new(file, true)),
    };
    let mut ar = tar::Archive::new(reader);
    let mut done = 0;
    for e in ar.entries()? {
        let mut e = e?;
        let kind = e.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            return Err(link_error());
        }
        if !(kind.is_dir() || kind.is_file() || kind.is_contiguous()) {
            continue; // metadata records (pax, GNU long names are read by the crate)
        }
        let path = e.path_bytes().into_owned();
        let name = std::str::from_utf8(&path).map_err(|_| invalid("entry name is not UTF-8"))?;
        let Some(target) = target_of(dest, name)? else {
            continue;
        };
        if kind.is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            write_new(&target, &mut e)?;
        }
        done += 1;
        progress(done);
    }
    Ok(())
}

fn extract_7z(archive: &Path, dest: &Path, progress: &mut dyn FnMut(usize)) -> io::Result<()> {
    /// FILE_ATTRIBUTE_REPARSE_POINT, and the Unix mode kept in the high
    /// half when bit 15 is set.
    const REPARSE: u32 = 0x400;
    const UNIX: u32 = 0x8000;
    let mut ar =
        sevenz_rust2::ArchiveReader::new(File::open(archive)?, sevenz_rust2::Password::empty())
            .map_err(invalid)?;
    let mut done = 0;
    let mut failure: Option<io::Error> = None;
    ar.for_each_entries(|entry, data| {
        let attrs = entry.windows_attributes();
        let unix_link = attrs & UNIX != 0 && (attrs >> 16) & 0o170000 == 0o120000;
        let step = if attrs & REPARSE != 0 || unix_link {
            Err(link_error())
        } else {
            target_of(dest, entry.name()).and_then(|t| match t {
                None => Ok(()),
                Some(t) if entry.is_directory() => std::fs::create_dir_all(&t),
                Some(t) => write_new(&t, data),
            })
        };
        match step {
            Ok(()) => {
                done += 1;
                progress(done);
                Ok(true)
            }
            Err(e) => {
                failure = Some(e);
                Err(sevenz_rust2::Error::from(io::Error::other("stopped")))
            }
        }
    })
    .map_err(|e| failure.take().unwrap_or_else(|| invalid(e)))
}

/// `(source path or None for a folder, entry name, modified)` for
/// everything in `sources`, folders walked (links skipped).
fn zip_inputs(sources: &[PathBuf]) -> io::Result<Vec<(Option<PathBuf>, String, SystemTime)>> {
    let mut out = Vec::new();
    for src in sources {
        let Some(top) = src.file_name().and_then(|n| n.to_str()) else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "name"));
        };
        let mut stack = vec![(src.clone(), top.to_owned())];
        while let Some((path, name)) = stack.pop() {
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if meta.is_dir() {
                out.push((None, format!("{name}/"), modified));
                for e in std::fs::read_dir(&path)? {
                    let e = e?;
                    if let Some(n) = e.file_name().to_str() {
                        stack.push((e.path(), format!("{name}/{n}")));
                    }
                }
            } else {
                out.push((Some(path), name, modified));
            }
        }
    }
    Ok(out)
}

/// Makes the ZIP `zip` (which must not exist) holding `sources`, calling
/// `progress(done, total)` per item. `offset(utc)` is the UTC offset
/// (seconds) at that instant, for the local times a ZIP keeps. The name is
/// taken first (never replacing anything), the data written under a
/// temporary name in the same folder and moved over it; both removed on
/// failure.
pub fn make_zip(
    sources: &[PathBuf],
    zip: &Path,
    offset: &dyn Fn(i64) -> i64,
    progress: &mut dyn FnMut(usize, usize),
) -> io::Result<()> {
    let inputs = zip_inputs(sources)?;
    File::create_new(zip)?;
    let dir = zip.parent().unwrap_or(Path::new("."));
    let temp = dir.join(fx_core::temp_name::temp_name(
        zip.file_name().unwrap_or_default(),
        std::process::id(),
        0,
    ));
    let result =
        write_zip(&inputs, &temp, offset, progress).and_then(|()| std::fs::rename(&temp, zip));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        let _ = std::fs::remove_file(zip);
    }
    result
}

fn write_zip(
    inputs: &[(Option<PathBuf>, String, SystemTime)],
    temp: &Path,
    offset: &dyn Fn(i64) -> i64,
    progress: &mut dyn FnMut(usize, usize),
) -> io::Result<()> {
    use zip::write::SimpleFileOptions;
    let mut w = zip::ZipWriter::new(BufWriter::new(File::create_new(temp)?));
    let total = inputs.len();
    for (done, (source, name, modified)) in inputs.iter().enumerate() {
        let utc = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let dos = fx_core::archive::local_secs_to_dos(utc + offset(utc));
        let time =
            zip::DateTime::try_from_msdos((dos >> 16) as u16, dos as u16).unwrap_or_default();
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(time);
        match source {
            None => w.add_directory(name.as_str(), options).map_err(invalid)?,
            Some(path) => {
                let mut f = File::open(path)?;
                let large = f.metadata()?.len() >= u64::from(u32::MAX);
                w.start_file(name.as_str(), options.large_file(large))
                    .map_err(invalid)?;
                io::copy(&mut f, &mut w)?;
            }
        }
        progress(done + 1, total);
    }
    let out = w.finish().map_err(invalid)?;
    out.into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip_with_unicode_names() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let src = d.join("origem");
        std::fs::create_dir_all(src.join("pasta ção").join("vazia")).unwrap();
        std::fs::write(src.join("pasta ção").join("ação 日本.txt"), b"ola").unwrap();
        std::fs::write(src.join("a b's.txt"), b"0123456789").unwrap();
        let zip = d.join("t.zip");
        let mut last = (0, 0);
        make_zip(
            &[src.join("pasta ção"), src.join("a b's.txt")],
            &zip,
            &|_| 0,
            &mut |k, n| last = (k, n),
        )
        .unwrap();
        assert_eq!(last, (4, 4), "2 folders and 2 files");
        assert!(
            make_zip(&[src.join("a b's.txt")], &zip, &|_| 0, &mut |_, _| {}).is_err(),
            "never replaces"
        );
        assert!(zip.is_file(), "the existing ZIP stays");
        assert_eq!(archive_of(&zip.join("pasta ção")), Some(zip.clone()));
        assert_eq!(archive_of(&src), None);
        let top = list_zip_folder(&zip, &zip, |_| 0).unwrap();
        let names: Vec<_> = top
            .iter()
            .map(|e| e.name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a b's.txt", "pasta ção"]);
        assert_eq!(top[0].size, 10);
        assert!(top[1].attributes.is_dir());
        // The file's own time, to the 2 s a ZIP keeps.
        let file_time = std::fs::metadata(src.join("a b's.txt"))
            .unwrap()
            .modified()
            .unwrap();
        let gap = file_time
            .duration_since(top[0].modified)
            .unwrap_or_else(|e| e.duration());
        assert!(gap <= Duration::from_secs(2), "{gap:?}");
        let inner = list_zip_folder(&zip, &zip.join("pasta ção"), |_| 0).unwrap();
        let names: Vec<_> = inner
            .iter()
            .map(|e| e.name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["ação 日本.txt", "vazia"]);
        // Extract the folder.
        let entries = read_zip(&zip).unwrap();
        let plan = fx_core::archive::zip_plan(&entries, &[], &["pasta ção"]);
        let out = staging_dir().unwrap();
        let mut steps = 0;
        extract_zip(&zip, &plan, &out, &mut |_, _| steps += 1).unwrap();
        assert_eq!(steps, plan.len());
        assert_eq!(
            std::fs::read(out.join("pasta ção").join("ação 日本.txt")).unwrap(),
            b"ola"
        );
        assert!(out.join("pasta ção").join("vazia").is_dir());
        assert!(!out.join("a b's.txt").exists());
        assert!(
            extract_zip(&zip, &plan, &out, &mut |_, _| {}).is_err(),
            "never replaces"
        );
        std::fs::remove_dir_all(&out).unwrap();
    }

    fn tar_bytes(link: bool) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(3);
        h.set_cksum();
        b.append_data(&mut h, "pasta ção/x 日本.txt", &b"abc"[..])
            .unwrap();
        if link {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            b.append_link(&mut h, "atalho", "../../fora").unwrap();
        }
        b.into_inner().unwrap()
    }

    #[test]
    fn tar_and_tar_gz_are_extracted_whole() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::write(d.join("t.tar"), tar_bytes(false)).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes(false)).unwrap();
        std::fs::write(d.join("t.tar.gz"), gz.finish().unwrap()).unwrap();
        for name in ["t.tar", "t.tar.gz"] {
            let out = d.join(format!("out-{name}"));
            std::fs::create_dir(&out).unwrap();
            let mut n = 0;
            extract_all_other(&d.join(name), &out, &mut |k| n = k).unwrap();
            assert_eq!(n, 1, "{name}");
            assert_eq!(
                std::fs::read(out.join("pasta ção").join("x 日本.txt")).unwrap(),
                b"abc"
            );
        }
        std::fs::write(d.join("l.tar"), tar_bytes(true)).unwrap();
        let out = d.join("out-link");
        std::fs::create_dir(&out).unwrap();
        assert!(extract_all_other(&d.join("l.tar"), &out, &mut |_| {}).is_err());
        assert!(!out.join("atalho").exists(), "links never extracted");
    }

    #[test]
    fn a_7z_is_extracted_whole() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir_all(d.join("src").join("pasta ção")).unwrap();
        std::fs::write(d.join("src").join("pasta ção").join("y.txt"), b"7z!").unwrap();
        let archive = d.join("t.7z");
        sevenz_rust2::compress_to_path(d.join("src"), &archive).unwrap();
        let out = d.join("out");
        std::fs::create_dir(&out).unwrap();
        let mut n = 0;
        extract_all_other(&archive, &out, &mut |k| n = k).unwrap();
        assert!(n >= 1);
        assert_eq!(
            std::fs::read(out.join("pasta ção").join("y.txt")).unwrap(),
            b"7z!"
        );
    }
}
