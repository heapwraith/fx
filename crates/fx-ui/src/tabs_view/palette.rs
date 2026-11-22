//! Command palette (task T1.24): Ctrl+K or Ctrl+Shift+P. Fuzzy search over
//! fx's commands (with their shortcut), Quick access and recent folders;
//! Enter runs the choice. A command runs its own shortcut on the window or
//! the active tab (`crate::commands`), so both paths behave the same.

#[allow(unused_imports)]
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
const ICON_COMMAND: &str = "\u{E756}";

#[derive(Debug, Clone)]
enum Item {
    Command(&'static Command),
    /// `name`: the sidebar's label (Portuguese names of known folders).
    Folder {
        path: PathBuf,
        pinned: bool,
        name: String,
    },
}

impl Item {
    fn label(&self) -> String {
        match self {
            Item::Command(c) => c.label().to_owned(),
            Item::Folder { name, .. } => name.clone(),
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
}

impl Palette {
    /// Recomputes `shown` for the query (a few hundred items: well under a
    /// millisecond).
    fn filter(&mut self) {
        self.cursor = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if self.query.trim().is_empty() {
            let recent: Vec<usize> = (0..self.items.len())
                .filter(|&i| matches!(self.items[i], Item::Folder { pinned: false, .. }))
                .take(RECENT_FIRST)
                .collect();
            let pinned = (0..self.items.len())
                .filter(|&i| matches!(self.items[i], Item::Folder { pinned: true, .. }));
            let commands =
                (0..self.items.len()).filter(|&i| matches!(self.items[i], Item::Command(_)));
            self.shown = recent.into_iter().chain(pinned).chain(commands).collect();
            return;
        }
        let mut scored: Vec<(usize, i32, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                let label = item.label();
                let score = fuzzy_score(&self.query, &label).or_else(|| match item {
                    Item::Folder { path, .. } => {
                        fuzzy_score(&self.query, &path.to_string_lossy()).map(|s| s - PATH_PENALTY)
                    }
                    Item::Command(_) => None,
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

    pub(super) fn open_palette(&mut self, cx: &mut Context<Self>) {
        let mut items: Vec<Item> = Vec::new();
        // Recent folders: other tabs first, then every tab's history, newest
        // first; no duplicates, not the current folder.
        let current = self.current().read(cx).path().to_path_buf();
        let mut recent: Vec<PathBuf> = Vec::new();
        let mut push = |p: PathBuf| {
            if p != current && !recent.contains(&p) && recent.len() < RECENT_MAX {
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
        let folder = |path: PathBuf, pinned: bool| Item::Folder {
            name: places.label(&path).to_string(),
            path,
            pinned,
        };
        items.extend(recent.into_iter().map(|p| folder(p, false)));
        items.extend(
            places
                .pinned
                .paths()
                .iter()
                .map(|p| folder(p.clone(), true)),
        );
        items.extend(COMMANDS.iter().map(Item::Command));
        let mut palette = Palette {
            query: String::new(),
            items,
            shown: Vec::new(),
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
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

    fn run_item(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.current();
        match item {
            Item::Folder { path, .. } => {
                tab.update(cx, |view, cx| view.open(path, window, cx));
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
                    let text = if crate::update::switch() {
                        tr!(
                            "Verificação diária de atualizações ligada",
                            "Daily update check on"
                        )
                    } else {
                        tr!(
                            "Verificação diária de atualizações desligada",
                            "Daily update check off"
                        )
                    };
                    view.show_notice(text.to_owned(), cx);
                }),
                Action::ReportCrash => tab.update(cx, |view, cx| view.report_crash(cx)),
                Action::CrashReports => tab.update(cx, |view, cx| {
                    let text = if crate::crash::switch() {
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
                Action::About => tab.update(cx, |view, cx| {
                    view.show_notice(format!("fx {} · MIT OR Apache-2.0", crate::VERSION), cx);
                }),
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
                    glyph: ICON_COMMAND,
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
        let card = div()
            .id("palette")
            .absolute()
            .top(px(space(12)))
            .left(px(space(4)))
            .right(px(space(4)))
            .mx_auto()
            .max_w(px(PANEL_WIDTH))
            .h(px(400.0))
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
                        div().text_color(rgba(t.text_secondary)).child(tr!(
                            "Digite um comando ou uma pasta…",
                            "Type a command or a folder…"
                        ))
                    } else {
                        div().truncate().child(format!("{}|", p.query))
                    }),
            )
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
                    .child(if p.shown.is_empty() {
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
            deferred(
                div()
                    .id("palette-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.palette = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
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
            .map(|(p, pinned)| Item::Folder {
                path: PathBuf::from(p),
                pinned: *pinned,
                name: Path::new(p)
                    .file_name()
                    .map_or_else(|| p.to_string(), |n| n.to_string_lossy().into_owned()),
            })
            .collect();
        items.extend(COMMANDS.iter().map(Item::Command));
        let mut p = Palette {
            query: String::new(),
            items,
            shown: Vec::new(),
            cursor: 0,
            scroll: UniformListScrollHandle::new(),
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
