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
//! History (task T1.3): Alt+Left/Right and mouse buttons 4/5 go back and
//! forward, restoring each folder's scroll position and selection. The path
//! bar is in `path_bar.rs`.

mod path_bar;

use std::cell::RefCell;
use std::collections::HashSet;
use std::ffi::OsString;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use fx_core::format::{date_time_utc, size_kb};
use fx_core::{Direction, Entry, FileList, History, MoveMode, Selection, SortKey, Step};
use fx_fs::Strategy;
use fx_shell::icons::icon_key;
use gpui::{
    Context, Entity, FocusHandle, Image, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, NavigationDirection, Pixels, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, canvas, div, img, point, prelude::*, px, uniform_list,
};

use crate::icons::Icons;
use crate::theme::{BODY_SIZE, CAPTION_SIZE, Density, Mode, RADIUS, Theme, rgba, space};
use crate::trace::Trace;

const SIZE_COL: f32 = 112.0;
const DATE_COL: f32 = 144.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const THUMB_MIN: f32 = 24.0;
/// Width of the accent bar on the selected row (Fluent list selection).
const INDICATOR_WIDTH: f32 = 3.0;
/// File icons are drawn at 16 px (small shell icons).
const ICON_SIZE: f32 = 16.0;

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
}

impl FolderView {
    /// `opened`: when the user asked for this tab (Ctrl+T), to time its first
    /// frame with rows (`FX_TRACE`).
    pub fn new(
        path: PathBuf,
        look: Look,
        trace: Option<Rc<RefCell<Trace>>>,
        opened: Option<Instant>,
        icons: Entity<Icons>,
        cx: &mut Context<Self>,
    ) -> Self {
        // Redraw when icons arrive.
        cx.observe(&icons, |_, _, cx| cx.notify()).detach();
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
        };
        view.start_loading(cx);
        view
    }

    /// Starts reading `self.path` on a worker thread.
    fn start_loading(&mut self, cx: &mut Context<Self>) {
        let (tx, rx) = channel::<Vec<Entry>>();
        let worker_path = self.path.clone();
        let worker = std::thread::Builder::new()
            .name("fx-enumerate".into())
            .spawn(move || fx_fs::enumerate(Strategy::FindFirstFileEx, &worker_path, &tx));
        self.status = match worker {
            Ok(worker) => {
                Self::receive(rx, worker, self.generation, cx);
                Status::Loading
            }
            Err(e) => Status::Failed(format!("could not start reader thread: {e}")),
        };
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
            let mut rx = rx;
            loop {
                // Block on the channel on a background thread, never on the UI thread.
                let (back, batch) = background
                    .spawn(async move {
                        let batch = rx.recv();
                        (rx, batch)
                    })
                    .await;
                rx = back;
                let Ok(batch) = batch else { break };
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
                Ok(Err(e)) => Some(e.to_string()),
                Err(_) => Some("reader thread panicked".to_owned()),
            };
            let Ok(Some(list)) = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return None;
                }
                view.status = Status::Sorting;
                cx.notify();
                Some(view.list.clone())
            }) else {
                return;
            };
            // The clone is dropped before we return to the UI thread, so
            // `Arc::make_mut` below does not copy the list.
            let order = background
                .spawn(async move {
                    let order = list.sorted_order(SortKey::Name, Direction::Ascending);
                    drop(list);
                    order
                })
                .await;
            let _ = this.update(cx, |view, cx| {
                if view.generation != generation {
                    return;
                }
                Arc::make_mut(&mut view.list).set_order(order);
                view.refilter();
                view.apply_restore();
                view.status = match failure {
                    Some(e) => Status::Failed(e),
                    None => Status::Ready,
                };
                if let Some(trace) = &view.trace {
                    trace.borrow_mut().record_loaded();
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_rows(
        &mut self,
        range: Range<usize>,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Vec<impl IntoElement + use<>> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().record_rows_built(range.len());
        }
        let t = self.look.theme;
        let row_h = self.look.density.row_height();
        // Icons of the rows drawn; unknown ones are requested after this pass.
        let icons = self.icons.read(cx);
        let show_icons = icons.enabled();
        let mut missing = Vec::new();
        let rows: Vec<(u32, Option<Arc<Image>>)> = range
            .filter_map(|row| self.rows().get(row).copied())
            .map(|i| {
                if !show_icons {
                    return (i, None);
                }
                let key = icon_key(
                    &self.path,
                    self.list.name(i),
                    self.list.attributes(i).is_dir(),
                );
                let image = icons.get(&key);
                if image.is_none() && icons.is_unknown(&key) {
                    missing.push(key);
                }
                (i, image)
            })
            .collect();
        if !missing.is_empty() {
            self.icons.update(cx, |icons, _| icons.request(missing));
        }
        rows.into_iter()
            .map(|(i, icon)| {
                let is_dir = self.list.attributes(i).is_dir();
                let selected = self.selection.is_selected(i);
                let is_cursor = self.selection.cursor() == Some(i);
                let size: SharedString = if is_dir {
                    SharedString::default()
                } else {
                    size_kb(self.list.size(i)).into()
                };
                div()
                    .id(i as usize)
                    .w_full()
                    .h(px(row_h))
                    .px(px(space(1)))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .flex()
                            .items_center()
                            .gap(px(space(3)))
                            .px(px(space(3)))
                            .rounded(px(RADIUS))
                            .when(selected, |d| d.bg(rgba(t.selected)))
                            .when(!selected, |d| d.hover(|s| s.bg(rgba(t.hover))))
                            // Always 1 px of border (transparent off the cursor),
                            // so the focus ring does not shift the text.
                            .border_1()
                            .border_color(if is_cursor && focused {
                                rgba(t.focus)
                            } else {
                                rgba(0x00000000)
                            })
                            .when(selected, |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top(px(row_h / 2.0 - 8.0))
                                        .w(px(INDICATOR_WIDTH))
                                        .h(px(16.0))
                                        .rounded(px(INDICATOR_WIDTH / 2.0))
                                        .bg(rgba(t.accent)),
                                )
                            })
                            // Fixed slot: the icon arriving later moves nothing.
                            .when(show_icons, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .size(px(ICON_SIZE))
                                        .when_some(icon, |d, image| {
                                            d.child(img(image).size(px(ICON_SIZE)))
                                        }),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(rgba(t.text))
                                    .child(SharedString::from(self.list.name(i).to_owned())),
                            )
                            .child(
                                div()
                                    .w(px(SIZE_COL))
                                    .flex_none()
                                    .flex()
                                    .justify_end()
                                    .text_color(rgba(t.text_secondary))
                                    .child(size),
                            )
                            .child(
                                div()
                                    .w(px(DATE_COL))
                                    .flex_none()
                                    .text_color(rgba(t.text_secondary))
                                    .child(date_time_utc(self.list.modified(i))),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, ev: &MouseDownEvent, window, cx| {
                            view.focus.focus(window);
                            view.click(i, ev, window, cx);
                            cx.notify();
                        }),
                    )
            })
            .collect()
    }

    fn render_scrollbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.scroll.0.borrow();
        let viewport = state.base_handle.bounds();
        let view_h = f32::from(viewport.size.height);
        let content_h = state
            .last_item_size
            .map_or(0.0, |s| f32::from(s.contents.height));
        let offset = -f32::from(state.base_handle.offset().y);
        let base = state.base_handle.clone();
        drop(state);

        if view_h <= 0.0 || content_h <= view_h {
            return div().id("scrollbar");
        }
        let thumb_h = (view_h * view_h / content_h).max(THUMB_MIN);
        let max_scroll = content_h - view_h;
        let thumb_top = (offset / max_scroll).clamp(0.0, 1.0) * (view_h - thumb_h);
        let entity = cx.entity();

        div()
            .id("scrollbar")
            .absolute()
            .top(px(thumb_top))
            .right_0()
            .w(px(SCROLLBAR_WIDTH))
            .h(px(thumb_h))
            .p(px(2.0))
            .child(
                div()
                    .size_full()
                    .rounded(px(RADIUS))
                    .bg(rgba(self.look.theme.thumb))
                    .hover(|s| s.bg(rgba(self.look.theme.thumb_hover))),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |thumb, _, window, _| {
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |ev: &MouseDownEvent, _, _, cx| {
                                if thumb.contains(&ev.position) {
                                    entity.update(cx, |view, _| {
                                        view.drag_offset = Some(ev.position.y - thumb.origin.y);
                                    });
                                }
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |_: &MouseUpEvent, _, _, cx| {
                                entity.update(cx, |view, _| view.drag_offset = None);
                            }
                        });
                        window.on_mouse_event(move |ev: &MouseMoveEvent, _, _, cx| {
                            let Some(grab) = entity.read(cx).drag_offset else {
                                return;
                            };
                            if !ev.dragging() {
                                return;
                            }
                            let top = f32::from(ev.position.y - viewport.origin.y - grab);
                            let fraction = (top / (view_h - thumb_h)).clamp(0.0, 1.0);
                            base.set_offset(point(px(0.0), px(-fraction * max_scroll)));
                            cx.notify(entity.entity_id());
                        });
                    },
                )
                // Pinned to the thumb's corner: without top/left an absolute child
                // sits at its static position, below the visible thumb.
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }

    /// The system theme changed (the theme is kept while `FX_THEME` forces one).
    /// Returns whether Mica should show: the backdrop is tinted by the *system*
    /// theme, so it only matches when the fx theme is the same.
    pub fn set_system_mode(&mut self, system: Mode) -> bool {
        if !self.look.forced_mode {
            self.look.theme = Theme::for_mode(system);
        }
        self.look.mica = self.look.mica_supported && self.look.theme.mode == system;
        self.look.mica
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
        let m = ev.modifiers;
        if m.shift {
            self.selection
                .extend_to(rows_of(&self.filtered, &self.list), i);
        } else if m.control {
            self.selection.toggle(i);
        } else {
            self.selection.select_only(i);
            if ev.click_count == 2 {
                self.open_cursor(window, cx);
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
        self.scroll.scroll_to_item(pos, strategy);
    }

    /// Rows that fit in the list viewport, minus one (PgUp/PgDn step).
    fn page_rows(&self) -> usize {
        let height = f32::from(self.scroll.0.borrow().base_handle.bounds().size.height);
        let rows = (height / self.look.density.row_height()).floor() as usize;
        rows.saturating_sub(1).max(1)
    }

    /// Enter: opens the cursor entry. Folders open in place; files open in
    /// task T1.9. Returns whether something happened.
    fn open_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.selection.cursor() else {
            return false;
        };
        if !self.list.attributes(i).is_dir() {
            return false;
        }
        let path = self.path.join(self.list.os_name(i));
        self.go_to(path, None, window, cx);
        true
    }

    /// Alt+Up / Backspace: opens the parent folder and selects the one we left.
    fn go_up(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(parent) = self.path.parent().map(Path::to_path_buf) else {
            return false;
        };
        let child = self.path.file_name().map(OsString::from);
        self.go_to(parent, child.map(Restore::cursor_on), window, cx);
        true
    }

    /// Opens another folder in this window, as a new history entry.
    fn go_to(
        &mut self,
        path: PathBuf,
        restore: Option<Restore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.history.current_mut().restore = self.snapshot();
        self.history.push(Visit {
            path: path.clone(),
            restore: Restore::default(),
        });
        self.load(path, restore, window, cx);
    }

    /// Alt+Left / mouse button 4.
    fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let snapshot = self.snapshot();
        if !self.history.can_go_back() {
            return false;
        }
        self.history.current_mut().restore = snapshot;
        if let Some(v) = self.history.go_back().cloned() {
            self.load(v.path, Some(v.restore), window, cx);
        }
        true
    }

    /// Alt+Right / mouse button 5.
    fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let snapshot = self.snapshot();
        if !self.history.can_go_forward() {
            return false;
        }
        self.history.current_mut().restore = snapshot;
        if let Some(v) = self.history.go_forward().cloned() {
            self.load(v.path, Some(v.restore), window, cx);
        }
        true
    }

    /// Selection and scroll position of the folder shown now.
    fn snapshot(&self) -> Restore {
        let selected = if self.selection.len() <= MAX_RESTORED {
            self.selection
                .selected()
                .map(|i| self.list.os_name(i))
                .collect()
        } else {
            Vec::new()
        };
        Restore {
            selected,
            cursor: self.selection.cursor().map(|i| self.list.os_name(i)),
            scroll: Some(f32::from(self.scroll.0.borrow().base_handle.offset().y)),
        }
    }

    /// Shows another folder in this window (history is the caller's job).
    fn load(
        &mut self,
        path: PathBuf,
        restore: Option<Restore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit = None;
        self.generation += 1;
        self.path = path;
        self.list = Arc::new(FileList::new());
        self.filter_text.clear();
        self.filtered = None;
        self.selection.clear();
        self.restore = restore;
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.0), px(0.0)));
        window.set_window_title(&window_title(&self.path));
        self.start_loading(cx);
        cx.notify();
    }

    /// Once loaded, restores the selection and scroll position saved for this
    /// folder (unless the user selected something meanwhile). Names that no
    /// longer exist are skipped.
    fn apply_restore(&mut self) {
        let Some(r) = self.restore.take() else {
            return;
        };
        if !self.selection.is_empty() {
            return;
        }
        let exact: HashSet<&OsString> = r.selected.iter().chain(&r.cursor).collect();
        let shown: HashSet<String> = exact
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        let mut selected = Vec::new();
        let mut cursor = None;
        if !shown.is_empty() {
            for i in 0..self.list.len() as u32 {
                if !shown.contains(self.list.name(i)) {
                    continue;
                }
                let name = self.list.os_name(i);
                if r.cursor.as_ref() == Some(&name) {
                    cursor = Some(i);
                }
                if r.selected.contains(&name) {
                    selected.push(i);
                }
            }
        }
        self.selection.restore(selected, cursor);
        match (r.scroll, cursor) {
            (Some(y), _) => self
                .scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.0), px(y))),
            (None, Some(i)) => {
                if let Some(pos) = self.selection.position(self.list.order(), i) {
                    self.scroll.scroll_to_item(pos, ScrollStrategy::Center);
                }
            }
            (None, None) => {}
        }
    }

    /// Recomputes the filter from scratch (after the list itself changed).
    fn refilter(&mut self) {
        if !self.filter_text.is_empty() {
            self.filtered = Some(self.list.filter(&self.filter_text));
        }
    }

    /// Applies a new filter text. Extending the text refines the previous result
    /// instead of scanning all entries again.
    fn set_filter(&mut self, text: String) {
        let refine = !self.filter_text.is_empty() && text.starts_with(self.filter_text.as_str());
        self.filtered = match (&self.filtered, text.is_empty()) {
            (_, true) => None,
            (Some(previous), false) if refine => Some(self.list.filter_within(&text, previous)),
            _ => Some(self.list.filter(&text)),
        };
        self.filter_text = text;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        // Keep the cursor on a visible row so Enter opens the first match. The
        // selection itself is kept (fx-core `Selection`).
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

    /// Keyboard: navigation keys move the cursor (see the module comment);
    /// printable characters extend the "type to filter" text, Backspace removes
    /// its last character (or goes up when it is empty) and Escape clears it
    /// (or the selection).
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let only_ctrl = m.control && !m.alt && !m.shift;
        let only_alt = m.alt && !m.control && !m.shift;
        if self.edit_key(ks, window, cx) {
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "path");
            }
            cx.notify();
            return;
        }
        let bar = match ks.key.as_str() {
            "l" if only_ctrl => true,
            "d" if only_alt => true,
            _ => false,
        };
        if bar {
            self.begin_edit(window, cx);
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "path");
            }
            return;
        }
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        let step = match ks.key.as_str() {
            "up" => Some(Step::Up(1)),
            "down" => Some(Step::Down(1)),
            "pageup" => Some(Step::Up(self.page_rows())),
            "pagedown" => Some(Step::Down(self.page_rows())),
            "home" => Some(Step::First),
            "end" => Some(Step::Last),
            _ => None,
        };
        let handled = match ks.key.as_str() {
            "up" if only_alt => self.go_up(window, cx).then_some("open"),
            "left" if only_alt => self.go_back(window, cx).then_some("open"),
            "right" if only_alt => self.go_forward(window, cx).then_some("open"),
            _ if m.alt || m.platform => None,
            _ if step.is_some() => {
                let mode = if m.shift {
                    MoveMode::Extend
                } else if m.control {
                    MoveMode::CursorOnly
                } else {
                    MoveMode::Select
                };
                if let Some(step) = step {
                    self.move_cursor(step, mode);
                }
                Some("move")
            }
            "enter" if plain => self.open_cursor(window, cx).then_some("open"),
            "backspace" if plain && self.filter_text.is_empty() => {
                self.go_up(window, cx).then_some("open")
            }
            "a" if only_ctrl => {
                self.selection
                    .select_all(rows_of(&self.filtered, &self.list));
                Some("move")
            }
            "space" if only_ctrl => {
                self.selection.toggle_cursor();
                Some("move")
            }
            "escape" if plain && self.filter_text.is_empty() && !self.selection.is_empty() => {
                self.selection.clear();
                Some("move")
            }
            _ => None,
        };
        if let Some(kind) = handled {
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, kind);
            }
            cx.notify();
            return;
        }
        let mut text = self.filter_text.clone();
        match ks.key.as_str() {
            "backspace" => {
                text.pop();
            }
            "escape" => text.clear(),
            _ => {
                let m = &ks.modifiers;
                if m.control || m.alt || m.platform || m.function {
                    return;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => text.push_str(c),
                    _ => return,
                }
            }
        }
        if text == self.filter_text {
            return;
        }
        self.set_filter(text);
        if let Some(trace) = &self.trace {
            trace.borrow_mut().input_started(started, "filter");
        }
        cx.notify();
    }

    fn status_text(&self) -> String {
        let text = self.load_text();
        match self.selection.len() {
            0 => text,
            1 => format!("{text} · 1 selecionado"),
            k => format!("{text} · {k} selecionados"),
        }
    }

    fn load_text(&self) -> String {
        let n = self.list.len();
        if let Some(filtered) = &self.filtered {
            return format!("filtro \"{}\": {} de {n}", self.filter_text, filtered.len());
        }
        match &self.status {
            Status::Loading => format!("{n} itens — carregando…"),
            Status::Sorting => format!("{n} itens — ordenando…"),
            Status::Ready => format!("{n} itens"),
            Status::Failed(e) => format!("{n} itens — erro: {e}"),
        }
    }

    /// Frame bookkeeping for `FX_TRACE=1`.
    fn trace_frame(&mut self, window: &mut Window) {
        let Some(trace) = &self.trace else { return };
        let mut t = trace.borrow_mut();
        t.mark("first_render");
        t.set_row_height(self.look.density.row_height());
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
        self.trace_frame(window);
        let t = self.look.theme;
        let count = self.rows().len();
        let focused = self.focus.is_focused(window);
        let row_h = self.look.density.row_height();
        let header_cell = |label: &'static str| {
            div()
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .child(label)
        };

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
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
            .child(self.render_path_bar(cx))
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
                            .px(px(space(4)))
                            .gap(px(space(3)))
                            .border_b_1()
                            .border_color(rgba(t.divider))
                            .child(header_cell("Nome").flex_1())
                            .child(
                                header_cell("Tamanho")
                                    .w(px(SIZE_COL))
                                    .flex_none()
                                    .flex()
                                    .justify_end(),
                            )
                            .child(header_cell("Data (UTC)").w(px(DATE_COL)).flex_none())
                            // Room for the scrollbar, so columns line up with the rows.
                            .child(div().w(px(SCROLLBAR_WIDTH)).flex_none()),
                    )
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .child(
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
                                .track_scroll(self.scroll.clone()),
                            )
                            .child(self.render_scrollbar(cx)),
                    ),
            )
    }
}
