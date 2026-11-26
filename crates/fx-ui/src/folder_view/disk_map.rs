//! Disk map (T6.1, D-068, D-078): what takes space below the folder, as a
//! nested treemap, every level at once, like a whole-disk map. One scan
//! (`fx_fs::disk_scan`, several threads) fills a `DiskTree` that the map
//! draws while it grows, with the real progress; with the index service the
//! totals of the first levels come from it at once, so the map has its final
//! shape before the folders are read. Zooming in (Enter, double click) and
//! out (Backspace) reuse what was read; going above the mapped folder reads
//! only the rest and grafts the old tree.
//!
//! Hover or select a block for its path, size and share; the side panel
//! splits the space by kind (1–8 highlight one) and lists the largest files.
//! Delete sends the chosen block to the Recycle Bin (with confirmation and
//! undo) and the totals drop once it is gone. F5 reads again; Esc closes.
//! The UI thread never waits on the tree: it reads it with `try_read` and
//! keeps the last layout while a batch is being written.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant};

use futures::StreamExt;
use fx_core::disk_tree::{
    DiskTree, KINDS, Kind, LayoutOptions, ROOT, Tile, TileKind, hit, layout, nearest,
};
use fx_core::format::{count, size_short};
use fx_core::treemap::Rect;
use fx_core::{tr, trf};
use gpui::{
    App, Bounds, Context, FontWeight, Hsla, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent,
    Pixels, SharedString, TextRun, Window, canvas, div, fill, linear_color_stop, linear_gradient,
    point, prelude::*, px, quad, size,
};

use super::{FolderView, Restore, rows_of};
use crate::theme::{
    CAPTION_SIZE, MAP_ALPHA_DARK, MAP_ALPHA_LIGHT, MAP_COLORS, RADIUS, Theme, rgba, space,
};

/// Width of the side panel; hidden when the map would be narrower than
/// `SIDE_MIN_MAP`.
const SIDE_WIDTH: f32 = 248.0;
const SIDE_MIN_MAP: f32 = 420.0;
const LARGEST: usize = 12;

/// One scan of a folder tree, shared by every map that shows a part of it.
struct Scan {
    id: u64,
    tree: Arc<RwLock<DiskTree>>,
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    started: Instant,
    took: Cell<Option<Duration>>,
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

thread_local! {
    /// The last scan, kept for the session: reopening the map, or opening it
    /// in another tab below the same folder, is instant.
    static LAST: RefCell<Option<Rc<Scan>>> = const { RefCell::new(None) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// What a tile shows, copied out of the tree when the layout is made.
#[derive(Clone)]
struct Info {
    name: SharedString,
    size: SharedString,
    parent: u32,
    kind: Kind,
    denied: bool,
    bytes: u64,
}

#[derive(Default)]
struct Paint {
    tiles: Vec<Tile>,
    infos: Vec<Info>,
}

/// The side panel and the header, copied out of the tree.
#[derive(Default, Clone)]
struct Summary {
    total: u64,
    files: u64,
    pending: u32,
    kinds: [u64; KINDS],
    /// Node, name, size, folder (relative to the map's folder).
    largest: Vec<(u32, SharedString, u64, SharedString)>,
    /// From the map's folder up to the scan's root: (path, name).
    crumbs: Vec<(PathBuf, SharedString)>,
}

/// Details of the block under the mouse or chosen.
#[derive(Clone)]
struct Detail {
    path: String,
    size: u64,
    share: f32,
    files: u64,
    kind: TileKind,
    denied: bool,
}

/// The open map.
pub(super) struct DiskMap {
    scan: Rc<Scan>,
    root: u32,
    selected: Option<u32>,
    hover: Option<u32>,
    highlight: Option<Kind>,
    paint: Rc<Paint>,
    key: (u64, u32, u32, u32),
    summary: Summary,
    details: Vec<(u32, Detail)>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    /// Free and total bytes, when the map shows a whole drive.
    drive: Option<(u64, u64)>,
    drive_asked: Option<PathBuf>,
    /// Blocks sent to the Recycle Bin, waiting to disappear.
    removing: Vec<u32>,
    /// "Smaller files" blocks being read one by one (zoomed in).
    expanding: Vec<u32>,
}

fn alpha(c: u32, a: u32) -> u32 {
    (c << 8) | (a & 0xFF)
}

fn with_alpha(c: u32, a: u32) -> u32 {
    (c & 0xFFFF_FF00) | (a & 0xFF)
}

fn hsla(c: u32) -> Hsla {
    rgba(c).into()
}

impl FolderView {
    /// Ctrl+Shift+M: opens the map of this folder, or closes it.
    pub(super) fn toggle_disk_map(&mut self, cx: &mut Context<Self>) {
        if self.disk_map.take().is_none() {
            let scan = LAST
                .with_borrow(|last| last.clone())
                .filter(|s| s.done.load(Ordering::Relaxed) && within(&self.path, &s.root))
                .unwrap_or_else(|| self.start_scan(self.path.clone(), None, cx));
            self.disk_map = Some(DiskMap {
                scan,
                root: ROOT,
                selected: None,
                hover: None,
                highlight: None,
                paint: Rc::default(),
                key: (u64::MAX, 0, 0, 0),
                summary: Summary::default(),
                details: Vec::new(),
                bounds: Rc::default(),
                drive: None,
                drive_asked: None,
                removing: Vec::new(),
                expanding: Vec::new(),
            });
        }
        cx.notify();
    }

    /// Starts reading `root`; `reuse` is a finished scan of a folder below it.
    fn start_scan(
        &mut self,
        root: PathBuf,
        reuse: Option<Arc<RwLock<DiskTree>>>,
        cx: &mut Context<Self>,
    ) -> Rc<Scan> {
        let id = NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        let scan = Rc::new(Scan {
            id,
            tree: Arc::new(RwLock::new(DiskTree::new(root.clone()))),
            root: root.clone(),
            cancel: Arc::default(),
            done: Arc::default(),
            started: Instant::now(),
            took: Cell::new(None),
        });
        let (tx, rx) = channel();
        let totals: fx_fs::disk_scan::Totals = Box::new(|path: &Path| {
            let totals = super::dir_sizes::service_totals(path)?;
            Some(
                totals
                    .into_iter()
                    .filter_map(|(p, bytes)| Some((p.file_name()?.to_owned(), bytes)))
                    .collect(),
            )
        });
        let started = fx_fs::disk_scan::scan_tree(
            scan.tree.clone(),
            reuse,
            Some(totals),
            scan.cancel.clone(),
            scan.done.clone(),
            tx,
        );
        if started.is_err() {
            scan.done.store(true, Ordering::Relaxed);
            self.notice = Some(tr!("não deu para ler a pasta", "could not read the folder").into());
        }
        LAST.with_borrow_mut(|last| *last = Some(scan.clone()));
        cx.spawn(async move |this, cx| {
            let mut rx = crate::chan::stream(rx);
            while rx.next().await.is_some() {
                while rx.try_recv().is_ok() {}
                let alive = this.update(cx, |view, cx| {
                    let Some(map) = view.disk_map.as_ref() else {
                        return false;
                    };
                    if map.scan.id != id {
                        return false;
                    }
                    if map.scan.done.load(Ordering::Relaxed) && map.scan.took.get().is_none() {
                        map.scan.took.set(Some(map.scan.started.elapsed()));
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
        scan
    }

    /// Every render: follows the folder (a new scan when it left the mapped
    /// one) and remakes the layout when the tree, folder or size changed.
    pub(super) fn sync_disk_map(&mut self, cx: &mut Context<Self>) {
        let Some(map) = self.disk_map.as_ref() else {
            return;
        };
        if !within(&self.path, &map.scan.root) {
            let old = map.scan.clone();
            let reuse = (old.done.load(Ordering::Relaxed) && within(&old.root, &self.path))
                .then(|| old.tree.clone());
            let scan = self.start_scan(self.path.clone(), reuse, cx);
            if let Some(map) = self.disk_map.as_mut() {
                map.scan = scan;
                map.selected = None;
                map.hover = None;
                map.removing.clear();
                map.expanding.clear();
                map.key = (u64::MAX, 0, 0, 0);
            }
        }
        self.ask_drive_space(cx);
        let list_bounds = self.scroll.0.borrow().base_handle.bounds();
        let path = self.path.clone();
        let Some(map) = self.disk_map.as_mut() else {
            return;
        };
        let measured = map.bounds.get();
        let (w, h) = if measured.size.width > px(0.0) {
            (
                f32::from(measured.size.width),
                f32::from(measured.size.height),
            )
        } else {
            let list_w = f32::from(list_bounds.size.width);
            let side = if list_w >= SIDE_MIN_MAP + SIDE_WIDTH {
                SIDE_WIDTH + space(2)
            } else {
                0.0
            };
            (
                (list_w - side - space(4)).max(320.0),
                (f32::from(list_bounds.size.height) - 120.0).max(200.0),
            )
        };
        let tree_lock = map.scan.tree.clone();
        // Never wait: a batch being written keeps the last layout.
        let Ok(tree) = tree_lock.try_read() else {
            return;
        };
        let root = nearest_node(&tree, &path);
        let key = (tree.version(), root, w as u32, h as u32);
        if key == map.key {
            return;
        }
        if map.root != root {
            map.selected = map
                .selected
                .filter(|&s| s != root && tree.is_within(s, root));
        }
        map.root = root;
        map.key = key;
        let tiles = layout(
            &tree,
            root,
            Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            &LayoutOptions::default(),
        );
        let infos = tiles.iter().map(|t| info(&tree, t)).collect();
        // A "smaller files" block with room to show its files: read them.
        let expand: Vec<(u32, PathBuf)> = tiles
            .iter()
            .filter(|t| {
                t.kind == TileKind::Rest
                    && t.rect.w * t.rect.h >= w * h * 0.02
                    && t.rect.w >= 96.0
                    && t.rect.h >= 64.0
                    && !map.expanding.contains(&t.node)
            })
            .map(|t| (t.node, tree.path_of(t.node)))
            .collect();
        map.paint = Rc::new(Paint { tiles, infos });
        map.removing.retain(|&n| !tree.is_removed(n));
        let largest = tree
            .largest_files(root, LARGEST)
            .into_iter()
            .map(|f| {
                let folder = tree.path_of(tree.parent(f).unwrap_or(ROOT));
                let rel = folder
                    .strip_prefix(&path)
                    .map(|r| r.display().to_string())
                    .unwrap_or_default();
                (
                    f,
                    SharedString::from(tree.name(f).to_owned()),
                    tree.bytes(f),
                    SharedString::from(rel),
                )
            })
            .collect();
        let mut crumbs = Vec::new();
        let mut k = Some(root);
        while let Some(n) = k {
            crumbs.push((tree.path_of(n), SharedString::from(tree.name(n).to_owned())));
            k = tree.parent(n);
        }
        crumbs.reverse();
        map.summary = Summary {
            total: tree.shown(root),
            files: tree.files(root),
            pending: tree.pending(ROOT),
            kinds: tree.by_kind(root),
            largest,
            crumbs,
        };
        let mut details = Vec::new();
        for n in [map.hover, map.selected].into_iter().flatten() {
            if n < tree.len() as u32 && !tree.is_removed(n) {
                details.push((n, detail(&tree, n, root, &path)));
            }
        }
        map.details = details;
        drop(tree);
        for (rest, folder) in expand {
            map.expanding.push(rest);
            let tree = tree_lock.clone();
            let id = map.scan.id;
            let (tx, rx) = channel::<()>();
            let _ = std::thread::Builder::new()
                .name("fx-map-expand".into())
                .spawn(move || {
                    if let Some(files) = fx_fs::disk_scan::read(&folder) {
                        tree.write()
                            .unwrap_or_else(PoisonError::into_inner)
                            .expand_rest(rest, files);
                        let _ = tx.send(());
                    }
                });
            cx.spawn(async move |this, cx| {
                let mut rx = crate::chan::stream(rx);
                if rx.next().await.is_some() {
                    let _ = this.update(cx, |view, cx| {
                        if view.disk_map.as_ref().is_some_and(|m| m.scan.id == id) {
                            cx.notify();
                        }
                    });
                }
            })
            .detach();
        }
    }

    /// The free space of the drive, when the map shows a whole one (read
    /// off the UI thread).
    fn ask_drive_space(&mut self, cx: &mut Context<Self>) {
        let Some(map) = self.disk_map.as_mut() else {
            return;
        };
        let is_drive = self.path.parent().is_none();
        if !is_drive {
            map.drive = None;
            map.drive_asked = None;
            return;
        }
        if map.drive_asked.as_deref() == Some(self.path.as_path()) {
            return;
        }
        map.drive_asked = Some(self.path.clone());
        let root = self.path.clone();
        cx.spawn(async move |this, cx| {
            let drives = cx.background_spawn(async { fx_shell::drives() }).await;
            let space = drives
                .into_iter()
                .find(|d| within(&root, &d.root) && within(&d.root, &root))
                .and_then(|d| d.space);
            let _ = this.update(cx, |view, cx| {
                if let Some(map) = view.disk_map.as_mut() {
                    map.drive = space;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Keys while the map is open. Returns whether it took them.
    pub(super) fn disk_map_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(map) = self.disk_map.as_mut() else {
            return false;
        };
        let m = ks.modifiers;
        if m.control || m.alt && ks.key != "up" {
            return false;
        }
        let dir = match ks.key.as_str() {
            "left" if !m.alt => Some((-1.0, 0.0)),
            "right" => Some((1.0, 0.0)),
            "up" if !m.alt => Some((0.0, -1.0)),
            "down" => Some((0.0, 1.0)),
            _ => None,
        };
        if let Some((dx, dy)) = dir {
            let tiles = &map.paint.tiles;
            let current = map.selected.and_then(|s| {
                tiles
                    .iter()
                    .position(|t| t.node == s && t.kind != TileKind::Unread)
            });
            let next = match current {
                None => tiles.iter().position(|t| t.kind != TileKind::Unread),
                Some(k) => {
                    let parent = map.paint.infos[k].parent;
                    let siblings: Vec<usize> = (0..tiles.len())
                        .filter(|&j| {
                            tiles[j].kind != TileKind::Unread && map.paint.infos[j].parent == parent
                        })
                        .collect();
                    nearest(tiles, k, &siblings, dx, dy).or(Some(k))
                }
            };
            if let Some(k) = next {
                map.selected = Some(tiles[k].node);
                map.key.0 = u64::MAX;
            }
            cx.notify();
            return true;
        }
        if let Some(d) = ks.key.parse::<usize>().ok().filter(|_| !m.shift) {
            map.highlight = match d {
                1..=KINDS => {
                    let k = Kind::ALL[d - 1];
                    (map.highlight != Some(k)).then_some(k)
                }
                _ => None,
            };
            cx.notify();
            return true;
        }
        match ks.key.as_str() {
            "escape" if map.highlight.is_some() => map.highlight = None,
            "escape" => self.disk_map = None,
            "up" | "backspace" => {
                self.go_up(window, cx);
            }
            "enter" => {
                if let Some(n) = map.selected {
                    self.open_node(n, window, cx);
                }
            }
            "delete" => {
                if let Some(n) = map.selected {
                    self.recycle_node(n, window, cx);
                }
            }
            "f5" => {
                let path = self.path.clone();
                let scan = self.start_scan(path, None, cx);
                if let Some(map) = self.disk_map.as_mut() {
                    map.scan = scan;
                    map.selected = None;
                    map.removing.clear();
                    map.expanding.clear();
                    map.key = (u64::MAX, 0, 0, 0);
                }
            }
            _ => return true,
        }
        cx.notify();
        true
    }

    /// Enter or double click: a folder becomes the map's folder (and the
    /// list's); a file is shown in the list, chosen.
    fn open_node(&mut self, n: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(map) = self.disk_map.as_ref() else {
            return;
        };
        let tree_lock = map.scan.tree.clone();
        let Ok(tree) = tree_lock.try_read() else {
            return;
        };
        if n as usize >= tree.len() {
            return;
        }
        let path = tree.path_of(n);
        let (is_dir, is_rest) = (tree.is_dir(n), tree.is_rest(n));
        drop(tree);
        if is_dir || is_rest {
            if path != self.path {
                self.go_to(path, None, window, cx);
            }
            if let Some(map) = self.disk_map.as_mut() {
                map.selected = None;
            }
            return;
        }
        let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        self.disk_map = None;
        if folder == self.path {
            let entry = rows_of(&self.filtered, &self.list)
                .iter()
                .copied()
                .find(|&i| self.list.os_name(i) == name);
            if let Some(entry) = entry {
                self.a11y_select(entry);
            }
        } else {
            self.go_to(
                folder.to_path_buf(),
                Some(Restore::cursor_on(name.to_owned())),
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Delete: the block to the Recycle Bin (asking first), then the map
    /// drops it once it is really gone from the disk.
    fn recycle_node(&mut self, n: u32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(map) = self.disk_map.as_mut() else {
            return;
        };
        let tree_lock = map.scan.tree.clone();
        let path = {
            let Ok(tree) = tree_lock.try_read() else {
                return;
            };
            if n == ROOT || n as usize >= tree.len() || tree.is_rest(n) || tree.is_removed(n) {
                return;
            }
            tree.path_of(n)
        };
        map.removing.push(n);
        let watched = path.clone();
        let tree = tree_lock.clone();
        let (tx, rx) = channel::<()>();
        // Waits (off the UI thread) for the item to leave the disk: the
        // confirmation, then the Recycle Bin, may take a while.
        let _ = std::thread::Builder::new()
            .name("fx-map-remove".into())
            .spawn(move || {
                let until = Instant::now() + Duration::from_secs(120);
                while Instant::now() < until {
                    if std::fs::symlink_metadata(&watched).is_err() {
                        tree.write()
                            .unwrap_or_else(PoisonError::into_inner)
                            .remove(n);
                        let _ = tx.send(());
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
        cx.spawn(async move |this, cx| {
            let mut rx = crate::chan::stream(rx);
            let gone = rx.next().await.is_some();
            let _ = this.update(cx, |view, cx| {
                if let Some(map) = view.disk_map.as_mut() {
                    if !gone {
                        map.removing.retain(|&r| r != n);
                    }
                    if map.selected == Some(n) && gone {
                        map.selected = None;
                    }
                    cx.notify();
                }
            });
        })
        .detach();
        self.recycle_confirmed(vec![path], window, cx);
    }

    fn map_hover(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(map) = self.disk_map.as_mut() else {
            return;
        };
        let b = map.bounds.get();
        let (x, y) = (
            f32::from(position.x - b.origin.x),
            f32::from(position.y - b.origin.y),
        );
        let node = hit(&map.paint.tiles, x, y)
            .map(|k| &map.paint.tiles[k])
            .filter(|t| t.kind != TileKind::Unread)
            .map(|t| t.node);
        if node != map.hover {
            map.hover = node;
            map.key.0 = u64::MAX;
            cx.notify();
        }
    }

    fn map_click(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.map_hover(ev.position, cx);
        let Some(map) = self.disk_map.as_mut() else {
            return;
        };
        let Some(n) = map.hover else { return };
        map.selected = Some(n);
        map.key.0 = u64::MAX;
        if ev.click_count >= 2 {
            self.open_node(n, window, cx);
        }
        cx.notify();
    }

    fn select_node(&mut self, n: u32, cx: &mut Context<Self>) {
        if let Some(map) = self.disk_map.as_mut() {
            map.selected = Some(n);
            map.key.0 = u64::MAX;
            cx.notify();
        }
    }

    pub(super) fn render_disk_map(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let map = self.disk_map.as_ref()?;
        let t = self.look.theme;
        let s = &map.summary;
        let font = self.look.font;
        let running = !map.scan.done.load(Ordering::Relaxed);
        let caption = |text: String| {
            div()
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .child(text)
        };

        // Breadcrumbs: the mapped folders, each one a click away.
        let last = s.crumbs.len().saturating_sub(1);
        let mut crumbs = div()
            .flex()
            .items_center()
            .gap(px(space(1)))
            .min_w_0()
            .overflow_hidden();
        // The nearest folders matter most: at most four, after a "…".
        let skip = s.crumbs.len().saturating_sub(4);
        if skip > 0 {
            crumbs = crumbs.child(div().text_color(rgba(t.text_secondary)).child("…"));
        }
        for (k, (path, name)) in s.crumbs.iter().enumerate().skip(skip) {
            if k > 0 {
                crumbs = crumbs.child(div().text_color(rgba(t.text_secondary)).child("›"));
            }
            let path = path.clone();
            let mut crumb = div()
                .id(("map-crumb", k))
                .min_w(px(24.0))
                .px(px(space(1)))
                .rounded(px(RADIUS))
                .truncate()
                .child(name.clone());
            if k == last {
                crumb = crumb.font_weight(FontWeight::SEMIBOLD);
            } else {
                crumb = crumb
                    .text_color(rgba(t.text_secondary))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgba(t.hover)))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.go_to(path.clone(), None, window, cx);
                    }));
            }
            crumbs = crumbs.child(crumb);
        }
        let totals = div()
            .flex_none()
            .text_color(rgba(t.text_secondary))
            .child(trf!(
                "{} · {} arquivos",
                "{} · {} files",
                size_short(s.total),
                count(s.files as usize)
            ));
        let status = if running {
            trf!(
                "Lendo · {} pastas na fila · {}",
                "Reading · {} folders queued · {}",
                count(s.pending as usize),
                secs(map.scan.started.elapsed())
            )
        } else {
            let took = map
                .scan
                .took
                .get()
                .unwrap_or_else(|| map.scan.started.elapsed());
            let age = map.scan.started.elapsed().saturating_sub(took);
            if age < Duration::from_secs(60) {
                trf!(
                    "Lido em {} · F5 lê de novo",
                    "Read in {} · F5 reads again",
                    secs(took)
                )
            } else {
                trf!(
                    "Lido há {} min · F5 lê de novo",
                    "Read {} min ago · F5 reads again",
                    age.as_secs() / 60
                )
            }
        };

        // The block under the mouse, or else the one chosen.
        let shown = map
            .hover
            .or(map.selected)
            .and_then(|n| map.details.iter().find(|(d, _)| *d == n))
            .map(|(_, d)| d.clone());
        let detail_line = match shown {
            Some(d) => {
                let what = match d.kind {
                    TileKind::Rest => trf!(
                        "{} arquivos menores que 256 KB",
                        "{} files under 256 KB",
                        count(d.files as usize)
                    ),
                    TileKind::Dir if d.denied => tr!("sem acesso", "no access").to_owned(),
                    TileKind::Dir => trf!("{} arquivos", "{} files", count(d.files as usize)),
                    _ => String::new(),
                };
                let mut line =
                    format!("{} · {} · {}", d.path, size_short(d.size), percent(d.share));
                if !what.is_empty() {
                    line.push_str(" · ");
                    line.push_str(&what);
                }
                line
            }
            None => tr!(
                "Passe o mouse sobre um bloco para ver o que é",
                "Hover a block to see what it is"
            )
            .to_owned(),
        };

        // A whole drive: used and free space.
        let drive_bar = map
            .drive
            .filter(|(_, total)| *total > 0)
            .map(|(free, total)| {
                let used = total.saturating_sub(free);
                div()
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .px(px(space(3)))
                    .pb(px(space(1)))
                    .child(
                        div()
                            .flex_1()
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(rgba(t.divider))
                            .child(
                                div()
                                    .h_full()
                                    .rounded(px(3.0))
                                    .w(gpui::relative(used as f32 / total as f32))
                                    .bg(rgba(t.accent)),
                            ),
                    )
                    .child(caption(trf!(
                        "{} usados de {} · {} livres",
                        "{} used of {} · {} free",
                        size_short(used),
                        size_short(total),
                        size_short(free)
                    )))
            });

        // The map itself, painted in one canvas.
        let paint = map.paint.clone();
        let bounds_cell = map.bounds.clone();
        let colors = MapColors::new(&t);
        let state = PaintState {
            hover: map.hover,
            selected: map.selected,
            highlight: map.highlight,
            removing: map.removing.clone(),
        };
        let area = div()
            .id("disk-map-area")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .on_mouse_move(cx.listener(|view, ev: &MouseMoveEvent, _, cx| {
                view.map_hover(ev.position, cx);
            }))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered && let Some(map) = view.disk_map.as_mut() {
                    map.hover = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, ev: &MouseDownEvent, window, cx| view.map_click(ev, window, cx)),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        // The first frame laid out for a guessed size: redo it.
                        if bounds_cell.get().size != bounds.size {
                            window.refresh();
                        }
                        bounds_cell.set(bounds);
                    },
                    move |bounds, (), window, cx| {
                        paint_map(&paint, &colors, &state, font, bounds, window, cx);
                    },
                )
                .size_full(),
            )
            .when(paint_is_empty(&map.paint) && !running, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(caption(
                            tr!(
                                "Nada ocupa espaço aqui: a pasta está vazia ou só tem itens vazios.",
                                "Nothing takes space here: the folder is empty or holds only empty items."
                            )
                            .to_owned(),
                        )),
                )
            });

        let side = self.render_map_side(map, cx);
        Some(
            div()
                .id("disk-map")
                .occlude()
                .absolute()
                .top(px(space(12)))
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .bg(rgba(t.base))
                .border_t_1()
                .border_color(rgba(t.divider))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .pt(px(space(2)))
                        .flex()
                        .items_center()
                        .gap(px(space(3)))
                        .text_size(px(CAPTION_SIZE))
                        .child(div().flex_1().min_w_0().child(crumbs))
                        .child(totals)
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgba(if running { t.accent } else { t.text_secondary }))
                                .child(status),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .min_w_0()
                        .truncate()
                        .child(detail_line),
                )
                .children(drive_bar)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .gap(px(space(2)))
                        .px(px(space(2)))
                        .child(area)
                        .children(side),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .truncate()
                        .child(tr!(
                            "Setas andam · Enter entra · Backspace sobe · 1–8 destacam um tipo · Delete: Lixeira · F5 lê de novo · Esc fecha",
                            "Arrows move · Enter goes in · Backspace goes up · 1–8 highlight a kind · Delete: Recycle Bin · F5 reads again · Esc closes"
                        )),
                ),
        )
    }

    /// The side panel: space by kind of file and the largest files.
    fn render_map_side(
        &self,
        map: &DiskMap,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let list_w = f32::from(self.scroll.0.borrow().base_handle.bounds().size.width);
        if list_w > 0.0 && list_w < SIDE_MIN_MAP + SIDE_WIDTH {
            return None;
        }
        let t = self.look.theme;
        let s = &map.summary;
        let all: u64 = s.kinds.iter().sum();
        let mut kinds = div().flex().flex_col().gap(px(2.0));
        let mut order: Vec<usize> = (0..KINDS).filter(|&k| s.kinds[k] > 0).collect();
        order.sort_by_key(|&k| std::cmp::Reverse(s.kinds[k]));
        for k in order {
            let kind = Kind::ALL[k];
            let bytes = s.kinds[k];
            let share = if all > 0 {
                bytes as f32 / all as f32
            } else {
                0.0
            };
            let on = map.highlight == Some(kind);
            let dim = map.highlight.is_some() && !on;
            kinds = kinds.child(
                div()
                    .id(("map-kind", k))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(space(2)))
                    .py(px(space(1)))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgba(t.selected)))
                    .hover(|d| d.bg(rgba(t.hover)))
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if let Some(map) = view.disk_map.as_mut() {
                            map.highlight = (map.highlight != Some(kind)).then_some(kind);
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .gap(px(space(2)))
                            .items_center()
                            .text_color(rgba(if dim { t.text_secondary } else { t.text }))
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(10.0))
                                    .rounded(px(2.0))
                                    .bg(rgba(alpha(MAP_COLORS[k], 0xE0))),
                            )
                            .child(div().flex_1().child(kind_name(kind)))
                            .child(div().text_color(rgba(t.text_secondary)).child(format!(
                                "{} · {}",
                                size_short(bytes),
                                percent(share)
                            ))),
                    )
                    .child(
                        div().h(px(3.0)).rounded(px(2.0)).bg(rgba(t.divider)).child(
                            div()
                                .h_full()
                                .rounded(px(2.0))
                                .w(gpui::relative(share.max(0.005)))
                                .bg(rgba(alpha(MAP_COLORS[k], 0xC0))),
                        ),
                    ),
            );
        }
        let mut largest = div().flex().flex_col();
        for (k, (node, name, bytes, folder)) in s.largest.iter().enumerate() {
            let node = *node;
            let chosen = map.selected == Some(node);
            largest = largest.child(
                div()
                    .id(("map-largest", k))
                    .flex()
                    .flex_col()
                    .px(px(space(2)))
                    .py(px(2.0))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .when(chosen, |d| d.bg(rgba(t.selected)))
                    .hover(|d| d.bg(rgba(t.hover)))
                    .on_click(cx.listener(move |view, ev: &gpui::ClickEvent, window, cx| {
                        view.select_node(node, cx);
                        if ev.click_count() >= 2 {
                            view.open_node(node, window, cx);
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .gap(px(space(2)))
                            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgba(t.text_secondary))
                                    .child(size_short(*bytes)),
                            ),
                    )
                    .when(!folder.is_empty(), |d| {
                        d.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgba(t.text_secondary))
                                .child(folder.clone()),
                        )
                    }),
            );
        }
        let title = |text: &'static str| {
            div()
                .px(px(space(2)))
                .pt(px(space(2)))
                .pb(px(space(1)))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
        };
        Some(
            div()
                .id("disk-map-side")
                .flex_none()
                .w(px(SIDE_WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .text_size(px(CAPTION_SIZE))
                .rounded(px(RADIUS * 2.0))
                .bg(rgba(t.layer))
                .border_1()
                .border_color(rgba(t.divider))
                .child(title(tr!("Por tipo", "By kind")))
                .child(kinds)
                .when(!s.largest.is_empty(), |d| {
                    d.child(title(tr!("Maiores arquivos", "Largest files")))
                        .child(largest)
                })
                .child(div().h(px(space(2)))),
        )
    }

    /// The map as a screen reader reads it: the blocks of the folder, in
    /// size order.
    pub(super) fn disk_map_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        let map = self.disk_map.as_ref()?;
        let total = map.summary.total;
        let tops: Vec<usize> = (0..map.paint.tiles.len())
            .filter(|&k| {
                map.paint.tiles[k].depth == 1 && map.paint.tiles[k].kind != TileKind::Unread
            })
            .collect();
        let items: Vec<Item> = tops
            .iter()
            .map(|&k| {
                let tile = &map.paint.tiles[k];
                let info = &map.paint.infos[k];
                let bytes = info.size.to_string();
                let share = (info.bytes * 100).checked_div(total).unwrap_or(0);
                Item::Choice {
                    label: info.name.to_string(),
                    description: Some(if share > 0 {
                        format!("{bytes}, {share}%")
                    } else {
                        bytes
                    }),
                    selected: map.selected == Some(tile.node),
                }
            })
            .collect();
        let focus = tops
            .iter()
            .position(|&k| map.selected == Some(map.paint.tiles[k].node))
            .or((!items.is_empty()).then_some(0));
        Some(Overlay {
            title: trf!("Mapa de {}", "Map of {}", self.path.display()),
            description: Some(
                tr!(
                    "O que ocupa espaço, do maior ao menor. Setas andam pelos blocos, Enter entra na pasta, Backspace sobe, Delete manda para a Lixeira, Esc fecha.",
                    "What takes space, largest first. Arrows move between blocks, Enter goes into a folder, Backspace goes up, Delete sends to the Recycle Bin, Esc closes."
                )
                .to_owned(),
            ),
            items,
            focus,
            positions: None,
            modal: true,
        })
    }
}

/// `path` is `root` or below it (names compared without case).
fn within(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    match (path.to_str(), root.to_str()) {
        (Some(p), Some(r)) => Path::new(&p.to_lowercase()).starts_with(r.to_lowercase()),
        _ => false,
    }
}

/// The node of `path`, or of the nearest folder above it already read.
fn nearest_node(tree: &DiskTree, path: &Path) -> u32 {
    let mut p = Some(path);
    while let Some(q) = p {
        if let Some(n) = tree.find(q).filter(|&n| tree.is_dir(n)) {
            return n;
        }
        p = q.parent();
    }
    ROOT
}

fn info(tree: &DiskTree, t: &Tile) -> Info {
    let n = t.node;
    let (name, size) = match t.kind {
        TileKind::Unread => (
            SharedString::from(tr!("lendo…", "reading…")),
            SharedString::from(size_short(tree.shown(n).saturating_sub(tree.bytes(n)))),
        ),
        TileKind::Rest => (
            SharedString::from(trf!(
                "{} arquivos menores",
                "{} smaller files",
                count(tree.files(n) as usize)
            )),
            SharedString::from(size_short(tree.bytes(n))),
        ),
        _ => (
            SharedString::from(tree.name(n).to_owned()),
            SharedString::from(if tree.complete(n) {
                size_short(tree.shown(n))
            } else {
                format!("{}…", size_short(tree.shown(n)))
            }),
        ),
    };
    Info {
        name,
        size,
        parent: if t.kind == TileKind::Unread {
            n
        } else {
            tree.parent(n).unwrap_or(ROOT)
        },
        kind: tree.kind(n),
        denied: tree.is_denied(n),
        bytes: tree.shown(n),
    }
}

fn detail(tree: &DiskTree, n: u32, root: u32, path: &Path) -> Detail {
    let total = tree.shown(root).max(1);
    let full = tree.path_of(n);
    let rel = full
        .strip_prefix(path)
        .ok()
        .map(|r| r.display().to_string())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| tree.name(tree.find(&full).unwrap_or(root)).to_owned());
    Detail {
        path: if tree.is_rest(n) {
            trf!("{} (arquivos soltos)", "{} (loose files)", rel)
        } else {
            rel
        },
        size: tree.shown(n),
        share: (tree.shown(n) as f64 / total as f64).min(1.0) as f32,
        files: tree.files(n),
        kind: if tree.is_dir(n) {
            TileKind::Dir
        } else if tree.is_rest(n) {
            TileKind::Rest
        } else {
            TileKind::File
        },
        denied: tree.is_denied(n),
    }
}

fn paint_is_empty(p: &Paint) -> bool {
    p.tiles.is_empty()
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Images => tr!("Imagens", "Images"),
        Kind::Video => tr!("Vídeos", "Videos"),
        Kind::Audio => tr!("Áudio", "Audio"),
        Kind::Documents => tr!("Documentos", "Documents"),
        Kind::Archives => tr!("Compactados e discos", "Archives and disk images"),
        Kind::Code => tr!("Código e compilação", "Code and builds"),
        Kind::Programs => tr!("Programas e dados", "Programs and data"),
        Kind::Other => tr!("Outros", "Other"),
    }
}

fn percent(share: f32) -> String {
    let p = share * 100.0;
    if p > 0.0 && p < 1.0 {
        "<1%".to_owned()
    } else {
        format!("{}%", p.round() as u32)
    }
}

fn secs(d: Duration) -> String {
    let s = d.as_secs_f32();
    if s < 10.0 {
        format!("{s:.1} s").replace('.', tr!(",", "."))
    } else {
        format!("{} s", s.round() as u64)
    }
}

/// Theme colors the canvas needs (it outlives the borrow of the view).
#[derive(Clone, Copy)]
struct MapColors {
    text: u32,
    secondary: u32,
    accent: u32,
    base: u32,
    frame: u32,
    unread: u32,
    high_contrast: bool,
}

impl MapColors {
    fn new(t: &Theme) -> Self {
        MapColors {
            text: t.text,
            secondary: t.text_secondary,
            accent: t.accent,
            base: t.base,
            frame: with_alpha(t.text, 0x0E),
            unread: with_alpha(t.text, 0x0A),
            high_contrast: t.high_contrast,
        }
    }
}

struct PaintState {
    hover: Option<u32>,
    selected: Option<u32>,
    highlight: Option<Kind>,
    removing: Vec<u32>,
}

fn paint_map(
    p: &Paint,
    c: &MapColors,
    s: &PaintState,
    font: &'static str,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let rect = |r: &Rect, inset: f32| {
        Bounds::new(
            point(px(ox + r.x + inset), px(oy + r.y + inset)),
            size(
                px((r.w - 2.0 * inset).max(0.0)),
                px((r.h - 2.0 * inset).max(0.0)),
            ),
        )
    };
    let mut hovered = None;
    let mut chosen = None;
    for (k, (tile, info)) in p.tiles.iter().zip(&p.infos).enumerate() {
        let r = &tile.rect;
        let b = rect(r, 0.5);
        let removing = s
            .removing
            .iter()
            .any(|&n| n == tile.node && tile.kind != TileKind::Unread);
        if tile.kind != TileKind::Unread {
            if s.hover == Some(tile.node) {
                hovered = Some(k);
            }
            if s.selected == Some(tile.node) {
                chosen = Some(k);
            }
        }
        match tile.kind {
            TileKind::Dir => {
                if c.high_contrast {
                    window.paint_quad(quad(
                        b,
                        px(3.0),
                        rgba(c.base),
                        px(1.0),
                        hsla(c.text),
                        Default::default(),
                    ));
                } else {
                    window.paint_quad(fill(b, rgba(c.frame)).corner_radii(px(3.0)));
                }
                if tile.header > 0.0 {
                    let size_w = text_width(window, &info.size, font);
                    let room = r.w - 12.0 - size_w - 8.0;
                    let name = if info.denied {
                        SharedString::from(format!(
                            "{} · {}",
                            info.name,
                            tr!("sem acesso", "no access")
                        ))
                    } else {
                        info.name.clone()
                    };
                    if room > 16.0 {
                        paint_text(
                            window,
                            cx,
                            &name,
                            ox + r.x + 6.0,
                            oy + r.y + 2.0,
                            room,
                            c.text,
                            FontWeight::SEMIBOLD,
                            font,
                        );
                        paint_text(
                            window,
                            cx,
                            &info.size,
                            ox + r.x + r.w - 6.0 - size_w,
                            oy + r.y + 2.0,
                            size_w + 1.0,
                            c.secondary,
                            FontWeight::NORMAL,
                            font,
                        );
                    } else {
                        paint_text(
                            window,
                            cx,
                            &name,
                            ox + r.x + 6.0,
                            oy + r.y + 2.0,
                            r.w - 12.0,
                            c.text,
                            FontWeight::SEMIBOLD,
                            font,
                        );
                    }
                }
            }
            TileKind::File | TileKind::Rest => {
                let rgb = MAP_COLORS[info.kind as usize];
                let dim = s.highlight.is_some_and(|h| h != info.kind) || removing;
                let scale = |a: u32| {
                    let a = if tile.kind == TileKind::Rest {
                        a * 3 / 5
                    } else {
                        a
                    };
                    if dim { a / 4 } else { a }
                };
                if c.high_contrast {
                    let border = if s.highlight == Some(info.kind) {
                        c.accent
                    } else {
                        c.text
                    };
                    window.paint_quad(quad(
                        b,
                        px(2.0),
                        rgba(c.base),
                        px(1.0),
                        hsla(border),
                        Default::default(),
                    ));
                } else {
                    let bg = linear_gradient(
                        135.0,
                        linear_color_stop(hsla(alpha(rgb, scale(MAP_ALPHA_LIGHT))), 0.0),
                        linear_color_stop(hsla(alpha(rgb, scale(MAP_ALPHA_DARK))), 1.0),
                    );
                    window.paint_quad(fill(b, bg).corner_radii(px(2.0)));
                }
                let text = if dim { c.secondary } else { c.text };
                if r.w >= 44.0 && r.h >= 16.0 {
                    paint_text(
                        window,
                        cx,
                        &info.name,
                        ox + r.x + 5.0,
                        oy + r.y + 2.0,
                        r.w - 10.0,
                        text,
                        FontWeight::NORMAL,
                        font,
                    );
                }
                if r.w >= 44.0 && r.h >= 32.0 {
                    paint_text(
                        window,
                        cx,
                        &info.size,
                        ox + r.x + 5.0,
                        oy + r.y + 16.0,
                        r.w - 10.0,
                        c.secondary,
                        FontWeight::NORMAL,
                        font,
                    );
                }
            }
            TileKind::Unread => {
                window.paint_quad(fill(b, rgba(c.unread)).corner_radii(px(2.0)));
                if r.w >= 44.0 && r.h >= 16.0 {
                    paint_text(
                        window,
                        cx,
                        &info.name,
                        ox + r.x + 5.0,
                        oy + r.y + 2.0,
                        r.w - 10.0,
                        c.secondary,
                        FontWeight::NORMAL,
                        font,
                    );
                }
            }
        }
        if removing && r.w >= 60.0 && r.h >= 48.0 {
            paint_text(
                window,
                cx,
                tr!("indo para a Lixeira…", "going to the Recycle Bin…"),
                ox + r.x + 5.0,
                oy + r.y + r.h - 18.0,
                r.w - 10.0,
                c.secondary,
                FontWeight::NORMAL,
                font,
            );
        }
    }
    if let Some(k) = hovered.filter(|&k| Some(k) != chosen) {
        window.paint_quad(quad(
            rect(&p.tiles[k].rect, 0.5),
            px(3.0),
            gpui::transparent_black(),
            px(1.0),
            hsla(with_alpha(c.text, 0x90)),
            Default::default(),
        ));
    }
    if let Some(k) = chosen {
        window.paint_quad(quad(
            rect(&p.tiles[k].rect, 0.5),
            px(3.0),
            gpui::transparent_black(),
            px(2.0),
            hsla(c.accent),
            Default::default(),
        ));
    }
}

fn run(text: &str, font: &'static str, weight: FontWeight, color: Hsla) -> TextRun {
    TextRun {
        len: text.len(),
        font: gpui::Font {
            weight,
            ..gpui::font(font)
        },
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn text_width(window: &Window, text: &SharedString, font: &'static str) -> f32 {
    let runs = [run(text, font, FontWeight::NORMAL, gpui::black())];
    let line = window
        .text_system()
        .shape_line(text.clone(), px(CAPTION_SIZE), &runs, None);
    f32::from(line.width)
}

/// One line of caption text, cut with "…" to `max_w`.
#[allow(clippy::too_many_arguments)]
fn paint_text(
    window: &mut Window,
    cx: &mut App,
    text: &str,
    x: f32,
    y: f32,
    max_w: f32,
    color: u32,
    weight: FontWeight,
    font: &'static str,
) {
    if max_w < 10.0 || text.is_empty() {
        return;
    }
    let color = hsla(color);
    let mut runs = vec![run(text, font, weight, color)];
    let fitted = {
        let mut wrapper = window.text_system().line_wrapper(
            gpui::Font {
                weight,
                ..gpui::font(font)
            },
            px(CAPTION_SIZE),
        );
        wrapper.truncate_line(
            SharedString::from(text.to_owned()),
            px(max_w),
            "…",
            &mut runs,
        )
    };
    let line = window
        .text_system()
        .shape_line(fitted, px(CAPTION_SIZE), &runs, None);
    let _ = line.paint(point(px(x), px(y)), px(14.0), window, cx);
}
