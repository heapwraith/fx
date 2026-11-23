//! Operation history panel (T1.19, D-028): every operation of this session
//! that can be undone, newest first, each with its own "Desfazer". Undoing an
//! older one leaves the newer ones alone; the reversal is an ordinary engine
//! job, so it never overwrites anything (SAFETY.md §2.3).

use fx_core::format::date_time;
use fx_core::tr;
use gpui::{ClickEvent, Context, MouseButton, MouseDownEvent, deferred, div, prelude::*, px};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

const PANEL_WIDTH: f32 = 380.0;
const PANEL_MAX_HEIGHT: f32 = 440.0;

impl TabsView {
    /// The history as a screen reader reads it: each operation and its
    /// "Desfazer".
    pub(super) fn history_overlay(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<(crate::a11y::Overlay, Vec<Option<super::WindowAct>>)> {
        use crate::a11y::{Item, Overlay};
        if !self.history_open {
            return None;
        }
        let done = self.media.ops.read(cx).done();
        let mut items = Vec::new();
        let mut acts = Vec::new();
        if done.is_empty() {
            items.push(Item::Text(
                tr!(
                    "Nada para desfazer nesta sessão.",
                    "Nothing to undo in this session."
                )
                .to_owned(),
            ));
            acts.push(None);
        }
        for op in done {
            items.push(Item::Button(fx_core::trf!(
                "Desfazer: {}",
                "Undo: {}",
                op.label
            )));
            acts.push(Some(super::WindowAct::Undo(op.index)));
        }
        let focus = (!acts.is_empty() && acts[0].is_some()).then_some(0);
        Some((
            Overlay {
                title: tr!("Histórico de operações", "Operation history").to_owned(),
                description: Some(tr!("Esc fecha.", "Esc closes.").to_owned()),
                items,
                focus,
                positions: None,
                modal: true,
            },
            acts,
        ))
    }

    pub(super) fn render_history(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.history_open {
            return None;
        }
        let t = self.look.theme;
        let done = self.media.ops.read(cx).done();
        let mut list = div()
            .id("history-list")
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .max_h(px(PANEL_MAX_HEIGHT - 64.0));
        if done.is_empty() {
            list = list.child(
                div()
                    .py(px(space(4)))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!(
                        "Nada para desfazer nesta sessão.",
                        "Nothing to undo in this session."
                    )),
            );
        }
        for op in done {
            let index = op.index;
            let offset = fx_shell::utc_offset_at(op.at);
            let stamp = date_time(op.at, offset);
            // "DD/MM/YYYY HH:MM": today only the time; an operation from an
            // earlier session (T6.6) shows its date too.
            let time = if same_day(op.at, std::time::SystemTime::now(), offset) {
                stamp.get(11..).unwrap_or(&stamp).to_owned()
            } else {
                stamp.clone()
            };
            list = list.child(
                div()
                    .id(("history", index))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .py(px(space(2)))
                    .border_b_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .min_w(px(40.0))
                            .flex_none()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(time),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(op.label))
                    .child(
                        div()
                            .id(("undo", index))
                            .flex_none()
                            .px(px(space(2)))
                            .py(px(space(1)))
                            .rounded(px(RADIUS))
                            .border_1()
                            .border_color(rgba(t.divider))
                            .hover(|s| s.bg(rgba(t.hover)))
                            .child(tr!("Desfazer", "Undo"))
                            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                view.media.ops.update(cx, |ops, cx| ops.undo_at(index, cx));
                            })),
                    ),
            );
        }
        let card = div()
            .id("history-panel")
            .absolute()
            .top(px(space(10)))
            .right(px(space(2)))
            .w(px(PANEL_WIDTH))
            .p(px(space(4)))
            .flex()
            .flex_col()
            .gap(px(space(2)))
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            // Clicks inside do not reach the backdrop (which closes it).
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(tr!("Histórico de operações", "Operation history")),
            )
            .child(
                div()
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!(
                        "Desfazer reverte só aquela operação, sem sobrescrever nada.",
                        "Undo reverts only that operation, without overwriting anything."
                    )),
            )
            .child(list);
        Some(
            deferred(crate::motion::enter(
                "history-in",
                div()
                    .id("history-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.history_open = false;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(1),
        )
    }
}

/// Whether `a` and `b` fall on the same local day (`offset`: seconds from UTC).
fn same_day(a: std::time::SystemTime, b: std::time::SystemTime, offset: i64) -> bool {
    let day = |t: std::time::SystemTime| {
        let secs = t
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        (secs + offset).div_euclid(86_400)
    };
    day(a) == day(b)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn days_follow_the_local_offset() {
        let t = |s| UNIX_EPOCH + Duration::from_secs(s);
        assert!(super::same_day(t(86_400), t(86_400 + 3_600), 0));
        assert!(!super::same_day(t(86_400 - 1), t(86_400), 0));
        // 23:30 UTC is already the next day at UTC+1.
        assert!(super::same_day(
            t(86_400 + 84_600),
            t(2 * 86_400 + 60),
            3_600
        ));
    }
}
