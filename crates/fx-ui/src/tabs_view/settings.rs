//! Settings (the 2026 pattern of Linear, Notion, Slack, VS Code, Zed: a gear
//! at the bottom of the sidebar and Ctrl+, open a dialog with categories on
//! the left; "Sobre" is the last category). Every control shows the real
//! state and acts on it; nothing here is a copy of a setting.

use std::sync::{Arc, OnceLock};

use fx_core::{tr, trf};
use gpui::{
    Context, Image, ImageFormat, MouseButton, MouseDownEvent, SharedString, deferred, div, img,
    prelude::*, px,
};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, Density, RADIUS, rgba, space};

const LOGO: &[u8] = include_bytes!("../../resources/logo-128.png");

/// The logo, made once (not copied and decoded again on every frame).
fn logo() -> Arc<Image> {
    static IMAGE: OnceLock<Arc<Image>> = OnceLock::new();
    IMAGE
        .get_or_init(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec())))
        .clone()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Updates,
    Privacy,
    System,
    About,
}

impl Section {
    pub(super) const ALL: [Section; 6] = [
        Section::General,
        Section::Appearance,
        Section::Updates,
        Section::Privacy,
        Section::System,
        Section::About,
    ];

    fn label(self) -> &'static str {
        match self {
            Section::General => tr!("Geral", "General"),
            Section::Appearance => tr!("Aparência", "Appearance"),
            Section::Updates => tr!("Atualizações", "Updates"),
            Section::Privacy => tr!("Privacidade", "Privacy"),
            Section::System => tr!("Sistema", "System"),
            Section::About => tr!("Sobre o fx", "About fx"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Section::General => "\u{E713}",
            Section::Appearance => "\u{E790}",
            Section::Updates => "\u{E895}",
            Section::Privacy => "\u{E72E}",
            Section::System => "\u{E7F8}",
            Section::About => "\u{E946}",
        }
    }
}

/// What a control does when used.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Act {
    /// `true`: English. Choosing the language already in use does nothing.
    Language(bool),
    ConfirmRecycle,
    RestoreSession,
    Density(Density),
    Hidden,
    Sidebar,
    AutoUpdate,
    CheckNow,
    CrashReports,
    DefaultManager(bool),
    IndexService(bool),
    HowTo,
    Link(&'static str),
}

impl TabsView {
    pub(super) fn open_settings(&mut self, section: Section, cx: &mut Context<Self>) {
        if self.settings != Some(section) {
            self.settings_focus = 0;
        }
        if self.settings.is_none() {
            self.settings_keyboard = false;
        }
        self.settings = Some(section);
        cx.notify();
    }

    /// Tab / Shift+Tab: the next or previous control of the section.
    pub(super) fn settings_move_focus(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(section) = self.settings else {
            return;
        };
        let n = controls(&self.setting_rows(section)).len();
        if n > 0 && !self.settings_keyboard {
            // The first Tab shows where the focus is.
            self.settings_keyboard = true;
            cx.notify();
        } else if n > 0 {
            self.settings_focus = if forward {
                (self.settings_focus + 1) % n
            } else {
                (self.settings_focus + n - 1) % n
            };
            cx.notify();
        }
    }

    /// Space / Enter: uses the focused control (only once it is visible).
    pub(super) fn settings_activate(&mut self, cx: &mut Context<Self>) {
        let Some(section) = self.settings.filter(|_| self.settings_keyboard) else {
            return;
        };
        let act = controls(&self.setting_rows(section))
            .get(self.settings_focus)
            .map(|c| c.act);
        if let Some(act) = act {
            self.act(act, cx);
        }
    }

    pub(super) fn act(&mut self, act: Act, cx: &mut Context<Self>) {
        let tab = self.current();
        match act {
            Act::Language(english) => {
                if fx_core::locale::english() != english {
                    crate::i18n::switch();
                    cx.refresh_windows();
                }
            }
            Act::ConfirmRecycle => {
                crate::prefs::CONFIRM_RECYCLE.switch();
            }
            Act::RestoreSession => {
                crate::prefs::RESTORE_SESSION.switch();
            }
            Act::Density(d) => self.set_density(d, cx),
            Act::Hidden => self.toggle_hidden(cx),
            Act::Sidebar => self.sidebar_open = !self.sidebar_open,
            Act::AutoUpdate => {
                crate::prefs::UPDATES.switch();
            }
            Act::CheckNow => tab.update(cx, |view, cx| view.check_updates(false, cx)),
            Act::CrashReports => {
                crate::prefs::CRASH_REPORTS.switch();
            }
            Act::DefaultManager(on) => tab.update(cx, |view, cx| view.default_manager(on, cx)),
            Act::IndexService(on) => tab.update(cx, |view, cx| view.index_service(on, cx)),
            Act::HowTo => {
                self.settings = None;
                self.open_guide(cx);
            }
            Act::Link(url) => tab.update(cx, |view, cx| view.open_url(url, cx)),
        }
        cx.notify();
    }
}

/// The control of a settings row.
pub(super) enum Control {
    None,
    Switch(bool, Act),
    /// A choice among options: label, selected, action.
    Segments(Vec<(&'static str, bool, Act)>),
    /// A value shown before the buttons (the version), then the buttons.
    Buttons(Option<String>, Vec<(&'static str, Act)>),
}

/// One settings row: what it is, what it does, how to change it. The same
/// data draws the screen and the screen reader's tree (review, D-066).
pub(super) struct Row {
    pub title: &'static str,
    pub detail: &'static str,
    pub control: Control,
}

/// A control that takes the keyboard (each option of a choice is one).
pub(super) struct Focusable {
    pub label: String,
    pub kind: FocusKind,
    pub act: Act,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusKind {
    Switch(bool),
    Option(bool),
    Button,
}

/// The controls of `rows` in Tab order.
pub(super) fn controls(rows: &[Row]) -> Vec<Focusable> {
    let mut out = Vec::new();
    for row in rows {
        match &row.control {
            Control::None => {}
            Control::Switch(on, act) => out.push(Focusable {
                label: row.title.to_owned(),
                kind: FocusKind::Switch(*on),
                act: *act,
            }),
            Control::Segments(options) => {
                for (label, selected, act) in options {
                    out.push(Focusable {
                        label: format!("{}: {label}", row.title),
                        kind: FocusKind::Option(*selected),
                        act: *act,
                    });
                }
            }
            Control::Buttons(_, buttons) => {
                for (label, act) in buttons {
                    out.push(Focusable {
                        label: (*label).to_owned(),
                        kind: FocusKind::Button,
                        act: *act,
                    });
                }
            }
        }
    }
    out
}

impl TabsView {
    /// The rows of `section`, with the real state of each setting.
    pub(super) fn setting_rows(&self, section: Section) -> Vec<Row> {
        let english = fx_core::locale::english();
        let density = self.look.density;
        match section {
            Section::General => vec![
                Row {
                    title: tr!("Idioma", "Language"),
                    detail: tr!(
                        "O padrão segue o idioma do Windows.",
                        "By default it follows the Windows language."
                    ),
                    control: Control::Segments(vec![
                        ("Português", !english, Act::Language(false)),
                        ("English", english, Act::Language(true)),
                    ]),
                },
                Row {
                    title: tr!(
                        "Confirmar antes de enviar para a Lixeira",
                        "Confirm before sending to the Recycle Bin"
                    ),
                    detail: tr!(
                        "Excluir de vez (Shift+Delete) sempre pergunta.",
                        "Deleting for good (Shift+Delete) always asks."
                    ),
                    control: Control::Switch(
                        crate::prefs::CONFIRM_RECYCLE.get(),
                        Act::ConfirmRecycle,
                    ),
                },
                Row {
                    title: tr!(
                        "Reabrir abas e janelas ao iniciar",
                        "Reopen tabs and windows at start"
                    ),
                    detail: tr!(
                        "Vale na próxima abertura do fx.",
                        "Applies the next time fx opens."
                    ),
                    control: Control::Switch(
                        crate::prefs::RESTORE_SESSION.get(),
                        Act::RestoreSession,
                    ),
                },
            ],
            Section::Appearance => vec![
                Row {
                    title: tr!("Densidade das linhas", "Row density"),
                    detail: tr!(
                        "Ctrl+1, Ctrl+2 e Ctrl+3 também trocam.",
                        "Ctrl+1, Ctrl+2 and Ctrl+3 switch it too."
                    ),
                    control: Control::Segments(vec![
                        (
                            tr!("Compacta", "Compact"),
                            density == Density::Compact,
                            Act::Density(Density::Compact),
                        ),
                        (
                            tr!("Padrão", "Standard"),
                            density == Density::Standard,
                            Act::Density(Density::Standard),
                        ),
                        (
                            tr!("Confortável", "Comfortable"),
                            density == Density::Comfortable,
                            Act::Density(Density::Comfortable),
                        ),
                    ]),
                },
                Row {
                    title: tr!("Mostrar itens ocultos", "Show hidden items"),
                    detail: tr!(
                        "Arquivos ocultos e de sistema (Ctrl+H).",
                        "Hidden and system files (Ctrl+H)."
                    ),
                    control: Control::Switch(self.look.show_hidden, Act::Hidden),
                },
                Row {
                    title: tr!("Barra lateral", "Sidebar"),
                    detail: tr!(
                        "Acesso rápido e unidades (Ctrl+B).",
                        "Quick access and drives (Ctrl+B)."
                    ),
                    control: Control::Switch(self.sidebar_open, Act::Sidebar),
                },
                Row {
                    title: tr!("Tema", "Theme"),
                    detail: tr!(
                        "Segue o tema claro ou escuro do Windows, e o alto contraste.",
                        "Follows the Windows light or dark theme, and high contrast."
                    ),
                    control: Control::None,
                },
            ],
            Section::Updates => {
                let version = crate::VERSION.split('+').next().unwrap_or(crate::VERSION);
                vec![
                    Row {
                        title: tr!("Atualizações automáticas", "Automatic updates"),
                        detail: tr!(
                            "Baixa a versão nova em segundo plano, confere e instala quando você fecha o fx.",
                            "Downloads the new version in the background, checks it and installs it when you close fx."
                        ),
                        control: Control::Switch(crate::prefs::UPDATES.get(), Act::AutoUpdate),
                    },
                    Row {
                        title: tr!("Versão instalada", "Installed version"),
                        detail: tr!("A versão deste fx.", "This fx's version."),
                        control: Control::Buttons(
                            Some(version.to_owned()),
                            vec![(tr!("Procurar agora", "Check now"), Act::CheckNow)],
                        ),
                    },
                ]
            }
            Section::Privacy => vec![
                Row {
                    title: tr!("Relatórios de falha", "Crash reports"),
                    detail: tr!(
                        "Se o fx fechar com uma falha, guarda um relatório sem caminhos nem nomes neste computador. Nada é enviado sem você.",
                        "If fx closes with a crash, it keeps a report with no paths or names on this computer. Nothing is sent without you."
                    ),
                    control: Control::Switch(crate::prefs::CRASH_REPORTS.get(), Act::CrashReports),
                },
                Row {
                    title: tr!("Dados que o fx envia", "Data fx sends"),
                    detail: tr!(
                        "Nenhum. A verificação de atualizações só consulta a lista de versões no GitHub.",
                        "None. The update check only reads the list of versions on GitHub."
                    ),
                    control: Control::None,
                },
            ],
            Section::System => vec![
                Row {
                    title: tr!("Gerenciador de arquivos padrão", "Default file manager"),
                    detail: tr!(
                        "Pastas, unidades e Win+E abrem no fx. Voltar ao Explorador restaura tudo.",
                        "Folders, drives and Win+E open in fx. Back to Explorer restores everything."
                    ),
                    control: Control::Buttons(
                        None,
                        vec![
                            (tr!("Usar o fx", "Use fx"), Act::DefaultManager(true)),
                            (
                                tr!("Usar o Explorador", "Use Explorer"),
                                Act::DefaultManager(false),
                            ),
                        ],
                    ),
                },
                Row {
                    title: tr!("Busca no disco inteiro", "Whole-disk search"),
                    detail: tr!(
                        "Um serviço do Windows mantém o índice dos discos NTFS. Pede administrador uma vez.",
                        "A Windows service keeps the index of NTFS disks. Asks for administrator once."
                    ),
                    control: Control::Buttons(
                        None,
                        vec![
                            (tr!("Ativar", "Turn on"), Act::IndexService(true)),
                            (tr!("Desativar", "Turn off"), Act::IndexService(false)),
                        ],
                    ),
                },
            ],
            Section::About => vec![
                Row {
                    title: tr!("Licença", "License"),
                    detail: tr!(
                        "Software livre, sob as licenças MIT ou Apache 2.0. Feito em Rust, nativo do Windows.",
                        "Free software, under the MIT or Apache 2.0 licenses. Made in Rust, native to Windows."
                    ),
                    control: Control::None,
                },
                Row {
                    title: "",
                    detail: "",
                    control: Control::Buttons(
                        None,
                        vec![
                            (tr!("Como usar (F1)", "How to use (F1)"), Act::HowTo),
                            (
                                tr!("Novidades", "What's new"),
                                Act::Link(
                                    "https://github.com/heapwraith/fx/blob/main/CHANGELOG.md",
                                ),
                            ),
                            (
                                tr!("Código e sugestões", "Code and feedback"),
                                Act::Link("https://github.com/heapwraith/fx"),
                            ),
                            (
                                tr!("Relatar um problema", "Report a problem"),
                                Act::Link("https://github.com/heapwraith/fx/issues/new/choose"),
                            ),
                        ],
                    ),
                },
            ],
        }
    }

    /// Settings as a screen reader reads them: the sections, then each row
    /// and its controls; the focus on the focused control. `acts[k]` is what
    /// a click on item `k` does.
    pub(super) fn settings_overlay(
        &self,
        section: Section,
    ) -> (crate::a11y::Overlay, Vec<Option<super::WindowAct>>) {
        use crate::a11y::{Item, Overlay};
        let rows = self.setting_rows(section);
        let mut items = Vec::new();
        let mut acts = Vec::new();
        for s in Section::ALL {
            items.push(Item::Choice {
                label: s.label().to_owned(),
                description: None,
                selected: s == section,
            });
            acts.push(Some(super::WindowAct::Section(s)));
        }
        if section == Section::About {
            items.push(Item::Text(about_line()));
            acts.push(None);
        }
        let mut focus = None;
        let mut control = 0usize;
        for row in &rows {
            let text = match (row.title.is_empty(), row.detail.is_empty()) {
                (true, true) => None,
                (_, true) => Some(row.title.to_owned()),
                (true, _) => Some(row.detail.to_owned()),
                _ => Some(format!("{}. {}", row.title, row.detail)),
            };
            if let Some(text) = text {
                items.push(Item::Text(text));
                acts.push(None);
            }
            if let Control::Buttons(Some(value), _) = &row.control {
                items.push(Item::Text(value.clone()));
                acts.push(None);
            }
            for c in controls(std::slice::from_ref(row)) {
                if control == self.settings_focus {
                    focus = Some(items.len());
                }
                items.push(match c.kind {
                    FocusKind::Switch(on) => Item::Switch { label: c.label, on },
                    FocusKind::Option(selected) => Item::Radio {
                        label: c.label,
                        checked: selected,
                    },
                    FocusKind::Button => Item::Button(c.label),
                });
                acts.push(Some(super::WindowAct::Settings(c.act)));
                control += 1;
            }
        }
        let overlay = Overlay {
            title: tr!("Configurações", "Settings").to_owned(),
            description: Some(trf!(
                "{}. Tab passa entre os controles, Espaço usa, ↑ e ↓ trocam de seção, Esc fecha.",
                "{}. Tab moves between controls, Space uses one, ↑ and ↓ change section, Esc closes.",
                section.label()
            )),
            items,
            focus,
            positions: None,
            modal: true,
        };
        (overlay, acts)
    }

    pub(super) fn render_settings(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let section = self.settings?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;

        let nav: Vec<_> = Section::ALL
            .iter()
            .map(|&s| {
                let active = s == section;
                div()
                    .id(SharedString::from(format!("settings-nav-{s:?}")))
                    .h(px(32.0))
                    .px(px(space(3)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .rounded(px(RADIUS))
                    .cursor_pointer()
                    .when(active, |d| d.bg(rgba(t.selected)))
                    .when(!active, |d| d.hover(|h| h.bg(rgba(t.hover))))
                    .child(
                        div()
                            .font_family(icon_font)
                            .text_size(px(14.0))
                            .child(s.icon()),
                    )
                    .child(s.label())
                    .on_click(cx.listener(move |view, _, _, cx| view.open_settings(s, cx)))
            })
            .collect();

        let rows = self.setting_rows(section);
        // No ring until the keyboard is used (focus-visible).
        let focus = if self.settings_keyboard {
            self.settings_focus
        } else {
            usize::MAX
        };
        // The keyboard focus: a 2 px accent ring; the others get the same
        // ring, transparent, so nothing moves when it changes.
        let ring = move |focused: bool, el: gpui::AnyElement| {
            div()
                .rounded(px(RADIUS + 2.0))
                .p(px(1.0))
                .border_2()
                .border_color(rgba(if focused { t.accent } else { 0x0000_0000 }))
                .child(el)
                .into_any_element()
        };
        // The whole row takes the click (a bigger target, WCAG 2.5.8).
        let switch = |id: SharedString, on: bool| {
            div()
                .id(id)
                .w(px(40.0))
                .h(px(20.0))
                .rounded_full()
                .cursor_pointer()
                .flex()
                .items_center()
                .px(px(3.0))
                .when(on, |d| d.bg(rgba(t.accent)).justify_end())
                .when(!on, |d| d.border_1().border_color(rgba(t.text_secondary)))
                .child(div().size(px(12.0)).rounded_full().bg(rgba(if on {
                    t.base
                } else {
                    t.text_secondary
                })))
                .into_any_element()
        };
        let button = |id: SharedString, label: &'static str, act: Act, cx: &mut Context<Self>| {
            div()
                .id(id)
                .h(px(30.0))
                .px(px(space(3)))
                .flex()
                .items_center()
                .rounded(px(RADIUS))
                .border_1()
                .border_color(rgba(t.divider))
                .bg(rgba(t.layer))
                .cursor_pointer()
                .hover(|h| h.bg(rgba(t.hover)))
                .child(label)
                .on_click(cx.listener(move |view, _, _, cx| view.act(act, cx)))
                .into_any_element()
        };
        let mut next = 0usize;
        let mut body = div().flex().flex_col();
        if section == Section::About {
            body = body.child(about_header(t));
        }
        for (r, row) in rows.into_iter().enumerate() {
            // Two or more buttons go under the text, across the row (they do
            // not fit beside it in a narrow window).
            let stacked = matches!(&row.control, Control::Buttons(_, b) if b.len() > 1);
            let row_act = match row.control {
                Control::Switch(_, act) => Some(act),
                _ => None,
            };
            let control: gpui::AnyElement = match row.control {
                Control::None => div().into_any_element(),
                Control::Switch(on, _) => {
                    let id = SharedString::from(format!("set-{r}-switch"));
                    let el = ring(next == focus, switch(id, on));
                    next += 1;
                    el
                }
                Control::Segments(options) => {
                    let mut seg = div()
                        .flex()
                        .rounded(px(RADIUS))
                        .border_1()
                        .border_color(rgba(t.divider))
                        .overflow_hidden();
                    for (k, (label, selected, act)) in options.into_iter().enumerate() {
                        let focused = next == focus;
                        next += 1;
                        seg = seg.child(
                            div()
                                .id(SharedString::from(format!("set-{r}-{k}")))
                                .h(px(28.0))
                                .px(px(space(3)))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .border_2()
                                .border_color(rgba(if focused { t.accent } else { 0x0000_0000 }))
                                .when(selected, |d| {
                                    d.bg(rgba(t.selected)).text_color(rgba(t.text))
                                })
                                .when(!selected, |d| {
                                    d.text_color(rgba(t.text_secondary))
                                        .hover(|h| h.bg(rgba(t.hover)))
                                })
                                .child(label)
                                .on_click(cx.listener(move |view, _, _, cx| view.act(act, cx))),
                        );
                    }
                    seg.into_any_element()
                }
                Control::Buttons(value, buttons) => {
                    let mut line = div().flex().flex_wrap().items_center().gap(px(space(2)));
                    if let Some(v) = value {
                        line = line.child(div().text_color(rgba(t.text_secondary)).child(v));
                    }
                    for (k, (label, act)) in buttons.into_iter().enumerate() {
                        let id = SharedString::from(format!("set-{r}-b{k}"));
                        line = line.child(ring(next == focus, button(id, label, act, cx)));
                        next += 1;
                    }
                    line.into_any_element()
                }
            };
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .when(!row.title.is_empty(), |d| d.child(row.title))
                .when(!row.detail.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(row.detail),
                    )
                });
            let line = div()
                .id(SharedString::from(format!("set-row-{r}")))
                .py(px(space(3)))
                .flex()
                .border_b_1()
                .border_color(rgba(t.divider));
            let line = if stacked {
                line.flex_col().gap(px(space(2))).child(text).child(control)
            } else {
                line.items_center()
                    .gap(px(space(4)))
                    .child(text)
                    .child(div().flex_none().child(control))
            };
            let line = match row_act {
                Some(act) => line
                    .cursor_pointer()
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.settings_keyboard = false;
                        view.act(act, cx)
                    })),
                None => line,
            };
            body = body.child(line);
        }

        // Never wider or taller than the window.
        let card = div()
            .id("settings")
            .w(px(780.0))
            .max_w(gpui::relative(0.94))
            .h(px(520.0))
            .max_h(gpui::relative(0.9))
            .flex()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .w(px(200.0))
                    .flex_none()
                    .p(px(space(2)))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .bg(rgba(t.layer))
                    .border_r_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .px(px(space(3)))
                            .py(px(space(3)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr!("Configurações", "Settings")),
                    )
                    .children(nav)
                    .child(div().flex_1())
                    .child(
                        div()
                            .px(px(space(3)))
                            .py(px(space(2)))
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(tr!("Tab e Espaço: teclado", "Tab and Space: keyboard"))
                            .child(tr!("Esc fecha", "Esc closes")),
                    ),
            )
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .p(px(space(6)))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .pb(px(space(2)))
                            .child(section.label()),
                    )
                    .child(body),
            );
        Some(
            deferred(crate::motion::enter(
                "settings-in",
                div()
                    .id("settings-backdrop")
                    // The closing click never reaches the list behind.
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
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.settings = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(2),
        )
    }
}

/// "fx", the slogan and the version, at the top of "Sobre o fx".
fn about_header(t: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .py(px(space(4)))
        .flex()
        .items_center()
        .gap(px(space(4)))
        // The logo keeps its size; the text shrinks and wraps (the long
        // slogan used to squeeze the logo to nothing and run off the card).
        .child(img(logo()).flex_none().size(px(64.0)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(space(1)))
                .child(
                    div()
                        .text_size(px(22.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("fx"),
                )
                .child(
                    div()
                        .text_color(rgba(t.text_secondary))
                        .child(super::guide::slogan()),
                )
                .child(
                    div()
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(version_text()),
                ),
        )
        .into_any_element()
}

/// "Versão 0.2.0 · compilação abc1234".
fn version_text() -> String {
    let version = crate::VERSION.split('+').next().unwrap_or(crate::VERSION);
    match crate::VERSION.split_once('+') {
        Some((_, c)) => trf!(
            "Versão {version} · compilação {c}",
            "Version {version} · build {c}"
        ),
        None => trf!("Versão {version}", "Version {version}"),
    }
}

/// The header of "Sobre o fx" as one line, for screen readers.
fn about_line() -> String {
    format!("fx. {}. {}.", super::guide::slogan(), version_text())
}
