//! Archives (T6.7): a ZIP is read from its central directory (nothing is
//! decompressed to list it) and browsed as a folder; entries are extracted
//! and ZIPs made by the .NET `ZipFile` of Windows, run in a hidden
//! PowerShell with no window and no prompt. Other archives (7z, RAR, tar)
//! are extracted whole by the `tar.exe` of Windows. Paths reach PowerShell
//! through a UTF-8 list file, never through the command line; which entry
//! goes where is decided here, with every name checked
//! (`fx_core::archive`), and the result is checked again for links.

use std::io::{self, BufRead, Read, Seek, SeekFrom};

use super::powershell::{list_file, ps_quote, run_powershell};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use fx_core::zip_list::{self, ZipEntry};

use crate::{Attributes, Entry};

/// CREATE_NO_WINDOW: no console window for the helper process.
const NO_WINDOW: u32 = 0x0800_0000;
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
    let mut f = std::fs::File::open(zip)?;
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

/// A fresh, empty folder under `%TEMP%\fx-extract`.
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
    let lines: Vec<String> = plan
        .iter()
        .filter_map(|(k, parts)| {
            let mut target = dest.to_path_buf();
            for p in parts {
                target.push(p);
            }
            let mut t = target.to_str()?.to_owned();
            if entries.get(*k)?.is_dir {
                t.push('\\');
            }
            Some(format!("{k}\t{t}"))
        })
        .collect();
    let list = list_file(&lines)?;
    let script = format!(
        "$ErrorActionPreference='Stop'\n\
         Add-Type -AssemblyName System.IO.Compression\n\
         Add-Type -AssemblyName System.IO.Compression.FileSystem\n\
         $lines=[IO.File]::ReadAllLines({list},[Text.Encoding]::UTF8)\n\
         $z=[IO.Compression.ZipFile]::OpenRead({zip})\n\
         try {{ $n=0\n\
           foreach($l in $lines) {{ if($l -eq '') {{ continue }}\n\
             $t=$l.Split([char]9,2); $e=$z.Entries[[int]$t[0]]; $d=$t[1]\n\
             if($d.EndsWith('\\')) {{ [void][IO.Directory]::CreateDirectory($d) }}\n\
             else {{ [void][IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($d))\n\
               [IO.Compression.ZipFileExtensions]::ExtractToFile($e,$d,$false) }}\n\
             $n++; [Console]::Out.WriteLine($n); [Console]::Out.Flush() }}\n\
         }} finally {{ $z.Dispose() }}\n",
        list = ps_quote(&list.0),
        zip = ps_quote(zip),
    );
    let total = lines.len();
    run_powershell(&script, &mut |l| {
        if let Ok(n) = l.parse::<usize>() {
            progress(n, total);
        }
    })?;
    check_tree(dest)
}

/// Extracts a whole non-ZIP archive (7z, RAR, tar…) into the empty folder
/// `dest` with the `tar.exe` of Windows, calling `progress(items)` as items
/// are written.
pub fn extract_all_other(
    archive: &Path,
    dest: &Path,
    progress: &mut dyn FnMut(usize),
) -> io::Result<()> {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let exe = Path::new(&root).join(r"System32\tar.exe");
    // bsdtar refuses absolute paths and `..` unless told otherwise (-P).
    let mut child = Command::new(exe)
        .arg("-xvf")
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .creation_flags(NO_WINDOW)
        .spawn()?;
    let mut done = 0;
    let mut last = String::new();
    if let Some(err) = child.stderr.take() {
        for l in io::BufReader::new(err).split(b'\n') {
            let l = l?;
            if l.starts_with(b"x ") {
                done += 1;
                progress(done);
            } else if !l.is_empty() {
                last = String::from_utf8_lossy(&l).trim().to_owned();
            }
        }
    }
    if !child.wait()?.success() {
        return Err(io::Error::other(if last.is_empty() {
            "tar failed".to_owned()
        } else {
            last
        }));
    }
    check_tree(dest)
}

/// Every item under `root` is a plain file or folder (no link).
fn check_tree(root: &Path) -> io::Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir)? {
            let e = e?;
            let kind = e.file_type()?;
            if kind.is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    fx_core::tr!(
                        "o arquivo compactado traz um link; nada foi extraído",
                        "the archive holds a link; nothing was extracted"
                    ),
                ));
            }
            if kind.is_dir() {
                stack.push(e.path());
            }
        }
    }
    Ok(())
}

/// `(source file or empty for a folder, entry name)` for everything in
/// `sources`, folders walked (links skipped).
fn zip_inputs(sources: &[PathBuf]) -> io::Result<Vec<(String, String)>> {
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
            if meta.is_dir() {
                out.push((String::new(), format!("{name}/")));
                for e in std::fs::read_dir(&path)? {
                    let e = e?;
                    if let Some(n) = e.file_name().to_str() {
                        stack.push((e.path(), format!("{name}/{n}")));
                    }
                }
            } else if let Some(p) = path.to_str() {
                out.push((p.to_owned(), name));
            }
        }
    }
    Ok(out)
}

/// Makes the ZIP `zip` (which must not exist) holding `sources`, calling
/// `progress(done, total)` per file. Written under a temporary name in the
/// same folder and renamed without replacing anything; removed on failure.
pub fn make_zip(
    sources: &[PathBuf],
    zip: &Path,
    progress: &mut dyn FnMut(usize, usize),
) -> io::Result<()> {
    if zip.exists() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    let inputs = zip_inputs(sources)?;
    let dir = zip.parent().unwrap_or(Path::new("."));
    let temp = dir.join(fx_core::temp_name::temp_name(
        zip.file_name().unwrap_or_default(),
        std::process::id(),
        0,
    ));
    let lines: Vec<String> = inputs.iter().map(|(s, n)| format!("{s}\t{n}")).collect();
    let list = list_file(&lines)?;
    let script = format!(
        "$ErrorActionPreference='Stop'\n\
         Add-Type -AssemblyName System.IO.Compression\n\
         Add-Type -AssemblyName System.IO.Compression.FileSystem\n\
         $lines=[IO.File]::ReadAllLines({list},[Text.Encoding]::UTF8)\n\
         $z=[IO.Compression.ZipFile]::Open({temp},'Create')\n\
         try {{ $n=0\n\
           foreach($l in $lines) {{ if($l -eq '') {{ continue }}\n\
             $t=$l.Split([char]9,2)\n\
             if($t[0] -eq '') {{ [void]$z.CreateEntry($t[1]) }}\n\
             else {{ [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($z,$t[0],$t[1],'Optimal') }}\n\
             $n++; [Console]::Out.WriteLine($n); [Console]::Out.Flush() }}\n\
         }} finally {{ $z.Dispose() }}\n",
        list = ps_quote(&list.0),
        temp = ps_quote(&temp),
    );
    let total = lines.len();
    let result = run_powershell(&script, &mut |l| {
        if let Ok(n) = l.parse::<usize>() {
            progress(n, total);
        }
    })
    .and_then(|()| super::ops::rename_no_replace(&temp, zip));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
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
            &mut |k, n| last = (k, n),
        )
        .unwrap();
        assert_eq!(last, (4, 4), "2 folders and 2 files");
        assert!(
            make_zip(&[src.join("a b's.txt")], &zip, &mut |_, _| {}).is_err(),
            "never replaces"
        );
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
        std::fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn a_tar_is_extracted_whole() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("src")).unwrap();
        std::fs::write(d.join("src").join("x.txt"), b"x").unwrap();
        let tar = d.join("t.tar");
        let root = std::env::var_os("SystemRoot").unwrap();
        let ok = Command::new(Path::new(&root).join(r"System32\tar.exe"))
            .arg("-cf")
            .arg(&tar)
            .arg("-C")
            .arg(d.join("src"))
            .arg("x.txt")
            .creation_flags(NO_WINDOW)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        let out = d.join("out");
        std::fs::create_dir(&out).unwrap();
        let mut n = 0;
        extract_all_other(&tar, &out, &mut |k| n = k).unwrap();
        assert_eq!(n, 1);
        assert_eq!(std::fs::read(out.join("x.txt")).unwrap(), b"x");
    }
}
