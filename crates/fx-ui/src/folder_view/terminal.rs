//! Embedded terminal (task T3.5): Ctrl+J opens PowerShell in the current
//! folder in a panel at the bottom of the tab (ConPTY, `fx_shell::conpty`;
//! screen model `fx_core::vt`). While it has the focus, every key goes to
//! the shell (Esc too), except Ctrl+J, which closes it. Clicking the list
//! gives the focus back to the list; clicking the panel, to the shell.

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::io::Read;
use std::sync::{Arc, Mutex, PoisonError};

use fx_core::vt::{Color, Screen};
use fx_shell::conpty::Pty;
use gpui::{
    Context, HighlightStyle, Hsla, Keystroke, MouseButton, SharedString, StyledText, canvas, div,
    prelude::*, px,
};

use super::FolderView;
use crate::theme::{CAPTION_SIZE, rgba, space};

const PANEL_HEIGHT: f32 = 280.0;
const FONT: &str = "Consolas";
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 17.0;
/// Width of one Consolas character at 13 px.
const CHAR_WIDTH: f32 = 7.2;
const SHELL: &str = "powershell.exe -NoLogo";

/// Campbell (the Windows Terminal default), the 16 ANSI colors.
const ANSI: [u32; 16] = [
    0x0C0C0CFF, 0xC50F1FFF, 0x13A10EFF, 0xC19C00FF, 0x0037DAFF, 0x881798FF, 0x3A96DDFF, 0xCCCCCCFF,
    0x767676FF, 0xE74856FF, 0x16C60CFF, 0xF9F1A5FF, 0x3B78FFFF, 0xB4009EFF, 0x61D6D6FF, 0xF2F2F2FF,
];

pub(super) struct Terminal {
    pty: Pty,
    screen: Arc<Mutex<Screen>>,
    pub(super) focused: bool,
    size: Arc<Mutex<(u16, u16)>>,
}

impl FolderView {
    /// Ctrl+J: opens the terminal (focused) or closes it.
    pub(super) fn toggle_terminal(&mut self, cx: &mut Context<Self>) {
        if self.terminal.take().is_some() {
            cx.notify();
            return;
        }
        let (cols, rows) = (100u16, 15u16);
        let (pty, mut output) = match fx_shell::conpty::spawn(SHELL, &self.path, cols, rows) {
            Ok(p) => p,
            Err(e) => {
                self.notice = Some(trf!(
                    "não foi possível abrir o terminal: {e}",
                    "could not open the terminal: {e}"
                ));
                cx.notify();
                return;
            }
        };
        let screen = Arc::new(Mutex::new(Screen::new(cols as usize, rows as usize)));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let feeding = screen.clone();
        let _ = std::thread::Builder::new()
            .name("fx-terminal".into())
            .spawn(move || {
                let mut buf = [0u8; 8192];
                while let Ok(n) = output.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    feeding
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .feed(&buf[..n]);
                    let _ = tx.send(());
                }
            });
        // Repaint when output arrives (bursts are coalesced into one frame).
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                let (back, got) = background
                    .spawn(async move {
                        let r = rx.recv();
                        while rx.try_recv().is_ok() {}
                        (rx, r)
                    })
                    .await;
                rx = back;
                if got.is_err() {
                    break;
                }
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        self.terminal = Some(Terminal {
            pty,
            screen,
            focused: true,
            size: Arc::new(Mutex::new((cols, rows))),
        });
        cx.notify();
    }

    /// Keys while the terminal has the focus. Returns whether it took them.
    pub(super) fn terminal_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(t) = self.terminal.as_mut().filter(|t| t.focused) else {
            return false;
        };
        let m = ks.modifiers;
        if ks.key == "j" && m.control && !m.alt && !m.shift {
            return false; // Ctrl+J closes (handled by the caller)
        }
        if let Some(bytes) = encode(ks) {
            let _ = t.pty.write(&bytes);
        }
        cx.notify();
        true
    }

    pub(super) fn render_terminal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let term = self.terminal.as_ref()?;
        let theme = self.look.theme;
        let screen = term.screen.lock().unwrap_or_else(PoisonError::into_inner);
        let default = Hsla::from(rgba(0xCCCCCCFF));
        let lines: Vec<gpui::AnyElement> = (0..screen.rows)
            .map(|r| {
                let text: String = screen.grid[r].iter().map(|c| c.ch).collect();
                let mut styles = Vec::new();
                let mut offset = 0;
                for c in &screen.grid[r] {
                    let len = c.ch.len_utf8();
                    let color = match c.fg {
                        Color::Default => None,
                        Color::Ansi(i) => Some(Hsla::from(rgba(ANSI[i as usize & 15]))),
                    };
                    if color.is_some() || c.bold {
                        styles.push((
                            offset..offset + len,
                            HighlightStyle {
                                color,
                                font_weight: c.bold.then_some(gpui::FontWeight::BOLD),
                                ..Default::default()
                            },
                        ));
                    }
                    offset += len;
                }
                div()
                    .h(px(LINE_HEIGHT))
                    .whitespace_nowrap()
                    .child(StyledText::new(SharedString::from(text)).with_highlights(styles))
                    .into_any_element()
            })
            .collect();
        let (crow, ccol) = screen.cursor;
        drop(screen);
        let size = term.size.clone();
        let focused = term.focused;
        Some(
            div()
                .id("terminal")
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(px(PANEL_HEIGHT))
                .flex()
                .flex_col()
                .bg(rgba(0x0C0C0CFF))
                .border_t_1()
                .border_color(rgba(if focused { theme.accent } else { theme.divider }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _, _, cx| {
                        if let Some(t) = view.terminal.as_mut() {
                            t.focused = true;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(2.0))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(0x9A9A9AFF))
                        .child(tr!(
                            "PowerShell · Ctrl+J fecha · clique na lista para voltar a ela",
                            "PowerShell · Ctrl+J closes · click the list to go back to it"
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .relative()
                        .px(px(space(2)))
                        .font_family(FONT)
                        .text_size(px(FONT_SIZE))
                        .text_color(default)
                        .overflow_hidden()
                        .children(lines)
                        // The cursor.
                        .when(focused, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(crow as f32 * LINE_HEIGHT))
                                    .left(px(space(2) + ccol as f32 * CHAR_WIDTH))
                                    .w(px(CHAR_WIDTH))
                                    .h(px(LINE_HEIGHT))
                                    .bg(rgba(0xCCCCCC99)),
                            )
                        })
                        // Measures the text area: the console follows its size.
                        .child(
                            canvas(
                                move |bounds, _, _| {
                                    let cols = ((f32::from(bounds.size.width) - 2.0 * space(2))
                                        / CHAR_WIDTH)
                                        .floor()
                                        .max(20.0)
                                        as u16;
                                    let rows = (f32::from(bounds.size.height) / LINE_HEIGHT)
                                        .floor()
                                        .max(3.0)
                                        as u16;
                                    let mut s = size.lock().unwrap_or_else(PoisonError::into_inner);
                                    *s = (cols, rows);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        ),
                ),
        )
    }

    /// Follows the panel's measured size (called every render).
    pub(super) fn sync_terminal_size(&mut self) {
        let Some(t) = self.terminal.as_mut() else {
            return;
        };
        let wanted = *t.size.lock().unwrap_or_else(PoisonError::into_inner);
        let mut screen = t.screen.lock().unwrap_or_else(PoisonError::into_inner);
        if (screen.cols as u16, screen.rows as u16) != wanted {
            screen.resize(wanted.0 as usize, wanted.1 as usize);
            t.pty.resize(wanted.0, wanted.1);
        }
    }
}

/// A key as VT input bytes.
fn encode(ks: &Keystroke) -> Option<Vec<u8>> {
    let m = ks.modifiers;
    let seq: &[u8] = match ks.key.as_str() {
        "enter" => b"\r",
        "backspace" => b"\x7f",
        "tab" => b"\t",
        "escape" => b"\x1b",
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "delete" => b"\x1b[3~",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "space" => b" ",
        k if m.control && !m.alt && k.len() == 1 && k.as_bytes()[0].is_ascii_lowercase() => {
            return Some(vec![k.as_bytes()[0] & 0x1f]);
        }
        _ => {
            return ks
                .key_char
                .as_deref()
                .filter(|c| !c.chars().any(char::is_control))
                .map(|c| c.as_bytes().to_vec());
        }
    };
    Some(seq.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_become_vt_input() {
        let k = |s: &str| encode(&Keystroke::parse(s).unwrap());
        assert_eq!(k("enter"), Some(b"\r".to_vec()));
        assert_eq!(k("up"), Some(b"\x1b[A".to_vec()));
        assert_eq!(k("ctrl-c"), Some(vec![3]));
        assert_eq!(k("backspace"), Some(vec![0x7f]));
    }
}
