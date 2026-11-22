//! Settings (the 2026 pattern of Linear, Notion, Slack, VS Code, Zed: a gear
//! at the bottom of the sidebar and Ctrl+, open a dialog with categories on
//! the left; "Sobre" is the last category). Every control shows the real
//! state and acts on it; nothing here is a copy of a setting.

use std::sync::Arc;

#[allow(unused_imports)]
use fx_core::{tr, trf};
use gpui::{
    Context, Image, ImageFormat, MouseButton, MouseDownEvent, SharedString, Window, deferred, div,
    img, prelude::*, px,
};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, Density, RADIUS, rgba, space};

const LOGO: &[u8] = include_bytes!("../../resources/logo-128.png");

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
    const ALL: [Section; 6] = [
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
#[derive(Debug, Clone, Copy)]
enum Act {
    Language,
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
        self.settings = Some(section);
        cx.notify();
    }

    fn act(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.current();
        match act {
            Act::Language => {
                crate::i18n::switch();
                cx.refresh_windows();
            }
            Act::ConfirmRecycle => {
                crate::prefs::switch_confirm_recycle();
            }
            Act::RestoreSession => {
                crate::prefs::switch_restore_session();
            }
            Act::Density(d) => self.set_density(d, cx),
            Act::Hidden => self.toggle_hidden(cx),
            Act::Sidebar => self.sidebar_open = !self.sidebar_open,
            Act::AutoUpdate => {
                crate::update::switch();
            }
            Act::CheckNow => tab.update(cx, |view, cx| view.check_updates(false, cx)),
            Act::CrashReports => {
                crate::crash::switch();
            }
            Act::DefaultManager(on) => tab.update(cx, |view, cx| view.default_manager(on, cx)),
            Act::IndexService(on) => tab.update(cx, |view, cx| view.index_service(on, cx)),
            Act::HowTo => {
                self.settings = None;
                self.open_guide(super::guide::Guide::HowTo, cx);
            }
            Act::Link(url) => tab.update(cx, |view, cx| view.open_url(url, cx)),
        }
        let _ = window;
        cx.notify();
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

        // A row: title, explanation, control.
        let row = |title: &'static str, detail: &'static str, control: gpui::AnyElement| {
            div()
                .py(px(space(3)))
                .flex()
                .items_center()
                .gap(px(space(4)))
                .border_b_1()
                .border_color(rgba(t.divider))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(title)
                        .child(
                            div()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .child(detail),
                        ),
                )
                .child(div().flex_none().child(control))
        };
        let switch = |id: &'static str, on: bool, act: Act, cx: &mut Context<Self>| {
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
                .on_click(cx.listener(move |view, _, window, cx| view.act(act, window, cx)))
                .into_any_element()
        };
        let button = |id: &'static str, label: &'static str, act: Act, cx: &mut Context<Self>| {
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
                .on_click(cx.listener(move |view, _, window, cx| view.act(act, window, cx)))
        };
        // Segmented choice: (label, selected, action).
        let segmented =
            |id: &'static str, options: Vec<(&'static str, bool, Act)>, cx: &mut Context<Self>| {
                div()
                    .flex()
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(rgba(t.divider))
                    .overflow_hidden()
                    .children(
                        options
                            .into_iter()
                            .enumerate()
                            .map(|(k, (label, selected, act))| {
                                div()
                                    .id(SharedString::from(format!("{id}-{k}")))
                                    .h(px(28.0))
                                    .px(px(space(3)))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .when(selected, |d| {
                                        d.bg(rgba(t.selected)).text_color(rgba(t.text))
                                    })
                                    .when(!selected, |d| {
                                        d.text_color(rgba(t.text_secondary))
                                            .hover(|h| h.bg(rgba(t.hover)))
                                    })
                                    .child(label)
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.act(act, window, cx)
                                    }))
                            }),
                    )
                    .into_any_element()
            };

        let english = fx_core::locale::english();
        let density = self.look.density;
        let mut body = div().flex().flex_col();
        match section {
            Section::General => {
                body = body
                    .child(row(
                        tr!("Idioma", "Language"),
                        tr!(
                            "O padrão segue o idioma do Windows.",
                            "By default it follows the Windows language."
                        ),
                        segmented(
                            "lang",
                            vec![
                                ("Português", !english, Act::Language),
                                ("English", english, Act::Language),
                            ],
                            cx,
                        ),
                    ))
                    .child(row(
                        tr!(
                            "Confirmar antes de enviar para a Lixeira",
                            "Confirm before sending to the Recycle Bin"
                        ),
                        tr!(
                            "Excluir de vez (Shift+Delete) sempre pergunta.",
                            "Deleting for good (Shift+Delete) always asks."
                        ),
                        switch(
                            "confirm-recycle",
                            crate::prefs::confirm_recycle(),
                            Act::ConfirmRecycle,
                            cx,
                        ),
                    ))
                    .child(row(
                        tr!(
                            "Reabrir abas e janelas ao iniciar",
                            "Reopen tabs and windows at start"
                        ),
                        tr!(
                            "Vale na próxima abertura do fx.",
                            "Applies the next time fx opens."
                        ),
                        switch(
                            "restore",
                            crate::prefs::restore_session(),
                            Act::RestoreSession,
                            cx,
                        ),
                    ));
            }
            Section::Appearance => {
                body = body
                    .child(row(
                        tr!("Densidade das linhas", "Row density"),
                        tr!(
                            "Ctrl+1, Ctrl+2 e Ctrl+3 também trocam.",
                            "Ctrl+1, Ctrl+2 and Ctrl+3 switch it too."
                        ),
                        segmented(
                            "density",
                            vec![
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
                            ],
                            cx,
                        ),
                    ))
                    .child(row(
                        tr!("Mostrar itens ocultos", "Show hidden items"),
                        tr!(
                            "Arquivos ocultos e de sistema (Ctrl+H).",
                            "Hidden and system files (Ctrl+H)."
                        ),
                        switch("hidden", self.look.show_hidden, Act::Hidden, cx),
                    ))
                    .child(row(
                        tr!("Barra lateral", "Sidebar"),
                        tr!(
                            "Acesso rápido e unidades (Ctrl+B).",
                            "Quick access and drives (Ctrl+B)."
                        ),
                        switch("sidebar", self.sidebar_open, Act::Sidebar, cx),
                    ))
                    .child(row(
                        tr!("Tema", "Theme"),
                        tr!(
                            "Segue o tema claro ou escuro do Windows, e o alto contraste.",
                            "Follows the Windows light or dark theme, and high contrast."
                        ),
                        div().into_any_element(),
                    ));
            }
            Section::Updates => {
                let version = crate::VERSION.split('+').next().unwrap_or(crate::VERSION);
                body = body
                    .child(row(
                        tr!("Atualizações automáticas", "Automatic updates"),
                        tr!(
                            "Baixa a versão nova em segundo plano, confere e instala quando você fecha o fx.",
                            "Downloads the new version in the background, checks it and installs it when you close fx."
                        ),
                        switch("auto-update", crate::update::enabled(), Act::AutoUpdate, cx),
                    ))
                    .child(row(
                        tr!("Versão instalada", "Installed version"),
                        tr!("A versão deste fx.", "This fx's version."),
                        div()
                            .flex()
                            .items_center()
                            .gap(px(space(3)))
                            .child(div().text_color(rgba(t.text_secondary)).child(version.to_owned()))
                            .child(button("check-now", tr!("Procurar agora", "Check now"), Act::CheckNow, cx))
                            .into_any_element(),
                    ));
            }
            Section::Privacy => {
                body = body
                    .child(row(
                        tr!("Relatórios de falha", "Crash reports"),
                        tr!(
                            "Se o fx fechar com uma falha, guarda um relatório sem caminhos nem nomes neste computador. Nada é enviado sem você.",
                            "If fx closes with a crash, it keeps a report with no paths or names on this computer. Nothing is sent without you."
                        ),
                        switch("crash", crate::crash::enabled(), Act::CrashReports, cx),
                    ))
                    .child(row(
                        tr!("Dados que o fx envia", "Data fx sends"),
                        tr!(
                            "Nenhum. A verificação de atualizações só consulta a lista de versões no GitHub.",
                            "None. The update check only reads the list of versions on GitHub."
                        ),
                        div().into_any_element(),
                    ));
            }
            Section::System => {
                body = body
                    .child(row(
                        tr!("Gerenciador de arquivos padrão", "Default file manager"),
                        tr!(
                            "Pastas, unidades e Win+E abrem no fx. Voltar ao Explorador restaura tudo.",
                            "Folders, drives and Win+E open in fx. Back to Explorer restores everything."
                        ),
                        div()
                            .flex()
                            .gap(px(space(2)))
                            .child(button("default-on", tr!("Usar o fx", "Use fx"), Act::DefaultManager(true), cx))
                            .child(button("default-off", tr!("Usar o Explorador", "Use Explorer"), Act::DefaultManager(false), cx))
                            .into_any_element(),
                    ))
                    .child(row(
                        tr!("Busca no disco inteiro", "Whole-disk search"),
                        tr!(
                            "Um serviço do Windows mantém o índice dos discos NTFS. Pede administrador uma vez.",
                            "A Windows service keeps the index of NTFS disks. Asks for administrator once."
                        ),
                        div()
                            .flex()
                            .gap(px(space(2)))
                            .child(button("index-on", tr!("Ativar", "Turn on"), Act::IndexService(true), cx))
                            .child(button("index-off", tr!("Desativar", "Turn off"), Act::IndexService(false), cx))
                            .into_any_element(),
                    ));
            }
            Section::About => {
                let version = crate::VERSION
                    .split('+')
                    .next()
                    .unwrap_or(crate::VERSION)
                    .to_owned();
                let commit = crate::VERSION.split_once('+').map(|(_, c)| c.to_owned());
                let logo = Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec()));
                body = body
                    .child(
                        div()
                            .py(px(space(4)))
                            .flex()
                            .items_center()
                            .gap(px(space(4)))
                            .child(img(logo).size(px(64.0)))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(space(1)))
                                    .child(div().text_size(px(22.0)).font_weight(gpui::FontWeight::SEMIBOLD).child("fx"))
                                    .child(div().text_color(rgba(t.text_secondary)).child(super::guide::slogan()))
                                    .child(
                                        div()
                                            .text_size(px(CAPTION_SIZE))
                                            .text_color(rgba(t.text_secondary))
                                            .child(match &commit {
                                                Some(c) => trf!("Versão {version} · compilação {c}", "Version {version} · build {c}"),
                                                None => trf!("Versão {version}", "Version {version}"),
                                            }),
                                    ),
                            ),
                    )
                    .child(row(
                        tr!("Licença", "License"),
                        tr!("Software livre, sob as licenças MIT ou Apache 2.0. Feito em Rust, nativo do Windows.", "Free software, under the MIT or Apache 2.0 licenses. Made in Rust, native to Windows."),
                        div().into_any_element(),
                    ))
                    .child(
                        div()
                            .pt(px(space(4)))
                            .flex()
                            .flex_wrap()
                            .gap(px(space(2)))
                            .child(button("about-howto", tr!("Como usar (F1)", "How to use (F1)"), Act::HowTo, cx))
                            .child(button("about-news", tr!("Novidades", "What's new"), Act::Link("https://github.com/heapwraith/fx/blob/main/CHANGELOG.md"), cx))
                            .child(button("about-code", tr!("Código e sugestões", "Code and feedback"), Act::Link("https://github.com/heapwraith/fx"), cx))
                            .child(button("about-issue", tr!("Relatar um problema", "Report a problem"), Act::Link("https://github.com/heapwraith/fx/issues/new/choose"), cx)),
                    );
            }
        }

        let card = div()
            .id("settings")
            .w(px(780.0))
            .h(px(520.0))
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
                            .child(tr!("Ctrl+, abre · Esc fecha", "Ctrl+, opens · Esc closes")),
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
