//! "Sobre o fx" and "Como usar o fx" (task T4.6): two dialogs over the
//! window. "Como usar" opens by itself the first time fx runs, and with F1;
//! both are in the palette. Esc or a click outside closes them.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[allow(unused_imports)]
use fx_core::{tr, trf};
use gpui::{
    Context, Image, ImageFormat, MouseButton, MouseDownEvent, SharedString, deferred, div, img,
    prelude::*, px,
};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

const LOGO: &[u8] = include_bytes!("../../resources/logo-128.png");
const REPO: &str = "https://github.com/heapwraith/fx";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
    About,
    HowTo,
}

/// Set at startup when fx runs for the first time.
static FIRST_RUN: AtomicBool = AtomicBool::new(false);
/// "Como usar" was shown this session (saved, so it opens only once).
static SEEN: AtomicBool = AtomicBool::new(false);

pub fn set_first_run(first: bool) {
    FIRST_RUN.store(first, Ordering::Relaxed);
}

/// The first window shows "Como usar"; later ones don't.
pub fn take_first_run() -> bool {
    FIRST_RUN.swap(false, Ordering::Relaxed)
}

/// Whether to save `guia_visto = true`.
pub fn seen() -> bool {
    SEEN.load(Ordering::Relaxed)
}

/// The slogan, shared with the installer and the README.
pub fn slogan() -> &'static str {
    tr!(
        "Pastas abrem na hora. Arquivos aparecem enquanto você digita.",
        "Folders open instantly. Files appear as you type."
    )
}

/// The shortcuts of "Como usar", by subject: (title, [(keys, what)]).
pub fn sections() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            tr!("Navegar", "Navigate"),
            vec![
                ("Enter", tr!("abrir", "open")),
                ("Alt+↑", tr!("pasta acima", "up one folder")),
                ("Alt+← →", tr!("voltar e avançar", "back and forward")),
                ("Ctrl+L", tr!("digitar um caminho", "type a path")),
                (
                    tr!("letras", "letters"),
                    tr!("filtrar a pasta", "filter the folder"),
                ),
            ],
        ),
        (
            tr!("Buscar", "Search"),
            vec![
                (
                    "Ctrl+F",
                    tr!(
                        "buscar aqui e nas subpastas",
                        "search here and in subfolders"
                    ),
                ),
                (
                    "Tab",
                    tr!("na busca: o disco inteiro", "in search: the whole disk"),
                ),
                (
                    "tipo:",
                    tr!("imagem, vídeo, documento…", "image, video, document…"),
                ),
                (
                    "tamanho:",
                    tr!(">10mb, data:semana, ext:pdf", ">10mb, date:week, ext:pdf"),
                ),
                (
                    "Ctrl+K",
                    tr!(
                        "paleta: todo comando e pasta",
                        "palette: every command and folder"
                    ),
                ),
            ],
        ),
        (
            tr!("Arquivos", "Files"),
            vec![
                ("F2", tr!("renomear", "rename")),
                ("Ctrl+Shift+N", tr!("nova pasta", "new folder")),
                (
                    "Ctrl+C X V",
                    tr!("copiar, recortar, colar", "copy, cut, paste"),
                ),
                (
                    "Delete",
                    tr!(
                        "Lixeira (Shift: excluir de vez)",
                        "Recycle Bin (Shift: delete for good)"
                    ),
                ),
                (
                    "Ctrl+Z",
                    tr!("desfazer qualquer operação", "undo any operation"),
                ),
            ],
        ),
        (
            tr!("Ver", "View"),
            vec![
                (
                    tr!("Espaço", "Space"),
                    tr!("pré-visualizar (Quick Look)", "preview (Quick Look)"),
                ),
                (
                    "Ctrl+H",
                    tr!("mostrar arquivos ocultos", "show hidden files"),
                ),
                ("Ctrl+1 2 3", tr!("densidade das linhas", "row density")),
                ("Ctrl+Shift+3", tr!("vista em colunas", "column view")),
                ("Ctrl+B", tr!("barra lateral", "sidebar")),
            ],
        ),
        (
            tr!("Abas e painéis", "Tabs and panes"),
            vec![
                (
                    "Ctrl+T W",
                    tr!("nova aba, fechar aba", "new tab, close tab"),
                ),
                ("Ctrl+Tab", tr!("próxima aba", "next tab")),
                ("F3", tr!("painel duplo", "dual pane")),
                (
                    "F5 F6",
                    tr!(
                        "copiar ou mover para o outro painel",
                        "copy or move to the other pane"
                    ),
                ),
                (
                    "Ctrl+J",
                    tr!("terminal na pasta atual", "terminal in this folder"),
                ),
            ],
        ),
        (
            tr!("Organizar", "Organize"),
            vec![
                (
                    "Alt+1…7",
                    tr!("etiqueta colorida (Alt+0 tira)", "color tag (Alt+0 clears)"),
                ),
                ("etiqueta:", tr!("buscar por etiqueta", "search by tag")),
                (
                    "Ctrl+Shift+H",
                    tr!("histórico de operações", "operation history"),
                ),
                ("F1", tr!("esta tela", "this screen")),
            ],
        ),
    ]
}

impl TabsView {
    pub(super) fn open_guide(&mut self, guide: Guide, cx: &mut Context<Self>) {
        if guide == Guide::HowTo {
            SEEN.store(true, Ordering::Relaxed);
        }
        self.guide = Some(guide);
        cx.notify();
    }

    fn open_link(&mut self, url: &'static str, cx: &mut Context<Self>) {
        let tab = self.current();
        tab.update(cx, |view, cx| view.open_url(url, cx));
    }

    pub(super) fn render_guide(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let guide = self.guide?;
        let t = self.look.theme;
        let card = match guide {
            Guide::About => self.about_card(cx).into_any_element(),
            Guide::HowTo => self.howto_card().into_any_element(),
        };
        Some(
            deferred(crate::motion::enter(
                "guide-in",
                div()
                    .id("guide-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::rgba(if t.mode == crate::theme::Mode::Dark {
                        0x00000066
                    } else {
                        0x00000033
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.guide = None;
                            cx.notify();
                        }),
                    )
                    .child(card),
            ))
            .with_priority(2),
        )
    }

    fn card(&self) -> gpui::Stateful<gpui::Div> {
        let t = self.look.theme;
        div()
            .id("guide-card")
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }

    fn about_card(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let version = crate::VERSION
            .split('+')
            .next()
            .unwrap_or(crate::VERSION)
            .to_owned();
        let commit = crate::VERSION.split_once('+').map(|(_, c)| c.to_owned());
        let link =
            |id: &'static str, label: &'static str, url: &'static str, cx: &mut Context<Self>| {
                div()
                    .id(id)
                    .text_color(rgba(t.accent))
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(label)
                    .on_click(cx.listener(move |view, _, _, cx| view.open_link(url, cx)))
            };
        let logo = Arc::new(Image::from_bytes(ImageFormat::Png, LOGO.to_vec()));
        self.card()
            .w(px(520.0))
            .p(px(space(8)))
            .gap(px(space(4)))
            .items_center()
            .child(img(logo).size(px(72.0)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(space(1)))
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("fx"),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_color(rgba(t.text_secondary))
                            .child(slogan()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(space(1)))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(match &commit {
                        Some(c) => trf!(
                            "Versão {version} · compilação {c}",
                            "Version {version} · build {c}"
                        ),
                        None => trf!("Versão {version}", "Version {version}"),
                    })
                    .child(tr!(
                        "Software livre, sob as licenças MIT ou Apache 2.0.",
                        "Free software, under the MIT or Apache 2.0 licenses."
                    ))
                    .child(tr!(
                        "Feito em Rust, nativo do Windows.",
                        "Made in Rust, native to Windows."
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap(px(space(4)))
                    .text_size(px(CAPTION_SIZE))
                    .child(
                        div()
                            .id("about-update")
                            .text_color(rgba(t.accent))
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child(tr!("Procurar atualizações", "Check for updates"))
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.guide = None;
                                let tab = view.current();
                                tab.update(cx, |tab, cx| tab.check_updates(false, cx));
                                cx.notify();
                            })),
                    )
                    .child(link(
                        "about-news",
                        tr!("Novidades", "What's new"),
                        "https://github.com/heapwraith/fx/blob/main/CHANGELOG.md",
                        cx,
                    ))
                    .child(link(
                        "about-code",
                        tr!("Código e sugestões", "Code and feedback"),
                        REPO,
                        cx,
                    ))
                    .child(
                        div()
                            .id("about-howto")
                            .text_color(rgba(t.accent))
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child(tr!("Como usar (F1)", "How to use (F1)"))
                            .on_click(
                                cx.listener(|view, _, _, cx| view.open_guide(Guide::HowTo, cx)),
                            ),
                    ),
            )
    }

    fn howto_card(&self) -> impl IntoElement + use<> {
        let t = self.look.theme;
        let key = |k: &str| {
            div().flex_none().w(px(108.0)).flex().child(
                div()
                    .px(px(space(1) + 2.0))
                    .py(px(1.0))
                    .rounded(px(RADIUS))
                    .border_1()
                    .border_color(rgba(t.divider))
                    .bg(rgba(t.layer))
                    .text_size(px(CAPTION_SIZE))
                    .child(SharedString::from(k.to_owned())),
            )
        };
        let sections = sections().into_iter().map(|(title, rows)| {
            div()
                .flex()
                .flex_col()
                .gap(px(space(2)))
                .w(px(300.0))
                .child(
                    div()
                        .text_size(px(CAPTION_SIZE))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgba(t.accent))
                        .child(title),
                )
                .children(rows.into_iter().map(|(k, what)| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space(2)))
                        .child(key(k))
                        .child(div().min_w_0().truncate().child(what))
                }))
        });
        self.card()
            .w(px(720.0))
            .max_h(px(560.0))
            .overflow_y_scroll()
            .p(px(space(6)))
            .gap(px(space(5)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space(1)))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(tr!("Como usar o fx", "How to use fx")),
                    )
                    .child(div().text_color(rgba(t.text_secondary)).child(tr!(
                        "Tudo se faz pelo teclado. Na dúvida, Ctrl+K encontra qualquer comando pelo nome.",
                        "Everything works from the keyboard. When in doubt, Ctrl+K finds any command by name."
                    ))),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(space(8)))
                    .gap_y(px(space(5)))
                    .children(sections),
            )
            .child(
                div()
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .child(tr!("F1 abre esta tela de novo · Esc fecha", "F1 opens this screen again · Esc closes")),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shortcut_has_a_text() {
        let s = sections();
        assert_eq!(s.len(), 6);
        assert!(s.iter().all(|(t, rows)| !t.is_empty() && !rows.is_empty()));
        assert!(
            s.iter()
                .flat_map(|(_, r)| r)
                .all(|(k, w)| !k.is_empty() && !w.is_empty())
        );
        assert!(slogan().contains("Pastas"));
    }
}
