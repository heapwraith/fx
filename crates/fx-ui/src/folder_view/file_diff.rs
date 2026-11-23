//! Comparing files (T6.17): "Comparar arquivos" in the palette, on the
//! selected files. Two texts show side by side, line by line (Myers,
//! `fx_core::diff`), with − + ~ in the margin (shape, not only color); N
//! and P (or F8 and Shift+F8) jump between blocks of changes. Two other
//! files say whether they are identical or where the first different byte
//! is (and the dimensions, for images). Three or more are grouped by
//! identical content. Everything is read on a thread; nothing is changed.

use std::path::PathBuf;

use futures::StreamExt;
use fx_core::diff::{self, Row, RowKind};
use fx_core::format::size_short;
use fx_core::{tr, trf};
use gpui::{
    Context, Keystroke, ScrollStrategy, SharedString, UniformListScrollHandle, div, prelude::*, px,
    uniform_list,
};

use super::FolderView;
use crate::theme::{CAPTION_SIZE, rgba, space};

/// Texts larger than this are compared as bytes only.
const TEXT_MAX: u64 = 8 << 20;
/// Characters of a line drawn.
const LINE_MAX: usize = 2_000;
const ROW_HEIGHT: f32 = 20.0;

pub(super) enum Outcome {
    Text {
        left: Vec<String>,
        right: Vec<String>,
        rows: Vec<Row>,
        starts: Vec<usize>,
    },
    Identical(u64),
    TooDifferent(usize, usize),
    Bytes {
        first: u64,
        sizes: (u64, u64),
        dims: (Option<String>, Option<String>),
    },
    /// Names grouped by identical content (3 or more files).
    Groups(Vec<Vec<String>>),
}

pub(super) struct FileDiff {
    names: Vec<String>,
    state: Option<Result<Outcome, String>>,
    scroll: UniformListScrollHandle,
    /// Top row shown (text) or block of changes reached.
    row: usize,
}

fn name_of(p: &std::path::Path) -> String {
    p.file_name().map_or_else(
        || p.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The comparison (worker thread).
fn compare(paths: &[PathBuf]) -> Result<Outcome, String> {
    let err = |e: std::io::Error| e.to_string();
    if paths.len() >= 3 {
        let mut groups: Vec<(u64, Vec<&PathBuf>)> = Vec::new();
        for p in paths {
            let size = std::fs::metadata(p).map_err(err)?.len();
            let mut placed = false;
            for (s, g) in &mut groups {
                if *s == size && fx_fs::duplicates::same_content(g[0], p).map_err(err)? {
                    g.push(p);
                    placed = true;
                    break;
                }
            }
            if !placed {
                groups.push((size, vec![p]));
            }
        }
        return Ok(Outcome::Groups(
            groups
                .into_iter()
                .map(|(_, g)| g.into_iter().map(|p| name_of(p)).collect())
                .collect(),
        ));
    }
    let (a, b) = (&paths[0], &paths[1]);
    let sizes = (
        std::fs::metadata(a).map_err(err)?.len(),
        std::fs::metadata(b).map_err(err)?.len(),
    );
    let Some(first) = fx_fs::duplicates::first_difference(a, b).map_err(err)? else {
        return Ok(Outcome::Identical(sizes.0));
    };
    if sizes.0 <= TEXT_MAX && sizes.1 <= TEXT_MAX {
        let (ba, bb) = (
            std::fs::read(a).map_err(err)?,
            std::fs::read(b).map_err(err)?,
        );
        if let (Some(ta), Some(tb)) = (diff::decode(&ba), diff::decode(&bb)) {
            let (la, lb) = (diff::lines(&ta), diff::lines(&tb));
            let Some(ops) = diff::diff(&la, &lb, diff::MAX_EDITS) else {
                return Ok(Outcome::TooDifferent(la.len(), lb.len()));
            };
            let rows = diff::side_by_side(&ops);
            let starts = diff::change_starts(&rows);
            let shown = |l: &&str| {
                let l = l.replace('\t', "    ");
                match l.char_indices().nth(LINE_MAX) {
                    Some((cut, _)) => format!("{}…", &l[..cut]),
                    None => l,
                }
            };
            return Ok(Outcome::Text {
                left: la.iter().map(shown).collect(),
                right: lb.iter().map(shown).collect(),
                rows,
                starts,
            });
        }
    }
    let dim = |p: &PathBuf| {
        fx_shell::property_values(p, &["System.Image.Dimensions"])
            .ok()
            .and_then(|v| v.into_iter().next().flatten())
    };
    Ok(Outcome::Bytes {
        first,
        sizes,
        dims: (dim(a), dim(b)),
    })
}

impl FolderView {
    /// "Comparar arquivos" (palette): the selected files.
    pub fn open_file_diff(&mut self, cx: &mut Context<Self>) {
        let files: Vec<PathBuf> = self
            .selected_paths()
            .into_iter()
            .filter(|p| !p.is_dir())
            .collect();
        self.file_diff_of(files, cx);
    }

    pub(super) fn file_diff_of(&mut self, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        if files.len() < 2 || self.archive.is_some() {
            self.show_notice(
                tr!(
                    "Selecione dois ou mais arquivos para comparar",
                    "Select two or more files to compare"
                )
                .to_owned(),
                cx,
            );
            return;
        }
        self.file_diff = Some(FileDiff {
            names: files.iter().map(|p| name_of(p)).collect(),
            state: None,
            scroll: UniformListScrollHandle::new(),
            row: 0,
        });
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Result<Outcome, String>>();
        let _ = std::thread::Builder::new()
            .name("fx-diff".into())
            .spawn(move || {
                let _ = tx.unbounded_send(compare(&files));
            });
        cx.spawn(async move |this, cx| {
            if let Some(outcome) = rx.next().await {
                let _ = this.update(cx, |view, cx| {
                    if let Some(d) = view.file_diff.as_mut() {
                        d.state = Some(outcome);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }

    /// Keys while the comparison is open. Returns whether it took them.
    pub(super) fn file_diff_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(d) = self.file_diff.as_mut() else {
            return false;
        };
        if ks.modifiers.control || ks.modifiers.alt {
            return false;
        }
        let (rows, starts) = match &d.state {
            Some(Ok(Outcome::Text { rows, starts, .. })) => (rows.len(), starts.clone()),
            _ => (0, Vec::new()),
        };
        let last = rows.saturating_sub(1);
        let shift = ks.modifiers.shift;
        match ks.key.as_str() {
            "escape" => {
                self.file_diff = None;
                cx.notify();
                return true;
            }
            "down" => d.row = (d.row + 1).min(last),
            "up" => d.row = d.row.saturating_sub(1),
            "pagedown" => d.row = (d.row + 20).min(last),
            "pageup" => d.row = d.row.saturating_sub(20),
            "home" => d.row = 0,
            "end" => d.row = last,
            "n" | "f8" if !shift || ks.key == "n" => {
                if let Some(&s) = starts.iter().find(|&&s| s > d.row) {
                    d.row = s;
                }
            }
            "p" | "f8" => {
                if let Some(&s) = starts.iter().rev().find(|&&s| s < d.row) {
                    d.row = s;
                }
            }
            _ => return true,
        }
        d.scroll.scroll_to_item(d.row, ScrollStrategy::Top);
        cx.notify();
        true
    }

    pub(super) fn render_file_diff(&self) -> Option<impl IntoElement + use<>> {
        let d = self.file_diff.as_ref()?;
        let t = self.look.theme;
        let title = match d.names.as_slice() {
            [a, b] => trf!("{} ↔ {}", "{} ↔ {}", a, b),
            names => trf!("Comparando {} arquivos", "Comparing {} files", names.len()),
        };
        let secondary = |s: String| {
            div()
                .p(px(space(4)))
                .text_color(rgba(t.text_secondary))
                .child(s)
        };
        let (summary, body): (String, gpui::AnyElement) = match &d.state {
            None => (
                tr!("Lendo os arquivos…", "Reading the files…").to_owned(),
                div().into_any_element(),
            ),
            Some(Err(e)) => (
                String::new(),
                secondary(trf!(
                    "Não foi possível ler: {e}",
                    "Could not read: {e}"
                ))
                .into_any_element(),
            ),
            Some(Ok(Outcome::Identical(size))) => (
                String::new(),
                secondary(trf!(
                    "Os arquivos são idênticos ({}, byte a byte).",
                    "The files are identical ({}, byte by byte).",
                    size_short(*size)
                ))
                .into_any_element(),
            ),
            Some(Ok(Outcome::TooDifferent(a, b))) => (
                String::new(),
                secondary(trf!(
                    "Os textos são diferentes demais para mostrar linha a linha (mais de {} trechos; {} e {} linhas).",
                    "The texts are too different to show line by line (over {} edits; {} and {} lines).",
                    diff::MAX_EDITS,
                    a,
                    b
                ))
                .into_any_element(),
            ),
            Some(Ok(Outcome::Bytes { first, sizes, dims })) => {
                let mut text = trf!(
                    "Diferentes a partir do byte {} ({} e {}).",
                    "Different from byte {} on ({} and {}).",
                    first,
                    size_short(sizes.0),
                    size_short(sizes.1)
                );
                if let (Some(a), Some(b)) = dims {
                    text.push_str(&trf!(
                        " Imagens de {} e {}.",
                        " Images of {} and {}.",
                        a,
                        b
                    ));
                }
                (String::new(), secondary(text).into_any_element())
            }
            Some(Ok(Outcome::Groups(groups))) => {
                let mut col = div().flex().flex_col().p(px(space(3))).gap(px(space(3)));
                for (k, g) in groups.iter().enumerate() {
                    let head = if g.len() > 1 {
                        trf!(
                            "Grupo {}: {} arquivos idênticos",
                            "Group {}: {} identical files",
                            k + 1,
                            g.len()
                        )
                    } else {
                        trf!(
                            "Grupo {}: diferente dos outros",
                            "Group {}: different from the others",
                            k + 1
                        )
                    };
                    col = col.child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(CAPTION_SIZE))
                                    .text_color(rgba(t.text_secondary))
                                    .child(head),
                            )
                            .children(g.iter().map(|n| div().child(SharedString::from(n.clone())))),
                    );
                }
                let same = groups.iter().filter(|g| g.len() > 1).count();
                (
                    trf!(
                        "{} grupos · {} com cópias idênticas · selecione dois para ver as linhas",
                        "{} groups · {} with identical copies · select two to see the lines",
                        groups.len(),
                        same
                    ),
                    col.into_any_element(),
                )
            }
            Some(Ok(Outcome::Text {
                left,
                right,
                rows,
                starts,
            })) => {
                let count = |k: RowKind| rows.iter().filter(|r| r.kind == k).count();
                let summary = trf!(
                    "{} trechos · {} alteradas · {} só à esquerda · {} só à direita",
                    "{} blocks · {} changed · {} left only · {} right only",
                    starts.len(),
                    count(RowKind::Changed),
                    count(RowKind::Removed),
                    count(RowKind::Added)
                );
                let rows = rows.clone();
                let (left, right) = (left.clone(), right.clone());
                let mono = "Consolas";
                let current = d.row;
                let list = uniform_list("diff-rows", rows.len(), move |range, _, _| {
                    range
                        .map(|k| {
                            let r = rows[k];
                            let (lm, rm) = match r.kind {
                                RowKind::Same => (" ", " "),
                                RowKind::Changed => ("~", "~"),
                                RowKind::Removed => ("−", " "),
                                RowKind::Added => (" ", "+"),
                            };
                            let side = |line: Option<usize>, text: &[String], mark: &'static str| {
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .gap(px(space(2)))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(48.0))
                                            .text_right()
                                            .text_color(rgba(t.text_secondary))
                                            .child(line.map_or_else(String::new, |l| (l + 1).to_string())),
                                    )
                                    .child(div().flex_none().w(px(12.0)).child(mark))
                                    .child(
                                        div().flex_1().min_w_0().truncate().child(
                                            line.and_then(|l| text.get(l)).cloned().unwrap_or_default(),
                                        ),
                                    )
                            };
                            div()
                                .w_full()
                                .h(px(ROW_HEIGHT))
                                .flex()
                                .items_center()
                                .when(r.kind != RowKind::Same, |x| x.bg(rgba(t.selected)))
                                .when(k == current, |x| x.border_l_2().border_color(rgba(t.accent)))
                                .child(side(r.left, &left, lm))
                                .child(div().flex_none().w(px(1.0)).h_full().bg(rgba(t.divider)))
                                .child(side(r.right, &right, rm))
                        })
                        .collect()
                })
                .track_scroll(d.scroll.clone())
                .flex_1()
                .w_full()
                .font_family(mono)
                .text_size(px(CAPTION_SIZE));
                (summary, list.into_any_element())
            }
        };
        Some(
            div()
                .id("file-diff")
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
                        .gap(px(space(3)))
                        .child(
                            div()
                                .flex_none()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .child(tr!("Comparar arquivos", "Compare files")),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(title))
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .child(summary),
                        ),
                )
                .child(div().flex_1().min_h_0().flex().flex_col().child(body))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!(
                            "N ou F8: próximo trecho · P ou Shift+F8: anterior · ↑ ↓ PgUp PgDn rolam · Esc fecha",
                            "N or F8: next block · P or Shift+F8: previous · ↑ ↓ PgUp PgDn scroll · Esc closes"
                        )),
                ),
        )
    }

    /// The comparison as a screen reader reads it: the blocks of changes.
    pub(super) fn file_diff_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        let d = self.file_diff.as_ref()?;
        let mut items = Vec::new();
        let mut focus = None;
        let description = match &d.state {
            Some(Ok(Outcome::Text {
                left,
                right,
                rows,
                starts,
            })) => {
                for (k, &s) in starts.iter().enumerate() {
                    let r = rows[s];
                    let l = r
                        .left
                        .and_then(|i| left.get(i))
                        .cloned()
                        .unwrap_or_default();
                    let rt = r
                        .right
                        .and_then(|i| right.get(i))
                        .cloned()
                        .unwrap_or_default();
                    let line = r.left.or(r.right).map_or(0, |i| i + 1);
                    items.push(Item::Choice {
                        label: trf!(
                            "linha {}: antes \"{}\", depois \"{}\"",
                            "line {}: before \"{}\", after \"{}\"",
                            line,
                            l,
                            rt
                        ),
                        description: None,
                        selected: s <= d.row && starts.get(k + 1).is_none_or(|&n| n > d.row),
                    });
                    if s <= d.row {
                        focus = Some(k);
                    }
                }
                trf!(
                    "{} trechos diferentes. N próximo, P anterior, Esc fecha.",
                    "{} blocks differ. N next, P previous, Esc closes.",
                    starts.len()
                )
            }
            Some(Ok(Outcome::Identical(_))) => {
                tr!("Os arquivos são idênticos.", "The files are identical.").to_owned()
            }
            Some(Ok(Outcome::Bytes { first, .. })) => trf!(
                "Diferentes a partir do byte {}.",
                "Different from byte {} on.",
                first
            ),
            Some(Ok(Outcome::TooDifferent(..))) => tr!(
                "Diferentes demais para mostrar linha a linha.",
                "Too different to show line by line."
            )
            .to_owned(),
            Some(Ok(Outcome::Groups(groups))) => {
                for g in groups {
                    items.push(Item::Choice {
                        label: g.join(", "),
                        description: Some(if g.len() > 1 {
                            tr!("idênticos", "identical").to_owned()
                        } else {
                            tr!("diferente", "different").to_owned()
                        }),
                        selected: false,
                    });
                }
                trf!("{} grupos.", "{} groups.", groups.len())
            }
            Some(Err(e)) => e.clone(),
            None => tr!("Lendo os arquivos…", "Reading the files…").to_owned(),
        };
        Some(Overlay {
            title: tr!("Comparar arquivos", "Compare files").to_owned(),
            description: Some(description),
            items,
            focus,
            positions: None,
            modal: true,
        })
    }
}
