//! "Como usar o fx" (task T4.6): a dialog over the window that opens by
//! itself the first time fx runs, with F1, the sidebar's "Ajuda" and the
//! palette. "Sobre" lives in the settings (settings.rs). Esc or a click
//! outside closes it.

use std::sync::atomic::{AtomicBool, Ordering};

#[allow(unused_imports)]
use fx_core::{tr, trf};
use gpui::{Context, MouseButton, MouseDownEvent, SharedString, deferred, div, prelude::*, px};

use super::TabsView;
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guide {
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
                    tr!("tipo:", "type:"),
                    tr!("imagem, vídeo, documento…", "image, video, document…"),
                ),
                (
                    tr!("tamanho:", "size:"),
                    tr!(">10mb, data:semana, ext:pdf", ">10mb, date:week, ext:pdf"),
                ),
                (
                    "Ctrl+K",
                    tr!("paleta: todo comando e pasta", "palette: every command"),
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
                        "Recycle Bin (Shift: for good)"
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
                (
                    tr!("etiqueta:", "tag:"),
                    tr!("buscar por etiqueta", "search by tag"),
                ),
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

    pub(super) fn render_guide(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let guide = self.guide?;
        let t = self.look.theme;
        let card = match guide {
            Guide::HowTo => self.howto_card().into_any_element(),
        };
        Some(
            deferred(crate::motion::enter(
                "guide-in",
                div()
                    .id("guide-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
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
