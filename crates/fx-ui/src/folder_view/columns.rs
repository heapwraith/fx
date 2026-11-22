//! Column view (task T3.3): Ctrl+Shift+3 shows the path as columns, one per
//! folder (`fx_core::miller`); choosing a folder opens the next column.
//! ↑↓ choose, → enters the chosen folder, ← goes back (and keeps going up),
//! Enter opens a file, Esc or Ctrl+Shift+3 leaves the columns and shows the
//! focused column's folder in the normal list. Each column is read on a
//! worker thread and drawn as a virtualized list, so a folder of 100,000
//! items does not block anything.

use fx_core::tr;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::channel;

use fx_core::miller::Miller;
use fx_shell::launch::Launch;
use gpui::{
    Context, Keystroke, MouseButton, MouseDownEvent, SharedString, Window, div, prelude::*, px,
    uniform_list,
};

use super::FolderView;
use crate::theme::{
    CAPTION_SIZE, ICON_CHEVRON, ICON_CHEVRON_LEFT, ICON_FOLDER, RADIUS, Theme, rgba, space,
};

const COLUMN_WIDTH: f32 = 260.0;
/// The strip at an edge with hidden columns.
const EDGE_WIDTH: f32 = 24.0;
const ROW_HEIGHT: f32 = 28.0;
/// Folders of the path shown when the columns open.
const START_COLUMNS: usize = 3;
const ICON_PAGE: &str = "\u{E7C3}";

#[derive(Debug, Clone)]
struct ColEntry {
    name: SharedString,
    is_dir: bool,
}

pub(super) struct Columns {
    model: Miller,
    entries: HashMap<PathBuf, Arc<Vec<ColEntry>>>,
    loading: HashSet<PathBuf>,
}

impl Columns {
    /// The folder of the focused column.
    pub(super) fn current_dir(&self) -> &Path {
        self.model.current_dir()
    }

    /// The entry chosen in the focused column, as a full path.
    pub(super) fn focused_chosen(&self) -> Option<PathBuf> {
        let col = self.model.columns.get(self.model.focus)?;
        Some(col.dir.join(col.chosen.as_deref()?))
    }

    fn entries_of(&self, k: usize) -> Option<&Arc<Vec<ColEntry>>> {
        self.entries.get(&self.model.columns.get(k)?.dir)
    }

    fn chosen_index(&self, k: usize) -> Option<usize> {
        let chosen = self.model.columns.get(k)?.chosen.as_deref()?;
        self.entries_of(k)?
            .iter()
            .position(|e| e.name.as_ref() == chosen)
    }
}

impl FolderView {
    /// Ctrl+Shift+3: opens the columns at the current folder, or closes them.
    pub(super) fn toggle_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.miller.is_some() {
            self.close_columns(window, cx);
            return;
        }
        self.miller = Some(Columns {
            model: Miller::new(&self.path, START_COLUMNS),
            entries: HashMap::new(),
            loading: HashSet::new(),
        });
        self.load_columns(cx);
        cx.notify();
    }

    /// Leaves the columns, showing the focused column's folder in the list.
    fn close_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.miller.take() {
            let dir = c.model.current_dir().to_path_buf();
            if dir != self.path {
                self.go_to(dir, None, window, cx);
            }
        }
        cx.notify();
    }

    /// Reads every column not read yet (in the background).
    fn load_columns(&mut self, cx: &mut Context<Self>) {
        let show_hidden = self.look.show_hidden;
        let Some(c) = self.miller.as_mut() else {
            return;
        };
        let wanted: Vec<PathBuf> = c
            .model
            .columns
            .iter()
            .map(|col| col.dir.clone())
            .filter(|d| !c.entries.contains_key(d) && !c.loading.contains(d))
            .collect();
        for dir in wanted {
            c.loading.insert(dir.clone());
            let (tx, rx) = futures::channel::oneshot::channel();
            let reading = dir.clone();
            let _ = std::thread::Builder::new()
                .name("fx-column".into())
                .spawn(move || {
                    let _ = tx.send(read_column(&reading, show_hidden));
                });
            cx.spawn(async move |this, cx| {
                let Ok(entries) = rx.await else { return };
                let _ = this.update(cx, |view, cx| {
                    if let Some(c) = view.miller.as_mut() {
                        c.loading.remove(&dir);
                        c.entries.insert(dir, Arc::new(entries));
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// Keys while the columns are open. Returns whether it handled the key.
    pub(super) fn columns_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(c) = self.miller.as_mut() else {
            return false;
        };
        let m = ks.modifiers;
        if m.control || m.alt || m.platform {
            return false;
        }
        let k = c.model.focus;
        match ks.key.as_str() {
            "escape" => self.close_columns(window, cx),
            "up" | "down" => {
                let Some(entries) = c.entries_of(k).cloned() else {
                    return true;
                };
                if entries.is_empty() {
                    return true;
                }
                let next = match (c.chosen_index(k), ks.key.as_str()) {
                    (None, _) => 0,
                    (Some(i), "up") => i.saturating_sub(1),
                    (Some(i), _) => (i + 1).min(entries.len() - 1),
                };
                let e = &entries[next];
                c.model.choose(k, &e.name, e.is_dir);
                self.load_columns(cx);
            }
            "right" => {
                if c.model.right() {
                    let k = c.model.focus;
                    if c.model.columns[k].chosen.is_none()
                        && let Some(first) = c.entries_of(k).and_then(|e| e.first().cloned())
                    {
                        c.model.choose(k, &first.name, first.is_dir);
                    }
                    self.load_columns(cx);
                }
            }
            "left" => {
                if c.model.left() {
                    self.load_columns(cx);
                }
            }
            "enter" => {
                let col = &c.model.columns[k];
                if let Some(name) = col.chosen.clone() {
                    let path = col.dir.join(&name);
                    let is_dir = c
                        .entries_of(k)
                        .is_some_and(|e| e.iter().any(|x| x.name.as_ref() == name && x.is_dir));
                    if is_dir {
                        c.model.right();
                        self.load_columns(cx);
                    } else {
                        self.launch(Launch::Open(path), cx);
                    }
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    fn column_clicked(&mut self, k: usize, i: usize, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(c) = self.miller.as_mut() else {
            return;
        };
        let Some(e) = c.entries_of(k).and_then(|e| e.get(i).cloned()) else {
            return;
        };
        c.model.choose(k, &e.name, e.is_dir);
        if ev.click_count >= 2 && !e.is_dir {
            let path = c.model.columns[k].dir.join(e.name.as_ref());
            self.launch(Launch::Open(path), cx);
        }
        self.load_columns(cx);
        cx.notify();
    }

    pub(super) fn render_columns(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let c = self.miller.as_ref()?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let view = cx.entity().downgrade();
        let mut row = div().id("columns-row").size_full().flex();
        // Only the columns that fit, always with the focused one and
        // preferring the rightmost (the current folder), like Finder. Strips
        // at the edges show how many are hidden and bring the next one in
        // (the mouse reaches every column; review, D-066).
        let len = c.model.columns.len();
        let fit = if self.last_list_width > 1.0 {
            (((self.last_list_width - 2.0 * EDGE_WIDTH) / COLUMN_WIDTH).floor() as usize).max(1)
        } else {
            len
        };
        let start = first_visible(len, fit, c.model.focus);
        let hidden_right = len.saturating_sub(start + fit);
        if start > 0 {
            row = row.child(edge_strip(
                "columns-left",
                ICON_CHEVRON_LEFT,
                start,
                start - 1,
                &view,
                t,
                icon_font,
            ));
        }
        for (k, col) in c.model.columns.iter().enumerate().skip(start).take(fit) {
            let focused = k == c.model.focus;
            let chosen = c.chosen_index(k);
            let body = match c.entries.get(&col.dir) {
                None => div()
                    .p(px(space(3)))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!("Lendo…", "Reading…"))
                    .into_any_element(),
                Some(entries) if entries.is_empty() => div()
                    .p(px(space(3)))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!("Pasta vazia", "Empty folder"))
                    .into_any_element(),
                Some(entries) => {
                    let entries = entries.clone();
                    let view = view.clone();
                    let (sel, acc, sec) = (t.selected, t.accent, t.text_secondary);
                    uniform_list(("column", k), entries.len(), move |range, _, _| {
                        range
                            .map(|i| {
                                let e = &entries[i];
                                let is_chosen = chosen == Some(i);
                                let view = view.clone();
                                div()
                                    .id(("col-entry", i))
                                    .w_full()
                                    .h(px(ROW_HEIGHT))
                                    .px(px(space(2)))
                                    .flex()
                                    .items_center()
                                    .gap(px(space(2)))
                                    .rounded(px(RADIUS))
                                    .when(is_chosen, |d| d.bg(rgba(sel)))
                                    .when(is_chosen && focused, |d| {
                                        d.border_l_2().border_color(rgba(acc))
                                    })
                                    .on_mouse_down(MouseButton::Left, move |ev, _, cx| {
                                        let _ =
                                            view.update(cx, |v, cx| v.column_clicked(k, i, ev, cx));
                                    })
                                    .child(
                                        div()
                                            .flex_none()
                                            .font_family(icon_font)
                                            .text_size(px(CAPTION_SIZE))
                                            .text_color(rgba(sec))
                                            .child(if e.is_dir { ICON_FOLDER } else { ICON_PAGE }),
                                    )
                                    .child(
                                        div().flex_1().min_w_0().truncate().child(e.name.clone()),
                                    )
                                    .when(e.is_dir, |d| {
                                        d.child(
                                            div()
                                                .flex_none()
                                                .font_family(icon_font)
                                                .text_size(px(8.0))
                                                .text_color(rgba(sec))
                                                .child(ICON_CHEVRON),
                                        )
                                    })
                            })
                            .collect()
                    })
                    .size_full()
                    .px(px(space(1)))
                    .into_any_element()
                }
            };
            row = row.child(
                div()
                    .flex_none()
                    .w(px(COLUMN_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .flex_none()
                            .px(px(space(3)))
                            .py(px(space(1)))
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(if focused { t.text } else { t.text_secondary }))
                            .truncate()
                            .child(folder_label(&col.dir)),
                    )
                    .child(div().flex_1().min_h_0().child(body)),
            );
        }
        if hidden_right > 0 {
            row = row.child(edge_strip(
                "columns-right",
                ICON_CHEVRON,
                hidden_right,
                start + fit,
                &view,
                t,
                icon_font,
            ));
        }
        Some(
            div()
                .id("columns")
                .absolute()
                .top(px(space(12)))
                .left_0()
                .right_0()
                .bottom_0()
                .bg(rgba(t.base))
                .border_t_1()
                .border_color(rgba(t.divider))
                .child(row),
        )
    }
}

impl FolderView {
    /// The column view as a screen reader reads it: the focused column's
    /// items around the chosen one ("x de N" over all of them).
    pub(super) fn columns_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        const AROUND: usize = 20;
        let c = self.miller.as_ref()?;
        let k = c.model.focus;
        let col = c.model.columns.get(k)?;
        let entries = c.entries_of(k).map(|e| e.as_slice()).unwrap_or_default();
        let chosen = c.chosen_index(k);
        let first = chosen.unwrap_or(0).saturating_sub(AROUND);
        let end = (first + 2 * AROUND).min(entries.len());
        let items: Vec<Item> = entries
            .get(first..end)
            .unwrap_or_default()
            .iter()
            .enumerate()
            .map(|(i, e)| Item::Choice {
                label: e.name.to_string(),
                description: e.is_dir.then(|| tr!("pasta", "folder").to_owned()),
                selected: chosen == Some(first + i),
            })
            .collect();
        let focus = chosen
            .and_then(|ch| ch.checked_sub(first))
            .filter(|_| !items.is_empty());
        Some(Overlay {
            title: folder_label(&col.dir),
            description: Some(
                tr!(
                    "Vista em colunas. ↑ e ↓ escolhem, → entra na pasta, ← volta, Esc volta à lista.",
                    "Column view. ↑ and ↓ choose, → enters the folder, ← goes back, Esc returns to the list."
                )
                .to_owned(),
            ),
            items,
            focus,
            positions: Some((first, entries.len())),
        })
    }
}

/// A strip at an edge: the chevron and how many columns are hidden there;
/// clicking it focuses column `reveal`, which brings it into view.
fn edge_strip(
    id: &'static str,
    icon: &'static str,
    hidden: usize,
    reveal: usize,
    view: &gpui::WeakEntity<FolderView>,
    t: Theme,
    icon_font: &'static str,
) -> impl IntoElement {
    let view = view.clone();
    div()
        .id(id)
        .flex_none()
        .w(px(EDGE_WIDTH))
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(space(1)))
        .border_r_1()
        .border_color(rgba(t.divider))
        .text_color(rgba(t.text_secondary))
        .hover(|d| d.bg(rgba(t.hover)))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            let _ = view.update(cx, |v, cx| v.focus_column(reveal, cx));
        })
        .child(
            div()
                .font_family(icon_font)
                .text_size(px(CAPTION_SIZE))
                .child(icon),
        )
        .child(div().text_size(px(CAPTION_SIZE)).child(hidden.to_string()))
}

impl FolderView {
    /// Moves the focus to column `k` (an edge strip was clicked).
    fn focus_column(&mut self, k: usize, cx: &mut Context<Self>) {
        let Some(c) = self.miller.as_mut() else {
            return;
        };
        while c.model.focus > k && c.model.left() {}
        while c.model.focus < k && c.model.right() {}
        self.load_columns(cx);
        cx.notify();
    }
}

fn folder_label(dir: &Path) -> String {
    dir.file_name().map_or_else(
        || dir.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// A column's entries: folders first, then files, natural order.
fn read_column(dir: &Path, show_hidden: bool) -> Vec<ColEntry> {
    let (tx, rx) = channel();
    if fx_fs::enumerate(fx_fs::Strategy::FindFirstFileEx, dir, &tx).is_err() {
        return Vec::new();
    }
    drop(tx);
    let mut out: Vec<(bool, String)> = rx
        .into_iter()
        .flatten()
        .filter(|e| {
            // A copy in progress is never shown, like in the list.
            let temp = e
                .name
                .to_str()
                .is_some_and(fx_core::temp_name::is_temp_name);
            !temp
                && (show_hidden
                    || !(e.attributes.contains(fx_core::Attributes::HIDDEN)
                        || e.attributes.contains(fx_core::Attributes::SYSTEM)))
        })
        .map(|e| (e.attributes.is_dir(), e.name.to_string_lossy().into_owned()))
        .collect();
    out.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| {
            fx_core::natural::natural_cmp(
                &fx_core::natural::fold(&a.1),
                &fx_core::natural::fold(&b.1),
            )
        })
    });
    out.into_iter()
        .map(|(is_dir, name)| ColEntry {
            name: name.into(),
            is_dir,
        })
        .collect()
}

/// The first column drawn when only `fit` of `len` columns fit: the
/// rightmost ones, moved left if needed to keep `focus` in view.
fn first_visible(len: usize, fit: usize, focus: usize) -> usize {
    len.saturating_sub(fit).min(focus)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_focused_column_is_always_drawn() {
        use super::first_visible;
        assert_eq!(first_visible(3, 5, 2), 0, "all fit");
        assert_eq!(first_visible(5, 2, 4), 3, "the two rightmost");
        assert_eq!(first_visible(5, 2, 1), 1, "focus moved left: still in view");
        assert_eq!(first_visible(5, 1, 0), 0);
    }

    use super::*;

    #[test]
    fn a_column_lists_folders_first_in_natural_order() {
        let t = tempfile::tempdir().unwrap();
        for d in ["b", "A10", "A2"] {
            std::fs::create_dir(t.path().join(d)).unwrap();
        }
        for f in ["z.txt", "x10.txt", "x2.txt"] {
            std::fs::write(t.path().join(f), b"").unwrap();
        }
        let names: Vec<String> = read_column(t.path(), false)
            .into_iter()
            .map(|e| e.name.to_string())
            .collect();
        assert_eq!(names, ["A2", "A10", "b", "x2.txt", "x10.txt", "z.txt"]);
    }
}
