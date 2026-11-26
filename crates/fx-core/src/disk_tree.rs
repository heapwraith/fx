//! The disk map's tree (T6.1, D-078): every folder below a root with its real
//! total, filled in while a scan reads the disk, and the nested treemap
//! layout drawn from it. One scan serves every level: zooming into a
//! subfolder or out again reuses what was read.
//!
//! Memory stays bounded on a whole drive: folders are always kept, files only
//! when they are big enough to ever be drawn (`KEEP_MIN`, at most
//! `KEEP_PER_DIR` per folder); the rest of a folder's files become one
//! "smaller files" node with their real total and count.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use crate::treemap::{Rect, squarify};

/// Files below this size are summed into their folder's "smaller files" node.
pub const KEEP_MIN: u64 = 256 * 1024;
/// Most files kept one by one in a folder (the largest).
pub const KEEP_PER_DIR: usize = 48;
/// Past this many nodes, new files are only summed.
pub const MAX_NODES: usize = 3_000_000;
/// Most files an expanded "smaller files" node shows.
pub const MAX_EXPANDED: usize = 20_000;

/// The kind of a file, by extension: the map's colors and the "by type" panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Images = 0,
    Video = 1,
    Audio = 2,
    Documents = 3,
    Archives = 4,
    Code = 5,
    Programs = 6,
    Other = 7,
}

pub const KINDS: usize = 8;

impl Kind {
    pub const ALL: [Kind; KINDS] = [
        Kind::Images,
        Kind::Video,
        Kind::Audio,
        Kind::Documents,
        Kind::Archives,
        Kind::Code,
        Kind::Programs,
        Kind::Other,
    ];

    pub fn of(name: &str) -> Kind {
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "heif" | "avif" | "bmp" | "tif"
            | "tiff" | "svg" | "ico" | "psd" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" => {
                Kind::Images
            }
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" | "m4v" | "mpg" | "mpeg" | "flv"
            | "ts" | "m2ts" | "3gp" => Kind::Video,
            "mp3" | "flac" | "wav" | "ogg" | "opus" | "m4a" | "aac" | "wma" | "aiff" | "mid" => {
                Kind::Audio
            }
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "odt"
            | "ods" | "odp" | "rtf" | "csv" | "epub" | "one" | "pst" | "ost" => Kind::Documents,
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "xz" | "bz2" | "zst" | "iso" | "img"
            | "vhd" | "vhdx" | "vmdk" | "cab" | "wim" | "esd" => Kind::Archives,
            "rs" | "c" | "h" | "cpp" | "hpp" | "cs" | "java" | "kt" | "go" | "py" | "js"
            | "jsx" | "tsx" | "mts" | "json" | "toml" | "yaml" | "yml" | "xml" | "html" | "css"
            | "sql" | "sh" | "ps1" | "rlib" | "rmeta" | "pdb" | "o" | "obj" | "lib" | "a"
            | "class" | "jar" | "pyc" | "map" | "wasm" => Kind::Code,
            "exe" | "dll" | "sys" | "msi" | "msix" | "appx" | "node" | "so" | "dylib" | "bin"
            | "dat" | "pak" | "ocx" | "mui" => Kind::Programs,
            _ => Kind::Other,
        }
    }
}

const DIR: u8 = 1;
/// The "smaller files" node of a folder.
const REST: u8 = 2;
const REMOVED: u8 = 4;
/// The folder could not be read (access denied, gone).
const DENIED: u8 = 8;

#[derive(Debug, Clone)]
struct Node {
    /// Bytes read so far below (a file: its size).
    bytes: u64,
    /// The total some other source knows (the index), while still reading.
    expected: u64,
    /// Files below (a file: 1; "smaller files": how many).
    files: u64,
    name_at: u32,
    name_len: u32,
    kind: u8,
    flags: u8,
    parent: u32,
    first: u32,
    count: u32,
    /// Folders below (and itself) not read yet.
    pending: u32,
    /// Index into `by_kind`, for folders.
    slot: u32,
}

/// What the scan read: a folder's entries.
#[derive(Debug, Clone)]
pub struct Listed {
    pub name: OsString,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct DiskTree {
    root_path: PathBuf,
    nodes: Vec<Node>,
    names: String,
    /// Names that are not valid Unicode, kept exactly (the pool has them
    /// for display only).
    odd: HashMap<u32, OsString>,
    by_kind: Vec<[u64; KINDS]>,
    /// Files of at least `BIG_FILE`, for "largest files".
    big: Vec<u32>,
    version: u64,
}

const BIG_FILE: u64 = 1024 * 1024;
pub const ROOT: u32 = 0;

impl DiskTree {
    /// A tree with only `root`, not read yet.
    pub fn new(root: PathBuf) -> Self {
        let mut t = DiskTree {
            root_path: root,
            nodes: Vec::new(),
            names: String::new(),
            odd: HashMap::new(),
            by_kind: Vec::new(),
            big: Vec::new(),
            version: 0,
        };
        let name = t
            .root_path
            .file_name()
            .map_or_else(|| t.root_path.as_os_str().to_owned(), OsStr::to_owned);
        let (name_at, name_len) = t.pool(&name, 0);
        t.by_kind.push([0; KINDS]);
        t.nodes.push(Node {
            bytes: 0,
            expected: 0,
            files: 0,
            name_at,
            name_len,
            kind: Kind::Other as u8,
            flags: DIR,
            parent: ROOT,
            first: 0,
            count: 0,
            pending: 1,
            slot: 0,
        });
        t
    }

    fn pool(&mut self, name: &OsStr, id: u32) -> (u32, u32) {
        let at = self.names.len() as u32;
        match name.to_str() {
            Some(s) => self.names.push_str(s),
            None => {
                self.names.push_str(&name.to_string_lossy());
                self.odd.insert(id, name.to_owned());
            }
        }
        (at, self.names.len() as u32 - at)
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Changes every time the tree does.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// The name for display (never used to build a path).
    pub fn name(&self, n: u32) -> &str {
        let node = &self.nodes[n as usize];
        &self.names[node.name_at as usize..(node.name_at + node.name_len) as usize]
    }

    pub fn os_name(&self, n: u32) -> OsString {
        self.odd
            .get(&n)
            .cloned()
            .unwrap_or_else(|| OsString::from(self.name(n)))
    }

    pub fn bytes(&self, n: u32) -> u64 {
        self.nodes[n as usize].bytes
    }

    pub fn files(&self, n: u32) -> u64 {
        self.nodes[n as usize].files
    }

    pub fn is_dir(&self, n: u32) -> bool {
        self.nodes[n as usize].flags & DIR != 0
    }

    /// The "smaller files" node of a folder.
    pub fn is_rest(&self, n: u32) -> bool {
        self.nodes[n as usize].flags & REST != 0
    }

    pub fn is_denied(&self, n: u32) -> bool {
        self.nodes[n as usize].flags & DENIED != 0
    }

    pub fn is_removed(&self, n: u32) -> bool {
        self.nodes[n as usize].flags & REMOVED != 0
    }

    pub fn kind(&self, n: u32) -> Kind {
        Kind::ALL[self.nodes[n as usize].kind as usize]
    }

    pub fn parent(&self, n: u32) -> Option<u32> {
        (n != ROOT).then(|| self.nodes[n as usize].parent)
    }

    /// Everything below has been read.
    pub fn complete(&self, n: u32) -> bool {
        self.nodes[n as usize].pending == 0
    }

    /// Folders not read yet below `n`.
    pub fn pending(&self, n: u32) -> u32 {
        self.nodes[n as usize].pending
    }

    /// The size the map gives `n`: what was read, or while reading, what the
    /// index knows when that is more.
    pub fn shown(&self, n: u32) -> u64 {
        let node = &self.nodes[n as usize];
        if node.pending == 0 {
            node.bytes
        } else {
            node.bytes.max(node.expected)
        }
    }

    /// Bytes below `n` by kind of file (a file: its own).
    pub fn by_kind(&self, n: u32) -> [u64; KINDS] {
        let node = &self.nodes[n as usize];
        if node.flags & DIR != 0 {
            self.by_kind[node.slot as usize]
        } else {
            let mut out = [0; KINDS];
            out[node.kind as usize] = node.bytes;
            out
        }
    }

    /// The children of `n`, removed ones left out. An expanded "smaller
    /// files" node gives its files instead of itself.
    pub fn children(&self, n: u32) -> impl Iterator<Item = u32> + '_ {
        let node = &self.nodes[n as usize];
        (node.first..node.first + node.count)
            .flat_map(move |c| {
                let child = &self.nodes[c as usize];
                if child.flags & REST != 0 && child.count > 0 {
                    child.first..child.first + child.count
                } else {
                    c..c + 1
                }
            })
            .filter(|&c| self.nodes[c as usize].flags & REMOVED == 0)
    }

    /// The "smaller files" node `rest` was expanded into its files.
    pub fn is_expanded(&self, rest: u32) -> bool {
        self.nodes[rest as usize].count > 0
    }

    /// Zoomed in, a folder's "smaller files" get room: `files` (the folder
    /// read again) become nodes of their own, shown in place of `rest`. The
    /// totals stay those of the scan; files kept one by one already are
    /// left out.
    pub fn expand_rest(&mut self, rest: u32, files: Vec<Listed>) {
        if !self.is_rest(rest) || self.is_expanded(rest) {
            return;
        }
        let dir = self.nodes[rest as usize].parent;
        let kept: std::collections::HashSet<OsString> = self
            .children(dir)
            .filter(|&c| !self.is_rest(c))
            .map(|c| self.os_name(c))
            .collect();
        let mut files: Vec<Listed> = files
            .into_iter()
            .filter(|f| !f.is_dir && f.size > 0 && !kept.contains(&f.name))
            .collect();
        files.sort_by_key(|f| std::cmp::Reverse(f.size));
        files.truncate(MAX_EXPANDED);
        if files.is_empty() {
            return;
        }
        let first = self.nodes.len() as u32;
        for f in files {
            let id = self.nodes.len() as u32;
            let (name_at, name_len) = self.pool(&f.name, id);
            let kind = f.name.to_str().map_or(Kind::Other, Kind::of);
            self.nodes.push(Node {
                bytes: f.size,
                expected: 0,
                files: 1,
                name_at,
                name_len,
                kind: kind as u8,
                flags: 0,
                parent: dir,
                first: 0,
                count: 0,
                pending: 0,
                slot: 0,
            });
        }
        let count = self.nodes.len() as u32 - first;
        let node = &mut self.nodes[rest as usize];
        node.first = first;
        node.count = count;
        self.version += 1;
    }

    pub fn depth(&self, mut n: u32) -> usize {
        let mut d = 0;
        while n != ROOT {
            n = self.nodes[n as usize].parent;
            d += 1;
        }
        d
    }

    /// `n` is `ancestor` or below it.
    pub fn is_within(&self, mut n: u32, ancestor: u32) -> bool {
        loop {
            if n == ancestor {
                return true;
            }
            if n == ROOT {
                return false;
            }
            n = self.nodes[n as usize].parent;
        }
    }

    /// The full path of `n` ("smaller files" gives its folder's).
    pub fn path_of(&self, n: u32) -> PathBuf {
        let mut chain = Vec::new();
        let mut k = if self.is_rest(n) {
            self.nodes[n as usize].parent
        } else {
            n
        };
        while k != ROOT {
            chain.push(k);
            k = self.nodes[k as usize].parent;
        }
        let mut path = self.root_path.clone();
        for k in chain.into_iter().rev() {
            path.push(self.os_name(k));
        }
        path
    }

    /// The node of `path`, when it is the root or a folder or file below it.
    pub fn find(&self, path: &Path) -> Option<u32> {
        let rel = strip_prefix_ci(path, &self.root_path)?;
        let mut n = ROOT;
        for part in rel.components() {
            let Component::Normal(name) = part else {
                return None;
            };
            n = self
                .children(n)
                .find(|&c| !self.is_rest(c) && self.os_name(c) == name)
                .or_else(|| {
                    let lower = name.to_str()?.to_lowercase();
                    self.children(n)
                        .find(|&c| !self.is_rest(c) && self.name(c).to_lowercase() == lower)
                })?;
        }
        Some(n)
    }

    /// The total another source knows for folder `n` (the index), used while
    /// it is still being read.
    pub fn set_expected(&mut self, n: u32, bytes: u64) {
        self.nodes[n as usize].expected = bytes;
        self.version += 1;
    }

    /// Folder `dir` was read: adds its entries and returns the subfolders
    /// (node and name) that still need reading.
    pub fn add_listing(&mut self, dir: u32, entries: Vec<Listed>) -> Vec<(u32, OsString)> {
        debug_assert!(self.is_dir(dir) && self.nodes[dir as usize].count == 0);
        let mut dirs = Vec::new();
        let mut files: Vec<(Listed, Kind)> = Vec::new();
        for e in entries {
            if e.is_dir {
                dirs.push(e);
            } else {
                let kind = e.name.to_str().map_or(Kind::Other, Kind::of);
                files.push((e, kind));
            }
        }
        files.sort_by_key(|(e, _)| std::cmp::Reverse(e.size));
        let room = self.nodes.len() < MAX_NODES;
        let keep = if room {
            files
                .iter()
                .take(KEEP_PER_DIR)
                .take_while(|(e, _)| e.size >= KEEP_MIN)
                .count()
        } else {
            0
        };
        let rest = &files[keep..];
        let first = self.nodes.len() as u32;
        let mut out = Vec::new();
        let (mut bytes, mut kinds) = (0u64, [0u64; KINDS]);
        for d in dirs {
            let id = self.nodes.len() as u32;
            let (name_at, name_len) = self.pool(&d.name, id);
            let slot = self.by_kind.len() as u32;
            self.by_kind.push([0; KINDS]);
            self.nodes.push(Node {
                bytes: 0,
                expected: 0,
                files: 0,
                name_at,
                name_len,
                kind: Kind::Other as u8,
                flags: DIR,
                parent: dir,
                first: 0,
                count: 0,
                pending: 1,
                slot,
            });
            out.push((id, d.name));
        }
        for (f, kind) in &files[..keep] {
            let id = self.nodes.len() as u32;
            let (name_at, name_len) = self.pool(&f.name, id);
            self.nodes.push(Node {
                bytes: f.size,
                expected: 0,
                files: 1,
                name_at,
                name_len,
                kind: *kind as u8,
                flags: 0,
                parent: dir,
                first: 0,
                count: 0,
                pending: 0,
                slot: 0,
            });
            if f.size >= BIG_FILE {
                self.big.push(id);
            }
            bytes += f.size;
            kinds[*kind as usize] += f.size;
        }
        if !rest.is_empty() {
            let id = self.nodes.len() as u32;
            let rest_bytes: u64 = rest.iter().map(|(e, _)| e.size).sum();
            let mut main = [0u64; KINDS];
            for (e, k) in rest {
                main[*k as usize] += e.size;
            }
            let dominant = (0..KINDS).max_by_key(|&k| main[k]).unwrap_or(KINDS - 1);
            let (name_at, name_len) = self.pool(OsStr::new(""), id);
            self.nodes.push(Node {
                bytes: rest_bytes,
                expected: 0,
                files: rest.len() as u64,
                name_at,
                name_len,
                kind: dominant as u8,
                flags: REST,
                parent: dir,
                first: 0,
                count: 0,
                pending: 0,
                slot: 0,
            });
            bytes += rest_bytes;
            for k in 0..KINDS {
                kinds[k] += main[k];
            }
        }
        let count = self.nodes.len() as u32 - first;
        {
            let node = &mut self.nodes[dir as usize];
            node.first = first;
            node.count = count;
        }
        let new_dirs = out.len() as u32;
        self.propagate(dir, bytes, files.len() as u64, &kinds);
        // This folder is read; its subfolders now wait.
        self.adjust_pending(dir, i64::from(new_dirs) - 1);
        self.version += 1;
        out
    }

    /// Folder `dir` could not be read: it counts as read, with nothing in it.
    pub fn listing_failed(&mut self, dir: u32) {
        self.nodes[dir as usize].flags |= DENIED;
        self.adjust_pending(dir, -1);
        self.version += 1;
    }

    fn propagate(&mut self, from: u32, bytes: u64, files: u64, kinds: &[u64; KINDS]) {
        let mut n = from;
        loop {
            let node = &mut self.nodes[n as usize];
            node.bytes += bytes;
            node.files += files;
            let slot = node.slot as usize;
            let parent = node.parent;
            for (k, v) in kinds.iter().enumerate() {
                self.by_kind[slot][k] += v;
            }
            if n == ROOT {
                break;
            }
            n = parent;
        }
    }

    fn adjust_pending(&mut self, from: u32, delta: i64) {
        let mut n = from;
        loop {
            let node = &mut self.nodes[n as usize];
            node.pending = (i64::from(node.pending) + delta).max(0) as u32;
            if n == ROOT {
                break;
            }
            n = node.parent;
        }
    }

    /// `n` went to the Recycle Bin: the totals above it drop at once.
    pub fn remove(&mut self, n: u32) {
        if n == ROOT || self.is_removed(n) {
            return;
        }
        let neg = self.by_kind(n);
        let (bytes, files, pending) = {
            let node = &self.nodes[n as usize];
            (node.bytes, node.files, node.pending)
        };
        self.nodes[n as usize].flags |= REMOVED;
        let mut k = self.nodes[n as usize].parent;
        loop {
            let node = &mut self.nodes[k as usize];
            node.bytes = node.bytes.saturating_sub(bytes);
            node.expected = node.expected.saturating_sub(bytes);
            node.files = node.files.saturating_sub(files);
            node.pending = node.pending.saturating_sub(pending);
            let slot = node.slot as usize;
            let parent = node.parent;
            for (i, v) in neg.iter().enumerate() {
                self.by_kind[slot][i] = self.by_kind[slot][i].saturating_sub(*v);
            }
            if k == ROOT {
                break;
            }
            k = parent;
        }
        self.version += 1;
    }

    /// Puts what `other` read (complete or not) in place of folder `at`,
    /// which must not be read yet. Going up from a folder already mapped
    /// reuses it instead of reading it again.
    pub fn graft(&mut self, at: u32, other: &DiskTree) {
        debug_assert!(self.is_dir(at) && self.nodes[at as usize].count == 0);
        let mut queue = vec![(ROOT, at)];
        while let Some((src, dst)) = queue.pop() {
            let s = &other.nodes[src as usize];
            // The files of an expanded "smaller files" belong to its folder.
            let owner = if s.flags & REST != 0 {
                self.nodes[dst as usize].parent
            } else {
                dst
            };
            let first = self.nodes.len() as u32;
            for c in s.first..s.first + s.count {
                let o = &other.nodes[c as usize];
                if o.flags & REMOVED != 0 {
                    continue;
                }
                let id = self.nodes.len() as u32;
                let name = other.os_name(c);
                let (name_at, name_len) = self.pool(&name, id);
                let slot = if o.flags & DIR != 0 {
                    self.by_kind.push(other.by_kind[o.slot as usize]);
                    self.by_kind.len() as u32 - 1
                } else {
                    0
                };
                self.nodes.push(Node {
                    name_at,
                    name_len,
                    parent: owner,
                    first: 0,
                    count: 0,
                    slot,
                    ..o.clone()
                });
                if o.flags & (DIR | REST) == 0 && o.bytes >= BIG_FILE {
                    self.big.push(id);
                }
                if o.flags & DIR != 0 || (o.flags & REST != 0 && o.count > 0) {
                    queue.push((c, id));
                }
            }
            let count = self.nodes.len() as u32 - first;
            let node = &mut self.nodes[dst as usize];
            node.first = first;
            node.count = count;
        }
        let r = &other.nodes[ROOT as usize];
        let (bytes, files, pending) = (r.bytes, r.files, r.pending);
        let kinds = other.by_kind[0];
        if other.is_denied(ROOT) {
            self.nodes[at as usize].flags |= DENIED;
        }
        self.propagate(at, bytes, files, &kinds);
        self.adjust_pending(at, i64::from(pending) - 1);
        self.version += 1;
    }

    /// The `n` largest files below `root`, largest first.
    pub fn largest_files(&self, root: u32, n: usize) -> Vec<u32> {
        let mut found: Vec<u32> = self
            .big
            .iter()
            .copied()
            .filter(|&f| !self.is_removed(f) && self.is_within(f, root) && self.alive(f))
            .collect();
        if found.len() < n && self.files(root) < 200_000 {
            // A small folder: its kept files, even under BIG_FILE.
            let mut stack = vec![root];
            found.clear();
            while let Some(d) = stack.pop() {
                for c in self.children(d) {
                    if self.is_dir(c) {
                        stack.push(c);
                    } else if !self.is_rest(c) {
                        found.push(c);
                    }
                }
            }
        }
        found.sort_by_key(|&f| std::cmp::Reverse(self.bytes(f)));
        found.truncate(n);
        found
    }

    /// No folder above `n` was removed.
    fn alive(&self, mut n: u32) -> bool {
        while n != ROOT {
            if self.is_removed(n) {
                return false;
            }
            n = self.nodes[n as usize].parent;
        }
        true
    }
}

/// `path` relative to `root`, comparing names without case (Windows).
fn strip_prefix_ci<'a>(path: &'a Path, root: &Path) -> Option<&'a Path> {
    if let Ok(rel) = path.strip_prefix(root) {
        return Some(rel);
    }
    let mut p = path.components();
    for r in root.components() {
        let c = p.next()?;
        let same = match (c.as_os_str().to_str(), r.as_os_str().to_str()) {
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase(),
            _ => c == r,
        };
        if !same {
            return None;
        }
    }
    Some(p.as_path())
}

/// What a tile of the map is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileKind {
    /// A folder: a frame, with its name on top when there is room.
    Dir,
    File,
    /// A folder's "smaller files".
    Rest,
    /// The part of a folder still being read (the index knows its total).
    Unread,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    /// The node (for `Unread`, the folder being read).
    pub node: u32,
    pub kind: TileKind,
    pub rect: Rect,
    pub depth: u16,
    /// For a folder: the height of its name strip (0 = no room for it).
    pub header: f32,
}

/// Sizes of the nested layout, in pixels.
#[derive(Debug, Clone, Copy)]
pub struct LayoutOptions {
    /// Name strip of a folder.
    pub header: f32,
    /// Gap between a folder's frame and its contents.
    pub pad: f32,
    /// Smallest tile drawn.
    pub min_tile: f32,
    /// A folder shows its contents only when the inside is at least this.
    pub min_nest: f32,
    /// Most tiles (the deepest are dropped first).
    pub max_tiles: usize,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            header: 18.0,
            pad: 2.0,
            min_tile: 2.0,
            min_nest: 10.0,
            max_tiles: 12_000,
        }
    }
}

/// The nested treemap of `root` in `area`: every child as a tile, folders
/// with their own contents inside, level by level, while there is room.
/// Parents come before their children, so the last tile under a point is the
/// deepest one.
pub fn layout(tree: &DiskTree, root: u32, area: Rect, o: &LayoutOptions) -> Vec<Tile> {
    let mut out = Vec::new();
    // Breadth first, so a full budget drops the deepest levels, not a side.
    let mut level = vec![(root, area, 0u16)];
    while !level.is_empty() {
        let mut next = Vec::new();
        for (dir, rect, depth) in level {
            let mut items: Vec<(Option<u32>, f64)> = tree
                .children(dir)
                .map(|c| (Some(c), tree.shown(c) as f64))
                .filter(|(_, s)| *s > 0.0)
                .collect();
            let unread = tree.shown(dir).saturating_sub(tree.bytes(dir));
            if unread > 0 {
                items.push((None, unread as f64));
            }
            items.sort_by(|a, b| b.1.total_cmp(&a.1));
            let sizes: Vec<f64> = items.iter().map(|(_, s)| *s).collect();
            for ((child, _), r) in items.iter().zip(squarify(&sizes, rect)) {
                if r.w < o.min_tile || r.h < o.min_tile {
                    continue;
                }
                if out.len() >= o.max_tiles {
                    return out;
                }
                let Some(c) = *child else {
                    out.push(Tile {
                        node: dir,
                        kind: TileKind::Unread,
                        rect: r,
                        depth: depth + 1,
                        header: 0.0,
                    });
                    continue;
                };
                let kind = if tree.is_dir(c) {
                    TileKind::Dir
                } else if tree.is_rest(c) {
                    TileKind::Rest
                } else {
                    TileKind::File
                };
                let mut header = 0.0;
                if kind == TileKind::Dir {
                    header = if r.h >= o.header * 2.2 && r.w >= o.header * 3.0 {
                        o.header
                    } else {
                        0.0
                    };
                    let inner = Rect {
                        x: r.x + o.pad,
                        y: r.y + header.max(o.pad),
                        w: r.w - 2.0 * o.pad,
                        h: r.h - header.max(o.pad) - o.pad,
                    };
                    if inner.w >= o.min_nest && inner.h >= o.min_nest {
                        next.push((c, inner, depth + 1));
                    }
                }
                out.push(Tile {
                    node: c,
                    kind,
                    rect: r,
                    depth: depth + 1,
                    header,
                });
            }
        }
        level = next;
    }
    out
}

/// The deepest tile under `(x, y)`.
pub fn hit(tiles: &[Tile], x: f32, y: f32) -> Option<usize> {
    tiles.iter().rposition(|t| {
        x >= t.rect.x && x < t.rect.x + t.rect.w && y >= t.rect.y && y < t.rect.y + t.rect.h
    })
}

/// Arrow keys on the map: the tile among `candidates` nearest to `from` in a
/// direction (`dx`, `dy` each -1, 0 or 1), by distance between centers,
/// favoring tiles straight ahead.
pub fn nearest(
    tiles: &[Tile],
    from: usize,
    candidates: &[usize],
    dx: f32,
    dy: f32,
) -> Option<usize> {
    let c = center(&tiles[from].rect);
    candidates
        .iter()
        .copied()
        .filter(|&k| k != from)
        .filter_map(|k| {
            let p = center(&tiles[k].rect);
            let (vx, vy) = (p.0 - c.0, p.1 - c.1);
            let along = vx * dx + vy * dy;
            if along <= 0.5 {
                return None;
            }
            let across = (vx * dy).abs() + (vy * dx).abs();
            Some((k, along + 2.0 * across))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(k, _)| k)
}

fn center(r: &Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> Listed {
        Listed {
            name: name.into(),
            is_dir: false,
            size,
        }
    }

    fn dir(name: &str) -> Listed {
        Listed {
            name: name.into(),
            is_dir: true,
            size: 0,
        }
    }

    const MB: u64 = 1024 * 1024;

    /// root: a/ (x.mp4 10 MB, y.jpg 2 MB), b/ (empty), z.zip 5 MB, tiny.txt 10 B
    fn sample() -> (DiskTree, u32, u32) {
        let mut t = DiskTree::new(PathBuf::from(r"C:\data"));
        let subs = t.add_listing(
            ROOT,
            vec![
                dir("a"),
                dir("b"),
                file("z.zip", 5 * MB),
                file("tiny.txt", 10),
            ],
        );
        let (a, b) = (subs[0].0, subs[1].0);
        assert!(!t.complete(ROOT));
        t.add_listing(a, vec![file("x.mp4", 10 * MB), file("y.jpg", 2 * MB)]);
        t.add_listing(b, vec![]);
        (t, a, b)
    }

    #[test]
    fn totals_add_up_and_finish() {
        let (t, a, b) = sample();
        assert!(t.complete(ROOT));
        assert_eq!(t.bytes(ROOT), 17 * MB + 10);
        assert_eq!(t.files(ROOT), 4);
        assert_eq!(t.bytes(a), 12 * MB);
        assert_eq!(t.bytes(b), 0);
        let k = t.by_kind(ROOT);
        assert_eq!(k[Kind::Video as usize], 10 * MB);
        assert_eq!(k[Kind::Images as usize], 2 * MB);
        assert_eq!(k[Kind::Archives as usize], 5 * MB);
        assert_eq!(k[Kind::Documents as usize], 10);
    }

    #[test]
    fn small_files_become_one_node() {
        let (t, _, _) = sample();
        let rest: Vec<u32> = t.children(ROOT).filter(|&c| t.is_rest(c)).collect();
        assert_eq!(rest.len(), 1);
        assert_eq!(t.bytes(rest[0]), 10);
        assert_eq!(t.files(rest[0]), 1);
        assert_eq!(t.path_of(rest[0]), PathBuf::from(r"C:\data"));
    }

    #[test]
    fn many_files_keep_only_the_largest() {
        let mut t = DiskTree::new(PathBuf::from(r"C:\big"));
        let entries = (0..100)
            .map(|k| file(&format!("f{k}.bin"), MB + k))
            .collect();
        t.add_listing(ROOT, entries);
        let kept = t.children(ROOT).filter(|&c| !t.is_rest(c)).count();
        assert_eq!(kept, KEEP_PER_DIR);
        assert_eq!(t.files(ROOT), 100);
        let largest = t.largest_files(ROOT, 1)[0];
        assert_eq!(t.name(largest), "f99.bin");
    }

    #[test]
    fn zooming_in_expands_the_smaller_files() {
        let mut t = DiskTree::new(PathBuf::from(r"C:\d"));
        t.add_listing(
            ROOT,
            vec![file("big.bin", MB), file("a.txt", 30), file("b.txt", 10)],
        );
        let rest = t.children(ROOT).find(|&c| t.is_rest(c)).unwrap();
        t.expand_rest(
            rest,
            vec![
                file("big.bin", MB),
                file("a.txt", 30),
                file("b.txt", 10),
                dir("x"),
            ],
        );
        let names: Vec<&str> = t.children(ROOT).map(|c| t.name(c)).collect();
        assert_eq!(names, ["big.bin", "a.txt", "b.txt"]);
        assert_eq!(t.bytes(ROOT), MB + 40);
        let a = t.find(Path::new(r"C:\d\a.txt")).unwrap();
        assert_eq!(t.path_of(a), PathBuf::from(r"C:\d\a.txt"));
        // Grafted, the expanded files come along under their folder.
        let mut up = DiskTree::new(PathBuf::from(r"C:\"));
        let d = up.add_listing(ROOT, vec![dir("d")])[0].0;
        up.graft(d, &t);
        let a = up.find(Path::new(r"C:\d\a.txt")).unwrap();
        assert_eq!(up.path_of(a), PathBuf::from(r"C:\d\a.txt"));
    }

    #[test]
    fn find_and_path_round_trip() {
        let (t, a, _) = sample();
        let x = t.find(Path::new(r"C:\data\a\x.mp4")).unwrap();
        assert_eq!(t.parent(x), Some(a));
        assert_eq!(t.path_of(x), PathBuf::from(r"C:\data\a\x.mp4"));
        assert_eq!(t.find(Path::new(r"C:\DATA\A")), Some(a));
        assert_eq!(t.find(Path::new(r"C:\data")), Some(ROOT));
        assert_eq!(t.find(Path::new(r"C:\other")), None);
        assert!(t.is_within(x, a) && !t.is_within(a, x));
    }

    #[test]
    fn removing_lowers_every_total_above() {
        let (mut t, a, _) = sample();
        let x = t.find(Path::new(r"C:\data\a\x.mp4")).unwrap();
        t.remove(x);
        assert_eq!(t.bytes(a), 2 * MB);
        assert_eq!(t.bytes(ROOT), 7 * MB + 10);
        assert_eq!(t.by_kind(ROOT)[Kind::Video as usize], 0);
        assert_eq!(t.children(a).count(), 1);
        assert!(t.largest_files(ROOT, 10).iter().all(|&f| f != x));
        t.remove(a);
        assert_eq!(t.bytes(ROOT), 5 * MB + 10);
    }

    #[test]
    fn expected_total_while_reading() {
        let mut t = DiskTree::new(PathBuf::from(r"C:\data"));
        let subs = t.add_listing(ROOT, vec![dir("a")]);
        let a = subs[0].0;
        t.set_expected(a, 100);
        assert_eq!(t.shown(a), 100);
        let tiles = layout(
            &t,
            ROOT,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 200.0,
                h: 100.0,
            },
            &LayoutOptions::default(),
        );
        assert!(
            tiles
                .iter()
                .any(|x| x.kind == TileKind::Unread && x.node == a)
        );
        t.add_listing(a, vec![file("f.bin", 40)]);
        // Read: the real total wins.
        assert_eq!(t.shown(a), 40);
    }

    #[test]
    fn failed_folder_counts_as_read() {
        let mut t = DiskTree::new(PathBuf::from(r"C:\data"));
        let a = t.add_listing(ROOT, vec![dir("a")])[0].0;
        t.listing_failed(a);
        assert!(t.complete(ROOT));
        assert!(t.is_denied(a));
    }

    #[test]
    fn graft_reuses_a_mapped_folder() {
        let (old, _, _) = sample();
        let mut t = DiskTree::new(PathBuf::from(r"C:\"));
        let subs = t.add_listing(ROOT, vec![dir("data"), file("pagefile.sys", 3 * MB)]);
        t.graft(subs[0].0, &old);
        assert!(t.complete(ROOT));
        assert_eq!(t.bytes(ROOT), 20 * MB + 10);
        let x = t.find(Path::new(r"C:\data\a\x.mp4")).unwrap();
        assert_eq!(t.bytes(x), 10 * MB);
        assert_eq!(t.by_kind(ROOT)[Kind::Video as usize], 10 * MB);
        assert_eq!(t.largest_files(ROOT, 1), vec![x]);
    }

    #[test]
    fn layout_nests_and_hits_the_deepest() {
        let (t, a, _) = sample();
        let area = Rect {
            x: 0.0,
            y: 0.0,
            w: 600.0,
            h: 400.0,
        };
        let tiles = layout(&t, ROOT, area, &LayoutOptions::default());
        let x = t.find(Path::new(r"C:\data\a\x.mp4")).unwrap();
        let ta = tiles.iter().position(|k| k.node == a).unwrap();
        let tx = tiles.iter().position(|k| k.node == x).unwrap();
        assert!(ta < tx, "parents first");
        let (ra, rx) = (tiles[ta].rect, tiles[tx].rect);
        assert!(rx.x >= ra.x && rx.x + rx.w <= ra.x + ra.w + 0.01);
        assert!(rx.y >= ra.y + tiles[ta].header);
        assert_eq!(tiles[tx].depth, 2);
        let (cx, cy) = (rx.x + rx.w / 2.0, rx.y + rx.h / 2.0);
        assert_eq!(hit(&tiles, cx, cy), Some(tx));
        // The empty folder takes no room.
        assert!(
            tiles
                .iter()
                .all(|k| t.bytes(k.node) > 0 || k.kind == TileKind::Unread)
        );
    }

    #[test]
    fn layout_respects_the_budget() {
        let (t, _, _) = sample();
        let area = Rect {
            x: 0.0,
            y: 0.0,
            w: 600.0,
            h: 400.0,
        };
        let o = LayoutOptions {
            max_tiles: 2,
            ..LayoutOptions::default()
        };
        assert_eq!(layout(&t, ROOT, area, &o).len(), 2);
    }

    #[test]
    fn arrows_pick_the_neighbor_in_that_direction() {
        let tile = |x: f32, y: f32| Tile {
            node: 0,
            kind: TileKind::File,
            rect: Rect {
                x,
                y,
                w: 10.0,
                h: 10.0,
            },
            depth: 1,
            header: 0.0,
        };
        let tiles = [
            tile(0.0, 0.0),
            tile(20.0, 0.0),
            tile(0.0, 20.0),
            tile(20.0, 20.0),
        ];
        let all = [0, 1, 2, 3];
        assert_eq!(nearest(&tiles, 0, &all, 1.0, 0.0), Some(1));
        assert_eq!(nearest(&tiles, 0, &all, 0.0, 1.0), Some(2));
        assert_eq!(nearest(&tiles, 3, &all, -1.0, 0.0), Some(2));
        assert_eq!(nearest(&tiles, 0, &all, -1.0, 0.0), None);
    }

    #[test]
    fn kinds_by_extension() {
        assert_eq!(Kind::of("praia.JPG"), Kind::Images);
        assert_eq!(Kind::of("filme.mkv"), Kind::Video);
        assert_eq!(Kind::of("relatório.pdf"), Kind::Documents);
        assert_eq!(Kind::of("backup.7z"), Kind::Archives);
        assert_eq!(Kind::of("main.rs"), Kind::Code);
        assert_eq!(Kind::of("app.dll"), Kind::Programs);
        assert_eq!(Kind::of("sem-extensao"), Kind::Other);
    }
}
