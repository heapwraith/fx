//! Queue panel and conflict dialog of a window (task T1.13).
//!
//! The panel sits at the bottom while copies or moves run: label, progress
//! bar, bytes done, pause/resume and cancel. A conflict shows one dialog over
//! the window: keep both (the default, SAFETY.md §2.3), replace or skip, with
//! "apply to all" for the rest of that job.

use fx_core::format::size_short;
use fx_ops::Resolution;
use gpui::{ClickEvent, Context, SharedString, deferred, div, prelude::*, px, relative};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, ICON_CLOSE, ICON_PAUSE, ICON_PLAY, RADIUS, rgba, space};

impl TabsView {
    /// The queue panel, or nothing when no copy or move runs.
    pub(super) fn render_queue(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let ops = self.media.ops.read(cx);
        if ops.active().is_empty() {
            return None;
        }
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let mut panel = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(space(1)))
            .px(px(space(4)))
            .py(px(space(2)))
            .border_t_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.layer));
        for job in ops.active() {
            let id = job.id;
            let paused = job.paused;
            let fraction = if job.bytes == 0 {
                0.0
            } else {
                (job.done as f32 / job.bytes as f32).clamp(0.0, 1.0)
            };
            let button = |name: SharedString, glyph: &'static str| {
                div()
                    .id(name)
                    .flex_none()
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADIUS))
                    .font_family(icon_font)
                    .text_size(px(10.0))
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(glyph)
            };
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .text_size(px(CAPTION_SIZE))
                                    .child(div().truncate().child(job.label.clone()))
                                    .child(
                                        div().flex_none().text_color(rgba(t.text_secondary)).child(
                                            format!(
                                                "{}{} de {}",
                                                if paused { "pausado · " } else { "" },
                                                size_short(job.done),
                                                size_short(job.bytes)
                                            ),
                                        ),
                                    ),
                            )
                            .child(
                                div().h(px(4.0)).rounded(px(2.0)).bg(rgba(t.divider)).child(
                                    div()
                                        .h_full()
                                        .w(relative(fraction))
                                        .rounded(px(2.0))
                                        .bg(rgba(t.accent)),
                                ),
                            ),
                    )
                    .child(
                        button(
                            format!("pause-{id}").into(),
                            if paused { ICON_PLAY } else { ICON_PAUSE },
                        )
                        .on_click(cx.listener(
                            move |view, _: &ClickEvent, _, cx| {
                                view.media
                                    .ops
                                    .update(cx, |ops, cx| ops.set_paused(id, !paused, cx));
                            },
                        )),
                    )
                    .child(button(format!("cancel-{id}").into(), ICON_CLOSE).on_click(
                        cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.media.ops.update(cx, |ops, cx| ops.cancel(id, cx));
                        }),
                    )),
            );
        }
        Some(panel)
    }

    /// The conflict dialog, over the whole window, while a job waits.
    pub(super) fn render_conflict(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let ask = self.media.ops.read(cx).current_ask()?.clone();
        let t = self.look.theme;
        let apply_all = self.apply_all;
        let choice =
            |id: &'static str, label: &'static str, resolution: Resolution, primary: bool| {
                div()
                    .id(id)
                    .px(px(space(3)))
                    .py(px(space(2)))
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(rgba(t.divider))
                    .when(primary, |d| d.bg(rgba(t.accent)).text_color(rgba(t.base)))
                    .when(!primary, |d| d.hover(|s| s.bg(rgba(t.hover))))
                    .child(label)
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        let all = view.apply_all;
                        view.apply_all = false;
                        view.media
                            .ops
                            .update(cx, |ops, cx| ops.answer(resolution, all, cx));
                    }))
            };
        let card = div()
            .w(px(440.0))
            .p(px(space(5)))
            .flex()
            .flex_col()
            .gap(px(space(3)))
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Já existe um item com esse nome"),
            )
            .child(
                div()
                    .text_color(rgba(t.text_secondary))
                    .child(format!("\"{}\" já existe em {}.", ask.name, ask.folder)),
            )
            .child(
                div()
                    .id("apply-all")
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .child(
                        div()
                            .size(px(16.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(rgba(t.text_secondary))
                            .when(apply_all, |d| d.bg(rgba(t.accent))),
                    )
                    .child("Aplicar a todos os conflitos desta operação")
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.apply_all = !view.apply_all;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(space(2)))
                    .child(choice("skip", "Pular", Resolution::Skip, false))
                    .child(choice("replace", "Substituir", Resolution::Replace, false))
                    .child(choice(
                        "keep-both",
                        "Manter os dois",
                        Resolution::KeepBoth,
                        true,
                    )),
            );
        Some(
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(0x00000066))
                    .child(card),
            )
            .with_priority(2),
        )
    }
}
