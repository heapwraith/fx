//! Details panel (T6.9 + T6.10, D-068): beside the list, the properties of
//! the item with the cursor as Windows names and formats them (dimensions,
//! duration, date taken, camera, author...), read on a thread of its own when
//! the item changes, nothing kept while closed. Alt+Shift+P (as in Explorer)
//! opens and closes.

use std::path::PathBuf;

use fx_core::tr;
use gpui::{Context, SharedString, div, prelude::*, px};

use super::{FolderView, rows_of};
use crate::theme::{CAPTION_SIZE, rgba, space};

const WIDTH: f32 = 300.0;

pub(super) struct Details {
    /// The item shown (the cursor's when it was read).
    path: Option<PathBuf>,
    props: Vec<(String, String)>,
    loading: bool,
    ticket: u64,
}

impl FolderView {
    /// Alt+Shift+P: the details panel on or off.
    pub(super) fn toggle_details(&mut self, cx: &mut Context<Self>) {
        self.details = match self.details {
            Some(_) => None,
            None => Some(Details {
                path: None,
                props: Vec::new(),
                loading: false,
                ticket: 0,
            }),
        };
        cx.notify();
    }

    /// The item the panel should show: the cursor's (or the folder itself).
    fn details_target(&self) -> PathBuf {
        let rows = rows_of(&self.filtered, &self.list);
        match self.selection.cursor().filter(|c| rows.contains(c)) {
            Some(c) => self.path.join(self.list.os_name(c)),
            None => self.path.clone(),
        }
    }

    /// Called while drawing: reads the properties again when the item changed.
    pub(super) fn refresh_details(&mut self, cx: &mut Context<Self>) {
        let target = self.details_target();
        let Some(d) = self.details.as_mut() else {
            return;
        };
        if d.path.as_ref() == Some(&target) {
            return;
        }
        d.path = Some(target.clone());
        d.loading = true;
        d.ticket += 1;
        let ticket = d.ticket;
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-details".into())
            .spawn(move || {
                let _ = tx.send(fx_shell::item_properties(&target).unwrap_or_default());
            });
        cx.spawn(async move |this, cx| {
            let props = rx.await.unwrap_or_default();
            let _ = this.update(cx, |view, cx| {
                if let Some(d) = view.details.as_mut().filter(|d| d.ticket == ticket) {
                    d.props = props;
                    d.loading = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn render_details(
        &self,
        _cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let d = self.details.as_ref()?;
        let t = self.look.theme;
        let name = d
            .path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| self.path.display().to_string());
        let mut body = div()
            .id("details-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(space(4)))
            .flex()
            .flex_col()
            .gap(px(space(3)));
        if d.loading && d.props.is_empty() {
            body = body.child(
                div()
                    .text_color(rgba(t.text_secondary))
                    .child(tr!("Lendo…", "Reading…")),
            );
        } else if d.props.is_empty() {
            body = body.child(div().text_color(rgba(t.text_secondary)).child(tr!(
                "O Windows não tem detalhes deste item.",
                "Windows has no details for this item."
            )));
        }
        for (label, value) in &d.props {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(SharedString::from(label.clone())),
                    )
                    .child(div().child(SharedString::from(value.clone()))),
            );
        }
        Some(
            div()
                .id("details")
                .flex_none()
                .w(px(WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(rgba(t.divider))
                .bg(rgba(t.layer))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(4)))
                        .pt(px(space(4)))
                        .pb(px(space(3)))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .truncate()
                        .child(SharedString::from(name)),
                )
                .child(body)
                .child(
                    div()
                        .flex_none()
                        .px(px(space(4)))
                        .py(px(space(2)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!("Alt+Shift+P fecha", "Alt+Shift+P closes")),
                ),
        )
    }
}
