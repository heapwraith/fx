//! Disk search (tasks T1.23, T2.6): Ctrl+F opens a search panel over the
//! tab; results appear while typing and every key cancels the previous
//! search.
//!
//! Two scopes, switched with Tab:
//! - **Disco inteiro**: the `fx-indexd` service when it runs (every NTFS
//!   volume, over its pipe), or, when fx itself is elevated, the MFT index of
//!   the current volume read in-process;
//! - **Nesta pasta**: a walk of the current folder and its subfolders on a
//!   worker thread (`fx_fs::search_tree_where`), results in batches.
//!
//! Filters are typed with the name (`tipo:imagem`, `ext:pdf`,
//! `tamanho:>10mb`, `data:semana`; `fx_core::search_filter`) and applied
//! inside each search, so they never cost a result place. Results show size
//! and date, sort by relevance, name, size or date (click the header), open
//! with Enter or a double click, show in their folder with Ctrl+Enter, drag
//! out, and have the Windows context menu.

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use fx_core::search_filter::Filters;
use fx_fs::{SearchHit, SearchSummary};
use fx_index::Index;
use fx_index::protocol::{Request, SearchFilter, Status};
use fx_shell::launch::Launch;
use gpui::{
    Context, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, ScrollStrategy,
    SharedString, UniformListScrollHandle, Window, deferred, div, prelude::*, px, uniform_list,
};

use super::{FolderView, Restore, local_date};
use crate::theme::{CAPTION_SIZE, ICON_FOLDER, ICON_SEARCH, RADIUS, rgba, space};

/// The folder walk stops after this many results (type more to narrow).
const TREE_LIMIT: usize = 2_000;
/// Most results asked of the index service.
const SERVICE_LIMIT: u32 = 1_000;
const PANEL_WIDTH: f32 = 760.0;
const ROW_HEIGHT: f32 = 40.0;
const SIZE_COL: f32 = 84.0;
const DATE_COL: f32 = 128.0;
const ICON_PAGE: &str = "\u{E7C3}";

/// One result.
#[derive(Debug, Clone)]
struct Found {
    path: PathBuf,
    is_dir: bool,
    name: SharedString,
    /// The folder it is in, for the second line.
    folder: SharedString,
    size: u64,
    /// Unix seconds.
    modified: i64,
    /// Position in relevance order.
    rank: usize,
}

impl Found {
    fn new(path: PathBuf, is_dir: bool, size: u64, modified: i64) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let folder = path
            .parent()
            .map_or_else(String::new, |p| p.display().to_string());
        Self {
            path,
            is_dir,
            name: name.into(),
            folder: folder.into(),
            size,
            modified,
            rank: 0,
        }
    }

    fn from_hit(h: SearchHit) -> Self {
        let modified = unix(h.modified);
        Self::new(h.path, h.is_dir, h.size, modified)
    }
}

fn unix(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

fn filetime_to_unix(ft: u64) -> i64 {
    (ft / 10_000_000) as i64 - 11_644_473_600
}

/// Where the search looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Disk,
    Folder,
}

/// What the "Disco inteiro" scope can use on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disk {
    /// Still asking the Service Control Manager.
    Unknown,
    Service,
    /// fx is elevated: its own MFT index of the current volume.
    OwnIndex,
    Unavailable,
}

/// Result order; `Relevance` is the order the search returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    Relevance,
    Name,
    Size,
    Modified,
}

/// The open search panel.
pub(super) struct Search {
    query: String,
    root: PathBuf,
    hits: Vec<Found>,
    cursor: usize,
    running: bool,
    summary: Option<String>,
    ticket: u64,
    cancel: Arc<AtomicBool>,
    scroll: UniformListScrollHandle,
    scope: Scope,
    /// The user chose the scope (Tab): do not switch it for them.
    scope_chosen: bool,
    disk: Disk,
    order: Order,
    filters: Filters,
    /// Mouse button down on a result: where, and which (drag out).
    press: Option<(Point<Pixels>, usize)>,
}

/// MFT indexes by volume ("C:"), for the whole process (elevated only).
enum IndexState {
    Loading,
    Ready(Arc<Index>),
    Failed,
}

fn indexes() -> &'static Mutex<HashMap<String, IndexState>> {
    static INDEXES: OnceLock<Mutex<HashMap<String, IndexState>>> = OnceLock::new();
    INDEXES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// "C:" for a path on a lettered drive (also `\\?\C:\...`).
fn volume_of(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                Some(format!("{}:", char::from(letter).to_ascii_uppercase()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// The ready index of `volume`, starting its read (once) when elevated.
fn index_for(volume: &str) -> Option<Arc<Index>> {
    if !fx_shell::is_elevated() {
        return None;
    }
    let mut map = indexes().lock().unwrap_or_else(PoisonError::into_inner);
    match map.get(volume) {
        Some(IndexState::Ready(index)) => return Some(index.clone()),
        Some(IndexState::Loading | IndexState::Failed) => return None,
        None => {}
    }
    map.insert(volume.to_owned(), IndexState::Loading);
    let vol = volume.to_owned();
    let _ = std::thread::Builder::new()
        .name("fx-index-read".into())
        .spawn(move || {
            let state = match fx_index::read_volume(&vol) {
                Ok(index) => IndexState::Ready(Arc::new(index)),
                Err(_) => IndexState::Failed,
            };
            indexes()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(vol, state);
        });
    None
}

/// The backend a search runs on (moved to its worker thread).
enum Backend {
    /// `etiqueta:` — the tagged items, wherever they are (T3.4).
    Tags,
    Service,
    OwnIndex(String, Arc<Index>),
    Folder(PathBuf),
}

impl FolderView {
    pub(super) fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search.is_some() {
            cx.notify();
            return;
        }
        self.search = Some(Search {
            query: String::new(),
            root: self.path.clone(),
            hits: Vec::new(),
            cursor: 0,
            running: false,
            summary: None,
            ticket: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            scroll: UniformListScrollHandle::new(),
            scope: Scope::Folder,
            scope_chosen: false,
            disk: Disk::Unknown,
            order: Order::Relevance,
            filters: Filters::default(),
            press: None,
        });
        // Elevated: start reading the MFT now, it takes a few seconds.
        let elevated_index = volume_of(&self.path).is_some_and(|v| {
            let _ = index_for(&v);
            fx_shell::is_elevated()
        });
        // Asking the Service Control Manager is a cross-process call: not on
        // the UI thread.
        cx.spawn(async move |this, cx| {
            let running = cx
                .background_spawn(async move {
                    fx_index::service::service_state() == fx_index::service::ServiceState::Running
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                let Some(s) = view.search.as_mut() else {
                    return;
                };
                s.disk = match (running, elevated_index) {
                    (true, _) => Disk::Service,
                    (false, true) => Disk::OwnIndex,
                    (false, false) => Disk::Unavailable,
                };
                if s.disk != Disk::Unavailable && !s.scope_chosen {
                    s.scope = Scope::Disk;
                    view.run_search(cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn close_search(&mut self) {
        if let Some(s) = self.search.take() {
            s.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Keys while the panel is open (all of them stay here). Returns whether
    /// the panel is open.
    pub(super) fn search_key(
        &mut self,
        ks: &Keystroke,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        let m = ks.modifiers;
        let mut query = search.query.clone();
        match ks.key.as_str() {
            "escape" => {
                self.close_search();
                return true;
            }
            "enter" => {
                let reveal = m.control;
                if let Some(found) = search.hits.get(search.cursor).cloned() {
                    self.close_search();
                    self.open_found(found, reveal, window, cx);
                }
                return true;
            }
            "tab" => {
                search.scope_chosen = true;
                match (search.scope, search.disk) {
                    (Scope::Folder, Disk::Service | Disk::OwnIndex) => search.scope = Scope::Disk,
                    (Scope::Folder, _) => {
                        search.summary = Some(
                            tr!("Para buscar no disco inteiro, ative o serviço na paleta (Ctrl+K)", "To search the whole disk, turn on the service in the palette (Ctrl+K)")
                                .into(),
                        );
                        cx.notify();
                        return true;
                    }
                    (Scope::Disk, _) => search.scope = Scope::Folder,
                }
                if let Some(trace) = &self.trace {
                    trace.borrow_mut().input_started(started, "search");
                }
                self.run_search(cx);
                return true;
            }
            "down" | "up" => {
                let n = search.hits.len();
                if n > 0 {
                    search.cursor = if ks.key == "down" {
                        (search.cursor + 1).min(n - 1)
                    } else {
                        search.cursor.saturating_sub(1)
                    };
                    search
                        .scroll
                        .scroll_to_item(search.cursor, ScrollStrategy::Top);
                }
                return true;
            }
            "backspace" => {
                query.pop();
            }
            "space" => query.push(' '),
            _ => {
                if m.control || m.alt || m.platform || m.function {
                    return true;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => query.push_str(c),
                    _ => return true,
                }
            }
        }
        if query != search.query {
            search.query = query;
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "search");
            }
            self.run_search(cx);
        }
        true
    }

    /// Cancels the running search and starts one for the current text.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.cancel.store(true, Ordering::Relaxed);
        search.cancel = Arc::new(AtomicBool::new(false));
        search.ticket += 1;
        search.hits.clear();
        search.cursor = 0;
        search.summary = None;
        search.scroll.scroll_to_item(0, ScrollStrategy::Top);
        let now = SystemTime::now();
        let filters = Filters::parse(&search.query, unix(now), fx_shell::utc_offset_at(now));
        search.filters = filters.clone();
        search.running = !filters.text.trim().is_empty() || filters.is_active();
        if !search.running {
            return;
        }
        let backend = match (search.scope, search.disk) {
            _ if filters.tags != 0 => Backend::Tags,
            (Scope::Disk, Disk::Service) => Backend::Service,
            (Scope::Disk, Disk::OwnIndex) => {
                match volume_of(&search.root).and_then(|v| index_for(&v).map(|i| (v, i))) {
                    Some((v, i)) => Backend::OwnIndex(v, i),
                    None => Backend::Folder(search.root.clone()),
                }
            }
            _ => Backend::Folder(search.root.clone()),
        };
        let (ticket, cancel) = (search.ticket, search.cancel.clone());
        let (tx, rx) = channel::<Vec<Found>>();
        let (done_tx, done_rx) = channel::<String>();
        let spawned = std::thread::Builder::new()
            .name("fx-search".into())
            .spawn(move || {
                let summary = match backend {
                    Backend::Service => service_search(&filters, &tx),
                    Backend::Tags => tag_search(&filters, &tx),
                    Backend::OwnIndex(volume, index) => {
                        index_search(&index, &volume, &filters, &cancel, &tx)
                    }
                    Backend::Folder(root) => tree_search(&root, &filters, &cancel, &tx),
                };
                drop(tx);
                let _ = done_tx.send(summary);
            });
        if spawned.is_err() {
            search.running = false;
            search.summary = Some(
                tr!(
                    "não foi possível iniciar a busca",
                    "could not start the search"
                )
                .into(),
            );
            return;
        }
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                let (back, batch) = background
                    .spawn(async move {
                        let b = rx.recv();
                        (rx, b)
                    })
                    .await;
                rx = back;
                let Ok(batch) = batch else { break };
                let current = this.update(cx, |view, cx| {
                    let Some(s) = view.search.as_mut().filter(|s| s.ticket == ticket) else {
                        return false;
                    };
                    let base = s.hits.len();
                    s.hits
                        .extend(batch.into_iter().enumerate().map(|(k, mut f)| {
                            f.rank = base + k;
                            f
                        }));
                    sort_found(&mut s.hits, s.order);
                    cx.notify();
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
            }
            let summary = background.spawn(async move { done_rx.recv() }).await;
            let _ = this.update(cx, |view, cx| {
                if let Some(s) = view.search.as_mut().filter(|s| s.ticket == ticket) {
                    s.running = false;
                    s.summary = summary.ok();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Header click: sort by that column; clicking the active one goes back
    /// to relevance.
    fn sort_results(&mut self, order: Order, cx: &mut Context<Self>) {
        if let Some(s) = self.search.as_mut() {
            s.order = if s.order == order {
                Order::Relevance
            } else {
                order
            };
            sort_found(&mut s.hits, s.order);
            s.cursor = 0;
            s.scroll.scroll_to_item(0, ScrollStrategy::Top);
            cx.notify();
        }
    }

    fn result_pressed(
        &mut self,
        i: usize,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        s.cursor = i;
        s.press = Some((ev.position, i));
        if ev.click_count >= 2
            && let Some(found) = s.hits.get(i).cloned()
        {
            self.close_search();
            self.open_found(found, false, window, cx);
        }
        cx.notify();
    }

    /// Drags a result out once the pointer moved far enough (like the list).
    fn result_dragged(&mut self, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        let Some((start, i)) = s.press else { return };
        if !ev.dragging() {
            s.press = None;
            return;
        }
        let d = ev.position - start;
        if f32::from(d.x).hypot(f32::from(d.y)) < super::dnd::DRAG_THRESHOLD {
            return;
        }
        s.press = None;
        let (Some(found), Some(hwnd)) = (s.hits.get(i), fx_shell::window_id(window)) else {
            return;
        };
        let paths = vec![found.path.clone()];
        cx.spawn(async move |this, cx| {
            let result = fx_shell::drag_files(hwnd, &paths);
            if let Err(e) = result {
                let _ = this.update(cx, |view, cx| {
                    view.notice = Some(trf!(
                        "não foi possível arrastar: {e}",
                        "could not drag: {e}"
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn result_menu(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.search.as_mut() else {
            return;
        };
        s.cursor = i;
        let Some(found) = s.hits.get(i) else { return };
        let path = found.path.clone();
        let folder = path
            .parent()
            .map_or_else(|| path.clone(), Path::to_path_buf);
        self.open_menu_in(vec![path], folder, window, cx);
        cx.notify();
    }

    fn open_found(
        &mut self,
        found: Found,
        reveal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent = found.path.parent().map(Path::to_path_buf);
        match (reveal || !found.is_dir, parent) {
            // Ctrl+Enter, or a file with Enter... Enter on a file opens it.
            (true, Some(parent)) if reveal => {
                let name: Option<OsString> = found.path.file_name().map(|n| n.to_os_string());
                let restore = Restore {
                    selected: name.iter().cloned().collect(),
                    cursor: name,
                    scroll: None,
                };
                self.go_to(parent, Some(restore), window, cx);
            }
            (true, _) => self.launch(Launch::Open(found.path), cx),
            (false, _) => self.go_to(found.path, None, window, cx),
        }
    }

    pub(super) fn render_search(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let s = self.search.as_ref()?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let where_ = s.root.file_name().map_or_else(
            || s.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let scope_label = match s.scope {
            Scope::Disk => tr!("Disco inteiro", "Whole disk").to_owned(),
            Scope::Folder => trf!("Em \"{where_}\"", "In \"{where_}\""),
        };
        let placeholder = match s.scope {
            Scope::Disk => tr!(
                "Buscar no disco inteiro — tipo:imagem, tamanho:>10mb, data:semana",
                "Search the whole disk — type:image, size:>10mb, date:week"
            ),
            Scope::Folder => tr!(
                "Buscar nesta pasta e subpastas — tipo:, ext:, tamanho:, data:",
                "Search this folder and subfolders — type:, ext:, size:, date:"
            ),
        };
        let status = match (&s.summary, s.running) {
            (_, true) => trf!("procurando… {}", "searching… {}", fx_core::format::items(s.hits.len())),
            (Some(text), false) => text.clone(),
            (None, false) => {
                tr!("Enter abre · Ctrl+Enter mostra na pasta · Tab muda onde buscar · Esc fecha", "Enter opens · Ctrl+Enter shows in folder · Tab changes where to search · Esc closes")
                    .to_owned()
            }
        };
        // Nothing found: say so where the results would be, with the next step.
        let empty_message = (!s.running
            && s.hits.is_empty()
            && !s.query.trim().is_empty()
            && s.summary.is_some())
        .then(|| {
            let q = s.query.trim().to_owned();
            let disk_ready = matches!(s.disk, Disk::Service | Disk::OwnIndex);
            match s.scope {
                Scope::Folder if !disk_ready => trf!(
                    "Nada com \"{q}\" nesta pasta. Tente menos letras ou outro filtro.",
                    "Nothing with \"{q}\" in this folder. Try fewer letters or another filter."
                ),
                Scope::Folder => trf!(
                    "Nada com \"{q}\" nesta pasta. Tab busca no disco inteiro.",
                    "Nothing with \"{q}\" in this folder. Tab searches the whole disk."
                ),
                Scope::Disk => trf!(
                    "Nada com \"{q}\" no disco. Tente menos letras ou outro filtro.",
                    "Nothing with \"{q}\" on the disk. Try fewer letters or another filter."
                ),
            }
        });
        let hits: Arc<Vec<Found>> = Arc::new(s.hits.clone());
        let cursor = s.cursor;
        let (accent, secondary, hover) = (t.selected, t.text_secondary, t.hover);
        let view = cx.entity().downgrade();
        let list = uniform_list("search-results", hits.len(), move |range, _, _| {
            range
                .map(|i| {
                    let f = &hits[i];
                    let (down, menu, drag) = (view.clone(), view.clone(), view.clone());
                    div()
                        .id(("search-hit", i))
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(space(3)))
                        .flex()
                        .items_center()
                        .gap(px(space(3)))
                        .rounded(px(RADIUS))
                        .hover(|d| d.bg(rgba(hover)))
                        .when(i == cursor, |d| d.bg(rgba(accent)))
                        .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
                            let _ = down.update(cx, |v, cx| v.result_pressed(i, ev, window, cx));
                        })
                        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                            let _ = menu.update(cx, |v, cx| v.result_menu(i, window, cx));
                        })
                        .on_mouse_move(move |ev, window, cx| {
                            let _ = drag.update(cx, |v, cx| v.result_dragged(ev, window, cx));
                        })
                        .child(
                            div()
                                .flex_none()
                                .font_family(icon_font)
                                .text_color(rgba(secondary))
                                .child(if f.is_dir { ICON_FOLDER } else { ICON_PAGE }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().truncate().child(f.name.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(CAPTION_SIZE))
                                        .text_color(rgba(secondary))
                                        .child(f.folder.clone()),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(SIZE_COL))
                                .flex()
                                .justify_end()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(secondary))
                                .child(if f.is_dir {
                                    String::new()
                                } else {
                                    fx_core::format::size_kb(f.size)
                                }),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(DATE_COL))
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(secondary))
                                .child(date_of(f.modified)),
                        )
                })
                .collect()
        })
        .track_scroll(s.scroll.clone())
        .flex_1()
        .px(px(space(1)));

        let header_cell = |label: &'static str, order: Order| {
            let active = s.order == order;
            div()
                .id(label)
                .flex()
                .items_center()
                .gap(px(space(1)))
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(if active { t.text } else { t.text_secondary }))
                .hover(|d| d.text_color(rgba(t.text)))
                .child(label)
                .when(active, |d| {
                    d.child(div().font_family(icon_font).text_size(px(8.0)).child(
                        if order == Order::Name {
                            "\u{E70E}"
                        } else {
                            "\u{E70D}"
                        },
                    ))
                })
                .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _, cx| {
                    view.sort_results(order, cx);
                }))
        };
        let header = div()
            .flex_none()
            .h(px(28.0))
            .px(px(space(4)))
            .flex()
            .items_center()
            .gap(px(space(3)))
            .border_b_1()
            .border_color(rgba(t.divider))
            // Lines up with the rows: icon column, then name.
            .child(div().flex_none().w(px(16.0)))
            .child(header_cell(tr!("Nome", "Name"), Order::Name).flex_1())
            .child(
                div()
                    .flex_none()
                    .w(px(SIZE_COL))
                    .flex()
                    .justify_end()
                    .child(header_cell(tr!("Tamanho", "Size"), Order::Size)),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(DATE_COL))
                    .child(header_cell(tr!("Modificado", "Modified"), Order::Modified)),
            );

        // Filters understood (chips) and words fx could not read.
        let chip = |text: String, problem: bool| {
            div()
                .flex_none()
                .px(px(space(2)))
                .rounded(px(RADIUS))
                .text_size(px(CAPTION_SIZE))
                .bg(rgba(if problem { t.hover } else { t.selected }))
                .text_color(rgba(if problem { t.text_secondary } else { t.text }))
                .child(text)
        };
        let chips: Vec<_> = s
            .filters
            .labels
            .iter()
            .map(|l| chip(l.clone(), false))
            .chain(
                s.filters
                    .problems
                    .iter()
                    .map(|p| chip(trf!("não entendi: {p}", "not understood: {p}"), true)),
            )
            .collect();

        let card = div()
            .id("search-panel")
            .absolute()
            .top(px(space(12)))
            .left(px(space(4)))
            .right(px(space(4)))
            .mx_auto()
            .max_w(px(PANEL_WIDTH))
            .h(px(480.0))
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .h(px(44.0))
                    .px(px(space(4)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .child(
                        div()
                            .flex_none()
                            .font_family(icon_font)
                            .text_color(rgba(t.text_secondary))
                            .child(ICON_SEARCH),
                    )
                    .child(if s.query.is_empty() {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgba(t.text_secondary))
                            .child(placeholder)
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(format!("{}|", s.query))
                    })
                    .child(
                        div()
                            .id("search-scope")
                            .flex_none()
                            .px(px(space(2)))
                            .py(px(2.0))
                            .rounded(px(RADIUS))
                            .border_1()
                            .border_color(rgba(t.divider))
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .max_w(px(200.0))
                            .truncate()
                            .child(format!("{scope_label} · Tab")),
                    ),
            )
            .when(!chips.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(space(4)))
                        .pb(px(space(2)))
                        .flex()
                        .flex_wrap()
                        .gap(px(space(1)))
                        .children(chips),
                )
            })
            .child(header)
            .child(if empty_message.is_some() {
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .pt(px(space(8)))
                    .px(px(space(4)))
                    .text_color(rgba(t.text_secondary))
                    .children(empty_message)
                    .into_any_element()
            } else {
                list.into_any_element()
            })
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .truncate()
                    .child(status),
            );
        Some(
            deferred(crate::motion::enter(
                "search-in",
                div()
                    .id("search-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(crate::theme::rgba(crate::theme::scrim(&self.look.theme)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.close_search();
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(1),
        )
    }
}

/// "21/11/2026 19:24", or empty when unknown.
fn date_of(unix_secs: i64) -> String {
    if unix_secs <= 0 {
        return String::new();
    }
    local_date(UNIX_EPOCH + std::time::Duration::from_secs(unix_secs as u64))
}

fn sort_found(hits: &mut [Found], order: Order) {
    match order {
        Order::Relevance => hits.sort_by_key(|f| f.rank),
        Order::Name => hits.sort_by(|a, b| {
            fx_core::natural::natural_cmp(&a.name, &b.name).then(a.rank.cmp(&b.rank))
        }),
        // Biggest and newest first, folders after files for size.
        Order::Size => hits.sort_by(|a, b| {
            a.is_dir
                .cmp(&b.is_dir)
                .then(b.size.cmp(&a.size))
                .then(a.rank.cmp(&b.rank))
        }),
        Order::Modified => {
            hits.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.rank.cmp(&b.rank)))
        }
    }
}

/// " · imagens · > 10 MB" for the summary line.
fn filter_suffix(filters: &Filters) -> String {
    filters.labels.iter().map(|l| format!(" · {l}")).collect()
}

/// Every item with the wanted tags, filtered by name and the other filters;
/// returns the summary line.
fn tag_search(filters: &Filters, tx: &std::sync::mpsc::Sender<Vec<Found>>) -> String {
    let needle = fx_core::natural::fold(filters.text.trim());
    let mut found = Vec::new();
    for path in crate::tags::tagged(filters.tags) {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue; // gone (deleted outside fx)
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let modified = meta.modified().map(unix).unwrap_or(0);
        if !needle.is_empty() && !fx_core::natural::fold(&name).contains(&needle) {
            continue;
        }
        if !filters.matches(&name, meta.is_dir(), meta.len(), modified) {
            continue;
        }
        found.push(Found::new(path, meta.is_dir(), meta.len(), modified));
    }
    let n = found.len();
    let _ = tx.send(found);
    trf!(
        "{} com essa etiqueta{}",
        "{} with that tag{}",
        fx_core::format::items(n),
        filter_suffix(filters)
    )
}

/// The index service (every NTFS volume); returns the summary line.
fn service_search(filters: &Filters, tx: &std::sync::mpsc::Sender<Vec<Found>>) -> String {
    let request = Request {
        query: filters.text.trim().to_owned(),
        limit: SERVICE_LIMIT,
        filter: SearchFilter::from_filters(filters),
        sizes_of: Vec::new(),
    };
    match fx_index::pipe::query(&request, 2000) {
        Ok(r) if r.status == Status::Indexing => tr!(
            "o serviço ainda está lendo os discos: tente em alguns segundos",
            "the service is still reading the disks: try again in a few seconds"
        )
        .into(),
        Ok(r) => {
            let n = r.hits.len();
            let found = r
                .hits
                .into_iter()
                .map(|h| Found::new(h.path_buf(), h.is_dir, h.size, filetime_to_unix(h.modified)))
                .collect();
            let _ = tx.send(found);
            let mut text = trf!(
                "{} no disco inteiro ({} indexados){}",
                "{} on the whole disk ({} indexed){}",
                fx_core::format::items(n),
                fx_core::format::count(r.items as usize),
                filter_suffix(filters)
            );
            if n as u32 >= SERVICE_LIMIT {
                text.push_str(tr!(
                    " · os mais relevantes: digite mais para refinar",
                    " · the most relevant: type more to narrow it"
                ));
            }
            text
        }
        Err(e) => trf!(
            "serviço de índice indisponível: {e} (Tab busca nesta pasta)",
            "index service unavailable: {e} (Tab searches this folder)"
        ),
    }
}

/// Folder walk; returns the summary line.
fn tree_search(
    root: &Path,
    filters: &Filters,
    cancel: &AtomicBool,
    tx: &std::sync::mpsc::Sender<Vec<Found>>,
) -> String {
    let (htx, hrx) = channel::<Vec<SearchHit>>();
    let keep = |h: &SearchHit| {
        let name = h
            .path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        filters.matches(&name, h.is_dir, h.size, unix(h.modified))
    };
    let summary = std::thread::scope(|scope| {
        let forward = scope.spawn(|| {
            for batch in hrx {
                let found = batch.into_iter().map(Found::from_hit).collect();
                if tx.send(found).is_err() {
                    break;
                }
            }
        });
        let summary =
            fx_fs::search_tree_where(root, filters.text.trim(), TREE_LIMIT, cancel, &htx, &keep);
        drop(htx);
        let _ = forward.join();
        summary
    });
    tree_summary(&summary) + &filter_suffix(filters)
}

fn tree_summary(s: &SearchSummary) -> String {
    let folders = match s.folders {
        1 => tr!("1 pasta", "1 folder").to_owned(),
        n => trf!("{} pastas", "{} folders", fx_core::format::count(n)),
    };
    let mut text = trf!(
        "{} em {folders}",
        "{} in {folders}",
        fx_core::format::items(s.hits)
    );
    if s.capped {
        text.push_str(tr!(
            " (parou no limite: digite mais para refinar)",
            " (stopped at the limit: type more to narrow it)"
        ));
    }
    if s.skipped > 0 {
        text.push_str(&trf!(
            " · {} sem acesso",
            " · {} without access",
            fx_core::format::count(s.skipped)
        ));
    }
    text
}

/// MFT index search (fx elevated); returns the summary line.
fn index_search(
    index: &Index,
    volume: &str,
    filters: &Filters,
    cancel: &AtomicBool,
    tx: &std::sync::mpsc::Sender<Vec<Found>>,
) -> String {
    let text = filters.text.trim();
    let keep = |slot: u32| {
        let modified = filetime_to_unix(index.modified(slot));
        filters
            .matches(
                index.name(slot),
                index.is_dir(slot),
                index.size(slot),
                modified,
            )
            .then(|| {
                if text.is_empty() {
                    (modified / 60).clamp(0, i64::from(i32::MAX)) as i32
                } else {
                    0
                }
            })
    };
    let Some(hits) = fx_index::search_where(index, text, cancel, None, &keep) else {
        return "cancelada".into();
    };
    let root = PathBuf::from(format!("{volume}\\"));
    let found: Vec<Found> = hits
        .iter()
        .filter_map(|h| {
            let rel = index.path(h.slot)?;
            Some(Found::new(
                root.join(rel),
                index.is_dir(h.slot),
                index.size(h.slot),
                filetime_to_unix(index.modified(h.slot)),
            ))
        })
        .collect();
    let n = found.len();
    let _ = tx.send(found);
    trf!(
        "{} no disco {volume} (índice da MFT, os {} melhores){}",
        "{} on disk {volume} (MFT index, the best {}){}",
        fx_core::format::items(n),
        fx_index::MAX_RESULTS,
        filter_suffix(filters)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes() {
        assert_eq!(volume_of(Path::new(r"c:\Users")).as_deref(), Some("C:"));
        assert_eq!(volume_of(Path::new(r"\\?\D:\x")).as_deref(), Some("D:"));
        assert_eq!(volume_of(Path::new(r"\\server\share\x")), None);
    }

    fn walk(dir: &Path, query: &str) -> (Vec<Found>, String) {
        let (tx, rx) = channel();
        let filters = Filters::parse(query, 0, 0);
        let summary = tree_search(dir, &filters, &AtomicBool::new(false), &tx);
        drop(tx);
        (rx.into_iter().flatten().collect(), summary)
    }

    #[test]
    fn tree_search_reports_results_and_summary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/Relatório.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("nada.txt"), b"x").unwrap();
        let (found, summary) = walk(dir.path(), "relatorio");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.as_ref(), "Relatório.txt");
        assert!(found[0].folder.ends_with('b'));
        assert_eq!(found[0].size, 1);
        assert_eq!(summary, "1 item em 3 pastas");
    }

    #[test]
    fn folder_search_applies_filters_even_without_a_name() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("grande.bin"), vec![0u8; 3000]).unwrap();
        std::fs::write(dir.path().join("pequeno.bin"), b"x").unwrap();
        std::fs::write(dir.path().join("foto.jpg"), b"x").unwrap();
        let (found, summary) = walk(dir.path(), "tamanho:>2kb");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.as_ref(), "grande.bin");
        assert_eq!(summary, "1 item em 1 pasta · > 2,0 KB");
        let (found, _) = walk(dir.path(), "tipo:imagem");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.as_ref(), "foto.jpg");
        let (found, _) = walk(dir.path(), "bin tipo:imagem");
        assert!(found.is_empty());
    }

    #[test]
    fn results_sort_by_column_and_back_to_relevance() {
        let mk = |name: &str, is_dir: bool, size: u64, modified: i64, rank: usize| Found {
            rank,
            ..Found::new(
                PathBuf::from(format!(r"C:\x\{name}")),
                is_dir,
                size,
                modified,
            )
        };
        let mut hits = vec![
            mk("b10.txt", false, 5, 300, 0),
            mk("pasta", true, 0, 100, 1),
            mk("b2.txt", false, 50, 200, 2),
        ];
        let names = |h: &[Found]| h.iter().map(|f| f.name.to_string()).collect::<Vec<_>>();
        sort_found(&mut hits, Order::Name);
        assert_eq!(
            names(&hits),
            ["b2.txt", "b10.txt", "pasta"],
            "natural order"
        );
        sort_found(&mut hits, Order::Size);
        assert_eq!(names(&hits), ["b2.txt", "b10.txt", "pasta"]);
        sort_found(&mut hits, Order::Modified);
        assert_eq!(names(&hits), ["b10.txt", "b2.txt", "pasta"]);
        sort_found(&mut hits, Order::Relevance);
        assert_eq!(names(&hits), ["b10.txt", "pasta", "b2.txt"]);
    }
}
