//! Timeline of the disks (T6.2, D-068): what was created, changed, renamed
//! or deleted in the last 10 minutes, hour or day, on every NTFS disk, from
//! the `fx-indexd` service (it follows the USN journal). Ctrl+Shift+L opens
//! it; Tab changes the period; Enter shows the item in its folder; it
//! refreshes by itself every few seconds while open (real changes only).

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fx_core::{tr, trf};
use fx_index::protocol::{ChangeKind, Request};
use gpui::{ClickEvent, Context, Keystroke, SharedString, Window, div, prelude::*, px};

use super::{FolderView, Restore};
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

/// How often the open timeline asks again.
const REFRESH: Duration = Duration::from_secs(3);
/// Most changes listed.
const LIMIT: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Period {
    TenMinutes,
    Hour,
    Today,
}

impl Period {
    fn seconds(self) -> i64 {
        match self {
            Period::TenMinutes => 600,
            Period::Hour => 3_600,
            Period::Today => 86_400,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Period::TenMinutes => tr!("10 minutos", "10 minutes"),
            Period::Hour => tr!("1 hora", "1 hour"),
            Period::Today => tr!("24 horas", "24 hours"),
        }
    }

    fn next(self) -> Self {
        match self {
            Period::TenMinutes => Period::Hour,
            Period::Hour => Period::Today,
            Period::Today => Period::TenMinutes,
        }
    }
}

#[derive(Debug, Clone)]
struct Item {
    path: PathBuf,
    kind: ChangeKind,
    /// Unix seconds.
    at: i64,
}

/// What the service said.
enum State {
    Loading,
    Ready(Vec<Item>),
    /// The service is not running: how to turn it on.
    NoService,
}

pub(super) struct Timeline {
    period: Period,
    state: State,
    cursor: usize,
    /// Bumped at each request; an older answer is dropped.
    ticket: u64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// "agora", "há 5 min", "há 2 h".
fn ago(at: i64, now: i64) -> String {
    let s = (now - at).max(0);
    if s < 60 {
        tr!("agora", "now").to_owned()
    } else if s < 3_600 {
        trf!("há {} min", "{} min ago", s / 60)
    } else {
        trf!("há {} h", "{} h ago", s / 3_600)
    }
}

fn kind_text(kind: ChangeKind) -> (&'static str, &'static str) {
    match kind {
        ChangeKind::Created => ("\u{E710}", tr!("criado", "created")),
        ChangeKind::Modified => ("\u{E70F}", tr!("alterado", "changed")),
        ChangeKind::Deleted => ("\u{E74D}", tr!("apagado", "deleted")),
        ChangeKind::Renamed => ("\u{E8AC}", tr!("renomeado ou movido", "renamed or moved")),
    }
}

impl FolderView {
    /// Ctrl+Shift+L: opens the timeline, or closes it.
    pub(super) fn toggle_timeline(&mut self, cx: &mut Context<Self>) {
        if self.timeline.take().is_some() {
            cx.notify();
            return;
        }
        self.timeline = Some(Timeline {
            period: Period::Hour,
            state: State::Loading,
            cursor: 0,
            ticket: 0,
        });
        self.ask_timeline(cx);
    }

    /// Asks the service (on a thread of its own) and, while the timeline
    /// stays open, asks again after `REFRESH`.
    fn ask_timeline(&mut self, cx: &mut Context<Self>) {
        let Some(t) = self.timeline.as_mut() else {
            return;
        };
        t.ticket += 1;
        let (ticket, since) = (t.ticket, now() - t.period.seconds());
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-timeline".into())
            .spawn(move || {
                let request = Request {
                    limit: LIMIT,
                    recent_since: Some(since),
                    ..Default::default()
                };
                let answer = fx_index::pipe::query(&request, 500).ok().map(|r| {
                    r.hits
                        .into_iter()
                        .filter_map(|h| {
                            Some(Item {
                                // An older service sends `\\.\C:\…`.
                                path: fx_index::recent::plain_path(h.path_buf()),
                                kind: h.change?,
                                at: (h.modified / 10_000_000) as i64 - 11_644_473_600,
                            })
                        })
                        .collect::<Vec<_>>()
                });
                let _ = tx.send(answer);
            });
        cx.spawn(async move |this, cx| {
            let answer = rx.await.ok().flatten();
            let again = this
                .update(cx, |view, cx| {
                    let Some(t) = view.timeline.as_mut().filter(|t| t.ticket == ticket) else {
                        return false;
                    };
                    t.state = match answer {
                        Some(items) => State::Ready(items),
                        None => State::NoService,
                    };
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if again {
                cx.background_executor().timer(REFRESH).await;
                let _ = this.update(cx, |view, cx| {
                    if view.timeline.as_ref().is_some_and(|t| t.ticket == ticket) {
                        view.ask_timeline(cx);
                    }
                });
            }
        })
        .detach();
    }

    fn timeline_items(&self) -> &[Item] {
        match self.timeline.as_ref().map(|t| &t.state) {
            Some(State::Ready(items)) => items,
            _ => &[],
        }
    }

    /// Keys while the timeline is open. Returns whether it took them.
    pub(super) fn timeline_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.timeline.is_none() || ks.modifiers.control || ks.modifiers.alt {
            return false;
        }
        let n = self.timeline_items().len();
        let Some(t) = self.timeline.as_mut() else {
            return false;
        };
        match ks.key.as_str() {
            "escape" => self.timeline = None,
            "tab" => {
                t.period = t.period.next();
                t.cursor = 0;
                t.state = State::Loading;
                self.ask_timeline(cx);
            }
            "down" if n > 0 => t.cursor = (t.cursor + 1).min(n - 1),
            "up" => t.cursor = t.cursor.saturating_sub(1),
            "home" => t.cursor = 0,
            "end" if n > 0 => t.cursor = n - 1,
            "enter" => {
                let k = t.cursor;
                self.show_change(k, window, cx);
            }
            _ => {}
        }
        cx.notify();
        true
    }

    /// Shows a changed item in its folder (a deleted one: its folder).
    fn show_change(&mut self, k: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.timeline_items().get(k).cloned() else {
            return;
        };
        let Some(folder) = item.path.parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let restore = (item.kind != ChangeKind::Deleted)
            .then(|| {
                item.path
                    .file_name()
                    .map(|n| Restore::cursor_on(n.to_os_string()))
            })
            .flatten();
        self.timeline = None;
        self.go_to(folder, restore, window, cx);
    }

    pub(super) fn render_timeline(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let tl = self.timeline.as_ref()?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let now = now();
        let mut list = div()
            .id("timeline-list")
            .track_scroll(&self.timeline_scroll.handle)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(space(2)));
        match &tl.state {
            State::Loading => {
                list = list.child(
                    div()
                        .p(px(space(4)))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!("Lendo…", "Reading…")),
                );
            }
            State::NoService => {
                list = list.child(div().p(px(space(4))).text_color(rgba(t.text_secondary)).child(tr!(
                    "A linha do tempo vem do serviço de busca no disco inteiro, que não está rodando. Configurações › Sistema › Busca no disco inteiro › Ativar.",
                    "The timeline comes from the whole-disk search service, which is not running. Settings › System › Whole-disk search › Turn on."
                )));
            }
            State::Ready(items) if items.is_empty() => {
                list = list.child(div().p(px(space(4))).text_color(rgba(t.text_secondary)).child(trf!(
                    "Nada mudou nos discos nos últimos {}. Tab escolhe outro período.",
                    "Nothing changed on the disks in the last {}. Tab picks another period.",
                    tl.period.label()
                )));
            }
            State::Ready(items) => {
                for (k, item) in items.iter().enumerate() {
                    let (glyph, what) = kind_text(item.kind);
                    let name = item
                        .path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let folder = item
                        .path
                        .parent()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    list = list.child(
                        div()
                            .id(("timeline", k))
                            .h(px(40.0))
                            .px(px(space(2)))
                            .flex()
                            .items_center()
                            .gap(px(space(3)))
                            .rounded(px(RADIUS))
                            .cursor_pointer()
                            .when(k == tl.cursor, |d| d.bg(rgba(t.selected)))
                            .when(k != tl.cursor, |d| d.hover(|h| h.bg(rgba(t.hover))))
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(icon_font)
                                    .text_size(px(CAPTION_SIZE))
                                    .child(glyph),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().truncate().child(SharedString::from(name)))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(CAPTION_SIZE))
                                            .text_color(rgba(t.text_secondary))
                                            .child(SharedString::from(folder)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(CAPTION_SIZE))
                                    .text_color(rgba(t.text_secondary))
                                    .child(format!("{what} · {}", ago(item.at, now))),
                            )
                            .on_click(cx.listener(move |view, ev: &ClickEvent, window, cx| {
                                if let Some(t) = view.timeline.as_mut() {
                                    t.cursor = k;
                                }
                                if ev.click_count() >= 2 {
                                    view.show_change(k, window, cx);
                                }
                                cx.notify();
                            })),
                    );
                }
            }
        }
        let chips = [Period::TenMinutes, Period::Hour, Period::Today].map(|p| {
            div()
                .px(px(space(2)))
                .py(px(2.0))
                .rounded(px(RADIUS))
                .text_size(px(CAPTION_SIZE))
                .when(p == tl.period, |d| {
                    d.bg(rgba(t.selected)).text_color(rgba(t.text))
                })
                .when(p != tl.period, |d| d.text_color(rgba(t.text_secondary)))
                .child(p.label())
        });
        Some(
            div()
                .id("timeline")
                .absolute()
                .top(px(space(12)))
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .bg(rgba(t.base))
                .border_t_1()
                .border_color(rgba(t.divider))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(2)))
                        .flex()
                        .items_center()
                        .gap(px(space(2)))
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(tr!("Linha do tempo", "Timeline")))
                        .children(chips)
                        .child(div().flex_1())
                        .child(div().text_size(px(CAPTION_SIZE)).text_color(rgba(t.text_secondary)).child(match &tl.state {
                            State::Ready(items) => fx_core::format::items(items.len()),
                            _ => String::new(),
                        })),
                )
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(list)
                        .child(crate::scrollbar::vertical(&self.timeline_scroll, &t)),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!(
                            "Tab muda o período · Enter mostra na pasta · atualiza sozinha · Esc fecha",
                            "Tab changes the period · Enter shows in folder · refreshes by itself · Esc closes"
                        )),
                ),
        )
    }

    /// The timeline as a screen reader reads it.
    pub(super) fn timeline_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item as A11yItem, Overlay};
        let tl = self.timeline.as_ref()?;
        let now = now();
        let items: Vec<A11yItem> = self
            .timeline_items()
            .iter()
            .enumerate()
            .map(|(k, item)| A11yItem::Choice {
                label: item
                    .path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
                description: Some(format!(
                    "{}, {}, {}",
                    kind_text(item.kind).1,
                    ago(item.at, now),
                    item.path.display()
                )),
                selected: k == tl.cursor,
            })
            .collect();
        let focus = (!items.is_empty()).then(|| tl.cursor.min(items.len() - 1));
        Some(Overlay {
            title: trf!("Linha do tempo: últimos {}", "Timeline: last {}", tl.period.label()),
            description: Some(
                tr!(
                    "O que mudou nos discos. Tab muda o período, Enter mostra na pasta, Esc fecha.",
                    "What changed on the disks. Tab changes the period, Enter shows in folder, Esc closes."
                )
                .to_owned(),
            ),
            items,
            focus,
            positions: None,
            modal: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods_cycle_and_times_read_well() {
        assert_eq!(Period::TenMinutes.next(), Period::Hour);
        assert_eq!(Period::Today.next(), Period::TenMinutes);
        assert_eq!(Period::Hour.seconds(), 3_600);
        let now = 10_000;
        assert!(ago(now - 10, now) == "agora" || ago(now - 10, now) == "now");
        assert!(ago(now - 300, now).contains('5'));
        assert!(ago(now - 7_300, now).contains('2'));
    }
}
