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

mod dual;
mod guide;
mod history;
mod palette;
mod queue;
mod settings;
mod sidebar;

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use accesskit::{Action, ActionRequest};
use fx_core::Tabs;
use gpui::{
    ClickEvent, Context, Entity, KeyDownEvent, MouseButton, MouseDownEvent, ScrollHandle,
    SharedString, Window, div, prelude::*, px,
};

use crate::a11y::{A11yHandle, row_entry};
use crate::folder_view::{FolderView, Look, ViewEvent, window_title};
use crate::icons::Media;
use crate::places::Places;
use crate::theme::{
    BODY_SIZE, CAPTION_SIZE, Density, ICON_ADD, ICON_CLOSE, ICON_HISTORY, ICON_MENU, Mode, RADIUS,
    Theme, rgba, space,
};
use crate::trace::Trace;

const TAB_HEIGHT: f32 = 32.0;
const TAB_MAX_WIDTH: f32 = 220.0;
/// Below this, tabs stop shrinking and the row scrolls (a label stays readable).
const TAB_MIN_WIDTH: f32 = 112.0;
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
    /// The tab row scrolls when tabs no longer fit at their minimum width.
    tab_scroll: ScrollHandle,
    /// Active tab at the last render (to scroll a newly active tab into view).
    shown_active: Option<usize>,
    /// Operation history panel (Ctrl+Shift+H or the clock button).
    history_open: bool,
    /// "Sobre" or "Como usar" (T4.6).
    guide: Option<guide::Guide>,
    /// The settings dialog and its section (Ctrl+, or the sidebar gear).
    settings: Option<settings::Section>,
    /// Command palette (Ctrl+K, Ctrl+Shift+P; T1.24).
    palette: Option<palette::Palette>,
    /// Second panes (F3, T3.2), by the tab's own view.
    seconds: std::collections::HashMap<gpui::EntityId, dual::Second>,
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
            tab_scroll: ScrollHandle::new(),
            shown_active: None,
            history_open: false,
            guide: None,
            settings: None,
            palette: None,
            seconds: std::collections::HashMap::new(),
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
        // The tab's View menu changes window-wide settings.
        cx.subscribe(&tab, |view, _, event: &ViewEvent, cx| match *event {
            ViewEvent::Density(d) => view.set_density(d, cx),
            ViewEvent::ToggleHidden => view.toggle_hidden(cx),
        })
        .detach();
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
    pub fn settings_part(&self) -> crate::store::UiSettings {
        let density = match self.look.density {
            Density::Compact => "compacta",
            Density::Standard => "padrão",
            Density::Comfortable => "confortável",
        };
        crate::store::UiSettings {
            density: density.to_owned(),
            sidebar: self.sidebar_open,
            show_hidden: self.look.show_hidden,
        }
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
                        let tab = view.current();
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
        let active = self.current();
        let tab = active.read(cx);
        window.set_window_title(&window_title(tab.path()));
        tab.focus_handle().focus(window);
        // Re-render it even if unchanged: GPUI would reuse its cached frame,
        // and its render is where `FX_TRACE` closes the key's measurement.
        active.update(cx, |_, cx| cx.notify());
    }

    fn open_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.current().read(cx).path().to_path_buf();
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
        let Some(closed) = self.tabs.close(i) else {
            return;
        };
        self.seconds.remove(&closed.entity_id());
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
            self.look.theme = Theme::current(system);
        }
        self.look.mica = self.look.mica_supported
            && self.look.theme.mode == system
            && !self.look.theme.high_contrast;
        for tab in self.tabs.iter() {
            tab.update(cx, |view, cx| {
                view.set_system_mode(system);
                cx.notify();
            });
        }
        cx.notify();
        self.look.mica
    }

    /// Row density for every tab of the window (Ctrl+1/2/3, View menu).
    fn set_density(&mut self, density: Density, cx: &mut Context<Self>) {
        self.look.density = density;
        for tab in self.tabs.iter() {
            tab.update(cx, |view, cx| {
                view.set_density(density);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Hidden and system files on or off for every tab (Ctrl+H, View menu);
    /// saved with the settings.
    fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        let show = !self.look.show_hidden;
        self.look.show_hidden = show;
        for tab in self.tabs.iter() {
            tab.update(cx, |view, cx| {
                view.set_show_hidden(show);
                cx.notify();
            });
        }
        cx.notify();
    }

    pub fn write_trace(&self) {
        if let Some(trace) = &self.trace {
            trace.borrow().write();
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        if self.palette_open() {
            self.palette_key(event, started, window, cx);
            cx.stop_propagation();
            return;
        }
        let ks = &event.keystroke;
        let m = ks.modifiers;
        // A conflict dialog is modal: it takes every key (T1.13; found in the
        // final QA, keys used to reach the list behind it).
        if self.media.ops.read(cx).current_ask().is_some() {
            let answer = match ks.key.as_str() {
                "enter" => Some(fx_ops::Resolution::KeepBoth),
                "escape" => Some(fx_ops::Resolution::Skip),
                "space" => {
                    self.apply_all = !self.apply_all;
                    None
                }
                _ => None,
            };
            if let Some(resolution) = answer {
                let all = self.apply_all;
                self.apply_all = false;
                self.media
                    .ops
                    .update(cx, |ops, cx| ops.answer(resolution, all, cx));
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.settings.is_some() && ks.key == "escape" {
            self.settings = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        // Ctrl+, : settings (the shortcut of macOS, VS Code, Zed, Slack...).
        if ks.key == "," && m.control && !m.alt && !m.shift {
            if self.settings.is_some() {
                self.settings = None;
                cx.notify();
            } else {
                self.open_settings(settings::Section::General, cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.guide.is_some() && ks.key == "escape" {
            self.guide = None;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if ks.key == "f1" && !m.control && !m.alt && !m.shift {
            if self.guide == Some(guide::Guide::HowTo) {
                self.guide = None;
                cx.notify();
            } else {
                self.open_guide(guide::Guide::HowTo, cx);
            }
            cx.stop_propagation();
            return;
        }
        if self.history_open && ks.key == "escape" {
            self.history_open = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        let handled = match ks.key.as_str() {
            "f3" if plain => {
                self.toggle_dual(window, cx);
                true
            }
            "tab" if plain => self.switch_pane(window, cx),
            "f5" if plain => self.transfer(true, window, cx),
            "f6" if plain => self.transfer(false, window, cx),
            _ => false,
        };
        if handled {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if !m.control || m.alt || m.platform {
            return;
        }
        match (ks.key.as_str(), m.shift) {
            ("h", true) => self.history_open = !self.history_open,
            ("k", false) | ("p", true) => self.open_palette(cx),
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
                self.set_density(density, cx);
            }
            ("h", false) => self.toggle_hidden(cx),
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
        let strip = div()
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
        let mut row = div()
            .id("tab-row")
            .flex_shrink()
            .min_w_0()
            .flex()
            .items_end()
            .gap(px(space(1)))
            .overflow_x_scroll()
            .track_scroll(&self.tab_scroll);
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
            let group: SharedString = format!("tab-{k}").into();
            row = row.child(
                div()
                    .id(("tab", k))
                    .group(group.clone())
                    .flex_shrink()
                    .min_w(px(TAB_MIN_WIDTH))
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
                            // Only on the active tab or under the mouse: more
                            // room for the label.
                            .when(!is_active, |d| {
                                d.invisible().group_hover(group, |s| s.visible())
                            })
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
        strip
            .child(row)
            .child(
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
            .child(div().flex_1())
            .child(
                div()
                    .id("history")
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
                    .when(self.history_open, |d| d.bg(rgba(t.selected)))
                    .child(ICON_HISTORY)
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.history_open = !view.history_open;
                        cx.notify();
                    })),
            )
    }
}

impl Render for TabsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.look.theme;
        let wide = f32::from(window.viewport_size().width) >= SIDEBAR_MIN_WINDOW;
        self.sync_pane_focus(window, cx);
        let active = self.tabs.active_index();
        if self.shown_active != Some(active) {
            self.shown_active = Some(active);
            self.tab_scroll.scroll_to_item(active);
        }
        div()
            .capture_key_down(cx.listener(Self::on_key_down))
            // Anchor for the dialog overlay (covers the queue panel too).
            .relative()
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
                    .child(self.render_panes(cx)),
            )
            .children(self.render_queue(cx))
            .children(self.render_history(cx))
            .children(self.render_guide(cx))
            .children(self.render_settings(cx))
            .children(self.render_palette(cx))
            .children(self.render_conflict(cx))
    }
}

/// "Como usar o fx" on the first run (T4.6): set at startup.
pub fn guide_first_run(first: bool) {
    guide::set_first_run(first);
}

pub fn guide_take_first_run() -> bool {
    guide::take_first_run()
}

pub fn guide_seen() -> bool {
    guide::seen()
}

impl TabsView {
    pub fn open_howto(&mut self, cx: &mut Context<Self>) {
        self.open_guide(guide::Guide::HowTo, cx);
    }
}

impl TabsView {
    /// The sidebar footer's "Configurações".
    pub(crate) fn open_settings_general(&mut self, cx: &mut Context<Self>) {
        self.open_settings(settings::Section::General, cx);
    }
}
