//! Folder window (task T0.5): a virtualized list with Name, Size and Date columns.
//!
//! The folder is read on a worker thread (fx-fs); batches are appended on the UI
//! thread (cheap: ~0.2 ms per 2,000 entries) and the name sort runs on the
//! background executor, so the UI thread never blocks.

use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use fx_core::format::{date_time_utc, size_kb};
use fx_core::{Direction, Entry, FileList, SortKey};
use fx_fs::Strategy;
use gpui::{
    Context, FocusHandle, KeyDownEvent, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollStrategy, SharedString, UniformListScrollHandle, Window, canvas, div, point, prelude::*,
    px, rgb, uniform_list,
};

use crate::trace::Trace;

const ROW_HEIGHT: f32 = 24.0;
const SIZE_COL: f32 = 110.0;
const DATE_COL: f32 = 140.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const THUMB_MIN: f32 = 24.0;

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
    trace: Option<Rc<RefCell<Trace>>>,
}

impl FolderView {
    pub fn new(path: PathBuf, trace: Option<Trace>, cx: &mut Context<Self>) -> Self {
        let mut trace = trace;
        if let Some(t) = &mut trace {
            t.mark("folder_requested");
        }
        let (tx, rx) = channel::<Vec<Entry>>();
        let worker_path = path.clone();
        let worker = std::thread::Builder::new()
            .name("fx-enumerate".into())
            .spawn(move || fx_fs::enumerate(Strategy::FindFirstFileEx, &worker_path, &tx));
        let status = match worker {
            Ok(worker) => {
                Self::receive(rx, worker, cx);
                Status::Loading
            }
            Err(e) => Status::Failed(format!("could not start reader thread: {e}")),
        };
        let trace = trace.map(|t| Rc::new(RefCell::new(t)));
        Self {
            path,
            list: Arc::new(FileList::new()),
            status,
            scroll: UniformListScrollHandle::new(),
            drag_offset: None,
            focus: cx.focus_handle(),
            filter_text: String::new(),
            filtered: None,
            trace,
        }
    }

    /// Appends batches as they arrive, then sorts by name off the UI thread.
    fn receive(
        rx: Receiver<Vec<Entry>>,
        worker: std::thread::JoinHandle<std::io::Result<usize>>,
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
                let alive = this.update(cx, |view, cx| {
                    if let Some(trace) = &view.trace {
                        trace.borrow_mut().mark("first_batch");
                    }
                    Arc::make_mut(&mut view.list).extend(batch);
                    view.refilter();
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }

            let result = background.spawn(async move { worker.join() }).await;
            let failure = match result {
                Ok(Ok(_)) => None,
                Ok(Err(e)) => Some(e.to_string()),
                Err(_) => Some("reader thread panicked".to_owned()),
            };
            let Ok(list) = this.update(cx, |view, cx| {
                view.status = Status::Sorting;
                cx.notify();
                view.list.clone()
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
                Arc::make_mut(&mut view.list).set_order(order);
                view.refilter();
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

    fn render_rows(&mut self, range: Range<usize>) -> Vec<impl IntoElement + use<>> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().record_rows_built(range.len());
        }
        let order = self.rows();
        range
            .filter_map(|row| order.get(row).copied())
            .map(|i| {
                let is_dir = self.list.attributes(i).is_dir();
                let size: SharedString = if is_dir {
                    SharedString::default()
                } else {
                    size_kb(self.list.size(i)).into()
                };
                div()
                    .w_full()
                    .h(px(ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(is_dir, |d| d.text_color(rgb(0x1d4ed8)))
                            .child(SharedString::from(self.list.name(i).to_owned())),
                    )
                    .child(
                        div()
                            .w(px(SIZE_COL))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .text_color(rgb(0x555555))
                            .child(size),
                    )
                    .child(
                        div()
                            .w(px(DATE_COL))
                            .flex_none()
                            .text_color(rgb(0x555555))
                            .child(date_time_utc(self.list.modified(i))),
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
                    .rounded_md()
                    .bg(rgb(0xb0b0b0))
                    .hover(|s| s.bg(rgb(0x808080))),
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

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Rows currently shown, in display order.
    fn rows(&self) -> &[u32] {
        self.filtered.as_deref().unwrap_or(self.list.order())
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
    }

    /// "Type to filter": printable characters extend the filter, Backspace
    /// removes the last character and Escape clears it.
    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        let ks = &event.keystroke;
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
            trace.borrow_mut().input_started(started);
        }
        cx.notify();
    }

    fn status_text(&self) -> String {
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
    fn trace_frame(&self, window: &mut Window) {
        let Some(trace) = &self.trace else { return };
        let mut t = trace.borrow_mut();
        t.mark("first_render");
        let items = self.list.len();
        if items > 0 {
            t.mark("first_render_with_rows");
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

    pub fn write_trace(&self) {
        if let Some(trace) = &self.trace {
            trace.borrow().write();
        }
    }
}

impl Render for FolderView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.trace_frame(window);
        let count = self.rows().len();
        let header_cell = |label: &'static str| div().text_color(rgb(0x555555)).child(label);

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .text_color(rgb(0x1a1a1a))
            .text_sm()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .bg(rgb(0xf3f3f3))
                    .child(div().truncate().child(self.path.display().to_string()))
                    .child(div().flex_none().pl_4().child(self.status_text())),
            )
            .child(
                div()
                    .h(px(ROW_HEIGHT))
                    .flex()
                    .items_center()
                    .px_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(0xe0e0e0))
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
                            cx.processor(|view: &mut Self, range: Range<usize>, _, _| {
                                view.render_rows(range)
                            }),
                        )
                        .size_full()
                        .pr(px(SCROLLBAR_WIDTH))
                        .track_scroll(self.scroll.clone()),
                    )
                    .child(self.render_scrollbar(cx)),
            )
    }
}
