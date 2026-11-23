//! Command palette (task T1.24): Ctrl+K or Ctrl+Shift+P. Fuzzy search over
//! fx's commands (with their shortcut), Quick access and recent folders;
//! Enter runs the choice. A command runs its own shortcut on the window or
//! the active tab (`crate::commands`), so both paths behave the same.

use fx_core::{tr, trf};
use std::path::PathBuf;
use std::time::Instant;

use fx_core::fuzzy::fuzzy_score;
use gpui::{
    Context, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, deferred, div, prelude::*, px, uniform_list,
};

use super::TabsView;
use crate::commands::{Action, COMMANDS, Command, Target};
use crate::theme::{CAPTION_SIZE, ICON_FOLDER, ICON_PIN, RADIUS, rgba, space};

const PANEL_WIDTH: f32 = 560.0;
const ROW_HEIGHT: f32 = 36.0;
const RECENT_MAX: usize = 15;
/// With an empty query, recent folders shown before the commands.
const RECENT_FIRST: usize = 5;
/// A folder matched only through its full path ranks below name matches.
const PATH_PENALTY: i32 = 30;
/// Folders that match the words in zoxide's way (T6.5) come before the fuzzy
/// matches, in order of use.
const KEYWORD_MATCH: i32 = 2_000_000;
/// The remembered folders offered (all of them: at most 500).
const FRECENT_MAX: usize = 500;

#[derive(Debug, Clone)]
enum Item {
    Command(&'static Command),
    /// `name`: the sidebar's label (Portuguese names of known folders).
    Folder {
        path: PathBuf,
        pinned: bool,
        name: String,
        /// Position among the folders opened, most used first (T6.5);
        /// `usize::MAX` for one never opened.
        order: usize,
    },
    /// A saved workspace (T6.13): opens in a window of its own.
    Workspace {
        name: String,
        tabs: usize,
    },
    /// Deletes a workspace (offered only when searching).
    RemoveWorkspace(String),
    /// A saved search (T6.3).
    Search(crate::store::SearchState),
    /// Forgets a saved search (offered only when searching).
    RemoveSearch(String),
}

impl Item {
    fn label(&self) -> String {
        match self {
            Item::Command(c) => c.label().to_owned(),
            Item::Folder { name, .. } => name.clone(),
            Item::Workspace { name, .. } => trf!("Espaço: {}", "Workspace: {}", name),
            Item::RemoveWorkspace(name) => {
                trf!("Excluir espaço: {}", "Delete workspace: {}", name)
            }
            Item::Search(q) => trf!("Busca: {}", "Search: {}", q.query),
            Item::RemoveSearch(query) => trf!("Esquecer busca: {}", "Forget search: {}", query),
        }
    }
}

pub(super) struct Palette {
    query: String,
    items: Vec<Item>,
    /// Indices into `items`, in display order.
    shown: Vec<usize>,
    cursor: usize,
    scroll: UniformListScrollHandle,
    /// Typing the name of a workspace to save (T6.13), not searching.
    naming: bool,
}

impl Palette {
    /// Recomputes `shown` for the query (a few hundred items: well under a
    /// millisecond).
    fn filter(&mut self) {
        self.cursor = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if self.naming {
            self.shown.clear();
            return;
        }
        if self.query.trim().is_empty() {
            let recent: Vec<usize> = (0..self.items.len())
                .filter(|&i| matches!(self.items[i], Item::Folder { pinned: false, .. }))
                .take(RECENT_FIRST)
                .collect();
            let pinned = (0..self.items.len())
                .filter(|&i| matches!(self.items[i], Item::Folder { pinned: true, .. }));
            let spaces = (0..self.items.len())
                .filter(|&i| matches!(self.items[i], Item::Workspace { .. } | Item::Search(_)));
            let commands =
                (0..self.items.len()).filter(|&i| matches!(self.items[i], Item::Command(_)));
            self.shown = recent
                .into_iter()
                .chain(pinned)
                .chain(spaces)
                .chain(commands)
                .collect();
            return;
        }
        let mut scored: Vec<(usize, i32, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let label = item.label();
                // "proj fx": every word in the path, in order, the last in
                // the folder's name; the most used first.
                if let Item::Folder { path, order, .. } = item
                    && fx_core::frecency::keywords_match(&self.query, path)
                {
                    let rank = i32::try_from((*order).min(1_000_000)).unwrap_or(1_000_000);
                    return Some((i, KEYWORD_MATCH - rank, label.len()));
                }
                let score = fuzzy_score(&self.query, &label).or_else(|| match item {
                    Item::Folder { path, .. } => {
                        fuzzy_score(&self.query, &path.to_string_lossy()).map(|s| s - PATH_PENALTY)
                    }
                    _ => None,
                })?;
                Some((i, score, label.len()))
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
        self.shown = scored.into_iter().map(|(i, _, _)| i).collect();
    }
}

impl TabsView {
    pub(super) fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    /// The palette asking the name of a workspace to save (T6.13).
    pub(super) fn open_palette_naming(&mut self, cx: &mut Context<Self>) {
        let mut p = Palette {
            query: String::new(),
            items: Vec::new(),
            shown: Vec::new(),
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
            naming: true,
        };
        p.filter();
        self.palette = Some(p);
        cx.notify();
    }

    pub(super) fn open_palette(&mut self, cx: &mut Context<Self>) {
        let mut items: Vec<Item> = Vec::new();
        // The folders opened, most used first (frequency and recency, T6.5);
        // then this session's: other tabs, every tab's history, newest first.
        // No duplicates, not the current folder.
        let current = self.current().read(cx).path().to_path_buf();
        let frecent = crate::visits::top(FRECENT_MAX);
        let mut recent: Vec<PathBuf> = Vec::new();
        let mut push = |p: PathBuf| {
            if p != current
                && !recent.contains(&p)
                && !frecent.contains(&p)
                && recent.len() < RECENT_MAX
            {
                recent.push(p);
            }
        };
        for tab in self.tabs.iter() {
            push(tab.read(cx).path().to_path_buf());
        }
        for tab in self.tabs.iter() {
            for p in tab.read(cx).recent_folders() {
                push(p);
            }
        }
        let places = self.places.read(cx);
        let folder = |path: PathBuf, pinned: bool, order: usize| Item::Folder {
            name: places.label(&path).to_string(),
            path,
            pinned,
            order,
        };
        items.extend(
            frecent
                .iter()
                .enumerate()
                .filter(|(_, p)| **p != current)
                .map(|(k, p)| folder(p.clone(), false, k)),
        );
        items.extend(recent.into_iter().map(|p| folder(p, false, usize::MAX)));
        items.extend(places.pinned.paths().iter().map(|p| {
            let order = frecent.iter().position(|f| f == p).unwrap_or(usize::MAX);
            folder(p.clone(), true, order)
        }));
        let spaces = crate::workspaces::list();
        items.extend(spaces.iter().map(|w| Item::Workspace {
            name: w.name.clone(),
            tabs: w.tabs.len(),
        }));
        items.extend(COMMANDS.iter().map(Item::Command));
        items.extend(spaces.into_iter().map(|w| Item::RemoveWorkspace(w.name)));
        let saved = crate::searches::list();
        items.extend(saved.iter().map(|q| Item::RemoveSearch(q.query.clone())));
        items.extend(saved.into_iter().map(Item::Search));
        let mut palette = Palette {
            query: String::new(),
            items,
            shown: Vec::new(),
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
            naming: false,
        };
        palette.filter();
        self.palette = Some(palette);
        self.history_open = false;
        cx.notify();
    }

    /// Keys while the palette is open (it takes all of them).
    pub(super) fn palette_key(
        &mut self,
        event: &KeyDownEvent,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let mut query = p.query.clone();
        match ks.key.as_str() {
            "escape" => {
                self.palette = None;
                self.focus_active(window, cx);
                cx.notify();
                return;
            }
            "enter" if p.naming => {
                let name = p.query.trim().to_owned();
                if !name.is_empty() {
                    self.palette = None;
                    self.save_workspace(name, cx);
                    self.focus_active(window, cx);
                }
                cx.notify();
                return;
            }
            "enter" => {
                let choice = p.shown.get(p.cursor).map(|&i| p.items[i].clone());
                self.palette = None;
                if let Some(item) = choice {
                    self.run_item(item, window, cx);
                }
                self.focus_active(window, cx);
                cx.notify();
                return;
            }
            "down" | "up" => {
                let n = p.shown.len();
                if n > 0 {
                    p.cursor = if ks.key == "down" {
                        (p.cursor + 1) % n
                    } else {
                        (p.cursor + n - 1) % n
                    };
                    p.scroll.scroll_to_item(p.cursor, ScrollStrategy::Top);
                    cx.notify();
                }
                return;
            }
            "backspace" => {
                query.pop();
            }
            "space" => query.push(' '),
            _ => {
                if m.control || m.alt || m.platform || m.function {
                    return;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => query.push_str(c),
                    _ => return,
                }
            }
        }
        if query != p.query {
            p.query = query;
            p.filter();
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "palette");
            }
            cx.notify();
        }
    }

    /// The palette as a screen reader reads it: the query field and the
    /// results, with the focus on the chosen one.
    pub(super) fn palette_overlay(
        &self,
    ) -> Option<(crate::a11y::Overlay, Vec<Option<super::WindowAct>>)> {
        use crate::a11y::{Item as A11yItem, Overlay};
        let p = self.palette.as_ref()?;
        let mut items = vec![A11yItem::Input {
            label: if p.naming {
                tr!("Nome do espaço de trabalho", "Workspace name")
            } else {
                tr!("Comando ou pasta", "Command or folder")
            }
            .to_owned(),
            value: p.query.clone(),
        }];
        let mut acts = vec![None];
        for (k, &i) in p.shown.iter().enumerate() {
            let description = match &p.items[i] {
                Item::Command(c) => c.shortcut(),
                Item::Folder { path, .. } => Some(path.display().to_string()),
                Item::Workspace { tabs, .. } => {
                    let tabs = super::workspace::tabs_text(*tabs);
                    Some(trf!(
                        "{tabs}, abre numa janela nova",
                        "{tabs}, opens in a new window"
                    ))
                }
                Item::Search(q) => Some(if q.disk {
                    tr!(
                        "busca salva, no disco inteiro",
                        "saved search, on the whole disk"
                    )
                    .to_owned()
                } else {
                    trf!(
                        "busca salva, em {}",
                        "saved search, in {}",
                        q.root.display()
                    )
                }),
                Item::RemoveWorkspace(_) | Item::RemoveSearch(_) => None,
            };
            items.push(A11yItem::Choice {
                label: p.items[i].label(),
                description,
                selected: k == p.cursor,
            });
            acts.push(Some(super::WindowAct::Palette(k)));
        }
        let focus = if p.shown.is_empty() {
            Some(0)
        } else {
            Some(1 + p.cursor)
        };
        Some((
            Overlay {
                title: tr!("Paleta de comandos", "Command palette").to_owned(),
                description: Some(
                    if p.naming {
                        tr!(
                            "Digite o nome do espaço de trabalho. Enter salva as abas desta janela, Esc cancela.",
                            "Type the workspace name. Enter saves this window's tabs, Esc cancels."
                        )
                    } else {
                        tr!(
                            "Digite para filtrar. ↑ e ↓ escolhem, Enter executa, Esc fecha.",
                            "Type to filter. ↑ and ↓ choose, Enter runs, Esc closes."
                        )
                    }
                    .to_owned(),
                ),
                items,
                focus,
                positions: None,
                modal: true,
            },
            acts,
        ))
    }

    /// A screen reader clicked result `k`: runs it, as Enter would.
    pub(super) fn palette_run(&mut self, k: usize, window: &mut Window, cx: &mut Context<Self>) {
        let choice = self
            .palette
            .as_ref()
            .and_then(|p| p.shown.get(k).map(|&i| p.items[i].clone()));
        self.palette = None;
        if let Some(item) = choice {
            self.run_item(item, window, cx);
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn run_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.current();
        match item {
            Item::Folder { path, .. } => {
                tab.update(cx, |view, cx| view.open(path, window, cx));
            }
            Item::Workspace { name, .. } => self.open_workspace(&name, cx),
            Item::Search(q) => tab.update(cx, |view, cx| view.open_saved_search(&q, cx)),
            Item::RemoveSearch(query) => {
                crate::searches::remove(&query);
                let text = trf!("Busca \"{}\" esquecida", "Search \"{}\" forgotten", query);
                tab.update(cx, |view, cx| view.show_notice(text, cx));
            }
            Item::RemoveWorkspace(name) => {
                crate::workspaces::remove(&name);
                let text = trf!("Espaço \"{}\" excluído", "Workspace \"{}\" deleted", name);
                tab.update(cx, |view, cx| view.show_notice(text, cx));
            }
            Item::Command(c) => match c.action {
                Action::Key(Target::Window, keys) => {
                    if let Ok(keystroke) = Keystroke::parse(keys) {
                        let event = KeyDownEvent {
                            keystroke,
                            is_held: false,
                        };
                        self.on_key_down(&event, window, cx);
                    }
                }
                Action::Key(Target::Tab, keys) => {
                    tab.update(cx, |view, cx| view.press(keys, window, cx));
                }
                Action::ToggleGrid => tab.update(cx, |view, cx| view.toggle_grid(cx)),
                Action::Sort(key) => tab.update(cx, |view, cx| view.sort_by_command(key, cx)),
                Action::IndexService(on) => tab.update(cx, |view, cx| view.index_service(on, cx)),
                Action::DefaultManager(on) => {
                    tab.update(cx, |view, cx| view.default_manager(on, cx));
                }
                Action::CloudPin(keep) => tab.update(cx, |view, cx| view.cloud_pin(keep, cx)),
                Action::Update => tab.update(cx, |view, cx| view.update_command(cx)),
                Action::UpdateChecks => tab.update(cx, |view, cx| {
                    let text = if crate::prefs::UPDATES.switch() {
                        tr!("Atualizações automáticas ligadas", "Automatic updates on")
                    } else {
                        tr!(
                            "Atualizações automáticas desligadas",
                            "Automatic updates off"
                        )
                    };
                    view.show_notice(text.to_owned(), cx);
                }),
                Action::ConfirmRecycle => tab.update(cx, |view, cx| {
                    let text = if crate::prefs::CONFIRM_RECYCLE.switch() {
                        tr!(
                            "O fx vai perguntar antes de enviar para a Lixeira",
                            "fx will ask before sending to the Recycle Bin"
                        )
                    } else {
                        tr!(
                            "Delete envia para a Lixeira sem perguntar (Ctrl+Z desfaz)",
                            "Delete sends to the Recycle Bin without asking (Ctrl+Z undoes it)"
                        )
                    };
                    view.show_notice(text.to_owned(), cx);
                }),
                Action::ReportCrash => tab.update(cx, |view, cx| view.report_crash(cx)),
                Action::CrashReports => tab.update(cx, |view, cx| {
                    let text = if crate::prefs::CRASH_REPORTS.switch() {
                        tr!(
                            "Relatórios de falha ligados (ficam só neste computador)",
                            "Crash reports on (kept on this computer only)"
                        )
                    } else {
                        tr!(
                            "Relatórios de falha desligados: nada é gravado",
                            "Crash reports off: nothing is written"
                        )
                    };
                    view.show_notice(text.to_owned(), cx);
                }),
                Action::Language => {
                    crate::i18n::switch();
                    cx.refresh_windows();
                }
                Action::About => self.open_settings(super::settings::Section::About, cx),
                Action::SaveWorkspace => self.open_palette_naming(cx),
                Action::Compare => self.start_compare(cx),
                Action::Image(action) => {
                    tab.update(cx, |view, cx| view.image_action(action, window, cx))
                }
                Action::Group(by) => tab.update(cx, |view, cx| view.set_group_by(by, cx)),
                Action::MediaColumn(col) => {
                    tab.update(cx, |view, cx| view.toggle_media_column(col, cx))
                }
            },
        }
    }

    pub(super) fn render_palette(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let p = self.palette.as_ref()?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        struct Row {
            glyph: &'static str,
            label: SharedString,
            detail: SharedString,
            shortcut: SharedString,
        }
        let rows: Vec<Row> = p
            .shown
            .iter()
            .map(|&i| match &p.items[i] {
                Item::Command(c) => Row {
                    glyph: c.icon(),
                    label: c.label().into(),
                    detail: SharedString::default(),
                    shortcut: c.shortcut().unwrap_or_default().into(),
                },
                item @ Item::Folder { path, pinned, .. } => Row {
                    glyph: if *pinned { ICON_PIN } else { ICON_FOLDER },
                    label: item.label().into(),
                    detail: path.display().to_string().into(),
                    shortcut: if *pinned {
                        tr!("Acesso rápido", "Quick access")
                    } else {
                        tr!("Recente", "Recent")
                    }
                    .into(),
                },
                item @ Item::Workspace { tabs, .. } => Row {
                    glyph: "\u{E7C4}", // TaskView
                    label: item.label().into(),
                    detail: super::workspace::tabs_text(*tabs).into(),
                    shortcut: tr!("Nova janela", "New window").into(),
                },
                item @ Item::Search(q) => Row {
                    glyph: "\u{E721}", // Search
                    label: item.label().into(),
                    detail: if q.disk {
                        tr!("no disco inteiro", "on the whole disk").into()
                    } else {
                        q.root.display().to_string().into()
                    },
                    shortcut: tr!("Busca salva", "Saved search").into(),
                },
                item @ Item::RemoveSearch(_) => Row {
                    glyph: "\u{E74D}",
                    label: item.label().into(),
                    detail: SharedString::default(),
                    shortcut: SharedString::default(),
                },
                item @ Item::RemoveWorkspace(_) => Row {
                    glyph: "\u{E74D}", // Delete
                    label: item.label().into(),
                    detail: SharedString::default(),
                    shortcut: SharedString::default(),
                },
            })
            .collect();
        let rows = std::sync::Arc::new(rows);
        let cursor = p.cursor;
        let (selected, secondary) = (t.selected, t.text_secondary);
        let list = uniform_list("palette-items", rows.len(), move |range, _, _| {
            range
                .map(|i| {
                    let r = &rows[i];
                    div()
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(space(3)))
                        .flex()
                        .items_center()
                        .gap(px(space(3)))
                        .rounded(px(RADIUS))
                        .when(i == cursor, |d| d.bg(rgba(selected)))
                        .child(
                            div()
                                .flex_none()
                                .w(px(16.0))
                                .font_family(icon_font)
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(secondary))
                                .child(r.glyph),
                        )
                        .child(
                            div()
                                .flex_none()
                                .max_w(px(240.0))
                                .truncate()
                                .child(r.label.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(secondary))
                                .child(r.detail.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(secondary))
                                .child(r.shortcut.clone()),
                        )
                })
                .collect()
        })
        .track_scroll(p.scroll.clone())
        .flex_1()
        .px(px(space(1)));
        // Centered at the top of the window (the backdrop lays it out: margins
        // do not center an absolute element).
        let card = div()
            .id("palette")
            .w_full()
            .max_w(px(PANEL_WIDTH))
            // Naming a workspace needs only a line, not the list's height.
            .when(!p.naming, |d| d.h(px(400.0)))
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
                    .h(px(44.0))
                    .px(px(space(4)))
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(rgba(t.divider))
                    .child(if p.query.is_empty() {
                        div().text_color(rgba(t.text_secondary)).child(if p.naming {
                            tr!(
                                "Nome do espaço de trabalho (ex.: Trabalho)…",
                                "Workspace name (e.g. Work)…"
                            )
                        } else {
                            tr!("Digite um comando ou uma pasta…", "Type a command or a folder…")
                        })
                    } else {
                        div().truncate().child(format!("{}|", p.query))
                    }),
            )
            .child(if p.naming {
                // Saving a workspace: what Enter will do.
                let tabs = super::workspace::tabs_text(self.tabs.iter().count());
                div()
                    .flex()
                    .justify_center()
                    .py(px(space(6)))
                    .px(px(space(4)))
                    .text_color(rgba(t.text_secondary))
                    .child(trf!(
                        "Enter salva esta janela ({tabs}) com esse nome; o mesmo nome atualiza o espaço.",
                        "Enter saves this window ({tabs}) under that name; the same name updates the workspace."
                    ))
                    .into_any_element()
            } else if p.shown.is_empty() {
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .pt(px(space(8)))
                    .px(px(space(4)))
                    .text_color(rgba(t.text_secondary))
                    .child(trf!(
                        "Nenhum comando ou pasta com \"{}\". Tente outra palavra.",
                        "No command or folder with \"{}\". Try another word.",
                        p.query
                    ))
                    .into_any_element()
            } else {
                list.into_any_element()
            })
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(if p.naming {
                        tr!("Enter salva · Esc cancela", "Enter saves · Esc cancels").to_owned()
                    } else if p.shown.is_empty() {
                        tr!("Nada encontrado", "Nothing found").to_owned()
                    } else {
                        tr!(
                            "↑↓ escolhe · Enter executa · Esc fecha",
                            "↑↓ chooses · Enter runs · Esc closes"
                        )
                        .to_owned()
                    }),
            );
        Some(
            deferred(crate::motion::enter(
                "palette-in",
                div()
                    .id("palette-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .justify_center()
                    .items_start()
                    .pt(px(space(12)))
                    .px(px(space(4)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.palette = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(3),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn palette(folders: &[(&str, bool)]) -> Palette {
        let mut items: Vec<Item> = folders
            .iter()
            .enumerate()
            .map(|(k, (p, pinned))| Item::Folder {
                path: PathBuf::from(p),
                pinned: *pinned,
                name: Path::new(p)
                    .file_name()
                    .map_or_else(|| p.to_string(), |n| n.to_string_lossy().into_owned()),
                // In the order given: as if the first were the most used.
                order: k,
            })
            .collect();
        items.extend(COMMANDS.iter().map(Item::Command));
        let mut p = Palette {
            query: String::new(),
            items,
            shown: Vec::new(),
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
            naming: false,
        };
        p.filter();
        p
    }

    fn first_label(p: &Palette) -> String {
        p.items[p.shown[0]].label()
    }

    #[test]
    fn empty_query_lists_recent_pinned_then_commands() {
        let p = palette(&[(r"C:\Projetos\fx", false), (r"C:\Fotos", true)]);
        assert_eq!(first_label(&p), "fx");
        assert_eq!(p.items[p.shown[1]].label(), "Fotos");
        assert_eq!(p.shown.len(), 2 + COMMANDS.len());
    }

    #[test]
    fn query_mixes_commands_and_folders() {
        let mut p = palette(&[(r"C:\Fotos\Praia 2026", false), (r"D:\Documentos", true)]);
        p.query = "praia".into();
        p.filter();
        assert_eq!(first_label(&p), "Praia 2026");
        p.query = "nova pasta".into();
        p.filter();
        assert_eq!(first_label(&p), "Nova pasta");
        // A folder found only by its path still shows up.
        p.query = "d:\\doc".into();
        p.filter();
        assert_eq!(first_label(&p), "Documentos");
        p.query = "zzzz".into();
        p.filter();
        assert!(p.shown.is_empty());
    }

    /// T6.13: workspaces show after the pinned folders; "Excluir" only when
    /// searching; naming lists nothing.
    #[test]
    fn workspaces_in_the_palette() {
        let mut p = palette(&[(r"C:\Fotos", true)]);
        p.items.push(Item::Workspace {
            name: "Trabalho".into(),
            tabs: 3,
        });
        p.items.push(Item::RemoveWorkspace("Trabalho".into()));
        p.filter();
        assert_eq!(
            p.items[p.shown[1]].label(),
            Item::Workspace {
                name: "Trabalho".into(),
                tabs: 3
            }
            .label()
        );
        assert!(
            !p.shown
                .iter()
                .any(|&i| matches!(p.items[i], Item::RemoveWorkspace(_))),
            "no delete without searching"
        );
        p.query = "excluir trab".into();
        p.filter();
        assert!(matches!(p.items[p.shown[0]], Item::RemoveWorkspace(_)));
        p.naming = true;
        p.filter();
        assert!(p.shown.is_empty());
    }

    /// T6.5: "proj fx" takes to the folder, the most used first, before any
    /// command or fuzzy match.
    #[test]
    fn words_find_the_most_used_folder() {
        let mut p = palette(&[
            (r"C:\Users\ana\Projetos\fx", false),
            (r"C:\Antigo\fx", false),
            (r"C:\Projetos\fx-site", false),
        ]);
        let path_of = |p: &Palette, k: usize| match &p.items[p.shown[k]] {
            Item::Folder { path, .. } => path.clone(),
            other => PathBuf::from(other.label()),
        };
        p.query = "proj fx".into();
        p.filter();
        assert_eq!(path_of(&p, 0), PathBuf::from(r"C:\Users\ana\Projetos\fx"));
        assert_eq!(path_of(&p, 1), PathBuf::from(r"C:\Projetos\fx-site"));
        p.query = "fx".into();
        p.filter();
        assert_eq!(path_of(&p, 0), PathBuf::from(r"C:\Users\ana\Projetos\fx"));
        assert_eq!(path_of(&p, 1), PathBuf::from(r"C:\Antigo\fx"));
        // "Sobre o fx" (a command) comes after the folders.
        assert!(matches!(p.items[p.shown[3]], Item::Command(_)));
    }

    #[test]
    fn filtering_is_far_below_a_frame() {
        let folders: Vec<(String, bool)> = (0..300)
            .map(|i| (format!(r"C:\Projetos\cliente {i}\docs"), i % 10 == 0))
            .collect();
        let refs: Vec<(&str, bool)> = folders.iter().map(|(p, b)| (p.as_str(), *b)).collect();
        let mut p = palette(&refs);
        let mut worst = std::time::Duration::ZERO;
        for q in ["c", "cl", "cli", "clie", "client", "cliente 2"] {
            p.query = q.into();
            let started = Instant::now();
            p.filter();
            worst = worst.max(started.elapsed());
        }
        println!("palette filter, 336 items, worst key: {worst:?}");
        // Debug build; the release number goes to the report.
        assert!(worst.as_millis() < 16, "{worst:?}");
    }
}
