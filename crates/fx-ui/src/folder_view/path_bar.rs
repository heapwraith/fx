//! Path bar (task T1.3): back/forward/up buttons, breadcrumbs (click one to go
//! there) and a text field with folder autocompletion (click the empty part of
//! the bar, Ctrl+L or Alt+D).
//!
//! Suggestions come from a cache of the parent folder's subfolders, listed on
//! the background executor the first time that parent is typed. A key only
//! filters that cache in memory, so it never waits for the disk; while the
//! listing runs, the field simply shows no suggestions yet.

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;

use fx_core::complete::{Candidates, split_typed};
use fx_fs::Strategy;
use gpui::{
    ClickEvent, Context, Keystroke, MouseButton, MouseDownEvent, SharedString, Window, canvas,
    deferred, div, prelude::*, px,
};

use super::{FolderView, Restore};

use crate::theme::{
    BODY_SIZE, CAPTION_SIZE, ICON_BACK, ICON_CHEVRON, ICON_FOLDER, ICON_FORWARD, ICON_GRID,
    ICON_LIST, ICON_SEARCH, ICON_UP, ICON_VIEW, RADIUS, rgba, space,
};

/// The button that stands for the folded leading crumbs.
const MORE: &str = "…";
/// Width of the chevron between crumbs.
const CHEVRON_WIDTH: f32 = 14.0;

/// Suggestions shown under the field.
const MAX_SUGGESTIONS: usize = 8;
const BAR_HEIGHT: f32 = 32.0;
const BUTTON_SIZE: f32 = 32.0;
/// The path bar never gets narrower than this (the status text gives way).
const BAR_MIN_WIDTH: f32 = 140.0;
const STATUS_MAX_WIDTH: f32 = 400.0;

/// The path bar while it is a text field.
#[derive(Debug, Clone)]
pub(super) struct PathEdit {
    text: String,
    /// Just opened (or Ctrl+A): the whole text is selected, so the next
    /// character replaces it.
    all_selected: bool,
    /// Indices into the cached candidates.
    matches: Vec<usize>,
    highlighted: usize,
    error: Option<&'static str>,
}

/// Subfolders of the last parent folder typed.
#[derive(Debug, Clone)]
pub(super) struct Completion {
    parent: String,
    /// `None` while the listing runs.
    candidates: Option<Arc<Candidates>>,
}

impl FolderView {
    /// Text of the path field while editing (for the accessibility tree).
    pub(super) fn edit_text(&self) -> Option<String> {
        self.edit.as_ref().map(|e| e.text.clone())
    }

    pub(super) fn begin_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit = Some(PathEdit {
            text: self.path.display().to_string(),
            all_selected: true,
            matches: Vec::new(),
            highlighted: 0,
            error: None,
        });
        self.update_completion(cx);
        self.focus.focus(window);
        cx.notify();
    }

    /// Keys while editing the path. Returns false for keys it ignores.
    pub(super) fn edit_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = &mut self.edit else {
            return false;
        };
        let m = ks.modifiers;
        let only_ctrl = m.control && !m.alt && !m.shift;
        let mut changed = true;
        match ks.key.as_str() {
            "escape" => {
                self.edit = None;
                return true;
            }
            "enter" => {
                self.submit(window, cx);
                return true;
            }
            "tab" => {
                let pick = edit.matches.get(edit.highlighted).copied();
                if let Some(i) = pick {
                    self.accept(i);
                }
                return true;
            }
            "up" | "down" => {
                let n = edit.matches.len();
                if n > 0 {
                    edit.highlighted = if ks.key == "up" {
                        (edit.highlighted + n - 1) % n
                    } else {
                        (edit.highlighted + 1) % n
                    };
                }
                return true;
            }
            "home" | "end" | "left" | "right" => {
                // The caret stays at the end; these only drop the selection.
                edit.all_selected = false;
                return true;
            }
            "backspace" => {
                if edit.all_selected {
                    edit.text.clear();
                } else {
                    edit.text.pop();
                }
            }
            "a" if only_ctrl => {
                edit.all_selected = true;
                return true;
            }
            // GPUI sends no character for the space bar.
            "space" if !(m.control || m.alt || m.platform) => {
                if edit.all_selected {
                    edit.text.clear();
                }
                edit.text.push(' ');
            }
            "v" if only_ctrl => {
                let pasted = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text())
                    .map(|t| t.lines().next().unwrap_or("").trim().to_owned())
                    .unwrap_or_default();
                if edit.all_selected {
                    edit.text.clear();
                }
                edit.text.push_str(&pasted);
            }
            _ => {
                if m.control || m.alt || m.platform || m.function {
                    return true;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => {
                        if edit.all_selected {
                            edit.text.clear();
                        }
                        edit.text.push_str(c);
                    }
                    _ => changed = false,
                }
            }
        }
        if changed {
            edit.all_selected = false;
            edit.error = None;
            self.update_completion(cx);
        }
        true
    }

    /// Recomputes the suggestions for the current text, listing the parent
    /// folder in the background when it is not the cached one.
    fn update_completion(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = &mut self.edit else { return };
        edit.highlighted = 0;
        edit.matches.clear();
        let Some((parent, partial)) = split_typed(&edit.text) else {
            return;
        };
        if let Some(c) = &self.completion
            && c.parent == parent
        {
            if let Some(candidates) = &c.candidates {
                edit.matches = candidates.matching(partial, MAX_SUGGESTIONS);
            }
            return;
        }
        let parent = parent.to_owned();
        self.completion = Some(Completion {
            parent: parent.clone(),
            candidates: None,
        });
        cx.spawn(async move |this, cx| {
            let dir = PathBuf::from(&parent);
            let names = cx.background_spawn(async move { subfolders(&dir) }).await;
            let _ = this.update(cx, |view, cx| {
                let Some(c) = &mut view.completion else {
                    return;
                };
                if c.parent != parent {
                    return;
                }
                c.candidates = Some(Arc::new(Candidates::new(names)));
                view.update_completion(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts suggestion `i` in the field, followed by a separator, so the next
    /// keys complete its subfolders.
    fn accept(&mut self, i: usize) {
        let (Some(edit), Some(c)) = (&mut self.edit, &self.completion) else {
            return;
        };
        let Some(candidates) = &c.candidates else {
            return;
        };
        edit.text = format!("{}{}\\", c.parent, candidates.name(i));
        edit.all_selected = false;
        edit.error = None;
    }

    /// Enter: goes to the typed folder once a background check confirms it
    /// exists; otherwise the field stays open with an error.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = &self.edit else { return };
        let typed = edit.text.trim().trim_matches('"').to_owned();
        if typed.is_empty() {
            self.edit = None;
            return;
        }
        let path = PathBuf::from(&typed);
        if !path.is_absolute() {
            if let Some(edit) = &mut self.edit {
                edit.error = Some(tr!("digite um caminho completo", "type a full path"));
            }
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let check = path.clone();
            let is_dir = cx.background_spawn(async move { check.is_dir() }).await;
            let _ = this.update_in(cx, |view, window, cx| {
                if view
                    .edit
                    .as_ref()
                    .is_none_or(|e| e.text.trim().trim_matches('"') != typed)
                {
                    return; // the text changed meanwhile
                }
                if is_dir {
                    view.edit = None;
                    view.go_to(path, None, window, cx);
                } else if let Some(edit) = &mut view.edit {
                    edit.error = Some(tr!("pasta não encontrada", "folder not found"));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_path_bar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let button = |id: &'static str, glyph: &'static str, enabled: bool| {
            div()
                .id(id)
                .flex_none()
                .size(px(BUTTON_SIZE))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(RADIUS))
                .font_family(icon_font)
                .text_size(px(CAPTION_SIZE))
                .child(glyph)
                .when(enabled, |d| d.hover(|s| s.bg(rgba(t.hover))))
                .when(!enabled, |d| d.opacity(0.4))
        };
        let status: SharedString = self.status_text(cx).into();
        let filtering = !self.filter_text.is_empty();
        let status_width = (text_width(
            window,
            &status,
            gpui::FontWeight::NORMAL,
            self.look.font,
            BODY_SIZE,
        ) + 2.0 * space(2)
            + if filtering {
                CAPTION_SIZE + space(1)
            } else {
                0.0
            }
            + 1.0)
            .min(STATUS_MAX_WIDTH);
        let can_back = self.history.can_go_back();
        let can_forward = self.history.can_go_forward();
        let can_up = self.path.parent().is_some();

        let bar = div()
            .id("path-bar")
            .relative()
            .flex_1()
            .min_w(px(BAR_MIN_WIDTH))
            .h(px(BAR_HEIGHT))
            .flex()
            .items_center()
            .rounded(px(RADIUS))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.layer));
        let bar = match &self.edit {
            None => bar.child(self.render_crumbs(window, cx)),
            Some(edit) => bar
                .border_color(rgba(t.accent))
                .px(px(space(2)))
                .cursor_text()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .when(edit.all_selected, |d| d.bg(rgba(t.selected)))
                        .child(edit.text.clone()),
                )
                .child(div().flex_none().w(px(1.0)).h(px(16.0)).bg(rgba(t.text)))
                .child(div().flex_1())
                .when_some(edit.error, |d, e| {
                    d.child(
                        div()
                            .flex_none()
                            .pl(px(space(2)))
                            .text_color(rgba(t.text_secondary))
                            .child(e),
                    )
                })
                .when(!edit.matches.is_empty(), |d| {
                    d.child(deferred(self.render_suggestions(edit, cx)).with_priority(1))
                }),
        };

        div()
            .h(px(space(12)))
            .flex()
            .items_center()
            .gap(px(space(1)))
            .px(px(space(2)))
            .child(button("back", ICON_BACK, can_back).on_click(cx.listener(
                |view, _: &ClickEvent, window, cx| {
                    view.go_back(window, cx);
                },
            )))
            .child(
                button("forward", ICON_FORWARD, can_forward).on_click(cx.listener(
                    |view, _: &ClickEvent, window, cx| {
                        view.go_forward(window, cx);
                    },
                )),
            )
            .child(button("up", ICON_UP, can_up).on_click(cx.listener(
                |view, _: &ClickEvent, window, cx| {
                    view.go_up(window, cx);
                },
            )))
            .child(bar)
            .child(
                button("view", if self.grid { ICON_LIST } else { ICON_GRID }, true).on_click(
                    cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.set_grid(!view.grid);
                        cx.notify();
                    }),
                ),
            )
            .child(
                button("view-menu", ICON_VIEW, true)
                    .when(self.view_menu, |d| d.bg(rgba(t.selected)))
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                        view.toggle_view_menu(cx);
                    })),
            )
            // The status is as wide as its text (up to a maximum) and shrinks
            // with an ellipsis when room runs out; the bar takes the rest. An
            // explicit width, not one from the text, is what lets GPUI
            // truncate with an ellipsis.
            .child(
                div()
                    .flex_shrink()
                    .w(px(status_width))
                    .min_w_0()
                    .pl(px(space(2)))
                    .pr(px(space(2)))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space(1)))
                    .text_color(rgba(t.text_secondary))
                    // While typing to filter, a magnifier says what the text is.
                    .when(!self.filter_text.is_empty(), |d| {
                        d.child(
                            div()
                                .flex_none()
                                .font_family(icon_font)
                                .text_size(px(CAPTION_SIZE))
                                .child(ICON_SEARCH),
                        )
                    })
                    .child(div().min_w_0().truncate().child(status)),
            )
    }

    /// Breadcrumbs: one button per folder from the root, then an empty area
    /// that turns the bar into a text field. When they do not fit, the
    /// leading ones fold into a "…" button with a menu (T2.3), so no crumb is
    /// ever cut in the middle and the current folder always shows.
    fn render_crumbs(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let mut ancestors: Vec<&Path> = self.path.ancestors().collect();
        ancestors.reverse();
        let last = ancestors.len().saturating_sub(1);
        let labels: Vec<SharedString> = ancestors
            .iter()
            .map(|dir| {
                dir.file_name()
                    .map_or_else(
                        || dir.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                    .into()
            })
            .collect();
        // Widths from the text system (cached shaping), as the row lays them out.
        let text_width = |text: &SharedString, weight: gpui::FontWeight, family: &'static str| {
            text_width(window, text, weight, family, BODY_SIZE)
        };
        let widths: Vec<f32> = labels
            .iter()
            .enumerate()
            .map(|(k, l)| {
                let weight = if k == last {
                    gpui::FontWeight::SEMIBOLD
                } else {
                    gpui::FontWeight::NORMAL
                };
                text_width(l, weight, self.look.font) + 2.0 * space(2)
            })
            .collect();
        let more_width = text_width(&MORE.into(), gpui::FontWeight::NORMAL, self.look.font)
            + 2.0 * space(2)
            + CHEVRON_WIDTH;
        let room = self.crumb_room.get() - 2.0 * space(1) - space(6);
        let first = first_shown(&widths, CHEVRON_WIDTH, more_width, room);

        let mut row = div().flex_none().flex().items_center().px(px(space(1)));
        if first > 0 {
            row = row
                .child(
                    div()
                        .id("crumb-more")
                        .flex_none()
                        .px(px(space(2)))
                        .py(px(space(1)))
                        .rounded(px(RADIUS))
                        .hover(|s| s.bg(rgba(t.hover)))
                        .when(self.crumb_menu, |d| d.bg(rgba(t.selected)))
                        .child(MORE)
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            view.crumb_menu = !view.crumb_menu;
                            cx.notify();
                        })),
                )
                .child(self.chevron());
        }
        for (k, dir) in ancestors.iter().enumerate().skip(first) {
            // Coming back up selects the folder we were in below this one.
            let child = ancestors
                .get(k + 1)
                .and_then(|c| c.file_name())
                .map(|n| n.to_os_string());
            let target = dir.to_path_buf();
            row = row.child(
                div()
                    .id(("crumb", k))
                    .flex_none()
                    .px(px(space(2)))
                    .py(px(space(1)))
                    .rounded(px(RADIUS))
                    .hover(|s| s.bg(rgba(t.hover)))
                    .when(k == last, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                    .child(labels[k].clone())
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        if k == last {
                            return;
                        }
                        let restore = child.clone().map(Restore::cursor_on);
                        view.go_to(target.clone(), restore, window, cx);
                    })),
            );
            if k < last {
                row = row.child(self.chevron());
            }
        }
        // The crumbs still clip from the start as a safety net (first frame,
        // before the width is known).
        let crumbs = div()
            .flex_shrink()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .justify_end()
            .overflow_hidden()
            .child(row);
        let room_cell = self.crumb_room.clone();
        div()
            .relative()
            .size_full()
            .min_w_0()
            .flex()
            .items_center()
            // Measures the area the crumbs may use, for the next frame.
            .child(
                canvas(
                    move |bounds, window, _| {
                        let w = f32::from(bounds.size.width);
                        if (room_cell.get() - w).abs() > 0.5 {
                            room_cell.set(w);
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(crumbs)
            .child(
                div()
                    .id("crumb-empty")
                    .flex_1()
                    .h_full()
                    .min_w(px(space(6)))
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, window, cx| {
                            view.begin_edit(window, cx);
                        }),
                    ),
            )
            .when(self.crumb_menu && first > 0, |d| {
                d.child(deferred(self.render_crumb_menu(&ancestors[..first], cx)).with_priority(1))
            })
    }

    fn chevron(&self) -> gpui::Div {
        div()
            .flex_none()
            .w(px(CHEVRON_WIDTH))
            .flex()
            .justify_center()
            .font_family(self.look.icon_font)
            .text_size(px(8.0))
            .text_color(rgba(self.look.theme.text_secondary))
            .child(ICON_CHEVRON)
    }

    /// The folders folded into "…", from the root down; a click opens one.
    fn render_crumb_menu(
        &self,
        hidden: &[&Path],
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let mut menu = div()
            .id("crumb-menu")
            .absolute()
            .top(px(BAR_HEIGHT + 2.0))
            .left_0()
            .min_w(px(200.0))
            .max_w(px(420.0))
            .py(px(space(1)))
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .on_mouse_down_out(cx.listener(|view, _: &MouseDownEvent, _, cx| {
                view.crumb_menu = false;
                cx.notify();
            }));
        for (k, dir) in hidden.iter().enumerate() {
            let label: SharedString = dir
                .file_name()
                .map_or_else(
                    || dir.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                )
                .into();
            let target = dir.to_path_buf();
            let child = hidden
                .get(k + 1)
                .copied()
                .or_else(|| self.path.ancestors().find(|a| a.parent() == Some(dir)))
                .and_then(|c| c.file_name())
                .map(|n| n.to_os_string());
            menu = menu.child(
                div()
                    .id(("crumb-menu-item", k))
                    .w_full()
                    .mx(px(space(1)))
                    .px(px(space(2)))
                    .py(px(space(1)))
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .rounded(px(RADIUS))
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(
                        div()
                            .flex_none()
                            .font_family(self.look.icon_font)
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(ICON_FOLDER),
                    )
                    .child(div().min_w_0().truncate().child(label))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        let restore = child.clone().map(Restore::cursor_on);
                        view.go_to(target.clone(), restore, window, cx);
                    })),
            );
        }
        menu
    }

    fn render_suggestions(
        &self,
        edit: &PathEdit,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let mut list = div()
            .absolute()
            .top(px(BAR_HEIGHT + 2.0))
            .left_0()
            .right_0()
            .py(px(space(1)))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_md();
        let Some(candidates) = self.completion.as_ref().and_then(|c| c.candidates.clone()) else {
            return list;
        };
        for (row, &i) in edit.matches.iter().enumerate() {
            let name: SharedString = candidates.name(i).to_owned().into();
            list = list.child(
                div()
                    .id(("suggestion", row))
                    .mx(px(space(1)))
                    .px(px(space(2)))
                    .py(px(space(1)))
                    .rounded(px(RADIUS))
                    .when(row == edit.highlighted, |d| d.bg(rgba(t.selected)))
                    .hover(|s| s.bg(rgba(t.hover)))
                    .child(name)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            view.accept(i);
                            view.submit(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            );
        }
        list
    }
}

/// Width of `text` in one line, as GPUI will lay it out (the shaping is
/// cached by the text system, so calling it every frame is cheap).
fn text_width(
    window: &Window,
    text: &SharedString,
    weight: gpui::FontWeight,
    family: &'static str,
    size: f32,
) -> f32 {
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::Font {
            weight,
            ..gpui::font(family)
        },
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .shape_line(text.clone(), px(size), &[run], None);
    f32::from(line.width)
}

/// The first crumb to show so that the crumbs from it to the last fit in
/// `room` (with a "…" button of width `more` before them when some are
/// folded). The last crumb always shows; `room <= 0` (not measured yet)
/// shows them all.
fn first_shown(widths: &[f32], chevron: f32, more: f32, room: f32) -> usize {
    let last = widths.len().saturating_sub(1);
    if room <= 0.0 {
        return 0;
    }
    let total: f32 = widths.iter().sum::<f32>() + chevron * last as f32;
    if total <= room {
        return 0;
    }
    // Drop crumbs from the start until the rest plus "…" fits.
    let mut used = total;
    for (k, w) in widths.iter().enumerate().take(last) {
        used -= w + chevron;
        if used + more <= room {
            return k + 1;
        }
    }
    last
}

/// Names of the subfolders of `dir` (runs on a background thread).
fn subfolders(dir: &Path) -> Vec<String> {
    let (tx, rx) = channel();
    if fx_fs::enumerate(Strategy::FindFirstFileEx, dir, &tx).is_err() {
        return Vec::new();
    }
    drop(tx);
    rx.into_iter()
        .flatten()
        .filter(|e| e.attributes.is_dir())
        .map(|e| e.name.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::first_shown;

    const CHEVRON: f32 = 14.0;
    const MORE: f32 = 40.0;

    #[test]
    fn all_crumbs_when_they_fit_or_before_measuring() {
        let w = [30.0, 60.0, 80.0];
        assert_eq!(first_shown(&w, CHEVRON, MORE, 500.0), 0);
        assert_eq!(first_shown(&w, CHEVRON, MORE, 0.0), 0);
        // Exactly the total: 170 + 2 chevrons.
        assert_eq!(first_shown(&w, CHEVRON, MORE, 198.0), 0);
    }

    #[test]
    fn leading_crumbs_fold_into_more() {
        let w = [30.0, 60.0, 80.0, 100.0];
        // Total 270 + 3 chevrons = 312. Without the first: 268, plus "…" = 308.
        assert_eq!(first_shown(&w, CHEVRON, MORE, 308.0), 1);
        // Without the first two: 194 + 40 = 234.
        assert_eq!(first_shown(&w, CHEVRON, MORE, 307.0), 2);
        assert_eq!(first_shown(&w, CHEVRON, MORE, 234.0), 2);
        // Only the last one (100 + 40).
        assert_eq!(first_shown(&w, CHEVRON, MORE, 233.0), 3);
    }

    #[test]
    fn the_current_folder_always_shows() {
        let w = [30.0, 60.0, 300.0];
        assert_eq!(first_shown(&w, CHEVRON, MORE, 50.0), 2);
        assert_eq!(first_shown(&[300.0], CHEVRON, MORE, 50.0), 0);
    }
}
