//! Embedded terminal (tasks T3.5, T5.4): Ctrl+J opens PowerShell (or the
//! Command Prompt, from the palette; Ctrl+J then reopens the last one) in
//! the current folder in a panel at the bottom of the tab (ConPTY,
//! `fx_shell::conpty`; screen model `fx_core::vt`). While it has the focus,
//! every key goes to the shell (Esc too), except Ctrl+J, which closes it.
//! Clicking the list gives the focus back to the list; clicking the panel,
//! to the shell.
//!
//! The wheel scrolls back through the history; dragging selects text,
//! which Ctrl+Shift+C (or Ctrl+C while something is selected) copies;
//! Ctrl+Shift+V pastes. When the tab opens another folder and the shell
//! waits at an empty prompt, it changes to that folder too.

use futures::StreamExt;
use fx_core::{tr, trf};
use std::io::Read;
use std::sync::{Arc, Mutex, PoisonError};

use std::sync::atomic::{AtomicBool, Ordering};

use fx_core::vt::{Color, Screen};
use fx_shell::conpty::Pty;
use gpui::{
    Bounds, ClipboardItem, Context, HighlightStyle, Hsla, Keystroke, MouseButton, MouseDownEvent,
    MouseMoveEvent, Pixels, Point, ScrollDelta, ScrollWheelEvent, SharedString, StyledText, canvas,
    div, prelude::*, px,
};

use super::FolderView;
use crate::theme::{CAPTION_SIZE, rgba, space};

const PANEL_HEIGHT: f32 = 280.0;
const FONT: &str = "Consolas";
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = 17.0;
/// Width of one Consolas character at 13 px.
const CHAR_WIDTH: f32 = 7.2;
/// The last shell opened (Ctrl+J opens it again): the Command Prompt
/// when set, else PowerShell.
static PREFER_CMD: AtomicBool = AtomicBool::new(false);

/// The shell in the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    PowerShell,
    Cmd,
}

impl Shell {
    fn command(self) -> &'static str {
        match self {
            Shell::PowerShell => "powershell.exe -NoLogo",
            Shell::Cmd => "cmd.exe",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Shell::PowerShell => "PowerShell",
            Shell::Cmd => tr!("Prompt de Comando", "Command Prompt"),
        }
    }

    /// The command that changes to `dir`, ending the line.
    fn cd(self, dir: &std::path::Path) -> Option<String> {
        let d = dir.to_str()?;
        Some(match self {
            Shell::PowerShell => format!("Set-Location -LiteralPath '{}'\r", d.replace('\'', "''")),
            // cmd has no escaping inside quotes; a quote in a path cannot
            // happen on Windows (it is not allowed in names).
            Shell::Cmd => format!("cd /d \"{d}\"\r"),
        })
    }
}

/// Campbell (the Windows Terminal default), the 16 ANSI colors.
const ANSI: [u32; 16] = [
    0x0C0C0CFF, 0xC50F1FFF, 0x13A10EFF, 0xC19C00FF, 0x0037DAFF, 0x881798FF, 0x3A96DDFF, 0xCCCCCCFF,
    0x767676FF, 0xE74856FF, 0x16C60CFF, 0xF9F1A5FF, 0x3B78FFFF, 0xB4009EFF, 0x61D6D6FF, 0xF2F2F2FF,
];

pub(super) struct Terminal {
    pty: Pty,
    shell: Shell,
    screen: Arc<Mutex<Screen>>,
    pub(super) focused: bool,
    size: Arc<Mutex<(u16, u16)>>,
    /// Where the first line of text is on screen (for mouse positions).
    area: Arc<Mutex<Bounds<Pixels>>>,
    /// Lines scrolled back from the bottom (0: following the output).
    back: usize,
    /// Selected text: anchor and end (line in history + grid, column).
    selection: Option<((usize, usize), (usize, usize))>,
    selecting: bool,
}

impl FolderView {
    /// The terminal (when it has the keyboard) as a screen reader reads it:
    /// the text on its screen.
    pub(super) fn terminal_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        let t = self.terminal.as_ref().filter(|t| t.focused)?;
        let screen = t.screen.lock().unwrap_or_else(PoisonError::into_inner);
        let text = (0..screen.rows)
            .map(|r| screen.line_text(r))
            .filter(|l| !l.trim().is_empty())
            .collect::<Vec<String>>()
            .join("\n");
        Some(Overlay {
            title: "Terminal".to_owned(),
            description: Some(trf!(
                "{} em {}. Ctrl+J fecha; um clique na lista volta para ela. Ctrl+Shift+C copia a seleção, Ctrl+Shift+V cola.",
                "{} in {}. Ctrl+J closes; a click on the list goes back to it. Ctrl+Shift+C copies the selection, Ctrl+Shift+V pastes.",
                t.shell.label(),
                self.path.display()
            )),
            items: vec![Item::Input {
                label: t.shell.label().to_owned(),
                value: text,
            }],
            focus: Some(0),
            positions: None,
            modal: true,
        })
    }
}

impl FolderView {
    /// Ctrl+J: opens the terminal (focused) with the last shell used, or
    /// closes it.
    pub(super) fn toggle_terminal(&mut self, cx: &mut Context<Self>) {
        if self.terminal.take().is_some() {
            cx.notify();
            return;
        }
        let shell = if PREFER_CMD.load(Ordering::Relaxed) {
            Shell::Cmd
        } else {
            Shell::PowerShell
        };
        self.open_terminal(shell, cx);
    }

    /// Opens the terminal with `shell` (palette), replacing an open one.
    pub fn open_terminal(&mut self, shell: Shell, cx: &mut Context<Self>) {
        self.terminal = None;
        PREFER_CMD.store(shell == Shell::Cmd, Ordering::Relaxed);
        let (cols, rows) = (100u16, 15u16);
        let (pty, mut output) =
            match fx_shell::conpty::spawn(shell.command(), &self.path, cols, rows) {
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
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
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
                    let _ = tx.unbounded_send(());
                }
            });
        // Repaint when output arrives (bursts are coalesced into one frame).
        cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                // A burst becomes one frame.
                while rx.try_recv().is_ok() {}
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();
        self.terminal = Some(Terminal {
            pty,
            shell,
            screen,
            focused: true,
            size: Arc::new(Mutex::new((cols, rows))),
            area: Arc::new(Mutex::new(Bounds::default())),
            back: 0,
            selection: None,
            selecting: false,
        });
        cx.notify();
    }

    /// The tab now shows `self.path`: a shell waiting at an empty prompt
    /// changes to it too (never in the middle of a typed line).
    pub(super) fn terminal_follow(&mut self) {
        let Some(t) = self.terminal.as_mut() else {
            return;
        };
        let idle = t
            .screen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .at_empty_prompt();
        if idle && let Some(cd) = t.shell.cd(&self.path) {
            let _ = t.pty.write(cd.as_bytes());
        }
    }

    /// The (line, column) under a mouse position, in history + grid lines.
    fn terminal_cell(t: &Terminal, at: Point<Pixels>) -> Option<(usize, usize)> {
        let area = *t.area.lock().unwrap_or_else(PoisonError::into_inner);
        let screen = t.screen.lock().unwrap_or_else(PoisonError::into_inner);
        let x = f32::from(at.x - area.origin.x);
        let y = f32::from(at.y - area.origin.y);
        if y < 0.0 {
            return None;
        }
        let row = ((y / LINE_HEIGHT) as usize).min(screen.rows.saturating_sub(1));
        let col = ((x.max(0.0) / CHAR_WIDTH) as usize).min(screen.cols.saturating_sub(1));
        Some((screen.first_shown(t.back) + row, col))
    }

    fn terminal_mouse_down(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        if let Some(t) = self.terminal.as_mut() {
            t.focused = true;
            t.selection = Self::terminal_cell(t, ev.position).map(|c| (c, c));
            t.selecting = t.selection.is_some();
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn terminal_mouse_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(t) = self.terminal.as_mut().filter(|t| t.selecting) else {
            return;
        };
        if ev.pressed_button != Some(MouseButton::Left) {
            t.selecting = false;
            return;
        }
        if let (Some(cell), Some((anchor, _))) = (Self::terminal_cell(t, ev.position), t.selection)
        {
            t.selection = Some((anchor, cell));
            cx.notify();
        }
    }

    fn terminal_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let Some(t) = self.terminal.as_mut() else {
            return;
        };
        let lines = match ev.delta {
            ScrollDelta::Lines(d) => d.y * 3.0,
            ScrollDelta::Pixels(d) => f32::from(d.y) / LINE_HEIGHT,
        };
        let history = t
            .screen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .history
            .len();
        let back = (t.back as f32 + lines).round().clamp(0.0, history as f32);
        t.back = back as usize;
        cx.stop_propagation();
        cx.notify();
    }

    /// The selected text, if a selection spans at least one character.
    fn terminal_selection_text(t: &Terminal) -> Option<String> {
        let (a, b) = t.selection.filter(|(a, b)| a != b)?;
        let text = t
            .screen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .text_between(a, b);
        (!text.is_empty()).then_some(text)
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
        let selected = Self::terminal_selection_text(t);
        // Copy: Ctrl+Shift+C, or Ctrl+C while something is selected (as in
        // Windows Terminal; without a selection Ctrl+C interrupts).
        if ks.key == "c" && m.control && !m.alt && (m.shift || selected.is_some()) {
            if let Some(text) = selected {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            t.selection = None;
            cx.notify();
            return true;
        }
        if ks.key == "v" && m.control && m.shift && !m.alt {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                // Line breaks as Enter; no other control characters.
                let clean: String = text
                    .replace("\r\n", "\r")
                    .replace('\n', "\r")
                    .chars()
                    .filter(|c| *c == '\r' || *c == '\t' || !c.is_control())
                    .collect();
                let _ = t.pty.write(clean.as_bytes());
            }
            t.back = 0;
            cx.notify();
            return true;
        }
        if let Some(bytes) = encode(ks) {
            let _ = t.pty.write(&bytes);
            // Typing goes back to the bottom and drops the selection.
            t.back = 0;
            t.selection = None;
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
        let first = screen.first_shown(term.back);
        let selection = term
            .selection
            .filter(|(a, b)| a != b)
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        let selected_color = theme.selected;
        // High contrast (T5.5): the system's window colors, no ANSI colors.
        let hc = theme.high_contrast;
        let (bg, fg, caption, caret) = if hc {
            (theme.base, theme.text, theme.text, theme.accent)
        } else {
            (0x0C0C0CFF, 0xCCCCCCFF, 0x9A9A9AFF, 0xCCCCCC99)
        };
        let default = Hsla::from(rgba(fg));
        let lines: Vec<gpui::AnyElement> = (0..screen.rows)
            .map(|r| {
                let k = first + r;
                let cells = screen.line(k).unwrap_or(&[]);
                let text: String = cells.iter().map(|c| c.ch).collect();
                let mut styles = Vec::new();
                let mut offset = 0;
                // The selected columns of this line.
                let picked = selection.and_then(|((la, ca), (lb, cb))| {
                    (la..=lb).contains(&k).then(|| {
                        let start = if k == la { ca } else { 0 };
                        let end = if k == lb { cb + 1 } else { cells.len() };
                        (start, end)
                    })
                });
                for c in cells {
                    let len = c.ch.len_utf8();
                    let color = match c.fg {
                        Color::Default => None,
                        Color::Ansi(_) if hc => None,
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
                    .relative()
                    .h(px(LINE_HEIGHT))
                    .whitespace_nowrap()
                    .when_some(picked, |d, (start, end)| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(start as f32 * CHAR_WIDTH))
                                .w(px(end.saturating_sub(start) as f32 * CHAR_WIDTH))
                                .h(px(LINE_HEIGHT))
                                .bg(rgba(selected_color)),
                        )
                    })
                    .child(StyledText::new(SharedString::from(text)).with_highlights(styles))
                    .into_any_element()
            })
            .collect();
        let (crow, ccol) = screen.cursor;
        drop(screen);
        let size = term.size.clone();
        let area = term.area.clone();
        // The cursor only while following the output.
        let focused = term.focused;
        let show_cursor = focused && term.back == 0;
        let label = term.shell.label();
        let scrolled = term.back;
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
                .bg(rgba(bg))
                .border_t_1()
                .border_color(rgba(if focused { theme.accent } else { theme.divider }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, ev: &MouseDownEvent, _, cx| view.terminal_mouse_down(ev, cx)),
                )
                .on_mouse_move(cx.listener(|view, ev: &MouseMoveEvent, _, cx| {
                    view.terminal_mouse_move(ev, cx);
                }))
                .on_scroll_wheel(cx.listener(|view, ev: &ScrollWheelEvent, _, cx| {
                    view.terminal_wheel(ev, cx);
                }))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(2.0))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(caption))
                        .child(if scrolled > 0 {
                            trf!(
                                "{label} · {scrolled} linhas acima · digite para voltar ao fim",
                                "{label} · {scrolled} lines up · type to go back to the end"
                            )
                        } else {
                            trf!(
                                "{label} · Ctrl+J fecha · arraste para selecionar, Ctrl+Shift+C copia · clique na lista para voltar a ela",
                                "{label} · Ctrl+J closes · drag to select, Ctrl+Shift+C copies · click the list to go back to it"
                            )
                        }),
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
                        // The first child is the top line: mouse positions
                        // are measured from it.
                        .on_children_prepainted(move |children, _, _| {
                            if let Some(first) = children.first() {
                                *area.lock().unwrap_or_else(PoisonError::into_inner) = *first;
                            }
                        })
                        .children(lines)
                        // The cursor.
                        .when(show_cursor, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(crow as f32 * LINE_HEIGHT))
                                    .left(px(space(2) + ccol as f32 * CHAR_WIDTH))
                                    .w(px(CHAR_WIDTH))
                                    .h(px(LINE_HEIGHT))
                                    .bg(rgba(caret)),
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
