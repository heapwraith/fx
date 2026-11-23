//! Sidebar (task T1.5): home, quick access (pinned folders, with a button to
//! pin or unpin the current folder and × to unpin) and drives with free space.
//! Clicking an item opens it in the active tab.

use fx_core::{tr, trf};
use std::path::{Path, PathBuf};

use fx_core::format::size_short;
use fx_shell::places::{DriveKind, KnownFolder};
use gpui::{App, ClickEvent, Context, SharedString, div, prelude::*, px, relative};

use super::TabsView;
use crate::places::drive_label;
use crate::theme::{
    CAPTION_SIZE, ICON_CLOSE, ICON_DESKTOP, ICON_DISC, ICON_DOCUMENTS, ICON_DOWNLOADS, ICON_DRIVE,
    ICON_FOLDER, ICON_HOME, ICON_MUSIC, ICON_NETWORK_DRIVE, ICON_PICTURES, ICON_PIN, ICON_UNPIN,
    ICON_USB, ICON_VIDEOS, RADIUS, rgba, space,
};

const WIDTH: f32 = 240.0;
const ITEM_HEIGHT: f32 = 32.0;

fn folder_icon(kind: Option<KnownFolder>) -> &'static str {
    match kind {
        Some(KnownFolder::Home) => ICON_HOME,
        Some(KnownFolder::Desktop) => ICON_DESKTOP,
        Some(KnownFolder::Downloads) => ICON_DOWNLOADS,
        Some(KnownFolder::Documents) => ICON_DOCUMENTS,
        Some(KnownFolder::Pictures) => ICON_PICTURES,
        Some(KnownFolder::Music) => ICON_MUSIC,
        Some(KnownFolder::Videos) => ICON_VIDEOS,
        None => ICON_FOLDER,
    }
}

fn drive_icon(kind: DriveKind) -> &'static str {
    match kind {
        DriveKind::Removable => ICON_USB,
        DriveKind::Network => ICON_NETWORK_DRIVE,
        DriveKind::Optical => ICON_DISC,
        DriveKind::Fixed | DriveKind::Other => ICON_DRIVE,
    }
}

impl TabsView {
    /// Opens `path` in the active tab.
    pub(super) fn open_in_active(
        &mut self,
        path: PathBuf,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let tab = self.current();
        tab.update(cx, |view, cx| view.open(path, window, cx));
        self.focus_active(window, cx);
        cx.notify();
    }

    /// The places of the sidebar in order (home, pinned, drives), for the
    /// keyboard (F6) and screen readers.
    pub(super) fn sidebar_targets(&self, cx: &App) -> Vec<(SharedString, PathBuf)> {
        let places = self.places.read(cx);
        let mut out = Vec::new();
        if let Some(home) = places.home() {
            out.push((places.label(home), home.to_path_buf()));
        }
        for path in places.pinned.paths() {
            out.push((places.label(path), path.clone()));
        }
        for d in &places.drives {
            out.push((drive_label(d), d.root.clone()));
        }
        out
    }

    /// Where F6 starts: the folder shown, if it is in the sidebar.
    pub(super) fn sidebar_current(&self, cx: &App) -> usize {
        let current = self.current().read(cx).path().to_path_buf();
        self.sidebar_targets(cx)
            .iter()
            .position(|(_, p)| *p == current)
            .unwrap_or(0)
    }

    /// The sidebar, while it has the keyboard, as a screen reader reads it.
    pub(super) fn sidebar_overlay(
        &self,
        cx: &App,
    ) -> Option<(crate::a11y::Overlay, Vec<Option<super::WindowAct>>)> {
        let k = self.sidebar_focus?;
        let targets = self.sidebar_targets(cx);
        let items = targets
            .iter()
            .enumerate()
            .map(|(i, (label, path))| crate::a11y::Item::Choice {
                label: label.to_string(),
                description: Some(path.display().to_string()),
                selected: i == k,
            })
            .collect();
        let acts = (0..targets.len())
            .map(|i| Some(super::WindowAct::Place(i)))
            .collect();
        Some((
            crate::a11y::Overlay {
                title: tr!("Barra lateral", "Sidebar").to_owned(),
                description: Some(
                    tr!(
                        "↑ e ↓ escolhem, Enter abre, Esc ou F6 voltam à lista.",
                        "↑ and ↓ choose, Enter opens, Esc or F6 go back to the list."
                    )
                    .to_owned(),
                ),
                items,
                focus: Some(k),
                positions: None,
                modal: false,
            },
            acts,
        ))
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let places = self.places.read(cx);
        let current = self.current().read(cx).path().to_path_buf();
        let pinned_here = places.pinned.contains(&current);

        // The place with the keyboard (F6) gets the focus ring.
        let focused: Option<PathBuf> = self
            .sidebar_focus
            .and_then(|k| self.sidebar_targets(cx).into_iter().nth(k))
            .map(|(_, p)| p);
        let ring = move |d: gpui::Stateful<gpui::Div>, path: &Path| {
            d.border_2()
                .border_color(rgba(if focused.as_deref() == Some(path) {
                    t.accent
                } else {
                    0x0000_0000
                }))
        };
        let item = |id: SharedString, icon: &'static str, label: SharedString, path: &Path| {
            let active = path == current;
            let target = path.to_path_buf();
            ring(div().id(id), path)
                .h(px(ITEM_HEIGHT))
                .mx(px(space(1)))
                .px(px(space(3)))
                .flex()
                .items_center()
                .gap(px(space(3)))
                .rounded(px(RADIUS))
                .when(active, |d| d.bg(rgba(t.selected)))
                .when(!active, |d| d.hover(|s| s.bg(rgba(t.hover))))
                .child(
                    div()
                        .flex_none()
                        .font_family(icon_font)
                        .text_size(px(14.0))
                        .child(icon),
                )
                .child(div().flex_1().min_w_0().truncate().child(label))
                .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                    view.open_in_active(target.clone(), window, cx);
                }))
        };
        let caption = |text: &'static str| {
            div()
                .h(px(space(8)))
                .px(px(space(4)))
                .flex()
                .items_center()
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .child(text)
        };

        let mut col = div()
            .id("sidebar")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(px(space(1)));

        if let Some(home) = places.home() {
            col = col.child(item("home".into(), ICON_HOME, places.label(home), home));
        }

        // Quick access: pinned folders, then the pin/unpin button for the
        // folder shown in the active tab.
        col = col.child(
            caption(tr!("Acesso rápido", "Quick access"))
                .justify_between()
                .child(
                    div()
                        .id("pin-current")
                        .size(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(RADIUS))
                        .font_family(icon_font)
                        .hover(|s| s.bg(rgba(t.hover)))
                        .child(if pinned_here { ICON_UNPIN } else { ICON_PIN })
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            let path = view.current().read(cx).path().to_path_buf();
                            view.places.update(cx, |places, cx| {
                                if !places.pinned.unpin(&path) {
                                    places.pinned.pin(path);
                                }
                                cx.notify();
                            });
                        })),
                ),
        );
        for (k, path) in places.pinned.paths().iter().enumerate() {
            let group: SharedString = format!("pinned-{k}").into();
            let unpin = path.clone();
            col = col.child(
                item(
                    format!("pin-{k}").into(),
                    folder_icon(places.known_kind(path)),
                    places.label(path),
                    path,
                )
                .group(group.clone())
                .child(
                    div()
                        .id(("unpin", k))
                        .flex_none()
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(RADIUS))
                        .font_family(icon_font)
                        .text_size(px(10.0))
                        .text_color(rgba(t.text_secondary))
                        .opacity(0.0)
                        .group_hover(group, |s| s.opacity(1.0))
                        .hover(|s| s.bg(rgba(t.hover)))
                        .child(ICON_CLOSE)
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            let path = unpin.clone();
                            view.places.update(cx, |places, cx| {
                                places.pinned.unpin(&path);
                                cx.notify();
                            });
                            cx.stop_propagation();
                        })),
                ),
            );
        }

        // Saved searches (T6.3): open in the active tab, refreshing by themselves.
        let saved = crate::searches::list();
        if !saved.is_empty() {
            col = col.child(caption(tr!("Buscas", "Searches")));
        }
        for (k, q) in saved.into_iter().enumerate() {
            let group: SharedString = format!("search-{k}").into();
            let (open, forget) = (q.clone(), q.query.clone());
            col = col.child(
                div()
                    .id(("saved-search", k))
                    .group(group.clone())
                    .h(px(ITEM_HEIGHT))
                    .mx(px(space(1)))
                    .px(px(space(3)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(
                        div()
                            .flex_none()
                            .font_family(icon_font)
                            .text_size(px(14.0))
                            .child("\u{E721}"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(q.query.clone())),
                    )
                    .child(
                        div()
                            .id(("forget-search", k))
                            .flex_none()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(RADIUS))
                            .font_family(icon_font)
                            .text_size(px(10.0))
                            .text_color(rgba(t.text_secondary))
                            .opacity(0.0)
                            .group_hover(group, |s| s.opacity(1.0))
                            .hover(|s| s.bg(rgba(t.hover)))
                            .child(ICON_CLOSE)
                            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                                crate::searches::remove(&forget);
                                cx.stop_propagation();
                                cx.notify();
                            })),
                    )
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        let q = open.clone();
                        view.current()
                            .update(cx, |tab, cx| tab.open_saved_search(&q, cx));
                    })),
            );
        }

        col = col.child(caption(tr!("Este computador", "This PC")));
        for (k, d) in places.drives.iter().enumerate() {
            let active = d.root == current;
            let target = d.root.clone();
            // Name, then a usage bar and "X livres de Y" below it.
            let mut text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(div().truncate().child(drive_label(d)));
            if let Some((free, total)) = d.space.filter(|(_, total)| *total > 0) {
                let used = 1.0 - free as f32 / total as f32;
                text = text
                    .child(
                        div().h(px(4.0)).rounded(px(2.0)).bg(rgba(t.divider)).child(
                            div()
                                .h_full()
                                .w(relative(used))
                                .rounded(px(2.0))
                                .bg(rgba(t.accent)),
                        ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(t.text_secondary))
                            .child(trf!(
                                "{} livres de {}",
                                "{} free of {}",
                                size_short(free),
                                size_short(total)
                            )),
                    );
            }
            col = col.child(
                ring(div().id(("drive", k)), &d.root)
                    .mx(px(space(1)))
                    .px(px(space(3)))
                    .py(px(space(2)))
                    .flex()
                    .items_start()
                    .gap(px(space(3)))
                    .rounded(px(RADIUS))
                    .when(active, |d| d.bg(rgba(t.selected)))
                    .when(!active, |d| d.hover(|s| s.bg(rgba(t.hover))))
                    .child(
                        div()
                            .flex_none()
                            .pt(px(2.0))
                            .font_family(icon_font)
                            .text_size(px(14.0))
                            .child(drive_icon(d.kind)),
                    )
                    .child(text)
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.open_in_active(target.clone(), window, cx);
                    })),
            );
        }
        // Fixed footer (the 2026 pattern: settings and help at the bottom
        // of the sidebar, always in view).
        let footer_item =
            |id: &'static str, icon: &'static str, label: &'static str, hint: &'static str| {
                div()
                    .id(id)
                    .h(px(ITEM_HEIGHT))
                    .mx(px(space(1)))
                    .px(px(space(3)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(
                        div()
                            .flex_none()
                            .font_family(icon_font)
                            .text_size(px(14.0))
                            .child(icon),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(label))
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(rgba(t.text_secondary))
                            .child(hint),
                    )
            };
        let footer = div()
            .flex_none()
            .py(px(space(1)))
            .border_t_1()
            .border_color(rgba(t.divider))
            .child(
                footer_item(
                    "sidebar-help",
                    crate::theme::ICON_HELP,
                    tr!("Ajuda", "Help"),
                    "F1",
                )
                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.open_howto(cx))),
            )
            .child(
                footer_item(
                    "sidebar-settings",
                    "\u{E713}",
                    tr!("Configurações", "Settings"),
                    "Ctrl+,",
                )
                .on_click(
                    cx.listener(|view, _: &ClickEvent, _, cx| view.open_settings_general(cx)),
                ),
            );
        div()
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgba(t.divider))
            .child(col)
            .child(footer)
    }
}
