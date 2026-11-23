//! Comparing the two panes (T6.11, D-068): what exists on one side only and
//! which files differ, below both folders (listed on a thread, compared by
//! `fx_core::compare`). Space marks; → copies the marked items to the right
//! pane's folder, ← to the left one, through the operations engine: a
//! conflict asks (replace, skip, keep both), nothing is overwritten unasked,
//! and it can be undone. From the palette: "Comparar com o outro painel".

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use fx_core::compare::{Diff, Difference, compare};
use fx_core::{tr, trf};
use fx_ops::{Job, Resolution};
use gpui::{
    Context, KeyDownEvent, MouseButton, SharedString, Window, deferred, div, prelude::*, px,
};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

pub(super) struct Compare {
    left: PathBuf,
    right: PathBuf,
    /// `None` while listing; the bool says a tree was too big to list whole.
    found: Option<(Vec<Difference>, bool)>,
    cursor: usize,
    marked: HashSet<usize>,
    cancel: Arc<AtomicBool>,
}

fn diff_text(d: Diff) -> &'static str {
    match d {
        Diff::OnlyLeft => tr!("só à esquerda", "left only"),
        Diff::OnlyRight => tr!("só à direita", "right only"),
        Diff::LeftNewer => tr!("mais novo à esquerda", "newer on the left"),
        Diff::RightNewer => tr!("mais novo à direita", "newer on the right"),
        Diff::SizeDiffers => tr!("tamanhos diferentes", "sizes differ"),
    }
}

impl TabsView {
    /// Palette: compares the active pane's folder with the other pane's.
    pub(super) fn start_compare(&mut self, cx: &mut Context<Self>) {
        let Some(other) = self.other() else {
            let text = tr!(
                "Abra o painel duplo (F3) com as duas pastas para comparar",
                "Open the dual pane (F3) with both folders to compare"
            );
            self.current()
                .update(cx, |v, cx| v.show_notice(text.to_owned(), cx));
            return;
        };
        let left = self.current().read(cx).path().to_path_buf();
        let right = other.read(cx).path().to_path_buf();
        let cancel = Arc::new(AtomicBool::new(false));
        self.compare = Some(Compare {
            left: left.clone(),
            right: right.clone(),
            found: None,
            cursor: 0,
            marked: HashSet::new(),
            cancel: cancel.clone(),
        });
        let (tx, rx) = futures::channel::oneshot::channel();
        let _ = std::thread::Builder::new()
            .name("fx-compare".into())
            .spawn(move || {
                let (l, cut_l) = fx_fs::tree::tree_items(&left, &cancel);
                let (r, cut_r) = fx_fs::tree::tree_items(&right, &cancel);
                let _ = tx.send((compare(&l, &r), cut_l || cut_r));
            });
        cx.spawn(async move |this, cx| {
            if let Ok(found) = rx.await {
                let _ = this.update(cx, |view, cx| {
                    if let Some(c) = view.compare.as_mut() {
                        c.found = Some(found);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    /// Keys while comparing (all of them stay here).
    pub(super) fn compare_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        let n = c.found.as_ref().map_or(0, |(d, _)| d.len());
        match event.keystroke.key.as_str() {
            "escape" => {
                c.cancel.store(true, Ordering::Relaxed);
                self.compare = None;
            }
            "down" if n > 0 => c.cursor = (c.cursor + 1).min(n - 1),
            "up" => c.cursor = c.cursor.saturating_sub(1),
            "space" if n > 0 && !event.is_held => {
                if !c.marked.remove(&c.cursor) {
                    c.marked.insert(c.cursor);
                }
            }
            "right" => self.copy_differences(true, window, cx),
            "left" => self.copy_differences(false, window, cx),
            _ => {}
        }
        cx.notify();
    }

    /// Copies the marked differences (or the one under the cursor) to the
    /// right side (`to_right`) or the left, one job per destination folder.
    fn copy_differences(&mut self, to_right: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.compare.as_mut() else {
            return;
        };
        let Some((diffs, _)) = c.found.as_ref() else {
            return;
        };
        let picked: Vec<usize> = if c.marked.is_empty() {
            vec![c.cursor]
        } else {
            let mut v: Vec<usize> = c.marked.iter().copied().collect();
            v.sort_unstable();
            v
        };
        let (from, to) = if to_right {
            (&c.left, &c.right)
        } else {
            (&c.right, &c.left)
        };
        let mut by_dest: BTreeMap<PathBuf, Vec<PathBuf>> = BTreeMap::new();
        for k in picked {
            let Some(d) = diffs.get(k) else { continue };
            // Only what exists on the side it is copied from.
            let exists_there = match d.diff {
                Diff::OnlyLeft => to_right,
                Diff::OnlyRight => !to_right,
                _ => true,
            };
            if !exists_there {
                continue;
            }
            let dest = to.join(d.rel.parent().unwrap_or(std::path::Path::new("")));
            by_dest.entry(dest).or_default().push(from.join(&d.rel));
        }
        c.marked.clear();
        let tab = self.current();
        for (dest, sources) in by_dest {
            let job = Job::copy(sources, dest).on_conflict(Resolution::Ask);
            tab.update(cx, |view, cx| view.queue_job(job, window, cx));
        }
        // The comparison is done again once the copies are queued.
        self.start_compare(cx);
    }

    pub(super) fn render_compare(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let c = self.compare.as_ref()?;
        let t = self.look.theme;
        let mut list = div()
            .id("compare-list")
            .track_scroll(&self.compare_scroll.handle)
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(space(2)));
        let summary = match &c.found {
            _ if c.left == c.right => tr!(
                "É a mesma pasta dos dois lados: abra outra no outro painel",
                "It is the same folder on both sides: open another one in the other pane"
            )
            .to_owned(),
            None => tr!("Comparando…", "Comparing…").to_owned(),
            Some((d, _)) if d.is_empty() => {
                tr!("As duas pastas são iguais", "Both folders are the same").to_owned()
            }
            Some((d, cut)) => {
                let base = fx_core::format::items(d.len());
                if *cut {
                    trf!(
                        "{base} (pasta grande: só os primeiros 200 mil itens)",
                        "{base} (large folder: first 200 thousand items only)"
                    )
                } else {
                    base
                }
            }
        };
        // Nothing to list (same folder, equal, still comparing): the
        // sentence goes in the middle, as an empty state, not on top of an
        // empty box.
        let empty = c.left == c.right || c.found.as_ref().is_none_or(|(d, _)| d.is_empty());
        if empty {
            list = list.flex().items_center().justify_center().child(
                div()
                    .max_w(px(420.0))
                    .text_center()
                    .text_color(rgba(t.text_secondary))
                    .child(summary.clone()),
            );
        }
        if let Some((diffs, _)) = &c.found {
            for (k, d) in diffs.iter().enumerate() {
                let marked = c.marked.contains(&k);
                list = list.child(
                    div()
                        .id(("diff", k))
                        .h(px(30.0))
                        .px(px(space(2)))
                        .flex()
                        .items_center()
                        .gap(px(space(3)))
                        .rounded(px(RADIUS))
                        .when(k == c.cursor, |x| x.bg(rgba(t.selected)))
                        .child(
                            div()
                                .flex_none()
                                .w(px(14.0))
                                .child(if marked { "•" } else { "" }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(SharedString::from(d.rel.display().to_string())),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .child(diff_text(d.diff)),
                        ),
                );
            }
        }
        let card = div()
            .id("compare")
            .w(px(720.0))
            .max_w(gpui::relative(0.94))
            .h(px(520.0))
            .max_h(gpui::relative(0.9))
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .p(px(space(4)))
                    .border_b_1()
                    .border_color(rgba(t.divider))
                    .flex()
                    .flex_col()
                    .gap(px(space(1)))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(tr!("Comparar pastas", "Compare folders")))
                    .child(div().text_size(px(CAPTION_SIZE)).text_color(rgba(t.text_secondary)).truncate().child(format!("← {}", c.left.display())))
                    .child(div().text_size(px(CAPTION_SIZE)).text_color(rgba(t.text_secondary)).truncate().child(format!("→ {}", c.right.display())))
                    .when(!empty, |d| d.child(div().text_size(px(CAPTION_SIZE)).child(summary))),
            )
            .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(list)
                        .child(crate::scrollbar::vertical(&self.compare_scroll, &t)),
                )
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!(
                        "Espaço marca · → copia para a direita · ← para a esquerda (conflitos perguntam, com desfazer) · Esc fecha",
                        "Space marks · → copies to the right · ← to the left (conflicts ask, undoable) · Esc closes"
                    )),
            );
        Some(
            deferred(
                div()
                    .id("compare-backdrop")
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(crate::theme::scrim(&t)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            if let Some(c) = view.compare.take() {
                                c.cancel.store(true, Ordering::Relaxed);
                            }
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
            .with_priority(2),
        )
    }

    /// The comparison as a screen reader reads it.
    pub(super) fn compare_overlay(
        &self,
    ) -> Option<(crate::a11y::Overlay, Vec<Option<super::WindowAct>>)> {
        use crate::a11y::{Item, Overlay};
        let c = self.compare.as_ref()?;
        let items: Vec<Item> = c
            .found
            .as_ref()
            .map(|(d, _)| {
                d.iter()
                    .enumerate()
                    .map(|(k, d)| Item::Choice {
                        label: d.rel.display().to_string(),
                        description: Some(diff_text(d.diff).to_owned()),
                        selected: k == c.cursor,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let acts = items.iter().map(|_| None).collect();
        let focus = (!items.is_empty()).then(|| c.cursor.min(items.len() - 1));
        Some((
            Overlay {
                title: tr!("Comparar pastas", "Compare folders").to_owned(),
                description: Some(trf!(
                    "Esquerda: {}. Direita: {}. Espaço marca, seta para a direita copia para a direita, para a esquerda copia para a esquerda, Esc fecha.",
                    "Left: {}. Right: {}. Space marks, right arrow copies to the right, left arrow to the left, Esc closes.",
                    c.left.display(),
                    c.right.display()
                )),
                items,
                focus,
                positions: None,
                modal: true,
            },
            acts,
        ))
    }
}
