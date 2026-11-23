//! Duplicate files (T6.4, D-068): files with exactly the same content under a
//! folder. Candidates by size first, then by their first and last 64 KiB,
//! and finally confirmed by comparing the whole content in 1 MiB blocks: no
//! guess, and no file is ever read whole into memory. Links are not
//! followed. Progress is reported as work really done.

use std::collections::HashMap;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Bytes read from each end of a candidate for the quick check.
const EDGE: u64 = 64 << 10;
const BLOCK: usize = 1 << 20;

/// Where the scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DupProgress {
    /// Files listed so far.
    Listing(u64),
    /// Candidates compared, of the total.
    Comparing(u64, u64),
}

/// A set of files with the same content (`size` bytes each), in path order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupGroup {
    pub size: u64,
    pub files: Vec<PathBuf>,
}

impl DupGroup {
    /// Space the copies take beyond one.
    pub fn wasted(&self) -> u64 {
        self.size * (self.files.len().saturating_sub(1)) as u64
    }
}

/// The duplicate files under `root` of at least `min_size` bytes, largest
/// waste first. Stops early (returning what it has) when `cancel` is set.
pub fn find_duplicates(
    root: &Path,
    min_size: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(DupProgress),
) -> Vec<DupGroup> {
    // 1. Every file by size.
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    let mut listed = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(e.path());
            } else if let Ok(meta) = e.metadata()
                && meta.len() >= min_size.max(1)
            {
                by_size.entry(meta.len()).or_default().push(e.path());
                listed += 1;
                if listed.is_multiple_of(500) {
                    progress(DupProgress::Listing(listed));
                }
            }
        }
    }
    progress(DupProgress::Listing(listed));
    let candidates: Vec<(u64, Vec<PathBuf>)> = by_size
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .collect();
    let total: u64 = candidates.iter().map(|(_, f)| f.len() as u64).sum();
    let mut done = 0u64;
    let mut groups = Vec::new();
    for (size, files) in candidates {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        // 2. Same size: split by the ends of the file.
        let mut by_edges: HashMap<u64, Vec<PathBuf>> = HashMap::new();
        for f in files {
            if let Ok(key) = edges_key(&f, size) {
                by_edges.entry(key).or_default().push(f);
            }
            done += 1;
            progress(DupProgress::Comparing(done, total));
        }
        // 3. Confirm by the whole content.
        for (_, same_edges) in by_edges.into_iter().filter(|(_, f)| f.len() > 1) {
            groups.extend(confirm(same_edges, size, cancel));
        }
    }
    for g in &mut groups {
        g.files.sort();
    }
    groups.sort_by(|a, b| b.wasted().cmp(&a.wasted()).then(a.files.cmp(&b.files)));
    groups
}

/// A key of the first and last `EDGE` bytes (and the size).
fn edges_key(path: &Path, size: u64) -> io::Result<u64> {
    let mut f = File::open(path)?;
    let mut h = DefaultHasher::new();
    size.hash(&mut h);
    let mut buf = vec![0u8; EDGE.min(size) as usize];
    f.read_exact(&mut buf)?;
    buf.hash(&mut h);
    if size > EDGE {
        f.seek(SeekFrom::Start(size - EDGE.min(size)))?;
        f.read_exact(&mut buf)?;
        buf.hash(&mut h);
    }
    Ok(h.finish())
}

/// Splits `files` (same size and ends) into groups of identical content.
fn confirm(mut files: Vec<PathBuf>, size: u64, cancel: &AtomicBool) -> Vec<DupGroup> {
    let mut groups = Vec::new();
    while files.len() > 1 {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let first = files.remove(0);
        let mut same = vec![first.clone()];
        files.retain(|other| {
            if same_content(&first, other).unwrap_or(false) {
                same.push(other.clone());
                false
            } else {
                true
            }
        });
        if same.len() > 1 {
            groups.push(DupGroup { size, files: same });
        }
    }
    groups
}

/// Whether two files have the same bytes (compared in blocks).
fn same_content(a: &Path, b: &Path) -> io::Result<bool> {
    let (mut fa, mut fb) = (File::open(a)?, File::open(b)?);
    let (mut ba, mut bb) = (vec![0u8; BLOCK], vec![0u8; BLOCK]);
    loop {
        let na = read_full(&mut fa, &mut ba)?;
        let nb = read_full(&mut fb, &mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

/// Reads until `buf` is full or the file ends.
fn read_full(f: &mut File, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..])? {
            0 => break,
            k => n += k,
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_identical_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("sub")).unwrap();
        let big: Vec<u8> = (0..200_000u32).map(|k| (k % 251) as u8).collect();
        let mut almost = big.clone();
        almost[100_000] ^= 1; // same size and ends, different middle
        std::fs::write(d.join("a.bin"), &big).unwrap();
        std::fs::write(d.join("sub").join("b.bin"), &big).unwrap();
        std::fs::write(d.join("c.bin"), &almost).unwrap();
        std::fs::write(d.join("x.txt"), b"same").unwrap();
        std::fs::write(d.join("sub").join("y.txt"), b"same").unwrap();
        std::fs::write(d.join("z.txt"), b"diff").unwrap();
        let cancel = AtomicBool::new(false);
        let mut steps = Vec::new();
        let groups = find_duplicates(d, 1, &cancel, &mut |p| steps.push(p));
        assert_eq!(groups.len(), 2, "{groups:?}");
        assert_eq!(
            groups[0].files,
            vec![d.join("a.bin"), d.join("sub").join("b.bin")]
        );
        assert_eq!(groups[0].wasted(), 200_000);
        assert_eq!(
            groups[1].files,
            vec![d.join("sub").join("y.txt"), d.join("x.txt")]
        );
        assert!(steps.contains(&DupProgress::Listing(6)));
        // A minimum size leaves small files out.
        assert_eq!(find_duplicates(d, 1_000, &cancel, &mut |_| {}).len(), 1);
    }

    #[test]
    fn cancelling_stops() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), b"1").unwrap();
        std::fs::write(dir.path().join("b"), b"1").unwrap();
        let cancel = AtomicBool::new(true);
        assert!(find_duplicates(dir.path(), 1, &cancel, &mut |_| {}).is_empty());
    }
}
