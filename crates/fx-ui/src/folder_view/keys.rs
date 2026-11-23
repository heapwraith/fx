//! The keyboard of a folder view: every key arrives here first.

use super::*;

impl FolderView {
    /// Keyboard: navigation keys move the cursor (see the module comment);
    /// printable characters extend the "type to filter" text, Backspace removes
    /// its last character (or goes up when it is empty) and Escape clears it
    /// (or the selection).
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let started = Instant::now();
        let ks = &event.keystroke;
        let m = ks.modifiers;
        let only_ctrl = m.control && !m.alt && !m.shift;
        let only_alt = m.alt && !m.control && !m.shift;
        if self.notice.take().is_some() {
            cx.notify();
        }
        if ks.key == "j" && m.control && !m.alt && !m.shift {
            self.toggle_terminal(cx);
            return;
        }
        if self.terminal_key(ks, cx) {
            return;
        }
        if self.batch_rename_key(ks, window, cx) {
            return;
        }
        // Ctrl+Shift+M: the disk map (T6.1).
        if ks.key == "m" && m.control && m.shift && !m.alt {
            self.toggle_disk_map(cx);
            return;
        }
        if self.disk_map_key(ks, window, cx) {
            return;
        }
        if self.search.is_none() && self.columns_key(ks, window, cx) {
            return;
        }
        // In the column view only the keys that act on the chosen item get
        // through; the rest would act on the list hidden behind it.
        if self.search.is_none() && self.miller.is_some() && !columns_allow(ks) {
            if ks.key == "f2" {
                self.show_notice(
                    tr!(
                        "Para renomear, volte à lista (Esc) ou use o menu de contexto",
                        "To rename, go back to the list (Esc) or use the context menu"
                    )
                    .to_owned(),
                    cx,
                );
            }
            return;
        }
        if self.search_key(ks, started, window, cx) {
            cx.notify();
            return;
        }
        if ks.key == "f" && only_ctrl {
            self.close_quick_look();
            self.open_search(cx);
            return;
        }
        if self.crumb_menu && ks.key == "escape" {
            self.crumb_menu = false;
            cx.notify();
            return;
        }
        if self.view_menu && ks.key == "escape" {
            self.view_menu = false;
            cx.notify();
            return;
        }
        // Quick Look: Space (with no filter being typed) opens and closes it;
        // Esc closes it. Arrows keep moving the cursor; the preview follows.
        let plain_key = !m.control && !m.alt && !m.shift && !m.platform;
        if plain_key
            && ((ks.key == "space" && self.filter_text.is_empty())
                || (ks.key == "escape" && self.quick_look_open()))
            && self.renaming.is_none()
            && self.edit.is_none()
        {
            self.toggle_quick_look(started, cx);
            cx.notify();
            return;
        }
        if self.rename_key(ks, window, cx) {
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "rename");
            }
            cx.notify();
            return;
        }
        if self.edit.is_none() && self.ops_key(ks, window, cx) {
            cx.notify();
            return;
        }
        if self.edit_key(ks, window, cx) {
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "path");
            }
            cx.notify();
            return;
        }
        let bar = match ks.key.as_str() {
            "l" if only_ctrl => true,
            "d" if only_alt => true,
            _ => false,
        };
        if bar {
            self.begin_edit(window, cx);
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "path");
            }
            return;
        }
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        if m.control && !m.alt {
            // With Shift, GPUI reports the digit's shifted character ("@")
            // and drops Shift; the keyboard state tells the digit.
            let digit = if m.shift {
                ks.key.parse::<u8>().ok()
            } else if !ks.key.chars().all(|c| c.is_ascii_alphanumeric()) {
                fx_shell::shifted_digit()
            } else {
                None
            };
            if digit == Some(3) {
                self.toggle_columns(window, cx);
                return;
            }
            let grid = match digit {
                Some(2) => Some(true),
                Some(6) => Some(false),
                _ => None,
            };
            if let Some(grid) = grid {
                self.set_grid(grid);
                cx.notify();
                return;
            }
        }
        // In the grid, Up/Down move a whole row and Left/Right one tile.
        let row = if self.grid { self.grid_cols } else { 1 };
        let step = match ks.key.as_str() {
            "up" => Some(Step::Up(row)),
            "down" => Some(Step::Down(row)),
            "left" if self.grid && !m.alt => Some(Step::Up(1)),
            "right" if self.grid && !m.alt => Some(Step::Down(1)),
            "pageup" => Some(Step::Up(self.page_rows())),
            "pagedown" => Some(Step::Down(self.page_rows())),
            "home" => Some(Step::First),
            "end" => Some(Step::Last),
            _ => None,
        };
        let handled = match ks.key.as_str() {
            "up" if only_alt => self.go_up(window, cx).then_some("open"),
            "left" if only_alt => self.go_back(window, cx).then_some("open"),
            "right" if only_alt => self.go_forward(window, cx).then_some("open"),
            "enter" if only_alt => {
                self.show_properties(cx);
                Some("open")
            }
            // Alt+1..7 toggles a color tag on the selection, Alt+0 clears (T3.4).
            d @ ("0" | "1" | "2" | "3" | "4" | "5" | "6" | "7") if only_alt => {
                let paths = self.targets();
                match d.parse::<u8>().unwrap_or(0) {
                    0 => crate::tags::clear(&paths),
                    n => crate::tags::toggle(&paths, 1 << (n - 1)),
                }
                Some("move")
            }
            _ if m.alt || m.platform => None,
            _ if step.is_some() => {
                let mode = if m.shift {
                    MoveMode::Extend
                } else if m.control {
                    MoveMode::CursorOnly
                } else {
                    MoveMode::Select
                };
                if let Some(step) = step {
                    self.move_cursor(step, mode);
                }
                Some("move")
            }
            "enter" if plain => self.open_cursor(window, cx).then_some("open"),
            "enter" if m.shift && !m.control && !m.alt => {
                let file = self
                    .selection
                    .cursor()
                    .filter(|&i| !self.list.attributes(i).is_dir())
                    .map(|i| self.path.join(self.list.os_name(i)));
                file.map(|p| {
                    self.launch(Launch::OpenWith(p), cx);
                    "open"
                })
            }
            "`" | "'" if only_ctrl => {
                self.launch(Launch::Terminal(self.path.clone()), cx);
                Some("open")
            }
            "backspace" if plain && self.filter_text.is_empty() => {
                self.go_up(window, cx).then_some("open")
            }
            "a" if only_ctrl => {
                self.selection
                    .select_all(rows_of(&self.filtered, &self.list));
                Some("move")
            }
            "space" if only_ctrl => {
                self.selection.toggle_cursor();
                Some("move")
            }
            "escape" if plain && self.filter_text.is_empty() && !self.selection.is_empty() => {
                self.selection.clear();
                Some("move")
            }
            _ => None,
        };
        if let Some(kind) = handled {
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, kind);
            }
            cx.notify();
            return;
        }
        let mut text = self.filter_text.clone();
        match ks.key.as_str() {
            "backspace" => {
                text.pop();
            }
            "escape" => text.clear(),
            _ => {
                let m = &ks.modifiers;
                if m.control || m.alt || m.platform || m.function {
                    return;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => text.push_str(c),
                    _ => return,
                }
            }
        }
        if text == self.filter_text {
            return;
        }
        self.set_filter(text);
        if let Some(trace) = &self.trace {
            trace.borrow_mut().input_started(started, "filter");
        }
        cx.notify();
    }
}

/// Keys that make sense in the column view, acting on the chosen entry or
/// the focused column's folder (`targets`, `folder_in_view`).
pub(super) fn columns_allow(ks: &gpui::Keystroke) -> bool {
    let m = ks.modifiers;
    let ctrl = m.control && !m.alt && !m.platform;
    match ks.key.as_str() {
        "delete" => !m.control && !m.alt,
        "c" | "x" | "v" => ctrl && !m.shift,
        "f" | "k" | "j" | "t" | "w" | "tab" => ctrl,
        "enter" => m.alt,
        _ => false,
    }
}
