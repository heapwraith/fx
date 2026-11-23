//! A minimal VT terminal screen for the embedded terminal (task T3.5): what
//! ConPTY sends (UTF-8 text, CR/LF/BS/TAB, CSI cursor and erase sequences,
//! SGR colors) becomes a grid of cells. Unknown sequences are skipped.

/// A cell color: one of the 16 ANSI colors, or the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    Default,
    Ansi(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bold: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bold: false,
        }
    }
}

#[derive(Debug)]
enum State {
    Ground,
    Escape,
    Csi(String),
    /// OSC (title etc.): skipped until BEL or ESC \.
    Osc(bool),
}

#[derive(Debug)]
pub struct Screen {
    pub cols: usize,
    pub rows: usize,
    /// Lines that scrolled off the top (oldest first), capped.
    pub history: Vec<Vec<Cell>>,
    pub grid: Vec<Vec<Cell>>,
    pub cursor: (usize, usize),
    fg: Color,
    bold: bool,
    state: State,
    /// Bytes of an incomplete UTF-8 character.
    pending: Vec<u8>,
}

const HISTORY: usize = 2000;

impl Screen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let (cols, rows) = (cols.max(1), rows.max(1));
        Self {
            cols,
            rows,
            history: Vec::new(),
            grid: vec![vec![Cell::default(); cols]; rows],
            cursor: (0, 0),
            fg: Color::Default,
            bold: false,
            state: State::Ground,
            pending: Vec::new(),
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let (cols, rows) = (cols.max(1), rows.max(1));
        for line in &mut self.grid {
            line.resize(cols, Cell::default());
        }
        while self.grid.len() > rows {
            let top = self.grid.remove(0);
            self.push_history(top);
        }
        while self.grid.len() < rows {
            self.grid.push(vec![Cell::default(); cols]);
        }
        self.cols = cols;
        self.rows = rows;
        self.cursor = (self.cursor.0.min(rows - 1), self.cursor.1.min(cols - 1));
    }

    fn push_history(&mut self, line: Vec<Cell>) {
        self.history.push(line);
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
    }

    /// A line of the grid as text (trailing spaces removed).
    pub fn line_text(&self, row: usize) -> String {
        let s: String = self.grid[row].iter().map(|c| c.ch).collect();
        s.trim_end().to_owned()
    }

    /// Lines in the history and the grid together (scrollback, T5.4).
    pub fn total_lines(&self) -> usize {
        self.history.len() + self.rows
    }

    /// Line `k` of history followed by grid (0 is the oldest kept).
    pub fn line(&self, k: usize) -> Option<&[Cell]> {
        match k.checked_sub(self.history.len()) {
            None => self.history.get(k).map(Vec::as_slice),
            Some(r) => self.grid.get(r).map(Vec::as_slice),
        }
    }

    /// The first line shown when scrolled `back` lines up (clamped to the
    /// history), as an index for [`Screen::line`].
    pub fn first_shown(&self, back: usize) -> usize {
        self.history.len() - back.min(self.history.len())
    }

    /// The text from `from` to `to` (line, column; either order), lines
    /// joined with CR LF and trailing spaces dropped: what a selection copies.
    pub fn text_between(&self, from: (usize, usize), to: (usize, usize)) -> String {
        let (a, b) = if from <= to { (from, to) } else { (to, from) };
        let mut out = Vec::new();
        for k in a.0..=b.0 {
            let Some(cells) = self.line(k) else { break };
            let start = if k == a.0 { a.1.min(cells.len()) } else { 0 };
            let end = if k == b.0 {
                (b.1 + 1).min(cells.len())
            } else {
                cells.len()
            };
            let text: String = cells[start..end.max(start)].iter().map(|c| c.ch).collect();
            out.push(text.trim_end().to_owned());
        }
        out.join("\r\n")
    }

    /// Whether the shell waits at its prompt with nothing typed: the cursor
    /// line ends in `>` (PowerShell `PS C:\x> `, cmd `C:\x>`) right before
    /// the cursor and is blank after it. Used to follow the tab's folder
    /// without mixing a `cd` into what someone is typing.
    pub fn at_empty_prompt(&self) -> bool {
        let (row, col) = self.cursor;
        let Some(cells) = self.grid.get(row) else {
            return false;
        };
        let before: String = cells[..col.min(cells.len())].iter().map(|c| c.ch).collect();
        let after_blank = cells[col.min(cells.len())..].iter().all(|c| c.ch == ' ');
        after_blank && before.trim_end().ends_with('>')
    }

    /// Feeds bytes from the terminal.
    pub fn feed(&mut self, bytes: &[u8]) {
        let mut data = std::mem::take(&mut self.pending);
        data.extend_from_slice(bytes);
        let mut i = 0;
        while i < data.len() {
            let b = data[i];
            let len = match b {
                0x00..=0x7F => 1,
                0xC0..=0xDF => 2,
                0xE0..=0xEF => 3,
                0xF0..=0xF7 => 4,
                _ => 1,
            };
            if i + len > data.len() {
                self.pending = data[i..].to_vec();
                return;
            }
            let ch = std::str::from_utf8(&data[i..i + len])
                .ok()
                .and_then(|s| s.chars().next())
                .unwrap_or('\u{FFFD}');
            self.char(ch);
            i += len;
        }
    }

    fn char(&mut self, ch: char) {
        match std::mem::replace(&mut self.state, State::Ground) {
            State::Ground => match ch {
                '\x1b' => self.state = State::Escape,
                '\r' => self.cursor.1 = 0,
                '\n' => self.line_feed(),
                '\x08' => self.cursor.1 = self.cursor.1.saturating_sub(1),
                '\t' => self.cursor.1 = ((self.cursor.1 / 8 + 1) * 8).min(self.cols - 1),
                '\x07' => {}
                c if c.is_control() => {}
                c => self.print(c),
            },
            State::Escape => match ch {
                '[' => self.state = State::Csi(String::new()),
                ']' => self.state = State::Osc(false),
                _ => {} // other escapes: ignored
            },
            State::Csi(mut params) => {
                if ('\x40'..='\x7e').contains(&ch) {
                    self.csi(&params, ch);
                } else {
                    params.push(ch);
                    self.state = State::Csi(params);
                }
            }
            State::Osc(esc) => match ch {
                '\x07' => {}
                '\\' if esc => {}
                '\x1b' => self.state = State::Osc(true),
                _ => self.state = State::Osc(false),
            },
        }
    }

    fn print(&mut self, c: char) {
        if self.cursor.1 >= self.cols {
            self.cursor.1 = 0;
            self.line_feed();
        }
        let (r, col) = self.cursor;
        self.grid[r][col] = Cell {
            ch: c,
            fg: self.fg,
            bold: self.bold,
        };
        self.cursor.1 += 1;
    }

    fn line_feed(&mut self) {
        if self.cursor.0 + 1 < self.rows {
            self.cursor.0 += 1;
        } else {
            let top = self.grid.remove(0);
            self.push_history(top);
            self.grid.push(vec![Cell::default(); self.cols]);
        }
    }

    fn csi(&mut self, params: &str, cmd: char) {
        let private = params.starts_with('?');
        let nums: Vec<usize> = params
            .trim_start_matches('?')
            .split(';')
            .map(|p| p.parse().unwrap_or(0))
            .collect();
        let n =
            |i: usize, default: usize| nums.get(i).copied().filter(|&v| v != 0).unwrap_or(default);
        let (rows, cols) = (self.rows, self.cols);
        match cmd {
            _ if private => {} // modes (cursor visibility...): ignored
            'A' => self.cursor.0 = self.cursor.0.saturating_sub(n(0, 1)),
            'B' => self.cursor.0 = (self.cursor.0 + n(0, 1)).min(rows - 1),
            'C' => self.cursor.1 = (self.cursor.1 + n(0, 1)).min(cols - 1),
            'D' => self.cursor.1 = self.cursor.1.saturating_sub(n(0, 1)),
            'G' => self.cursor.1 = (n(0, 1) - 1).min(cols - 1),
            'H' | 'f' => {
                self.cursor = ((n(0, 1) - 1).min(rows - 1), (n(1, 1) - 1).min(cols - 1));
            }
            'J' => {
                let (r, c) = self.cursor;
                match nums.first().copied().unwrap_or(0) {
                    0 => {
                        self.clear_line(r, c, cols);
                        for row in r + 1..rows {
                            self.clear_line(row, 0, cols);
                        }
                    }
                    1 => {
                        for row in 0..r {
                            self.clear_line(row, 0, cols);
                        }
                        self.clear_line(r, 0, c + 1);
                    }
                    _ => {
                        for row in 0..rows {
                            self.clear_line(row, 0, cols);
                        }
                    }
                }
            }
            'K' => {
                let (r, c) = self.cursor;
                match nums.first().copied().unwrap_or(0) {
                    0 => self.clear_line(r, c, cols),
                    1 => self.clear_line(r, 0, c + 1),
                    _ => self.clear_line(r, 0, cols),
                }
            }
            'X' => {
                let (r, c) = self.cursor;
                self.clear_line(r, c, (c + n(0, 1)).min(cols));
            }
            'm' => self.sgr(&nums),
            _ => {}
        }
    }

    fn clear_line(&mut self, row: usize, from: usize, to: usize) {
        for cell in &mut self.grid[row][from.min(self.cols)..to.min(self.cols)] {
            *cell = Cell::default();
        }
    }

    fn sgr(&mut self, nums: &[usize]) {
        let mut i = 0;
        while i < nums.len() {
            match nums[i] {
                0 => {
                    self.fg = Color::Default;
                    self.bold = false;
                }
                1 => self.bold = true,
                22 => self.bold = false,
                v @ 30..=37 => self.fg = Color::Ansi((v - 30) as u8),
                39 => self.fg = Color::Default,
                v @ 90..=97 => self.fg = Color::Ansi((v - 90 + 8) as u8),
                // 256-color and RGB: skip their arguments.
                38 => i += if nums.get(i + 1) == Some(&5) { 2 } else { 4 },
                48 => i += if nums.get(i + 1) == Some(&5) { 2 } else { 4 },
                _ => {}
            }
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollback_selection_and_prompt() {
        let mut s = Screen::new(10, 2);
        s.feed(b"one\r\ntwo\r\nthree\r\nPS C:\\> ");
        assert_eq!(s.history.len(), 2);
        assert_eq!(s.total_lines(), 4);
        assert_eq!(s.first_shown(0), 2, "the grid");
        assert_eq!(s.first_shown(1), 1);
        assert_eq!(s.first_shown(99), 0, "clamped to the history");
        assert_eq!(s.text_between((0, 1), (1, 1)), "ne\r\ntw");
        assert_eq!(
            s.text_between((2, 4), (0, 0)),
            "one\r\ntwo\r\nthree",
            "either order"
        );
        assert!(s.at_empty_prompt());
        s.feed(b"dir");
        assert!(!s.at_empty_prompt(), "something typed");
    }

    #[test]
    fn text_newlines_and_wrap() {
        let mut s = Screen::new(5, 3);
        s.feed(b"ab\r\ncdefgh");
        assert_eq!(s.line_text(0), "ab");
        assert_eq!(s.line_text(1), "cdefg");
        assert_eq!(s.line_text(2), "h");
        s.feed(b"\r\nX");
        assert_eq!(s.history.len(), 1, "the top line scrolled off");
        assert_eq!(s.line_text(2), "X");
    }

    #[test]
    fn utf8_split_across_reads() {
        let mut s = Screen::new(10, 1);
        let bytes = "ação".as_bytes();
        s.feed(&bytes[..2]);
        s.feed(&bytes[2..]);
        assert_eq!(s.line_text(0), "ação");
    }

    #[test]
    fn cursor_moves_and_erases() {
        let mut s = Screen::new(10, 3);
        s.feed(b"hello\x1b[2;3Hxy\x1b[1;1H\x1b[K");
        assert_eq!(s.line_text(0), "");
        assert_eq!(s.line_text(1), "  xy");
        s.feed(b"\x1b[2J");
        assert!((0..3).all(|r| s.line_text(r).is_empty()));
        s.feed(b"\x1b]0;title\x07ok");
        assert_eq!(s.line_text(0), "ok", "OSC title skipped");
    }

    #[test]
    fn colors() {
        let mut s = Screen::new(10, 1);
        s.feed(b"\x1b[1;31mR\x1b[0mn\x1b[92mg\x1b[38;5;200mx\x1b[38;2;1;2;3my");
        assert_eq!(s.grid[0][0].fg, Color::Ansi(1));
        assert!(s.grid[0][0].bold);
        assert_eq!(s.grid[0][1].fg, Color::Default);
        assert_eq!(s.grid[0][2].fg, Color::Ansi(10));
        assert_eq!(
            s.line_text(0),
            "Rngxy",
            "extended colors are skipped, text kept"
        );
    }

    #[test]
    fn resize_keeps_the_bottom() {
        let mut s = Screen::new(4, 3);
        s.feed(b"1\r\n2\r\n3");
        s.resize(4, 2);
        assert_eq!(s.line_text(0), "2");
        assert_eq!(s.line_text(1), "3");
        assert_eq!(s.history.len(), 1);
    }
}
