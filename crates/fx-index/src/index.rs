//! In-memory file index (task T0.7), platform-neutral.
//!
//! Stored by columns: one UTF-8 buffer for names, one for folded names (search
//! keys), and parallel vectors for parent, size, date and flags. Full paths are
//! not stored; [`Index::path`] rebuilds them by walking up the parents.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

use fx_core::natural::fold_into;

/// File id within a volume (the NTFS MFT record number).
pub type FileId = u64;

/// Separator after each folded name in the search buffer. Never part of a name.
pub(crate) const SEP: char = '\0';

const NONE: u32 = u32::MAX;
const FLAG_DIR: u8 = 1 << 0;
const FLAG_DELETED: u8 = 1 << 1;

/// Longest parent chain followed by [`Index::path`] (guards against cycles in a
/// corrupt volume).
const MAX_DEPTH: usize = 2048;

/// One file as read from the volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: FileId,
    pub parent: FileId,
    /// UTF-16 name, as stored by NTFS (may contain unpaired surrogates).
    pub name: Vec<u16>,
    pub is_dir: bool,
    pub size: u64,
    /// Last write time, FILETIME (100 ns ticks since 1601).
    pub modified: u64,
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    /// Id of the volume root: entries whose parent is the root are top level.
    root: FileId,
    names: String,
    name_ends: Vec<u32>,
    pub(crate) folded: String,
    /// End of each folded name (before its separator).
    pub(crate) folded_ends: Vec<u32>,
    ids: Vec<u32>,
    parents: Vec<u32>,
    sizes: Vec<u64>,
    modified: Vec<u64>,
    flags: Vec<u8>,
    /// Names that are not valid UTF-16, kept exactly for path building.
    raw_names: HashMap<u32, Vec<u16>>,
    /// Record number -> slot, `NONE` when absent.
    slot_of: Vec<u32>,
    live: usize,
}

impl Index {
    pub fn new(root: FileId) -> Self {
        Self {
            root,
            ..Self::default()
        }
    }

    pub fn with_capacity(root: FileId, files: usize) -> Self {
        let mut index = Self::new(root);
        index.names.reserve(files * 24);
        index.folded.reserve(files * 25);
        index.name_ends.reserve(files);
        index.folded_ends.reserve(files);
        index.ids.reserve(files);
        index.parents.reserve(files);
        index.sizes.reserve(files);
        index.modified.reserve(files);
        index.flags.reserve(files);
        index
    }

    /// Number of live entries (deleted ones are not counted).
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// Number of slots, including deleted ones (for iteration and search).
    pub fn slots(&self) -> usize {
        self.ids.len()
    }

    /// Adds `record`, replacing any live entry with the same id (renames and
    /// moves are a replacement: the old slot is marked deleted).
    pub fn insert(&mut self, record: &Record) -> Result<(), IndexFull> {
        let id = u32::try_from(record.id).map_err(|_| IndexFull)?;
        let parent = u32::try_from(record.parent).map_err(|_| IndexFull)?;
        let slot = u32::try_from(self.ids.len()).map_err(|_| IndexFull)?;
        self.remove(record.id);

        self.make_room(record.name.len());
        let name = match String::from_utf16(&record.name) {
            Ok(s) => s,
            Err(_) => {
                self.raw_names.insert(slot, record.name.clone());
                String::from_utf16_lossy(&record.name)
            }
        };
        self.names.push_str(&name);
        self.name_ends.push(offset(self.names.len())?);
        fold_into(&name, &mut self.folded);
        self.folded_ends.push(offset(self.folded.len())?);
        self.folded.push(SEP);
        self.ids.push(id);
        self.parents.push(parent);
        self.sizes.push(record.size);
        self.modified.push(record.modified);
        self.flags.push(if record.is_dir { FLAG_DIR } else { 0 });

        let id = id as usize;
        if self.slot_of.len() <= id {
            let extra = (id + 1 - self.slot_of.len()).max(self.slot_of.len() / GROWTH_DIVISOR);
            self.slot_of.reserve_exact(extra);
            self.slot_of.resize(id + 1, NONE);
        }
        self.slot_of[id] = slot;
        self.live += 1;
        Ok(())
    }

    /// Grows full buffers by a small step (1/32, at least `MIN_GROWTH` entries)
    /// instead of letting a push double them: after `shrink_to_fit`, the
    /// first change of a live index would otherwise double its memory (seen
    /// in the service: 71 MB → 139 MB for one new file).
    fn make_room(&mut self, name_units: usize) {
        fn grow<T>(v: &mut Vec<T>) {
            if v.len() == v.capacity() {
                v.reserve_exact((v.len() / GROWTH_DIVISOR).max(MIN_GROWTH));
            }
        }
        fn grow_text(s: &mut String, need: usize) {
            if s.capacity() - s.len() < need {
                s.reserve_exact((s.len() / GROWTH_DIVISOR).max(MIN_GROWTH * 16).max(need));
            }
        }
        // A UTF-16 unit is at most 3 UTF-8 bytes; folding may add a separator.
        grow_text(&mut self.names, name_units * 3);
        grow_text(&mut self.folded, name_units * 3 + 1);
        grow(&mut self.name_ends);
        grow(&mut self.folded_ends);
        grow(&mut self.ids);
        grow(&mut self.parents);
        grow(&mut self.sizes);
        grow(&mut self.modified);
        grow(&mut self.flags);
    }

    /// Marks the entry with `id` as deleted. Returns whether it existed.
    pub fn remove(&mut self, id: FileId) -> bool {
        let Some(slot) = self.slot(id) else {
            return false;
        };
        self.flags[slot as usize] |= FLAG_DELETED;
        self.slot_of[id as usize] = NONE;
        self.live -= 1;
        true
    }

    /// Slot of the live entry with `id`.
    pub fn slot(&self, id: FileId) -> Option<u32> {
        let slot = *self.slot_of.get(usize::try_from(id).ok()?)?;
        (slot != NONE).then_some(slot)
    }

    pub fn is_live(&self, slot: u32) -> bool {
        self.flags[slot as usize] & FLAG_DELETED == 0
    }

    pub fn id(&self, slot: u32) -> FileId {
        FileId::from(self.ids[slot as usize])
    }

    /// Display name (lossy only for names that are not valid UTF-16).
    pub fn name(&self, slot: u32) -> &str {
        let s = slot as usize;
        let start = if s == 0 {
            0
        } else {
            self.name_ends[s - 1] as usize
        };
        &self.names[start..self.name_ends[s] as usize]
    }

    pub(crate) fn folded(&self, slot: u32) -> &str {
        let s = slot as usize;
        let start = if s == 0 {
            0
        } else {
            self.folded_ends[s - 1] as usize + 1
        };
        &self.folded[start..self.folded_ends[s] as usize]
    }

    pub fn is_dir(&self, slot: u32) -> bool {
        self.flags[slot as usize] & FLAG_DIR != 0
    }

    pub fn size(&self, slot: u32) -> u64 {
        self.sizes[slot as usize]
    }

    pub fn modified(&self, slot: u32) -> u64 {
        self.modified[slot as usize]
    }

    /// Path of `slot` relative to the volume root, rebuilt from the parents.
    /// `None` if a parent is missing (orphan) or the chain is too deep.
    pub fn path(&self, slot: u32) -> Option<PathBuf> {
        let mut parts: Vec<OsString> = Vec::new();
        let mut current = slot;
        for _ in 0..MAX_DEPTH {
            parts.push(self.os_name(current));
            let parent = FileId::from(self.parents[current as usize]);
            if parent == self.root {
                return Some(parts.iter().rev().collect());
            }
            current = self.slot(parent)?;
        }
        None
    }

    fn os_name(&self, slot: u32) -> OsString {
        match self.raw_names.get(&slot) {
            #[cfg(windows)]
            Some(raw) => {
                use std::os::windows::ffi::OsStringExt;
                OsString::from_wide(raw)
            }
            #[cfg(not(windows))]
            Some(raw) => OsString::from(String::from_utf16_lossy(raw)),
            None => OsString::from(self.name(slot)),
        }
    }

    /// Appends all entries of `other` (built in parallel on another thread) by
    /// concatenating its buffers; nothing is re-folded. Entries of `other` win
    /// over existing entries with the same id.
    pub fn append(&mut self, other: Index) -> Result<(), IndexFull> {
        let slot_base = offset(self.ids.len())?;
        let name_base = offset(self.names.len())?;
        let folded_base = offset(self.folded.len())?;
        offset(self.names.len() + other.names.len())?;
        offset(self.folded.len() + other.folded.len())?;
        offset(self.ids.len() + other.ids.len())?;

        for slot in 0..other.slots() as u32 {
            if other.is_live(slot) {
                self.remove(other.id(slot));
            }
        }
        self.names.push_str(&other.names);
        self.folded.push_str(&other.folded);
        self.name_ends
            .extend(other.name_ends.iter().map(|e| e + name_base));
        self.folded_ends
            .extend(other.folded_ends.iter().map(|e| e + folded_base));
        self.ids.extend_from_slice(&other.ids);
        self.parents.extend_from_slice(&other.parents);
        self.sizes.extend_from_slice(&other.sizes);
        self.modified.extend_from_slice(&other.modified);
        self.flags.extend_from_slice(&other.flags);
        for (slot, raw) in other.raw_names {
            self.raw_names.insert(slot + slot_base, raw);
        }
        for slot in 0..other.ids.len() as u32 {
            if other.flags[slot as usize] & FLAG_DELETED == 0 {
                let id = other.ids[slot as usize] as usize;
                if self.slot_of.len() <= id {
                    self.slot_of.resize(id + 1, NONE);
                }
                self.slot_of[id] = slot + slot_base;
                self.live += 1;
            }
        }
        Ok(())
    }

    /// Heap memory used by the index, from the capacity of every buffer.
    pub fn heap_bytes(&self) -> usize {
        use std::mem::size_of;
        self.names.capacity()
            + self.folded.capacity()
            + (self.name_ends.capacity() + self.folded_ends.capacity()) * size_of::<u32>()
            + (self.ids.capacity() + self.parents.capacity()) * size_of::<u32>()
            + (self.sizes.capacity() + self.modified.capacity()) * size_of::<u64>()
            + self.flags.capacity()
            + self.slot_of.capacity() * size_of::<u32>()
            + self
                .raw_names
                .values()
                .map(|v| v.capacity() * 2 + 32)
                .sum::<usize>()
    }

    /// Releases spare capacity (call once the initial build is done).
    pub fn shrink_to_fit(&mut self) {
        self.names.shrink_to_fit();
        self.folded.shrink_to_fit();
        self.name_ends.shrink_to_fit();
        self.folded_ends.shrink_to_fit();
        self.ids.shrink_to_fit();
        self.parents.shrink_to_fit();
        self.sizes.shrink_to_fit();
        self.modified.shrink_to_fit();
        self.flags.shrink_to_fit();
        self.slot_of.shrink_to_fit();
    }
}

/// Buffers of a live index grow by 1/32 of their size when full.
const GROWTH_DIVISOR: usize = 32;
/// ... and by at least this many entries.
const MIN_GROWTH: usize = 4096;

fn offset(len: usize) -> Result<u32, IndexFull> {
    u32::try_from(len).map_err(|_| IndexFull)
}

/// The index outgrew its 32-bit offsets or ids (≥ 4 GiB of names or ≥ 4 billion
/// records), far beyond any real volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexFull;

impl std::fmt::Display for IndexFull {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("index full: more than 4 GiB of names or 4 billion records")
    }
}

impl std::error::Error for IndexFull {}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: FileId = 5;

    fn rec(id: FileId, parent: FileId, name: &str, is_dir: bool) -> Record {
        Record {
            id,
            parent,
            name: name.encode_utf16().collect(),
            is_dir,
            size: id * 10,
            modified: 1,
        }
    }

    /// One change after the initial build must not double the memory.
    #[test]
    fn a_live_index_grows_in_small_steps() {
        let mut index = Index::new(ROOT);
        for id in 0..200_000u64 {
            index
                .insert(&rec(100 + id, ROOT, &format!("arquivo {id}.txt"), false))
                .unwrap();
        }
        index.shrink_to_fit();
        let built = index.heap_bytes();
        index
            .insert(&rec(100 + 200_000, ROOT, "novo.txt", false))
            .unwrap();
        let after = index.heap_bytes();
        assert!(
            after < built + built / 16,
            "one insert grew {built} -> {after} bytes"
        );
        assert_eq!(
            index.path(index.slot(100 + 200_000).unwrap()).unwrap(),
            PathBuf::from("novo.txt")
        );
    }

    fn sample() -> Index {
        let mut index = Index::new(ROOT);
        for r in [
            rec(30, ROOT, "Users", true),
            rec(31, 30, "ana", true),
            rec(32, 31, "Relatório.pdf", false),
            rec(33, ROOT, "Windows", true),
        ] {
            index.insert(&r).unwrap();
        }
        index
    }

    #[test]
    fn paths_are_rebuilt_from_parents() {
        let index = sample();
        let slot = index.slot(32).unwrap();
        assert_eq!(
            index.path(slot).unwrap(),
            ["Users", "ana", "Relatório.pdf"]
                .iter()
                .collect::<PathBuf>()
        );
        assert_eq!(
            index.path(index.slot(33).unwrap()).unwrap(),
            PathBuf::from("Windows")
        );
        assert_eq!(index.name(slot), "Relatório.pdf");
        assert_eq!(index.folded(slot), "relatorio.pdf");
        assert!(!index.is_dir(slot));
        assert!(index.is_dir(index.slot(31).unwrap()));
        assert_eq!(index.size(slot), 320);
        assert_eq!(index.len(), 4);
    }

    #[test]
    fn orphans_and_cycles_have_no_path() {
        let mut index = sample();
        index.insert(&rec(40, 999, "orphan.txt", false)).unwrap();
        assert!(index.path(index.slot(40).unwrap()).is_none());
        index.insert(&rec(50, 51, "a", true)).unwrap();
        index.insert(&rec(51, 50, "b", true)).unwrap();
        assert!(index.path(index.slot(50).unwrap()).is_none());
    }

    #[test]
    fn rename_move_and_delete() {
        let mut index = sample();
        // Rename + move "Relatório.pdf" into Windows.
        index.insert(&rec(32, 33, "novo.pdf", false)).unwrap();
        assert_eq!(index.len(), 4);
        let slot = index.slot(32).unwrap();
        assert_eq!(
            index.path(slot).unwrap(),
            ["Windows", "novo.pdf"].iter().collect::<PathBuf>()
        );
        assert_eq!(index.slots(), 5, "old slot kept as a tombstone");
        assert!(index.remove(32));
        assert!(!index.remove(32));
        assert!(index.slot(32).is_none());
        assert!(!index.is_live(slot));
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn invalid_utf16_names_round_trip() {
        let mut index = Index::new(ROOT);
        let raw = vec![0x61, 0xD800, 0x62];
        index
            .insert(&Record {
                name: raw.clone(),
                ..rec(30, ROOT, "", false)
            })
            .unwrap();
        let slot = index.slot(30).unwrap();
        assert_eq!(index.name(slot), "a\u{FFFD}b");
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let path = index.path(slot).unwrap();
            assert_eq!(path.as_os_str().encode_wide().collect::<Vec<_>>(), raw);
        }
    }

    #[test]
    fn append_merges_partial_indexes() {
        let mut a = Index::new(ROOT);
        a.insert(&rec(30, ROOT, "Users", true)).unwrap();
        let mut b = Index::new(ROOT);
        b.insert(&rec(31, 30, "ana", true)).unwrap();
        b.insert(&rec(32, 31, "x.txt", false)).unwrap();
        b.remove(32);
        b.insert(&Record {
            name: vec![0x61, 0xD800],
            ..rec(33, ROOT, "", false)
        })
        .unwrap();
        a.append(b).unwrap();
        assert_eq!(a.len(), 3);
        assert_eq!(a.name(a.slot(33).unwrap()), "a\u{FFFD}");
        assert_eq!(a.folded(a.slot(31).unwrap()), "ana");
        assert_eq!(
            a.path(a.slot(31).unwrap()).unwrap(),
            ["Users", "ana"].iter().collect::<PathBuf>()
        );
        assert!(a.slot(32).is_none());
        assert!(a.heap_bytes() > 0);
        a.shrink_to_fit();
    }

    #[test]
    fn ids_beyond_32_bits_are_rejected() {
        let mut index = Index::new(ROOT);
        assert_eq!(
            index.insert(&rec(1 << 40, ROOT, "x", false)),
            Err(IndexFull)
        );
        assert!(index.is_empty());
    }
}
