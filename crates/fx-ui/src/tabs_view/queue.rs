//! Queue panel and conflict dialog of a window (task T1.13).
//!
//! The panel sits at the bottom while copies or moves run: label, progress
//! bar, bytes done, pause/resume and cancel. A conflict shows one dialog over
//! the window: keep both (the default, SAFETY.md §2.3), replace or skip, with
//! "apply to all" for the rest of that job.

use fx_core::format::size_short;
#[allow(unused_imports)]
use fx_core::{tr, trf};
use fx_ops::Resolution;
use gpui::{ClickEvent, Context, SharedString, deferred, div, prelude::*, px, relative};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, ICON_CLOSE, ICON_PAUSE, ICON_PLAY, RADIUS, rgba, space};

impl TabsView {
    /// The queue panel, or nothing when no copy or move runs.
    pub(super) fn render_queue(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let ops = self.media.ops.read(cx);
        if ops.active().is_empty() && ops.recent().is_empty() {
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
        for job in ops.active().iter().chain(ops.recent()) {
            let id = job.id;
            let paused = job.paused;
            let finished = job.finished;
            let status = progress_text(job);
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
                                        div()
                                            .flex_none()
                                            .text_color(rgba(t.text_secondary))
                                            .child(status),
                                    ),
                            )
                            .child(
                                div()
                                    .h(px(6.0))
                                    .rounded(px(3.0))
                                    // A track that shows in both themes.
                                    .bg(rgba((t.text_secondary & 0xFFFF_FF00) | 0x33))
                                    .child(
                                        div()
                                            .h_full()
                                            .w(relative(fraction))
                                            .rounded(px(3.0))
                                            .bg(rgba(t.accent)),
                                    ),
                            ),
                    )
                    .when(!finished, |d| {
                        d.child(
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
                        .child(
                            button(format!("cancel-{id}").into(), ICON_CLOSE).on_click(
                                cx.listener(move |view, _: &ClickEvent, _, cx| {
                                    view.media.ops.update(cx, |ops, cx| ops.cancel(id, cx));
                                }),
                            ),
                        )
                    }),
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
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(tr!(
                "Já existe um item com esse nome",
                "An item with that name already exists"
            )))
            .child(div().text_color(rgba(t.text_secondary)).child(trf!(
                "\"{}\" já existe em {}.",
                "\"{}\" already exists in {}.",
                ask.name,
                ask.folder
            )))
            .children(
                compare(ask.existing, ask.incoming)
                    .into_iter()
                    .map(|(label, text)| {
                        div()
                            .flex()
                            .gap(px(space(2)))
                            .child(
                                div()
                                    .w(px(72.0))
                                    .flex_none()
                                    .text_color(rgba(t.text_secondary))
                                    .child(label),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(text))
                    }),
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
                    .child(tr!(
                        "Aplicar a todos os conflitos desta operação",
                        "Apply to all conflicts in this operation"
                    ))
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
                    .child(choice(
                        "skip",
                        tr!("Pular", "Skip"),
                        Resolution::Skip,
                        false,
                    ))
                    .child(choice(
                        "replace",
                        tr!("Substituir", "Replace"),
                        Resolution::Replace,
                        false,
                    ))
                    .child(choice(
                        "keep-both",
                        tr!("Manter os dois", "Keep both"),
                        Resolution::KeepBoth,
                        true,
                    )),
            )
            .child(
                div()
                    .text_size(px(crate::theme::CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!(
                        "Enter mantém os dois · Esc pula · Espaço marca \"aplicar a todos\"",
                        "Enter keeps both · Esc skips · Space checks \"apply to all\""
                    )),
            );
        Some(
            deferred(crate::motion::enter(
                "conflict-in",
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
            ))
            .with_priority(2),
        )
    }
}

/// The two sides of a conflict, one line each: size, date and which one is
/// newer, so "Substituir" is not a blind choice.
fn compare(
    existing: Option<fx_ops::ItemInfo>,
    incoming: Option<fx_ops::ItemInfo>,
) -> Vec<(&'static str, String)> {
    let newer = match (
        existing.and_then(|i| i.modified),
        incoming.and_then(|i| i.modified),
    ) {
        (Some(a), Some(b)) if a > b => Some(false),
        (Some(a), Some(b)) if b > a => Some(true),
        _ => None,
    };
    let line = |info: Option<fx_ops::ItemInfo>, is_newer: bool| {
        let Some(info) = info else {
            return tr!("não foi possível ler", "could not read").to_owned();
        };
        let mut parts = Vec::new();
        parts.push(if info.is_dir {
            "pasta".to_owned()
        } else {
            fx_core::format::size_short(info.bytes)
        });
        if let Some(m) = info.modified {
            parts.push(fx_core::format::date_time(m, fx_shell::utc_offset_at(m)));
        }
        if is_newer {
            parts.push(tr!("mais recente", "most recent").to_owned());
        }
        parts.join(" · ")
    };
    if existing.is_none() && incoming.is_none() {
        return Vec::new();
    }
    vec![
        (
            tr!("Existente", "Existing"),
            line(existing, newer == Some(false)),
        ),
        (tr!("Novo", "New"), line(incoming, newer == Some(true))),
    ]
}

/// "1,0 GB de 3,0 GB · 33% · 850 MB/s · 3 s restantes", or "Concluído".
fn progress_text(job: &crate::ops::JobView) -> String {
    if job.finished {
        return trf!("Concluído · {}", "Done · {}", size_short(job.bytes));
    }
    let paused = if job.paused {
        tr!("pausado · ", "paused · ")
    } else {
        ""
    };
    let percent = (job.done * 100).checked_div(job.bytes).unwrap_or(0);
    let mut text = trf!(
        "{paused}{} de {} · {percent}%",
        "{paused}{} of {} · {percent}%",
        size_short(job.done),
        size_short(job.bytes)
    );
    let secs = job.started.elapsed().as_secs_f64();
    if !job.paused && secs > 0.5 && job.done > 0 {
        let speed = job.done as f64 / secs;
        let left = (job.bytes.saturating_sub(job.done) as f64 / speed).ceil() as u64;
        let speed = size_short(speed as u64);
        text.push_str(&match left {
            0..=59 => trf!(
                " · {speed}/s · {left} s restantes",
                " · {speed}/s · {left} s left"
            ),
            _ => {
                let min = left.div_ceil(60);
                trf!(
                    " · {speed}/s · {min} min restantes",
                    " · {speed}/s · {min} min left"
                )
            }
        });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::compare;
    use fx_ops::ItemInfo;
    use std::time::{Duration, SystemTime};

    #[test]
    fn compare_marks_the_newer_side() {
        let old = ItemInfo {
            is_dir: false,
            bytes: 1536,
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000)),
        };
        let new = ItemInfo {
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            ..old
        };
        let lines = compare(Some(old), Some(new));
        assert_eq!(lines[0].0, "Existente");
        assert!(lines[0].1.starts_with("1,5 KB · ") && !lines[0].1.contains("recente"));
        assert!(lines[1].1.ends_with("mais recente"));
        assert!(compare(None, None).is_empty());
        assert_eq!(compare(None, Some(new))[0].1, "não foi possível ler");
    }
}
