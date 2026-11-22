//! Batch rename panel (task T2.8): F2 with several items selected.
//!
//! Three fields (Tab moves between them): Buscar, Substituir por and Modelo
//! (`{nome}`, `{n}`, `{n:3}`). Alt+R turns the search into a regular
//! expression, Alt+M makes it case-sensitive, Alt+C cycles the letter case.
//! The preview shows every old → new name live, with what is wrong (invalid,
//! repeated, already in the folder); Enter applies only when nothing is
//! wrong, as one operation that a single Ctrl+Z reverts (`Job::batch_rename`).

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::collections::HashSet;
use std::sync::Arc;

use fx_core::batch_rename::{Planned, Rules, plan};
use fx_ops::Job;
use gpui::{
    Context, Keystroke, MouseButton, SharedString, UniformListScrollHandle, Window, deferred, div,
    prelude::*, px, uniform_list,
};

use super::{FolderView, rows_of};
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

const PANEL_WIDTH: f32 = 720.0;
const ROW_HEIGHT: f32 = 26.0;
const FIELDS: usize = 3;

fn field_label(k: usize) -> &'static str {
    match k {
        0 => tr!("Buscar", "Search"),
        1 => tr!("Substituir por", "Replace with"),
        _ => tr!("Modelo", "Template"),
    }
}

/// One preview line: old name, new name, problem, whether it changes.
type PreviewRow = (SharedString, SharedString, Option<&'static str>, bool);

pub(super) struct BatchRename {
    entries: Vec<u32>,
    names: Vec<String>,
    /// Lowercase names of the folder's other items.
    others: HashSet<String>,
    rules: Rules,
    field: usize,
    plan: Result<Vec<Planned>, String>,
    scroll: UniformListScrollHandle,
}

impl BatchRename {
    fn field_mut(&mut self) -> &mut String {
        match self.field {
            0 => &mut self.rules.find,
            1 => &mut self.rules.replace,
            _ => &mut self.rules.template,
        }
    }

    fn field_text(&self, k: usize) -> &str {
        match k {
            0 => &self.rules.find,
            1 => &self.rules.replace,
            _ => &self.rules.template,
        }
    }

    fn replan(&mut self) {
        let others = &self.others;
        self.plan = plan(&self.names, &self.rules, &|n: &str| {
            others.contains(&n.to_lowercase())
        });
    }

    /// (names that change, names with a problem).
    fn counts(&self) -> (usize, usize) {
        match &self.plan {
            Ok(p) => (
                p.iter().filter(|p| p.changes()).count(),
                p.iter().filter(|p| p.problem.is_some()).count(),
            ),
            Err(_) => (0, 0),
        }
    }
}

impl FolderView {
    /// F2 with more than one visible item selected. Returns whether it opened.
    pub(super) fn open_batch_rename(&mut self) -> bool {
        let rows = rows_of(&self.filtered, &self.list);
        let entries: Vec<u32> = self.selection.selected_in(rows).collect();
        if entries.len() < 2 {
            return false;
        }
        let names: Vec<String> = entries
            .iter()
            .map(|&i| self.list.name(i).to_owned())
            .collect();
        let mine: HashSet<String> = names.iter().map(|n| n.to_lowercase()).collect();
        // Every item of the folder counts, hidden ones too.
        let others = (0..self.list.len() as u32)
            .map(|i| self.list.name(i).to_lowercase())
            .filter(|n| !mine.contains(n))
            .collect();
        let mut b = BatchRename {
            entries,
            names,
            others,
            rules: Rules::default(),
            field: 0,
            plan: Ok(Vec::new()),
            scroll: UniformListScrollHandle::new(),
        };
        b.replan();
        self.batch_rename = Some(b);
        true
    }

    /// Keys while the panel is open (all of them stay here).
    pub(super) fn batch_rename_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(b) = self.batch_rename.as_mut() else {
            return false;
        };
        let m = ks.modifiers;
        match ks.key.as_str() {
            "escape" => self.batch_rename = None,
            "enter" => self.apply_batch_rename(window, cx),
            "tab" => {
                b.field = if m.shift {
                    (b.field + FIELDS - 1) % FIELDS
                } else {
                    (b.field + 1) % FIELDS
                };
            }
            "backspace" if m.control => {
                b.field_mut().clear();
                b.replan();
            }
            "backspace" => {
                b.field_mut().pop();
                b.replan();
            }
            "r" if m.alt => {
                b.rules.regex = !b.rules.regex;
                b.replan();
            }
            "m" if m.alt => {
                b.rules.case_sensitive = !b.rules.case_sensitive;
                b.replan();
            }
            "c" if m.alt => {
                b.rules.case = b.rules.case.next();
                b.replan();
            }
            "space" if !m.control && !m.alt => {
                b.field_mut().push(' ');
                b.replan();
            }
            _ => {
                if !(m.control || m.alt || m.platform || m.function)
                    && let Some(c) = ks.key_char.as_deref()
                    && !c.chars().any(char::is_control)
                {
                    b.field_mut().push_str(c);
                    b.replan();
                }
            }
        }
        cx.notify();
        true
    }

    /// Enter: queues the renames when nothing is wrong.
    fn apply_batch_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.batch_rename.as_ref() else {
            return;
        };
        let (changes, problems) = b.counts();
        let Ok(planned) = &b.plan else { return };
        if changes == 0 || problems > 0 {
            return;
        }
        let pairs: Vec<_> = b
            .entries
            .iter()
            .zip(planned)
            .filter(|(_, p)| p.changes())
            .map(|(&i, p)| (self.path.join(self.list.os_name(i)), p.new.clone()))
            .collect();
        self.batch_rename = None;
        self.submit_job(Job::batch_rename(pairs), window, cx);
    }

    pub(super) fn render_batch_rename(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let b = self.batch_rename.as_ref()?;
        let t = self.look.theme;
        let (changes, problems) = b.counts();
        let field = |k: usize| {
            let focused = b.field == k;
            let text = b.field_text(k);
            div()
                .flex()
                .items_center()
                .gap(px(space(3)))
                .child(
                    div()
                        .flex_none()
                        .w(px(110.0))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(field_label(k)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(28.0))
                        .px(px(space(2)))
                        .flex()
                        .items_center()
                        .rounded(px(RADIUS))
                        .border_1()
                        .border_color(rgba(if focused { t.accent } else { t.divider }))
                        .bg(rgba(t.layer))
                        .truncate()
                        .child(if focused {
                            format!("{text}|")
                        } else {
                            text.to_owned()
                        }),
                )
        };
        let yes_no = |on: bool| {
            if on {
                tr!("sim", "yes")
            } else {
                tr!("não", "no")
            }
        };
        let options = trf!(
            "Alt+R expressão regular: {} · Alt+M diferenciar maiúsculas: {} · Alt+C letras: {}",
            "Alt+R regular expression: {} · Alt+M match case: {} · Alt+C letters: {}",
            yes_no(b.rules.regex),
            yes_no(b.rules.case_sensitive),
            b.rules.case.label()
        );
        let footer = match &b.plan {
            Err(e) => trf!("Expressão inválida: {e}", "Invalid expression: {e}"),
            Ok(_) if problems > 0 => trf!(
                "{} com problema: corrija para aplicar · Esc cancela",
                "{} with a problem: fix them to apply · Esc cancels",
                fx_core::format::items(problems)
            ),
            Ok(_) if changes == 0 => tr!(
                "Nada muda ainda · {nome} é o nome atual, {n:3} numera · Esc cancela",
                "Nothing changes yet · {nome} is the current name, {n:3} numbers · Esc cancels"
            )
            .to_owned(),
            Ok(_) => trf!(
                "Enter renomeia {} (um Ctrl+Z desfaz tudo) · Esc cancela",
                "Enter renames {} (one Ctrl+Z undoes it all) · Esc cancels",
                fx_core::format::items(changes)
            ),
        };
        let rows: Arc<Vec<PreviewRow>> = Arc::new(match &b.plan {
            Ok(p) => p
                .iter()
                .map(|p| {
                    (
                        p.old.clone().into(),
                        p.new.clone().into(),
                        p.problem.map(|x| x.text()),
                        p.changes(),
                    )
                })
                .collect(),
            Err(_) => b
                .names
                .iter()
                .map(|n| (n.clone().into(), n.clone().into(), None, false))
                .collect(),
        });
        let (secondary, text_color) = (t.text_secondary, t.text);
        let list = uniform_list("batch-rename-rows", rows.len(), move |range, _, _| {
            range
                .map(|i| {
                    let (old, new, problem, changes) = rows[i].clone();
                    div()
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(space(3)))
                        .flex()
                        .items_center()
                        .gap(px(space(2)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgba(secondary))
                                .child(old),
                        )
                        .child(div().flex_none().text_color(rgba(secondary)).child("→"))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(rgba(if changes { text_color } else { secondary }))
                                .when(problem.is_some(), |d| d.line_through())
                                .child(new),
                        )
                        .when_some(problem, |d, why| {
                            d.child(
                                div()
                                    .flex_none()
                                    .text_size(px(CAPTION_SIZE))
                                    .text_color(rgba(secondary))
                                    .child(format!("⚠ {why}")),
                            )
                        })
                })
                .collect()
        })
        .track_scroll(b.scroll.clone())
        .flex_1();
        let card = div()
            .id("batch-rename")
            .absolute()
            .top(px(space(12)))
            .left(px(space(4)))
            .right(px(space(4)))
            .mx_auto()
            .max_w(px(PANEL_WIDTH))
            .h(px(480.0))
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .pt(px(space(3)))
                    .pb(px(space(2)))
                    .flex()
                    .flex_col()
                    .gap(px(space(2)))
                    .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(trf!(
                        "Renomear {}",
                        "Rename {}",
                        fx_core::format::items(b.names.len())
                    )))
                    .child(field(0))
                    .child(field(1))
                    .child(field(2))
                    .child(
                        div()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .truncate()
                            .child(options),
                    ),
            )
            .child(div().flex_none().h(px(1.0)).bg(rgba(t.divider)))
            .child(list)
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .truncate()
                    .child(footer),
            );
        Some(
            deferred(crate::motion::enter(
                "batch-rename-in",
                div()
                    .id("batch-rename-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _, _, cx| {
                            view.batch_rename = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(1),
        )
    }
}
