//! Path bar (task T1.3): back/forward/up buttons, breadcrumbs (click one to go
//! there) and a text field with folder autocompletion (click the empty part of
//! the bar, Ctrl+L or Alt+D).
//!
//! Suggestions come from a cache of the parent folder's subfolders, listed on
//! the background executor the first time that parent is typed. A key only
//! filters that cache in memory, so it never waits for the disk; while the
//! listing runs, the field simply shows no suggestions yet.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;

use fx_core::complete::{Candidates, split_typed};
use fx_fs::Strategy;
use gpui::{
    ClickEvent, Context, Keystroke, MouseButton, MouseDownEvent, SharedString, Window, deferred,
    div, prelude::*, px,
};

use super::{FolderView, Restore};
use crate::theme::{
    CAPTION_SIZE, ICON_BACK, ICON_CHEVRON, ICON_FORWARD, ICON_GRID, ICON_LIST, ICON_UP, RADIUS,
    rgba, space,
};

/// Suggestions shown under the field.
const MAX_SUGGESTIONS: usize = 8;
const BAR_HEIGHT: f32 = 32.0;
const BUTTON_SIZE: f32 = 32.0;
/// The path bar never gets narrower than this (the status text gives way).
const BAR_MIN_WIDTH: f32 = 140.0;
const STATUS_MAX_WIDTH: f32 = 280.0;

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
                edit.error = Some("digite um caminho completo");
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
                    edit.error = Some("pasta não encontrada");
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_path_bar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            None => bar.child(self.render_crumbs(cx)),
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
            // Status shrinks first (with an ellipsis); the bar keeps room.
            .child(
                div()
                    .flex_shrink()
                    .min_w_0()
                    .max_w(px(STATUS_MAX_WIDTH))
                    .pl(px(space(2)))
                    .pr(px(space(2)))
                    .truncate()
                    .text_color(rgba(t.text_secondary))
                    .child(self.status_text(cx)),
            )
    }

    /// Breadcrumbs: one button per folder from the root, then an empty area
    /// that turns the bar into a text field.
    fn render_crumbs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let mut ancestors: Vec<&Path> = self.path.ancestors().collect();
        ancestors.reverse();
        let last = ancestors.len().saturating_sub(1);
        // The crumbs sit in a box that clips from the *start* when they do not
        // fit, so the current folder (the last crumb) always shows.
        let mut row = div().flex_none().flex().items_center().px(px(space(1)));
        for (k, dir) in ancestors.iter().enumerate() {
            let label: SharedString = dir
                .file_name()
                .map_or_else(
                    || dir.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                )
                .into();
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
                    .child(label)
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        if k == last {
                            return;
                        }
                        let restore = child.clone().map(Restore::cursor_on);
                        view.go_to(target.clone(), restore, window, cx);
                    })),
            );
            if k < last {
                row = row.child(
                    div()
                        .flex_none()
                        .font_family(self.look.icon_font)
                        .text_size(px(8.0))
                        .text_color(rgba(t.text_secondary))
                        .child(ICON_CHEVRON),
                );
            }
        }
        let crumbs = div()
            .flex_shrink()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .justify_end()
            .overflow_hidden()
            .child(row);
        div()
            .size_full()
            .min_w_0()
            .flex()
            .items_center()
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
