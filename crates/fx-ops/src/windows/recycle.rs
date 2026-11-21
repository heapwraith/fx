//! Recycle Bin (task T1.12): delete with `IFileOperation` (the Explorer's
//! own engine, so the item shows in the Recycle Bin and can be restored from
//! there too), and find and restore a recycled item for undo.
//!
//! Each recycled item is a pair in `<volume>\$Recycle.Bin\<user SID>\`:
//! `$Rxxxxxx.ext` holds the data and `$Ixxxxxx.ext` its original path and
//! deletion time. Restoring renames `$R` back (never replacing anything) and
//! removes `$I`, which is what the Recycle Bin's own "Restore" does.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::UI::Shell::{
    FOF_NOCONFIRMATION, FOF_SILENT, FOF_WANTNUKEWARNING, FOFX_RECYCLEONDELETE, FileOperation,
    IFileOperation, IFileOperationProgressSink, IShellItem, SHCreateItemFromParsingName,
    SHQUERYRBINFO, SHQueryRecycleBinW,
};
use windows::core::PCWSTR;

use super::{rename_no_replace, to_io};
use crate::Recycled;

/// Root (`C:\`, `\\server\share\`) of an absolute path.
fn root_of(path: &Path) -> Option<PathBuf> {
    match path.components().next()? {
        Component::Prefix(p) => {
            let mut root = PathBuf::from(p.as_os_str());
            root.push("\\");
            Some(root)
        }
        _ => None,
    }
}

/// Whether the volume at `root` has a Recycle Bin (`subst` drives and most
/// network shares do not: the shell would delete for good there).
pub(crate) fn has_recycle_bin(root: &Path) -> bool {
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `wide` is NUL-terminated and `info` a sized, writable struct.
    unsafe { SHQueryRecycleBinW(PCWSTR(wide.as_ptr()), &mut info) }.is_ok()
}

/// Sends `paths` to the Recycle Bin, by their canonical paths (so a `subst`
/// drive uses the real volume's Recycle Bin). Refuses, deleting nothing, when
/// a volume has no Recycle Bin: the shell would delete those items for good.
pub(crate) fn recycle(paths: &[PathBuf]) -> io::Result<()> {
    let real: Vec<PathBuf> = paths.iter().map(|p| canonical(p)).collect();
    for p in &real {
        let ok = root_of(p).is_some_and(|root| has_recycle_bin(&root));
        if !ok {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!(
                    "a unidade de \"{}\" não tem Lixeira; nada foi excluído (Shift+Delete exclui permanentemente)",
                    p.display()
                ),
            ));
        }
    }
    let paths = &real;
    // SAFETY: COM is initialized on this worker thread (`com_init`).
    let op: IFileOperation =
        unsafe { CoCreateInstance(&FileOperation, None, CLSCTX_ALL) }.map_err(to_io)?;
    let flags = FOF_NOCONFIRMATION | FOF_SILENT | FOF_WANTNUKEWARNING | FOFX_RECYCLEONDELETE;
    // SAFETY: `op` is a valid IFileOperation of this thread.
    unsafe { op.SetOperationFlags(flags) }.map_err(to_io)?;
    for p in paths {
        let wide: Vec<u16> = std::path::absolute(p)?
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let item: IShellItem =
            unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.map_err(to_io)?;
        // SAFETY: `item` is a valid shell item; no progress sink.
        unsafe { op.DeleteItem(&item, None::<&IFileOperationProgressSink>) }.map_err(to_io)?;
    }
    // SAFETY: valid object; runs the queued deletions synchronously.
    unsafe { op.PerformOperations() }.map_err(to_io)?;
    // SAFETY: valid object.
    let aborted = unsafe { op.GetAnyOperationsAborted() }.map_err(to_io)?;
    if aborted.as_bool() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "exclusão cancelada",
        ));
    }
    Ok(())
}

/// `<volume>\$Recycle.Bin` of the volume holding `path`.
fn bin_of(path: &Path) -> Option<PathBuf> {
    let abs = std::path::absolute(path).ok()?;
    match abs.components().next()? {
        Component::Prefix(p) => {
            let mut root = PathBuf::from(p.as_os_str());
            root.push("\\$Recycle.Bin");
            Some(root)
        }
        _ => None,
    }
}

/// Original path and deletion time stored in a `$I` file (format 1: Vista
/// to 8.1, fixed 260 characters; format 2: Windows 10+, length-prefixed).
fn parse_info(bytes: &[u8]) -> Option<(PathBuf, u64)> {
    let u64_at = |at: usize| {
        bytes
            .get(at..at + 8)
            .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default()))
    };
    let version = u64_at(0)?;
    let deleted = u64_at(16)?;
    let (start, chars) = match version {
        1 => (24, 260),
        2 => {
            let n = bytes.get(24..28)?;
            (28, u32::from_le_bytes([n[0], n[1], n[2], n[3]]) as usize)
        }
        _ => return None,
    };
    let raw = bytes.get(start..start + chars * 2)?;
    let (units, _) = raw.as_chunks::<2>();
    let wide: Vec<u16> = units
        .iter()
        .map(|&u| u16::from_le_bytes(u))
        .take_while(|&c| c != 0)
        .collect();
    use std::os::windows::ffi::OsStringExt;
    Some((PathBuf::from(std::ffi::OsString::from_wide(&wide)), deleted))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
}

/// `C:\x` from `\\?\C:\x` and `\\server\share` from `\\?\UNC\server\share`.
fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.as_os_str().to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

/// The path the Recycle Bin will record for `item`: its canonical form,
/// through `subst` drives, junctions and links. Call it before recycling.
pub(crate) fn canonical(item: &Path) -> PathBuf {
    std::fs::canonicalize(item).map_or_else(|_| item.to_path_buf(), |c| strip_verbatim(&c))
}

/// The most recent recycled item that came from `original` (or from its
/// canonical form `real`, which decides the volume's Recycle Bin).
pub(crate) fn find_recycled(original: &Path, real: &Path) -> Option<Recycled> {
    let original = std::path::absolute(original).ok()?;
    let bin = bin_of(real).or_else(|| bin_of(&original))?;
    let mut best: Option<Recycled> = None;
    // One folder per user SID; other users' folders are not readable.
    for user in std::fs::read_dir(&bin).ok()?.flatten() {
        let Ok(items) = std::fs::read_dir(user.path()) else {
            continue;
        };
        for item in items.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            let Some(suffix) = name.strip_prefix("$I") else {
                continue;
            };
            let Ok(bytes) = std::fs::read(item.path()) else {
                continue;
            };
            let Some((from, deleted_at)) = parse_info(&bytes) else {
                continue;
            };
            if !same_path(&from, &original) && !same_path(&from, real) {
                continue;
            }
            let data = user.path().join(format!("$R{suffix}"));
            if !data.exists() || best.as_ref().is_some_and(|b| b.deleted_at >= deleted_at) {
                continue;
            }
            best = Some(Recycled {
                original: original.clone(),
                info: item.path(),
                data,
                deleted_at,
            });
        }
    }
    best
}

/// Puts a recycled item back. If its original name is taken now, it comes
/// back under a "keep both" name; returns where it went.
pub(crate) fn restore(item: &Recycled) -> io::Result<PathBuf> {
    let dir = item
        .original
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "sem pasta de origem"))?;
    std::fs::create_dir_all(dir)?;
    let mut target = item.original.clone();
    if std::fs::symlink_metadata(&target).is_ok() {
        let name = target
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        target = crate::keep_both_name(dir, &name, |p| std::fs::symlink_metadata(p).is_ok());
    }
    rename_no_replace(&item.data, &target)?;
    std::fs::remove_file(&item.info)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info_v2(path: &str, deleted: u64) -> Vec<u8> {
        let wide: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let mut b = Vec::new();
        b.extend_from_slice(&2u64.to_le_bytes());
        b.extend_from_slice(&123u64.to_le_bytes());
        b.extend_from_slice(&deleted.to_le_bytes());
        b.extend_from_slice(&(wide.len() as u32).to_le_bytes());
        for c in wide {
            b.extend_from_slice(&c.to_le_bytes());
        }
        b
    }

    #[test]
    fn recycle_bin_presence() {
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        assert!(has_recycle_bin(&PathBuf::from(format!("{system}\\"))));
        // An unused drive letter has no Recycle Bin, and nothing is deleted.
        let free = ('D'..='Z')
            .rev()
            .find(|c| !Path::new(&format!("{c}:\\")).exists())
            .unwrap();
        let root = PathBuf::from(format!("{free}:\\"));
        assert!(!has_recycle_bin(&root));
        let err = recycle(&[root.join("x.txt")]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn verbatim_prefixes_are_removed() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\a")),
            PathBuf::from(r"C:\a")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\srv\share\a")),
            PathBuf::from(r"\\srv\share\a")
        );
        assert_eq!(strip_verbatim(Path::new(r"D:\b")), PathBuf::from(r"D:\b"));
    }

    #[test]
    fn parses_info_files() {
        let (p, t) = parse_info(&info_v2(r"C:\Fotos\praia ☀.jpg", 99)).unwrap();
        assert_eq!((p, t), (PathBuf::from(r"C:\Fotos\praia ☀.jpg"), 99));
        let mut v1 = vec![0u8; 24 + 520];
        v1[0] = 1;
        v1[16] = 7;
        for (i, c) in r"D:\a.txt".encode_utf16().enumerate() {
            v1[24 + 2 * i..26 + 2 * i].copy_from_slice(&c.to_le_bytes());
        }
        assert_eq!(parse_info(&v1), Some((PathBuf::from(r"D:\a.txt"), 7)));
        assert_eq!(parse_info(&[9; 40]), None);
        assert_eq!(
            bin_of(Path::new(r"c:\x\y")),
            Some(PathBuf::from(r"c:\$Recycle.Bin"))
        );
    }
}
