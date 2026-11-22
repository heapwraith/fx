//! Folder window (task T0.5): a virtualized list with Name, Size and Date columns,
//! styled with the Fluent 2 tokens of `theme.rs` (task T1.1).
//!
//! The folder is read on a worker thread (fx-fs); batches are appended on the UI
//! thread (cheap: ~0.2 ms per 2,000 entries) and the name sort runs on the
//! background executor, so the UI thread never blocks.
//!
//! Selection and keyboard (task T1.2): arrows, Home/End, PgUp/PgDn (Shift
//! extends, Ctrl moves only the cursor), Ctrl+A, Ctrl+Space, Ctrl/Shift+click;
//! Enter or double click enters a folder; Alt+Up and Backspace go up.
//!
//! Opening (task T1.9): Enter / double click open files with their default
//! app off the UI thread, Shift+Enter "Open with…", Ctrl+` a terminal here.
//!
//! History (task T1.3): Alt+Left/Right and mouse buttons 4/5 go back and
//! forward, restoring each folder's scroll position and selection. The path
//! bar is in `path_bar.rs`.

mod actions;
mod batch_rename;
mod columns;
mod dir_sizes;
mod dnd;
mod edit_ops;
mod grid;
mod keys;
mod navigation;
mod path_bar;
mod quick_look;
mod rows;
mod search;
mod terminal;
mod view_menu;

pub use view_menu::ViewEvent;

use futures::StreamExt;
use fx_core::{tr, trf};
use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant, SystemTime};

use fx_core::format::{date_time, items, size_kb};
use fx_core::{
    Change, Direction, Entry, FileList, History, MoveMode, Selection, SortKey, Step, next_sort,
    type_label,
};
use fx_fs::Strategy;
use fx_shell::icons::{IconKey, icon_key_cloud};
use fx_shell::launch::Launch;
use gpui::{
    Context, Entity, ExternalPaths, FocusHandle, Image, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, NavigationDirection, Pixels, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, canvas, div, img, point, prelude::*, px, uniform_list,
};

use crate::a11y::{A11yHandle, Row, Snapshot};
use crate::icons::{Icons, Media};
use crate::ops::Ops;
use crate::theme::{BODY_SIZE, CAPTION_SIZE, Density, Mode, RADIUS, Theme, rgba, space};
use crate::thumbs::Thumbs;
use crate::trace::Trace;

const SIZE_COL: f32 = 112.0;
const DATE_COL: f32 = 144.0;
const TYPE_COL: f32 = 136.0;
/// Limits when a column is resized by dragging its header border (T1.20).
const COL_MIN: f32 = 56.0;
const COL_MAX: f32 = 480.0;
/// Width of the grab area on a header border.
const RESIZE_HANDLE: f32 = 8.0;
const ICON_SORT_UP: &str = "\u{E70E}";
/// Hidden and system items, when shown, are drawn faded (like Explorer).
const HIDDEN_OPACITY: f32 = 0.55;
const ICON_SORT_DOWN: &str = "\u{E70D}";
const SCROLLBAR_WIDTH: f32 = 12.0;
const THUMB_MIN: f32 = 24.0;
/// Width of the accent bar on the selected row (Fluent list selection).
const INDICATOR_WIDTH: f32 = 3.0;
/// File icons are drawn at 16 px (small shell icons).
const ICON_SIZE: f32 = 16.0;
/// Enter opens at most this many selected files at once.
const MAX_OPEN: usize = 32;

/// How the window is drawn: theme, density, font and whether Mica shows
/// through the background.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub theme: Theme,
    /// Set when `FX_THEME` forces a theme: system changes are then ignored.
    pub forced_mode: bool,
    pub density: Density,
    pub font: &'static str,
    pub icon_font: &'static str,
    /// New tabs start in the grid view (`FX_VIEW=grid`).
    pub grid: bool,
    /// The sidebar starts open (saved setting).
    pub sidebar: bool,
    /// Hidden and system files are shown (Ctrl+H, saved setting, T1.21).
    pub show_hidden: bool,
    /// Mica is available (Windows 11 22H2+ and not disabled by `FX_MICA=0`).
    pub mica_supported: bool,
    /// Mica shows through right now (see `FolderView::set_system_mode`).
    pub mica: bool,
}

/// What a folder looked like when we left it, to restore it on the way back.
#[derive(Debug, Clone, Default)]
struct Restore {
    /// Selected names (at most `MAX_RESTORED` of them).
    selected: Vec<OsString>,
    cursor: Option<OsString>,
    /// Scroll offset (`None`: center the cursor).
    scroll: Option<f32>,
}

impl Restore {
    /// Select and center one entry (going up, or clicking a breadcrumb).
    fn cursor_on(name: OsString) -> Self {
        Restore {
            selected: vec![name.clone()],
            cursor: Some(name),
            scroll: None,
        }
    }
}

/// Larger selections are not remembered by the history (only the cursor), so
/// leaving a folder never copies more than this many names on the UI thread.
const MAX_RESTORED: usize = 10_000;

#[derive(Debug, Clone)]
struct Visit {
    path: PathBuf,
    restore: Restore,
}

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Loading,
    Sorting,
    Ready,
    Failed(String),
}

pub struct FolderView {
    path: PathBuf,
    list: Arc<FileList>,
    status: Status,
    scroll: UniformListScrollHandle,
    /// Offset of the pointer inside the thumb while dragging it.
    drag_offset: Option<Pixels>,
    focus: FocusHandle,
    /// "Type to filter" text (task T0.6).
    filter_text: String,
    /// Rows shown while filtering (indices in display order); `None` = all.
    filtered: Option<Vec<u32>>,
    selection: Selection,
    /// Bumped on every navigation; batches of an older folder read are dropped.
    generation: u64,
    /// Selection and scroll to apply once the folder is loaded.
    restore: Option<Restore>,
    history: History<Visit>,
    /// The path bar while it is a text field.
    edit: Option<path_bar::PathEdit>,
    completion: Option<path_bar::Completion>,
    look: Look,
    trace: Option<Rc<RefCell<Trace>>>,
    /// Set until the first frame with rows of a tab opened with Ctrl+T.
    opened: Option<Instant>,
    icons: Entity<Icons>,
    thumbs: Entity<Thumbs>,
    /// Grid view (task T1.7) instead of the details list.
    grid: bool,
    /// Tiles per grid row, from the last layout.
    grid_cols: usize,
    /// Watches the folder shown (task T1.8); replaced on navigation.
    watcher: Option<fx_fs::Watcher>,
    /// Changes that arrived before the list was sorted.
    pending_changes: Vec<Change>,
    /// A message for the status bar (e.g. a file that could not be opened),
    /// cleared by the next key.
    notice: Option<String>,
    /// UI Automation of the window (task T1.10).
    a11y: Option<A11yHandle>,
    ops: Entity<Ops>,
    /// Inline rename in progress (F2, task T1.12).
    renaming: Option<edit_ops::Rename>,
    /// A "new folder" job whose folder should be renamed when it appears.
    awaiting_folder: Option<fx_ops::JobId>,
    rename_when_listed: Option<OsString>,
    /// Where and which row was pressed (a drag out may start, task T1.15).
    press: Option<(gpui::Point<Pixels>, u32)>,
    /// Columns shown for the current list width.
    columns: Columns,
    last_list_width: f32,
    /// When the current read started, and how long it took to have the
    /// whole list sorted (shown in the status bar: speed made visible).
    load_started: Option<Instant>,
    loaded_in: Option<Duration>,
    /// Sort column and direction of this tab (kept across folders, T1.20).
    sort: (SortKey, Direction),
    /// Bumped by each re-sort: a late result of an older one is dropped.
    sort_ticket: u64,
    widths: Widths,
    resizing: Option<Resize>,
    /// The "Exibir" menu is open (T1.21).
    view_menu: bool,
    /// Totals of the subfolders being computed (T2.7).
    dir_sizes: dir_sizes::DirSizes,
    /// The batch rename panel (F2 with several items, T2.8).
    batch_rename: Option<batch_rename::BatchRename>,
    /// The column view (Ctrl+Shift+3, T3.3).
    miller: Option<columns::Columns>,
    /// The embedded terminal (Ctrl+J, T3.5).
    terminal: Option<terminal::Terminal>,
    /// Width of the breadcrumb area in the last frame (T2.3), and whether
    /// the menu of the collapsed leading crumbs ("…") is open.
    crumb_room: std::rc::Rc<std::cell::Cell<f32>>,
    crumb_menu: bool,
    /// Quick Look (T1.22): the open preview, the ticket of the newest
    /// request, and images to free on the next frame.
    quick_look: Option<quick_look::QuickLook>,
    quick_look_ticket: u64,
    stale_images: Vec<Arc<gpui::RenderImage>>,
    /// Disk search panel (Ctrl+F, T1.23).
    search: Option<search::Search>,
}

/// Which optional columns fit (T1.19): in a narrow list the type goes
/// first, then the date, then the size, so names stay readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Columns {
    kind: bool,
    size: bool,
    date: bool,
}

impl Columns {
    /// Name column keeps at least this much room.
    const NAME_MIN: f32 = 200.0;
    const ALL: Columns = Columns {
        kind: true,
        size: true,
        date: true,
    };

    fn for_width(width: f32, w: Widths) -> Self {
        if width <= 0.0 {
            return Self::ALL; // before the first layout
        }
        let room = width - SCROLLBAR_WIDTH - 2.0 * space(4) - ICON_SIZE - Self::NAME_MIN;
        let gap = space(3);
        Self {
            size: room >= w.size + gap,
            date: room >= w.size + w.date + 2.0 * gap,
            kind: room >= w.size + w.date + w.kind + 3.0 * gap,
        }
    }
}

/// Widths of the optional columns (the name takes the rest), per tab.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Widths {
    kind: f32,
    size: f32,
    date: f32,
}

impl Default for Widths {
    fn default() -> Self {
        Self {
            kind: TYPE_COL,
            size: SIZE_COL,
            date: DATE_COL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Col {
    Kind,
    Size,
    Date,
}

impl Widths {
    fn get(&self, c: Col) -> f32 {
        match c {
            Col::Kind => self.kind,
            Col::Size => self.size,
            Col::Date => self.date,
        }
    }

    fn set(&mut self, c: Col, w: f32) {
        let w = w.clamp(COL_MIN, COL_MAX);
        match c {
            Col::Kind => self.kind = w,
            Col::Size => self.size = w,
            Col::Date => self.date = w,
        }
    }
}

/// A header border being dragged: the column, where the drag started and
/// the width then.
#[derive(Debug, Clone, Copy)]
struct Resize {
    col: Col,
    start_x: f32,
    start_w: f32,
}

impl FolderView {
    /// `opened`: when the user asked for this tab (Ctrl+T), to time its first
    /// frame with rows (`FX_TRACE`).
    pub fn new(
        path: PathBuf,
        look: Look,
        trace: Option<Rc<RefCell<Trace>>>,
        opened: Option<Instant>,
        media: Media,
        a11y: Option<A11yHandle>,
        cx: &mut Context<Self>,
    ) -> Self {
        let Media { icons, thumbs, ops } = media;
        // Status messages, and the folder a "new folder" job created.
        cx.observe(&ops, |view, _, cx| {
            view.check_new_folder(cx);
            cx.notify();
        })
        .detach();
        // Redraw when icons or thumbnails arrive.
        cx.observe(&icons, |_, _, cx| cx.notify()).detach();
        cx.observe(&thumbs, |view, _, cx| {
            if view.grid {
                cx.notify();
            }
        })
        .detach();
        if let Some(t) = &trace {
            t.borrow_mut().mark("folder_requested");
        }
        let history = History::new(
            Visit {
                path: path.clone(),
                restore: Restore::default(),
            },
            History::<Visit>::DEFAULT_LIMIT,
        );
        let mut view = Self {
            path,
            list: Arc::new(FileList::new()),
            status: Status::Loading,
            scroll: UniformListScrollHandle::new(),
            drag_offset: None,
            focus: cx.focus_handle(),
            filter_text: String::new(),
            filtered: None,
            selection: Selection::new(),
            generation: 0,
            restore: None,
            history,
            edit: None,
            completion: None,
            look,
            trace,
            opened,
            icons,
            thumbs,
            grid: look.grid,
            grid_cols: 1,
            watcher: None,
            pending_changes: Vec::new(),
            notice: None,
            a11y,
            ops,
            renaming: None,
            awaiting_folder: None,
            rename_when_listed: None,
            press: None,
            columns: Columns::ALL,
            last_list_width: 0.0,
            load_started: None,
            loaded_in: None,
            sort: (SortKey::Name, Direction::Ascending),
            sort_ticket: 0,
            widths: Widths::default(),
            resizing: None,
            view_menu: false,
            dir_sizes: dir_sizes::DirSizes::default(),
            batch_rename: None,
            miller: None,
            terminal: None,
            crumb_room: std::rc::Rc::default(),
            crumb_menu: false,
            quick_look: None,
            quick_look_ticket: 0,
            stale_images: Vec::new(),
            search: None,
        };
        view.start_loading(cx);
        view
    }

    /// Starts reading `self.path` on a worker thread.
    fn start_loading(&mut self, cx: &mut Context<Self>) {
        // Watch first, then read: nothing that changes during the read is lost
        // (duplicates are harmless, see `FileList::apply`).
        // Both run on the worker thread (opening the folder is disk I/O), the
        // watcher armed before the read starts.
        self.pending_changes.clear();
        self.watcher = None;
        self.load_started = Some(Instant::now());
        self.loaded_in = None;
        let (changes_tx, changes_rx) = channel::<Vec<Change>>();
        Self::receive_changes(changes_rx, self.generation, cx);
        let (watcher_tx, watcher_rx) = channel::<fx_fs::Watcher>();
        Self::receive_watcher(watcher_rx, self.generation, cx);
        let (tx, rx) = channel::<Vec<Entry>>();
        let worker_path = self.path.clone();
        let worker = std::thread::Builder::new()
            .name("fx-enumerate".into())
            .spawn(move || {
                if let Ok(w) = fx_fs::watch(&worker_path, changes_tx) {
                    let _ = watcher_tx.send(w);
                }
                fx_fs::enumerate(Strategy::FindFirstFileEx, &worker_path, &tx)
            });
        self.status = match worker {
            Ok(worker) => {
                Self::receive(rx, worker, self.generation, cx);
                Status::Loading
            }
            Err(e) => Status::Failed(read_failure(&e)),
        };
    }

    /// Keeps the watcher the worker made (dropped if the view moved on).
    fn receive_watcher(rx: Receiver<fx_fs::Watcher>, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let watcher = crate::chan::next(rx).await;
            if let Some(w) = watcher {
                let _ = this.update(cx, |view, _| {
                    if view.generation == generation {
                        view.watcher = Some(w);
                    }
                });
            }
        })
        .detach();
    }

    /// Applies watcher batches as they arrive (buffered until the list is sorted).
    fn receive_changes(rx: Receiver<Vec<Change>>, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut rx = crate::chan::stream(rx);
            while let Some(batch) = rx.next().await {
                let current = this.update(cx, |view, cx| {
                    if view.generation != generation {
                        return false;
                    }
                    if matches!(view.status, Status::Ready | Status::Failed(_)) {
                        view.apply_changes(batch, cx);
                    } else {
                        view.pending_changes.extend(batch);
                    }
                    true
                });
                if !matches!(current, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Applies folder changes in place, keeping the row under the cursor (or
    /// the first visible row) where it is on screen. A lost-events overflow
    /// reads the folder again.
    fn apply_changes(&mut self, changes: Vec<Change>, cx: &mut Context<Self>) {
        if changes.is_empty() {
            return;
        }
        let item_h = self.item_height();
        let per_item = if self.grid { self.grid_cols } else { 1 };
        let offset = f32::from(self.scroll.0.borrow().base_handle.offset().y);
        let first_visible = ((-offset / item_h).max(0.0) as usize) * per_item;
        let rows = rows_of(&self.filtered, &self.list);
        let anchor = self
            .selection
            .cursor()
            .and_then(|c| self.selection.position(rows, c).map(|p| (c, p)))
            .or_else(|| rows.get(first_visible).map(|&i| (i, first_visible)));

        let done = Arc::make_mut(&mut self.list).apply(changes);
        if done.reload {
            let restore = self.snapshot();
            self.reload(restore, cx);
            return;
        }
        for &i in &done.removed {
            self.selection.forget(i);
        }
        for &(old, new) in &done.renamed {
            self.selection.moved(old, new);
        }
        self.refilter();
        let rows = rows_of(&self.filtered, &self.list);
        if let Some((entry, before)) = anchor
            && let Some(after) = rows.iter().position(|&i| i == entry)
            && after / per_item != before / per_item
        {
            let shift = (after / per_item) as f32 - (before / per_item) as f32;
            let y = (offset - shift * item_h).min(0.0);
            self.scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.0), px(y)));
        }
        self.try_rename_listed();
        cx.notify();
    }

    /// Reads the current folder again (watcher overflow), keeping selection
    /// and scroll position.
    fn reload(&mut self, restore: Restore, cx: &mut Context<Self>) {
        self.generation += 1;
        self.stop_dir_sizes();
        self.list = Arc::new(FileList::new());
        self.filtered = None;
        self.selection.clear();
        self.restore = Some(restore);
        self.start_loading(cx);
        cx.notify();
    }

    /// Appends batches as they arrive, then sorts by name off the UI thread.
    /// Stops as soon as the view navigated elsewhere (`generation` changed):
    /// dropping the receiver makes the worker stop too.
    fn receive(
        rx: Receiver<Vec<Entry>>,
        worker: std::thread::JoinHandle<std::io::Result<usize>>,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = crate::chan::stream(rx);
            while let Some(batch) = rx.next().await {
                let current = this.update(cx, |view, cx| {
                    if view.generation != generation {
                        return false;
                    }
                    if let Some(trace) = &view.trace {
                        trace.borrow_mut().mark("first_batch");
                    }
                    Arc::make_mut(&mut view.list).extend(batch);
                    view.refilter();
                    cx.notify();
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
            }

            let result = background.spawn(async move { worker.join() }).await;
            let failure = match result {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(read_failure(&e)),
                Err(_) => Some(
                    tr!(
                        "Erro interno ao ler esta pasta.",
                        "Internal error while reading this folder."
                    )
                    .to_owned(),
                ),
            };
            let Ok(Some(list)) = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return None;
                }
                view.status = Status::Sorting;
                cx.notify();
                Some((view.list.clone(), view.sort))
            }) else {
                return;
            };
            let (list, (key, direction)) = list;
            // The clone is dropped before we return to the UI thread, so
            // `Arc::make_mut` below does not copy the list.
            let order = background
                .spawn(async move {
                    let order = list.sorted_order(key, direction);
                    drop(list);
                    order
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                if !Arc::make_mut(&mut view.list).set_sorted_order(order, key, direction)
                    || view.sort != (key, direction)
                {
                    // The sort changed while this one ran: sort again (the
                    // list keeps its arrival order meanwhile).
                    view.status = Status::Ready;
                    view.resort(cx);
                }
                view.refilter();
                view.apply_restore();
                view.forget_unseen_hidden();
                view.status = match failure {
                    Some(e) => Status::Failed(e),
                    None => Status::Ready,
                };
                view.loaded_in = view.load_started.take().map(|s| s.elapsed());
                if let Some(trace) = &view.trace {
                    trace.borrow_mut().record_loaded();
                }
                let pending = std::mem::take(&mut view.pending_changes);
                view.apply_changes(pending, cx);
                view.start_dir_sizes(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The system theme changed (the theme is kept while `FX_THEME` forces one).
    /// Returns whether Mica should show: the backdrop is tinted by the *system*
    /// theme, so it only matches when the fx theme is the same.
    pub fn set_system_mode(&mut self, system: Mode) -> bool {
        if !self.look.forced_mode {
            self.look.theme = Theme::current(system);
        }
        self.look.mica = self.look.mica_supported
            && self.look.theme.mode == system
            && !self.look.theme.high_contrast;
        self.look.mica
    }

    /// The inline rename field: the selected stem highlighted, caret at the end.
    fn render_rename_box(&self, r: &edit_ops::Rename, t: Theme) -> impl IntoElement + use<> {
        let selected_len = r.selected.min(r.text.len());
        let (selected, unselected) = r.text.split_at(selected_len);
        let caret = r.caret.clamp(selected_len, r.text.len()) - selected_len;
        let (before, after) = unselected.split_at(caret);
        div()
            .flex_1()
            .min_w_0()
            .h(px(self.look.density.row_height() - 8.0))
            .flex()
            .items_center()
            .px(px(space(1)))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(rgba(t.accent))
            .bg(rgba(t.base))
            .text_color(rgba(t.text))
            .overflow_hidden()
            .when(!selected.is_empty(), |d| {
                d.child(div().bg(rgba(t.selected)).child(selected.to_owned()))
            })
            .child(before.to_owned())
            .child(div().flex_none().w(px(1.0)).h(px(16.0)).bg(rgba(t.text)))
            .child(after.to_owned())
    }

    /// Sends the accessibility tree for the rows on screen (`first` is the
    /// display position of `shown[0]`). Free when no screen reader listens
    /// except for building a few dozen nodes.
    fn publish_a11y(&self, first: usize, shown: &[u32]) {
        let Some(handle) = &self.a11y else { return };
        let rows = shown
            .iter()
            .enumerate()
            .map(|(k, &i)| Row {
                entry: i,
                position: first + k,
                name: self.list.name(i).to_owned(),
                description: if self.list.attributes(i).is_dir() {
                    tr!("pasta", "folder").to_owned()
                } else {
                    trf!(
                        "{}, modificado em {}",
                        "{}, modified {}",
                        size_kb(self.list.size(i)),
                        local_date(self.list.modified(i))
                    )
                },
                selected: self.selection.is_selected(i),
            })
            .collect();
        handle.publish(Snapshot {
            title: window_title(&self.path),
            path: self.path.display().to_string(),
            editing: self.edit_text(),
            total: self.rows().len(),
            rows,
            cursor: self.selection.cursor(),
        });
    }

    /// A screen reader asked to focus or activate the row of `entry`.
    pub fn a11y_select(&mut self, entry: u32) {
        let rows = rows_of(&self.filtered, &self.list);
        if let Some(pos) = self.selection.position(rows, entry) {
            self.selection.select_only(entry);
            let item = if self.grid { pos / self.grid_cols } else { pos };
            self.scroll.scroll_to_item(item, ScrollStrategy::Center);
        }
    }

    /// Runs a shortcut as if it was typed (command palette, T1.24): the same
    /// code path as the keyboard.
    pub fn press(&mut self, keys: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(keystroke) = gpui::Keystroke::parse(keys) {
            let event = KeyDownEvent {
                keystroke,
                is_held: false,
            };
            self.on_key_down(&event, window, cx);
        }
    }

    pub fn show_notice(&mut self, text: String, cx: &mut Context<Self>) {
        self.notice = Some(text);
        cx.notify();
    }

    /// Grid ⇄ list (the button next to the path bar; command palette).
    pub fn toggle_grid(&mut self, cx: &mut Context<Self>) {
        self.set_grid(!self.grid);
        cx.notify();
    }

    /// Sorts by `key` from the palette: a new column starts at its first
    /// direction; the same column flips.
    pub fn sort_by_command(&mut self, key: SortKey, cx: &mut Context<Self>) {
        self.sort_by(key, cx);
    }

    /// A column header was clicked: sort by it (again: other direction).
    fn sort_by(&mut self, key: SortKey, cx: &mut Context<Self>) {
        self.sort = next_sort(self.sort, key);
        self.resort(cx);
        cx.notify(); // the header arrow changes now; rows when the sort lands
    }

    /// Sorts the list again by `self.sort`, off the UI thread (T1.20). While
    /// the folder is still being read, the end of the read sorts it instead.
    fn resort(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.status, Status::Ready | Status::Failed(_)) {
            return;
        }
        self.sort_ticket += 1;
        let (ticket, generation) = (self.sort_ticket, self.generation);
        let (key, direction) = self.sort;
        let list = self.list.clone();
        cx.spawn(async move |this, cx| {
            let order = cx
                .background_spawn(async move {
                    let order = list.sorted_order(key, direction);
                    drop(list);
                    order
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation || view.sort_ticket != ticket {
                    return;
                }
                if Arc::make_mut(&mut view.list).set_sorted_order(order, key, direction) {
                    view.refilter();
                    view.reveal_cursor();
                    cx.notify();
                } else {
                    // Entries came or went meanwhile: sort the current list.
                    view.resort(cx);
                }
            });
        })
        .detach();
    }

    /// After a re-sort: the cursor row in view (or the top, without one).
    fn reveal_cursor(&mut self) {
        let rows = rows_of(&self.filtered, &self.list);
        let pos = self
            .selection
            .cursor()
            .and_then(|c| self.selection.position(rows, c));
        match pos {
            Some(pos) => {
                let item = if self.grid { pos / self.grid_cols } else { pos };
                self.scroll.scroll_to_item(item, ScrollStrategy::Center);
            }
            None => self.scroll.scroll_to_item(0, ScrollStrategy::Top),
        }
    }

    /// This tab's state, for the saved session (task T1.17).
    pub fn export(&self) -> crate::store::TabState {
        let r = self.snapshot();
        let (back, _, forward) = self.history.parts();
        let paths = |v: &[Visit]| v.iter().map(|v| v.path.clone()).collect();
        crate::store::TabState {
            path: self.path.clone(),
            scroll: r.scroll.unwrap_or(0.0),
            grid: self.grid,
            selected: r
                .selected
                .iter()
                .map(|n| n.to_string_lossy().into_owned())
                .collect(),
            cursor: r.cursor.map(|c| c.to_string_lossy().into_owned()),
            back: paths(back),
            forward: paths(forward),
            sort: Some((
                self.sort.0.id().to_owned(),
                self.sort.1 == Direction::Descending,
            )),
            widths: Some([self.widths.size, self.widths.kind, self.widths.date]),
        }
    }

    /// Restores a saved tab (right after `new` on the same folder): view,
    /// history, and the selection and scroll once the folder is read.
    pub fn apply_saved(&mut self, t: crate::store::TabState) {
        self.grid = t.grid;
        if let Some(key) = t.sort.as_ref().and_then(|(k, _)| SortKey::from_id(k)) {
            let desc = t.sort.as_ref().is_some_and(|(_, d)| *d);
            let dir = if desc {
                Direction::Descending
            } else {
                Direction::Ascending
            };
            self.sort = (key, dir);
        }
        if let Some([size, kind, date]) = t.widths {
            let mut w = Widths::default();
            w.set(Col::Size, size);
            w.set(Col::Kind, kind);
            w.set(Col::Date, date);
            self.widths = w;
        }
        self.restore = Some(Restore {
            selected: t.selected.into_iter().map(OsString::from).collect(),
            cursor: t.cursor.map(OsString::from),
            scroll: Some(t.scroll),
        });
        let visit = |path: PathBuf| Visit {
            path,
            restore: Restore::default(),
        };
        self.history = History::from_parts(
            t.back.into_iter().map(visit).collect(),
            visit(self.path.clone()),
            t.forward.into_iter().map(visit).collect(),
            History::<Visit>::DEFAULT_LIMIT,
        );
    }

    /// Opens `path` in this tab (sidebar), as a new history entry.
    pub fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if path != self.path {
            self.go_to(path, None, window, cx);
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Row density (Ctrl+1/2/3, applied to every tab by the window).
    pub fn set_density(&mut self, density: Density) {
        self.look.density = density;
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Rows currently shown, in display order.
    fn rows(&self) -> &[u32] {
        rows_of(&self.filtered, &self.list)
    }

    /// Mouse down on entry `i`: click selects it, Ctrl+click toggles it,
    /// Shift+click selects the range from the anchor, double click opens it.
    fn click(&mut self, i: u32, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.edit = None;
        if self.renaming.as_ref().is_some_and(|r| r.entry != i) {
            self.end_rename(true, window, cx);
        }
        let m = ev.modifiers;
        if m.shift {
            self.selection
                .extend_to(rows_of(&self.filtered, &self.list), i);
        } else if m.control {
            self.selection.toggle(i);
        } else {
            // Pressing an already selected row keeps the selection, so the
            // whole selection can be dragged.
            if !self.selection.is_selected(i) || ev.click_count == 2 {
                self.selection.select_only(i);
            }
            if ev.click_count == 2 {
                self.open_cursor(window, cx);
            } else {
                self.press_row(ev.position, i);
            }
        }
    }

    /// Moves the cursor and scrolls just enough to show it.
    fn move_cursor(&mut self, step: Step, mode: MoveMode) {
        let rows = rows_of(&self.filtered, &self.list);
        let before = self
            .selection
            .cursor()
            .and_then(|c| self.selection.position(rows, c));
        let Some(pos) = self.selection.move_cursor(rows, step, mode) else {
            return;
        };
        // Scrolling up shows the row at the top edge, scrolling down at the
        // bottom edge (the list only scrolls when the row is outside the view).
        let up = step == Step::First || before.is_some_and(|b| pos < b);
        let strategy = if up {
            ScrollStrategy::Top
        } else {
            ScrollStrategy::Bottom
        };
        // In the grid each list item is a row of tiles.
        let item = if self.grid { pos / self.grid_cols } else { pos };
        self.scroll.scroll_to_item(item, strategy);
    }

    /// Entries a PgUp/PgDn moves: the rows that fit, minus one (whole grid
    /// rows in the grid).
    fn page_rows(&self) -> usize {
        let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
        let rows = (height / self.item_height()).floor() as usize;
        let rows = rows.saturating_sub(1).max(1);
        if self.grid {
            rows * self.grid_cols
        } else {
            rows
        }
    }

    /// Height of one list item: a row, or a row of tiles.
    fn item_height(&self) -> f32 {
        if self.grid {
            grid::TILE_HEIGHT
        } else {
            self.look.density.row_height()
        }
    }

    /// Ctrl+Shift+2 / Ctrl+Shift+6 and the bar button.
    fn set_grid(&mut self, grid: bool) {
        if self.grid != grid {
            self.grid = grid;
            self.scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.0), px(0.0)));
            if let Some(c) = self.selection.cursor()
                && let Some(pos) = self
                    .selection
                    .position(rows_of(&self.filtered, &self.list), c)
            {
                let item = if grid { pos / self.grid_cols } else { pos };
                self.scroll.scroll_to_item(item, ScrollStrategy::Center);
            }
        }
    }

    /// Recomputes the filter from scratch (after the list itself changed).
    /// Recomputes the shown rows: the name filter, minus hidden and system
    /// entries unless they are shown (T1.21).
    fn refilter(&mut self) {
        let text = (!self.filter_text.is_empty()).then(|| self.list.filter(&self.filter_text));
        self.filtered = self.list.visible(text, self.look.show_hidden);
    }

    /// Ctrl+H or the View menu (for the whole window, through `TabsView`).
    /// Hiding drops hidden entries from the selection and moves the cursor
    /// off them: an operation never acts on an item that cannot be seen.
    pub fn set_show_hidden(&mut self, show: bool) {
        self.look.show_hidden = show;
        self.refilter();
        self.forget_unseen_hidden();
    }

    /// While hidden entries are not shown, none of them stays selected or
    /// under the cursor (after Ctrl+H, or a selection restored by name).
    fn forget_unseen_hidden(&mut self) {
        if self.look.show_hidden {
            return;
        }
        let had_cursor = self.selection.cursor().is_some();
        {
            let hidden: Vec<u32> = self
                .list
                .order()
                .iter()
                .copied()
                .filter(|&i| self.list.is_hidden(i))
                .collect();
            for i in hidden {
                self.selection.forget(i);
            }
        }
        // The cursor was on a hidden entry: put it on a shown row.
        if had_cursor && self.selection.cursor().is_none() {
            self.keep_cursor_visible();
        }
    }

    /// The cursor on a shown row (the first one when its entry is not shown).
    fn keep_cursor_visible(&mut self) {
        let rows = rows_of(&self.filtered, &self.list);
        let visible = self
            .selection
            .cursor()
            .is_some_and(|c| self.selection.position(rows, c).is_some());
        if !visible {
            self.selection
                .move_cursor(rows, Step::First, MoveMode::CursorOnly);
        }
    }

    /// Applies a new filter text. Extending the text refines the previous result
    /// instead of scanning all entries again.
    fn set_filter(&mut self, text: String) {
        let refine = !self.filter_text.is_empty() && text.starts_with(self.filter_text.as_str());
        let by_name = match (&self.filtered, text.is_empty()) {
            (_, true) => None,
            // The previous result already left hidden entries out.
            (Some(previous), false) if refine => Some(self.list.filter_within(&text, previous)),
            _ => Some(self.list.filter(&text)),
        };
        self.filtered = self.list.visible(by_name, self.look.show_hidden);
        self.filter_text = text;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        // Keep the cursor on a visible row so Enter opens the first match. The
        // selection itself is kept (fx-core `Selection`).
        self.keep_cursor_visible();
    }

    fn status_text(&self, cx: &gpui::App) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        if let Some(notice) = self.ops.read(cx).notice() {
            return notice.to_owned();
        }
        // The selection goes right after the count: when the status is cut
        // with an ellipsis, the hidden count and the load time go first.
        let (head, tail) = self.load_text();
        let selected = match self.selection.len() {
            0 => String::new(),
            1 => tr!(" · 1 selecionado", " · 1 selected").to_owned(),
            k => {
                let k = fx_core::format::count(k);
                trf!(" · {k} selecionados", " · {k} selected")
            }
        };
        format!("{head}{selected}{tail}")
    }

    /// The status without the selection: (count, rest).
    fn load_text(&self) -> (String, String) {
        let n = self.list.len();
        if let Some(filtered) = self
            .filtered
            .as_ref()
            .filter(|_| !self.filter_text.is_empty())
        {
            let head = trf!(
                "filtro \"{}\": {} de {}",
                "filter \"{}\": {} of {}",
                self.filter_text,
                fx_core::format::count(filtered.len()),
                fx_core::format::count(n)
            );
            return (head, String::new());
        }
        // Without a name filter, `filtered` only leaves hidden entries out.
        let shown = rows_of(&self.filtered, &self.list).len();
        let hidden = match n - shown {
            0 => String::new(),
            1 => tr!(" · 1 oculto", " · 1 hidden").to_owned(),
            k => trf!(" · {} ocultos", " · {} hidden", fx_core::format::count(k)),
        };
        let tail = match &self.status {
            Status::Loading => tr!(" — carregando…", " — loading…").to_owned(),
            Status::Sorting => tr!(" — ordenando…", " — sorting…").to_owned(),
            Status::Ready => match self.loaded_in {
                Some(d) => trf!(
                    "{hidden} · lida em {}",
                    "{hidden} · read in {}",
                    elapsed_text(d)
                ),
                None => hidden,
            },
            // The message itself is shown in the list area.
            Status::Failed(_) => tr!(" · erro", " · error").to_owned(),
        };
        (items(shown), tail)
    }

    /// Message shown in the middle of the list: an error, an empty folder or
    /// a filter with no match.
    fn placeholder(&self) -> Option<String> {
        if let Status::Failed(e) = &self.status {
            return Some(e.clone());
        }
        if let Some(filtered) = &self.filtered
            && filtered.is_empty()
        {
            if self.filter_text.is_empty() {
                return (!self.list.is_empty() && matches!(self.status, Status::Ready)).then(
                    || {
                        tr!(
                            "Só há itens ocultos aqui. Ctrl+H mostra.",
                            "There are only hidden items here. Ctrl+H shows them."
                        )
                        .to_owned()
                    },
                );
            }
            return Some(trf!(
                "Nada corresponde a \"{}\".",
                "Nothing matches \"{}\".",
                self.filter_text
            ));
        }
        (matches!(self.status, Status::Ready) && self.list.is_empty())
            .then(|| tr!("Esta pasta está vazia.", "This folder is empty.").to_owned())
    }

    /// Frame bookkeeping for `FX_TRACE=1`.
    fn trace_frame(&mut self, window: &mut Window) {
        let Some(trace) = &self.trace else { return };
        let mut t = trace.borrow_mut();
        t.mark("first_render");
        t.set_row_height(self.item_height());
        let items = self.list.len();
        if items > 0 {
            t.mark("first_render_with_rows");
            if let Some(opened) = self.opened.take() {
                let trace = trace.clone();
                window.on_next_frame(move |_, _| trace.borrow_mut().record_tab_open(opened));
            }
        }
        if items > 0 && !t.has_first_frame() {
            let trace = trace.clone();
            window.on_next_frame(move |_, cx| {
                let mut t = trace.borrow_mut();
                t.record_first_frame(items);
                if t.exit_after_first_frame {
                    t.write();
                    cx.quit();
                }
            });
        }
        if let Some(started) = t.take_pending_input() {
            let trace = trace.clone();
            // Runs at the start of the next frame, i.e. after this one (which shows
            // the key's result) was drawn and presented.
            window.on_next_frame(move |_, _| trace.borrow_mut().record_input(started));
        }
        let offset = f32::from(self.scroll.0.borrow().base_handle.offset().y);
        if t.observe_scroll(offset) {
            let trace = trace.clone();
            window.on_next_frame(move |_, _| trace.borrow_mut().record_frame());
            window.request_animation_frame();
        }
    }
}

impl FolderView {
    /// A column header: click sorts by it (an arrow shows the direction);
    /// fixed columns have a grab area on their left border to resize them.
    fn header_cell(
        &self,
        label: &'static str,
        key: SortKey,
        col: Option<Col>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let t = self.look.theme;
        let arrow = (self.sort.0 == key).then_some(match self.sort.1 {
            Direction::Ascending => ICON_SORT_UP,
            Direction::Descending => ICON_SORT_DOWN,
        });
        let mut cell = div()
            .id(label)
            .relative()
            .h_full()
            .flex()
            .items_center()
            .gap(px(space(1)))
            .min_w_0()
            .text_size(px(CAPTION_SIZE))
            .text_color(rgba(t.text_secondary))
            .hover(|s| s.text_color(rgba(t.text)))
            // Sizes line up on the right, like the column.
            .when(key == SortKey::Size, |d| d.justify_end())
            .child(div().min_w_0().truncate().child(label))
            .when_some(arrow, |d, glyph| {
                d.child(
                    div()
                        .flex_none()
                        .font_family(self.look.icon_font)
                        .text_size(px(8.0))
                        .child(glyph),
                )
            })
            .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _, cx| {
                view.sort_by(key, cx);
            }));
        if let Some(col) = col {
            let width = self.widths.get(col);
            cell = cell.w(px(width)).flex_none().child(
                div()
                    .id(SharedString::from(format!("resize-{label}")))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    // Centered on the gap between this column and the previous one.
                    .left(px(-(space(3) + RESIZE_HANDLE) / 2.0))
                    .w(px(RESIZE_HANDLE))
                    .cursor_col_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, ev: &MouseDownEvent, _, cx| {
                            view.resizing = Some(Resize {
                                col,
                                start_x: f32::from(ev.position.x),
                                start_w: view.widths.get(col),
                            });
                            // Not a click on the header (no sort).
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        cell
    }

    /// While a header border is dragged: follows the mouse anywhere in the
    /// window until the button is released.
    fn resize_tracker(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        self.resizing?;
        let entity = cx.entity();
        Some(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    window.on_mouse_event({
                        let entity = entity.clone();
                        move |_: &MouseUpEvent, _, _, cx| {
                            entity.update(cx, |view, cx| {
                                view.resizing = None;
                                cx.notify();
                            });
                        }
                    });
                    window.on_mouse_event(move |ev: &MouseMoveEvent, _, _, cx| {
                        entity.update(cx, |view, cx| {
                            let Some(r) = view.resizing else { return };
                            if !ev.dragging() {
                                // Released outside the window.
                                view.resizing = None;
                            } else {
                                // The border is on the column's left: dragging
                                // left widens it (the name column gives way).
                                let x = f32::from(ev.position.x);
                                view.widths.set(r.col, r.start_w + (r.start_x - x));
                            }
                            cx.notify();
                        });
                    });
                },
            )
            .absolute()
            .size_0(),
        )
    }
}

/// Modification date in the local time zone of that date.
fn local_date(t: SystemTime) -> String {
    date_time(t, fx_shell::utc_offset_at(t))
}

/// "9 ms", "0,4 ms", "1,2 s".
fn elapsed_text(d: Duration) -> String {
    let ms = d.as_secs_f64() * 1000.0;
    if ms < 10.0 {
        format!("{ms:.1} ms").replace('.', ",")
    } else if ms < 1000.0 {
        format!("{ms:.0} ms")
    } else {
        format!("{:.1} s", ms / 1000.0).replace('.', ",")
    }
}

/// A folder that could not be read, in words (shown in the list area).
fn read_failure(e: &std::io::Error) -> String {
    use std::io::ErrorKind;
    match (e.kind(), e.raw_os_error()) {
        (ErrorKind::PermissionDenied, _) | (_, Some(5)) => tr!(
            "Sem permissão para abrir esta pasta.",
            "No permission to open this folder."
        )
        .to_owned(),
        (ErrorKind::NotFound, _) | (_, Some(2 | 3)) => tr!(
            "Esta pasta não existe mais.",
            "This folder no longer exists."
        )
        .to_owned(),
        (_, Some(21)) => tr!(
            "A unidade não está pronta (sem disco ou desconectada).",
            "The drive is not ready (no disc or disconnected)."
        )
        .to_owned(),
        (_, Some(53 | 64 | 67 | 1222)) => tr!(
            "O local de rede não está disponível.",
            "The network location is not available."
        )
        .to_owned(),
        (_, Some(code)) => trf!(
            "Não foi possível ler esta pasta (erro {code} do Windows).",
            "Could not read this folder (Windows error {code})."
        ),
        _ => trf!(
            "Não foi possível ler esta pasta ({e}).",
            "Could not read this folder ({e})."
        ),
    }
}

/// Rows shown, in display order: the filter result, or every entry.
fn rows_of<'a>(filtered: &'a Option<Vec<u32>>, list: &'a FileList) -> &'a [u32] {
    filtered.as_deref().unwrap_or(list.order())
}

/// Window title: folder name, then "fx" (read by the taskbar and the Narrator).
pub fn window_title(path: &Path) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    format!("{name} — fx")
}

impl Render for FolderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(handle) = &self.a11y {
            handle.set_overlay(crate::a11y::Layer::Tab, self.tab_overlay());
        }
        self.trace_frame(window);
        self.sync_quick_look(window, cx);
        self.sync_terminal_size();
        let t = self.look.theme;
        let focused = self.focus.is_focused(window);
        let row_h = self.look.density.row_height();
        if self.grid {
            let width = f32::from(self.scroll.0.borrow().base_handle.bounds().size.width);
            self.grid_cols = grid::columns(width);
        }
        let count = if self.grid {
            self.rows().len().div_ceil(self.grid_cols)
        } else {
            self.rows().len()
        };
        if let Some(trace) = &self.trace {
            trace.borrow_mut().set_list_items(count);
        }
        let grid_view = self.grid;
        let cols = self.grid_cols;
        let list_width = f32::from(self.scroll.0.borrow().base_handle.bounds().size.width);
        self.columns = Columns::for_width(list_width, self.widths);
        // `list_width` is the last frame's. Once this frame is laid out, check
        // the new width: after a resize, draw once more so the columns (and
        // grid tiles) follow it.
        self.last_list_width = list_width;
        cx.on_next_frame(window, |view, _, cx| {
            let now = f32::from(view.scroll.0.borrow().base_handle.bounds().size.width);
            if (now - view.last_list_width).abs() > 0.5 {
                cx.notify();
            }
        });
        let columns = self.columns;

        div()
            .track_focus(&self.focus)
            // A click outside the terminal panel gives the keys back to the
            // list (the panel stops its own clicks).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, _: &MouseDownEvent, _, cx| {
                    if let Some(t) = view.terminal.as_mut().filter(|t| t.focused) {
                        t.focused = false;
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            // Anchor for the View menu.
            .relative()
            .size_full()
            .flex()
            .flex_col()
            // With Mica the backdrop shows through; otherwise paint its tint.
            .when(!self.look.mica, |d| d.bg(rgba(t.base)))
            .font_family(self.look.font)
            .text_size(px(BODY_SIZE))
            .text_color(rgba(t.text))
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|view, _: &MouseDownEvent, window, cx| {
                    view.go_back(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|view, _: &MouseDownEvent, window, cx| {
                    view.go_forward(window, cx);
                }),
            )
            .child(self.render_path_bar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .bg(rgba(t.layer))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .h(px(row_h))
                            .flex()
                            .items_center()
                            // Rows: 4 px outside, 1 px border, 12 px inside.
                            .px(px(space(4) + 1.0))
                            .gap(px(space(3)))
                            .border_b_1()
                            .border_color(rgba(t.divider))
                            .child(
                                self.header_cell(tr!("Nome", "Name"), SortKey::Name, None, cx)
                                    .flex_1(),
                            )
                            .when(!grid_view && columns.kind, |d| {
                                d.child(self.header_cell(
                                    tr!("Tipo", "Type"),
                                    SortKey::Type,
                                    Some(Col::Kind),
                                    cx,
                                ))
                            })
                            .when(!grid_view && columns.size, |d| {
                                d.child(self.header_cell(
                                    tr!("Tamanho", "Size"),
                                    SortKey::Size,
                                    Some(Col::Size),
                                    cx,
                                ))
                            })
                            .when(!grid_view && columns.date, |d| {
                                d.child(self.header_cell(
                                    tr!("Modificado em", "Date modified"),
                                    SortKey::Modified,
                                    Some(Col::Date),
                                    cx,
                                ))
                            })
                            .children(self.resize_tracker(cx))
                            // Room for the scrollbar, so columns line up with the rows
                            // (the negative margin cancels the gap before it).
                            .child(div().w(px(SCROLLBAR_WIDTH)).ml(px(-space(3))).flex_none()),
                    )
                    .child(
                        div()
                            .id("list-area")
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .on_mouse_move(cx.listener(|view, ev: &MouseMoveEvent, window, cx| {
                                view.maybe_drag(ev, window, cx);
                            }))
                            // Right click on the empty part: the folder's own menu.
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(|view, _: &MouseDownEvent, window, cx| {
                                    view.open_menu(Vec::new(), window, cx);
                                }),
                            )
                            .drag_over::<ExternalPaths>(move |s, _, _, _| {
                                s.border_1().border_color(rgba(t.accent))
                            })
                            .on_drop(cx.listener(|view, paths: &ExternalPaths, window, cx| {
                                let dest = view.path.clone();
                                view.drop_into(paths.paths().to_vec(), dest, window, cx);
                            }))
                            .child(if grid_view {
                                uniform_list(
                                    "tiles",
                                    count,
                                    cx.processor(
                                        move |view: &mut Self, range: Range<usize>, _, cx| {
                                            view.render_grid_rows(range, cols, focused, cx)
                                        },
                                    ),
                                )
                                .size_full()
                                .pr(px(SCROLLBAR_WIDTH))
                                .track_scroll(self.scroll.clone())
                                .into_any_element()
                            } else {
                                uniform_list(
                                    "rows",
                                    count,
                                    cx.processor(
                                        move |view: &mut Self, range: Range<usize>, _, cx| {
                                            view.render_rows(range, focused, cx)
                                        },
                                    ),
                                )
                                .size_full()
                                .pr(px(SCROLLBAR_WIDTH))
                                .track_scroll(self.scroll.clone())
                                .into_any_element()
                            })
                            .child(self.render_scrollbar(cx))
                            .when_some(self.placeholder(), |d, text| {
                                d.child(
                                    div()
                                        .absolute()
                                        .top(px(space(10)))
                                        .left_0()
                                        .right_0()
                                        .flex()
                                        .justify_center()
                                        .px(px(space(4)))
                                        .text_color(rgba(t.text_secondary))
                                        .child(text),
                                )
                            }),
                    ),
            )
            .children(self.render_quick_look(cx))
            .children(self.render_search(cx))
            .children(self.render_columns(cx))
            .children(self.render_terminal(cx))
            .children(self.render_batch_rename(cx))
            .children(self.render_view_menu(cx))
    }
}

/// The cloud state glyph next to a name (T3.9): a cloud for items only in
/// the cloud, a check for those on this device, a filled check for the ones
/// always kept. `true`: drawn in the accent color.
fn cloud_glyph(a: fx_core::Attributes) -> Option<(&'static str, bool)> {
    match fx_core::CloudState::of(a) {
        fx_core::CloudState::None => None,
        fx_core::CloudState::CloudOnly => Some((crate::theme::ICON_CLOUD, false)),
        fx_core::CloudState::Available => Some((crate::theme::ICON_CLOUD_LOCAL, true)),
        fx_core::CloudState::AlwaysKept => Some((crate::theme::ICON_CLOUD_KEPT, true)),
    }
}

/// Whether the list may read an item's content (thumbnail, per-file icon):
/// never for one only in the cloud, which would download it (T3.9).
pub(crate) fn may_read_content(a: fx_core::Attributes) -> bool {
    !fx_core::CloudState::of(a).is_cloud_only()
}

impl FolderView {
    /// The icon of entry `i`; never one that reads a cloud-only file.
    fn icon_key_of(&self, i: u32) -> IconKey {
        let a = self.list.attributes(i);
        icon_key_cloud(
            &self.path,
            self.list.name(i),
            a.is_dir(),
            !may_read_content(a),
        )
    }
}

impl FolderView {
    /// The tab's panel with the keyboard, for the accessibility tree.
    fn tab_overlay(&self) -> Option<crate::a11y::Overlay> {
        self.batch_rename_overlay()
            .or_else(|| self.quick_look_overlay())
            .or_else(|| self.view_menu_overlay())
            .or_else(|| self.search_overlay())
            .or_else(|| self.terminal_overlay())
            .or_else(|| self.columns_overlay())
    }

    /// The folder the user is looking at: the focused column in the column
    /// view, the listed folder otherwise.
    pub(super) fn folder_in_view(&self) -> PathBuf {
        self.miller
            .as_ref()
            .map_or_else(|| self.path.clone(), |c| c.current_dir().to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::{COL_MAX, COL_MIN, Col, Columns, Widths, elapsed_text, read_failure};
    use std::time::Duration;

    #[test]
    fn cloud_only_items_are_never_read() {
        use fx_core::Attributes as A;
        assert!(super::may_read_content(A::empty()));
        assert!(super::may_read_content(A::PINNED));
        assert!(super::may_read_content(A::UNPINNED));
        assert!(!super::may_read_content(A::RECALL));
        assert!(!super::may_read_content(A::OFFLINE.with(A::UNPINNED)));
        assert_eq!(super::cloud_glyph(A::empty()), None);
        assert_eq!(
            super::cloud_glyph(A::RECALL),
            Some((crate::theme::ICON_CLOUD, false))
        );
        assert_eq!(
            super::cloud_glyph(A::PINNED),
            Some((crate::theme::ICON_CLOUD_KEPT, true))
        );
    }

    #[test]
    fn read_failures_are_in_words() {
        let denied = std::io::Error::from_raw_os_error(5);
        assert_eq!(
            read_failure(&denied),
            "Sem permissão para abrir esta pasta."
        );
        let gone = std::io::Error::from_raw_os_error(3);
        assert_eq!(read_failure(&gone), "Esta pasta não existe mais.");
        let other = std::io::Error::from_raw_os_error(1117);
        assert!(read_failure(&other).contains("1117"));
    }

    #[test]
    fn elapsed_texts() {
        assert_eq!(elapsed_text(Duration::from_micros(400)), "0,4 ms");
        assert_eq!(elapsed_text(Duration::from_millis(92)), "92 ms");
        assert_eq!(elapsed_text(Duration::from_millis(1234)), "1,2 s");
    }

    #[test]
    fn widths_are_clamped() {
        let mut w = Widths::default();
        w.set(Col::Date, 10.0);
        w.set(Col::Kind, 9999.0);
        assert_eq!((w.get(Col::Date), w.get(Col::Kind)), (COL_MIN, COL_MAX));
    }

    #[test]
    fn narrow_lists_drop_type_then_date_then_size() {
        let w = Widths::default();
        let at = |width: f32| {
            let c = Columns::for_width(width, w);
            (c.kind, c.size, c.date)
        };
        assert_eq!(at(0.0), (true, true, true));
        assert_eq!(at(900.0), (true, true, true));
        assert_eq!(at(650.0), (false, true, true));
        assert_eq!(at(450.0), (false, true, false));
        assert_eq!(at(300.0), (false, false, false));
        // Wider columns need more room.
        let wide = Widths { date: 300.0, ..w };
        assert!(!Columns::for_width(600.0, wide).date);
    }
}
