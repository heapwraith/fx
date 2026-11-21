//! Window root (task T1.4): a strip of tabs over the active folder view. Each
//! tab is a `FolderView` with its own history, selection and list.
//!
//! Keys (captured before the folder view sees them): Ctrl+T new tab (same
//! folder), Ctrl+W close, Ctrl+Tab / Ctrl+Shift+Tab switch, Ctrl+1/2/3 row
//! density for every tab. Mouse: click activates, middle click or × closes,
//! + opens, dragging a tab reorders. Closing the last tab closes the window.
//!
//! Sidebar (task T1.5, `sidebar.rs`): quick access and drives, left of the
//! active tab; ☰ or Ctrl+B collapses it.

mod queue;
mod sidebar;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use accesskit::{Action, ActionRequest};
use fx_core::Tabs;
use gpui::{
    ClickEvent, Context, Entity, KeyDownEvent, MouseButton, MouseDownEvent, SharedString, Window,
    div, prelude::*, px,
};

use crate::a11y::{A11yHandle, row_entry};
use crate::folder_view::{FolderView, Look, window_title};
use crate::icons::Media;
use crate::places::Places;
use crate::theme::{
    BODY_SIZE, CAPTION_SIZE, Density, ICON_ADD, ICON_CLOSE, ICON_MENU, Mode, RADIUS, Theme, rgba,
    space,
};
use crate::trace::Trace;

const TAB_HEIGHT: f32 = 32.0;
const TAB_MAX_WIDTH: f32 = 220.0;
const CLOSE_SIZE: f32 = 20.0;
/// Below this window width the sidebar hides itself.
const SIDEBAR_MIN_WINDOW: f32 = 720.0;

/// What a window gets from the app: shared places and media caches, and its
/// own accessibility adapter (with the screen reader's action requests).
pub struct WindowServices {
    pub places: Entity<Places>,
    pub media: Media,
    pub a11y: Option<(A11yHandle, Receiver<ActionRequest>)>,
}

pub struct TabsView {
    tabs: Tabs<Entity<FolderView>>,
    look: Look,
    trace: Option<Rc<RefCell<Trace>>>,
    places: Entity<Places>,
    media: Media,
    a11y: Option<A11yHandle>,
    sidebar_open: bool,
    /// "Apply to all" box of the conflict dialog.
    apply_all: bool,
}

/// What is carried while a tab is dragged (also its drag preview).
#[derive(Debug, Clone)]
struct DraggedTab {
    index: usize,
    label: SharedString,
    theme: Theme,
    font: &'static str,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme;
        div()
            .h(px(TAB_HEIGHT))
            .px(px(space(3)))
            .flex()
            .items_center()
            .rounded(px(RADIUS))
            .bg(rgba(t.base))
            .border_1()
            .border_color(rgba(t.divider))
            .shadow_md()
            .font_family(self.font)
            .text_size(px(BODY_SIZE))
            .text_color(rgba(t.text))
            .child(self.label.clone())
    }
}

impl TabsView {
    pub fn new(
        path: PathBuf,
        look: Look,
        trace: Option<Rc<RefCell<Trace>>>,
        services: WindowServices,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let WindowServices {
            places,
            media,
            a11y,
        } = services;
        let (a11y, actions) = match a11y {
            Some((h, rx)) => (Some(h), Some(rx)),
            None => (None, None),
        };
        if let Some(rx) = actions {
            Self::listen_a11y(rx, cx);
        }
        let first = Self::new_tab(
            path,
            look,
            trace.clone(),
            None,
            media.clone(),
            a11y.clone(),
            cx,
        );
        cx.observe(&places, |_, _, cx| cx.notify()).detach();
        cx.observe(&media.ops, |_, _, cx| cx.notify()).detach();
        let view = Self {
            tabs: Tabs::new(first),
            look,
            trace,
            places,
            media,
            sidebar_open: look.sidebar,
            a11y,
            apply_all: false,
        };
        view.focus_active(window, cx);
        view
    }

    fn new_tab(
        path: PathBuf,
        look: Look,
        trace: Option<Rc<RefCell<Trace>>>,
        opened: Option<Instant>,
        media: Media,
        a11y: Option<A11yHandle>,
        cx: &mut Context<Self>,
    ) -> Entity<FolderView> {
        let tab = cx.new(|cx| FolderView::new(path, look, trace, opened, media, a11y, cx));
        // Re-render the strip when the tab's folder (its label) changes.
        cx.observe(&tab, |_, _, cx| cx.notify()).detach();
        tab
    }

    /// This window's state for the saved session (task T1.17).
    pub fn export(&self, window: &Window, cx: &gpui::App) -> crate::store::WindowState {
        let b = window.bounds();
        crate::store::WindowState {
            bounds: Some([
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width),
                f32::from(b.size.height),
            ]),
            active: self.tabs.active_index(),
            tabs: self.tabs.iter().map(|t| t.read(cx).export()).collect(),
        }
    }

    /// Row density and sidebar, for the saved settings.
    pub fn settings_part(&self) -> (String, bool) {
        let density = match self.look.density {
            Density::Compact => "compacta",
            Density::Standard => "padrão",
            Density::Comfortable => "confortável",
        };
        (density.to_owned(), self.sidebar_open)
    }

    /// Reopens a saved window's tabs (the first tab already shows the first
    /// folder).
    pub fn restore_tabs(
        &mut self,
        state: crate::store::WindowState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut saved = state.tabs.into_iter();
        if let Some(first) = saved.next() {
            self.tabs
                .active()
                .update(cx, |tab, _| tab.apply_saved(first));
        }
        for t in saved {
            let tab = Self::new_tab(
                t.path.clone(),
                self.look,
                None,
                None,
                self.media.clone(),
                self.a11y.clone(),
                cx,
            );
            tab.update(cx, |tab, _| tab.apply_saved(t));
            self.tabs.open(tab);
        }
        self.tabs
            .activate(state.active.min(self.tabs.len().saturating_sub(1)));
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Focus and click requests from screen readers go to the active tab.
    fn listen_a11y(rx: Receiver<ActionRequest>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                // Wait on a background thread, never on the UI thread.
                let (back, request) = background
                    .spawn(async move {
                        let r = rx.recv();
                        (rx, r)
                    })
                    .await;
                rx = back;
                let Ok(request) = request else { break };
                let alive = this.update(cx, |view, cx| {
                    let wanted = matches!(request.action, Action::Focus | Action::Click);
                    if let (true, Some(entry)) = (wanted, row_entry(request.target_node)) {
                        let tab = view.tabs.active().clone();
                        tab.update(cx, |tab, cx| {
                            tab.a11y_select(entry);
                            cx.notify();
                        });
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Keys go to the active tab; the window title follows it.
    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.tabs.active().clone();
        let tab = active.read(cx);
        window.set_window_title(&window_title(tab.path()));
        tab.focus_handle().focus(window);
        // Re-render it even if unchanged: GPUI would reuse its cached frame,
        // and its render is where `FX_TRACE` closes the key's measurement.
        active.update(cx, |_, cx| cx.notify());
    }

    fn open_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.tabs.active().read(cx).path().to_path_buf();
        let opened = self.trace.as_ref().map(|_| Instant::now());
        let tab = Self::new_tab(
            path,
            self.look,
            self.trace.clone(),
            opened,
            self.media.clone(),
            self.a11y.clone(),
            cx,
        );
        self.tabs.open(tab);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn close_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.close(i).is_none() {
            return;
        }
        if self.tabs.is_empty() {
            self.write_trace();
            window.remove_window();
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn activate(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.activate(i) {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    /// The system theme changed: every tab follows. Returns whether Mica shows.
    pub fn set_system_mode(&mut self, system: Mode, cx: &mut Context<Self>) -> bool {
        if !self.look.forced_mode {
            self.look.theme = Theme::for_mode(system);
        }
        self.look.mica = self.look.mica_supported && self.look.theme.mode == system;
        for tab in self.tabs.iter() {
            tab.update(cx, |view, cx| {
                view.set_system_mode(system);
                cx.notify();
            });
        }
        cx.notify();
        self.look.mica
    }

    pub fn write_trace(&self) {
        if let Some(trace) = &self.trace {
            trace.borrow().write();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        let ks = &event.keystroke;
        let m = ks.modifiers;
        if !m.control || m.alt || m.platform {
            return;
        }
        match (ks.key.as_str(), m.shift) {
            ("t", false) => self.open_tab(window, cx),
            ("w", false) => self.close_tab(self.tabs.active_index(), window, cx),
            ("tab", false) => {
                self.tabs.next();
                self.focus_active(window, cx);
            }
            ("b", false) => self.sidebar_open = !self.sidebar_open,
            ("tab", true) => {
                self.tabs.previous();
                self.focus_active(window, cx);
            }
            (key @ ("1" | "2" | "3"), false) => {
                let density = match key {
                    "1" => Density::Compact,
                    "2" => Density::Standard,
                    _ => Density::Comfortable,
                };
                self.look.density = density;
                for tab in self.tabs.iter() {
                    tab.update(cx, |view, cx| {
                        view.set_density(density);
                        cx.notify();
                    });
                }
            }
            _ => return,
        }
        if let Some(trace) = &self.trace {
            trace.borrow_mut().input_started(started, "tab");
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn render_strip(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let active = self.tabs.active_index();
        let mut strip = div()
            .h(px(space(10)))
            .flex_none()
            .flex()
            .items_end()
            .gap(px(space(1)))
            .px(px(space(2)))
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .id("toggle-sidebar")
                    .flex_none()
                    .mb(px(space(1)))
                    .size(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS))
                    .font_family(icon_font)
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(ICON_MENU)
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.sidebar_open = !view.sidebar_open;
                        cx.notify();
                    })),
            );
        for (k, tab) in self.tabs.iter().enumerate() {
            let path = tab.read(cx).path();
            let label: SharedString = path
                .file_name()
                .map_or_else(
                    || path.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                )
                .into();
            let is_active = k == active;
            let dragged = DraggedTab {
                index: k,
                label: label.clone(),
                theme: t,
                font: self.look.font,
            };
            strip = strip.child(
                div()
                    .id(("tab", k))
                    .flex_shrink()
                    .min_w(px(space(16)))
                    .max_w(px(TAB_MAX_WIDTH))
                    .h(px(TAB_HEIGHT))
                    .pl(px(space(3)))
                    .pr(px(space(1)))
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .rounded_t(px(RADIUS * 2.0))
                    .when(is_active, |d| d.bg(rgba(t.layer)).text_color(rgba(t.text)))
                    .when(!is_active, |d| {
                        d.text_color(rgba(t.text_secondary))
                            .hover(|s| s.bg(rgba(t.hover)))
                    })
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .child(
                        div()
                            .id(("close", k))
                            .flex_none()
                            .size(px(CLOSE_SIZE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(RADIUS))
                            .font_family(icon_font)
                            .text_size(px(10.0))
                            .hover(|s| s.bg(rgba(t.hover)))
                            .child(ICON_CLOSE)
                            .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                                view.close_tab(k, window, cx);
                                cx.stop_propagation();
                            })),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            view.activate(k, window, cx);
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            view.close_tab(k, window, cx);
                        }),
                    )
                    .on_drag(dragged, |d, _, _, cx| cx.new(|_| d.clone()))
                    .drag_over::<DraggedTab>(move |s, _, _, _| s.bg(rgba(t.selected)))
                    .on_drop(cx.listener(move |view, d: &DraggedTab, window, cx| {
                        if view.tabs.move_tab(d.index, k) {
                            view.focus_active(window, cx);
                            cx.notify();
                        }
                    })),
            );
        }
        strip.child(
            div()
                .id("new-tab")
                .flex_none()
                .mb(px(space(1)))
                .size(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS))
                .font_family(icon_font)
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .hover(|s| s.bg(rgba(t.hover)))
                .child(ICON_ADD)
                .on_click(cx.listener(|view, _: &ClickEvent, window, cx| {
                    view.open_tab(window, cx);
                })),
        )
    }
}

impl Render for TabsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.look.theme;
        let wide = f32::from(window.viewport_size().width) >= SIDEBAR_MIN_WINDOW;
        div()
            .capture_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .flex()
            .flex_col()
            // With Mica the backdrop shows through; otherwise paint its tint.
            .when(!self.look.mica, |d| d.bg(rgba(t.base)))
            .font_family(self.look.font)
            .text_size(px(BODY_SIZE))
            .text_color(rgba(t.text))
            .child(self.render_strip(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    // A narrow window hides the sidebar (the preference stays).
                    .when(self.sidebar_open && wide, |d| {
                        d.child(self.render_sidebar(cx))
                    })
                    .child(div().flex_1().min_w_0().child(self.tabs.active().clone())),
            )
            .children(self.render_queue(cx))
            .children(self.render_conflict(cx))
    }
}
