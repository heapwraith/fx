//! Sidebar (task T1.5): home, quick access (pinned folders, with a button to
//! pin or unpin the current folder and × to unpin) and drives with free space.
//! Clicking an item opens it in the active tab.

use std::path::{Path, PathBuf};

use fx_core::format::size_short;
use fx_shell::places::{DriveKind, KnownFolder};
use gpui::{ClickEvent, Context, SharedString, div, prelude::*, px, relative};

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
    fn open_in_active(&mut self, path: PathBuf, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let tab = self.current();
        tab.update(cx, |view, cx| view.open(path, window, cx));
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let places = self.places.read(cx);
        let current = self.current().read(cx).path().to_path_buf();
        let pinned_here = places.pinned.contains(&current);

        let item = |id: SharedString, icon: &'static str, label: SharedString, path: &Path| {
            let active = path == current;
            let target = path.to_path_buf();
            div()
                .id(id)
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
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .overflow_y_scroll()
            .py(px(space(1)))
            .border_r_1()
            .border_color(rgba(t.divider));

        if let Some(home) = places.home() {
            col = col.child(item("home".into(), ICON_HOME, places.label(home), home));
        }

        // Quick access: pinned folders, then the pin/unpin button for the
        // folder shown in the active tab.
        col = col.child(
            caption("Acesso rápido").justify_between().child(
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

        col = col.child(caption("Este computador"));
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
                            .child(format!(
                                "{} livres de {}",
                                size_short(free),
                                size_short(total)
                            )),
                    );
            }
            col = col.child(
                div()
                    .id(("drive", k))
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
        col
    }
}
