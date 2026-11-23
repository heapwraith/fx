//! Disk map (T6.1, D-068): what takes space in the folder, as a squarified
//! treemap (`fx_core::treemap`). Files have their size; subfolders their real
//! total, from the index or summed in the background (`dir_sizes`), with the
//! progress shown while that runs (nothing estimated). Enter goes into a
//! folder (the map stays), Backspace goes up, Delete sends to the Recycle Bin
//! (with confirmation and undo), Esc closes. Ctrl+Shift+M opens and closes.

use fx_core::format::size_short;
use fx_core::treemap::{Rect, squarify};
use fx_core::{tr, trf};
use gpui::{ClickEvent, Context, Keystroke, SharedString, Window, div, prelude::*, px, relative};

use super::{FolderView, rows_of};
use crate::theme::{CAPTION_SIZE, MAP_COLORS, RADIUS, rgba, space};

/// Most blocks drawn; the rest becomes one "outros" block.
const MAX_BLOCKS: usize = 150;

/// The open map: which block has the keyboard (in size order).
pub(super) struct DiskMap {
    cursor: usize,
}

/// One block: an entry of the list, or the rest together.
struct Block {
    entry: Option<u32>,
    name: String,
    bytes: u64,
    is_dir: bool,
    /// For a folder: its total is final.
    known: bool,
    color: u32,
}

/// The color group of a file by its extension (folders are group 0).
fn group(name: &str, is_dir: bool) -> usize {
    if is_dir {
        return 0;
    }
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "heic" | "bmp" | "tif" | "tiff" | "svg"
        | "raw" | "cr2" | "nef" | "arw" => 1,
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "wmv" | "m4v" => 2,
        "mp3" | "flac" | "wav" | "ogg" | "m4a" | "aac" | "wma" => 3,
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "md" | "odt" | "ods"
        | "rtf" | "csv" => 4,
        "zip" | "7z" | "rar" | "tar" | "gz" | "xz" | "iso" | "cab" | "msi" => 5,
        _ => 6,
    }
}

impl FolderView {
    /// Ctrl+Shift+M: opens the map of this folder, or closes it.
    pub(super) fn toggle_disk_map(&mut self, cx: &mut Context<Self>) {
        self.disk_map = match self.disk_map {
            Some(_) => None,
            None => Some(DiskMap { cursor: 0 }),
        };
        cx.notify();
    }

    /// The blocks, largest first (zero-byte items are left out).
    fn map_blocks(&self) -> (Vec<Block>, usize) {
        let mut all: Vec<Block> = rows_of(&self.filtered, &self.list)
            .iter()
            .map(|&i| {
                let is_dir = self.list.attributes(i).is_dir();
                let name = self.list.name(i).to_owned();
                Block {
                    entry: Some(i),
                    bytes: self.list.size(i),
                    known: !is_dir || self.dir_sizes.is_known(i),
                    color: MAP_COLORS[group(&name, is_dir)],
                    name,
                    is_dir,
                }
            })
            .collect();
        let empty = all.iter().filter(|b| b.bytes == 0).count();
        all.retain(|b| b.bytes > 0);
        all.sort_by_key(|b| std::cmp::Reverse(b.bytes));
        if all.len() > MAX_BLOCKS {
            let rest = all.split_off(MAX_BLOCKS - 1);
            all.push(Block {
                entry: None,
                name: trf!("outros {} itens", "{} other items", rest.len()),
                bytes: rest.iter().map(|b| b.bytes).sum(),
                is_dir: false,
                known: true,
                color: MAP_COLORS[6],
            });
        }
        (all, empty)
    }

    /// Keys while the map is open. Returns whether it took them.
    pub(super) fn disk_map_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(map) = self.disk_map.as_ref() else {
            return false;
        };
        let (blocks, _) = self.map_blocks();
        let n = blocks.len();
        let cursor = map.cursor.min(n.saturating_sub(1));
        let m = ks.modifiers;
        if m.control || m.alt && ks.key != "up" {
            return false;
        }
        match ks.key.as_str() {
            "escape" => self.disk_map = None,
            "right" | "down" if n > 0 => self.set_map_cursor((cursor + 1) % n),
            "left" | "up" if m.alt => {
                self.go_up(window, cx);
                self.set_map_cursor(0);
            }
            "left" | "up" if n > 0 => self.set_map_cursor((cursor + n - 1) % n),
            "home" => self.set_map_cursor(0),
            "end" if n > 0 => self.set_map_cursor(n - 1),
            "backspace" => {
                self.go_up(window, cx);
                self.set_map_cursor(0);
            }
            "enter" => self.open_block(cursor, window, cx),
            "delete" => {
                if let Some(entry) = blocks.get(cursor).and_then(|b| b.entry) {
                    let path = self.path.join(self.list.os_name(entry));
                    self.recycle_confirmed(vec![path], window, cx);
                }
            }
            _ => return true,
        }
        cx.notify();
        true
    }

    fn set_map_cursor(&mut self, k: usize) {
        if let Some(map) = self.disk_map.as_mut() {
            map.cursor = k;
        }
    }

    /// Enter or double click: into a folder (the map stays), or back to the
    /// list with the file chosen.
    fn open_block(&mut self, k: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (blocks, _) = self.map_blocks();
        let Some(block) = blocks.get(k) else { return };
        let Some(entry) = block.entry else { return };
        if block.is_dir {
            let path = self.path.join(self.list.os_name(entry));
            self.go_to(path, None, window, cx);
            self.set_map_cursor(0);
        } else {
            self.disk_map = None;
            self.a11y_select(entry);
        }
        cx.notify();
    }

    pub(super) fn render_disk_map(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let map = self.disk_map.as_ref()?;
        let t = self.look.theme;
        let (blocks, empty) = self.map_blocks();
        let cursor = map.cursor.min(blocks.len().saturating_sub(1));
        let total: u64 = blocks.iter().map(|b| b.bytes).sum();
        let pending = blocks.iter().filter(|b| b.is_dir && !b.known).count();
        // The real proportions of the area: the list's.
        let bounds = self.scroll.0.borrow().base_handle.bounds();
        let (w, h) = (
            f32::from(bounds.size.width).max(320.0),
            (f32::from(bounds.size.height) - 40.0).max(200.0),
        );
        let sizes: Vec<f64> = blocks.iter().map(|b| b.bytes as f64).collect();
        let rects = squarify(
            &sizes,
            Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
        );
        let status = if self.dir_sizes.running() && pending > 0 {
            trf!("somando {} pastas…", "summing {} folders…", pending)
        } else if empty > 0 {
            trf!(
                "{} itens vazios fora do mapa",
                "{} empty items not shown",
                empty
            )
        } else {
            String::new()
        };
        let mut area = div().relative().flex_1().min_h_0().m(px(space(2)));
        for (k, (b, r)) in blocks.iter().zip(&rects).enumerate() {
            if r.w < 1.0 || r.h < 1.0 {
                continue;
            }
            let focused = k == cursor;
            let big = r.w > 70.0 && r.h > 34.0;
            let mut block = div()
                .id(("map-block", k))
                .absolute()
                .left(relative(r.x / w))
                .top(relative(r.y / h))
                .w(relative(r.w / w))
                .h(relative(r.h / h))
                .p(px(1.0))
                .child(
                    div()
                        .size_full()
                        .rounded(px(RADIUS))
                        .bg(rgba(b.color))
                        .border_2()
                        .border_color(rgba(if focused { t.accent } else { 0x0000_0000 }))
                        .overflow_hidden()
                        .px(px(space(1)))
                        .when(big, |d| {
                            d.child(
                                div()
                                    .text_size(px(CAPTION_SIZE))
                                    .truncate()
                                    .child(SharedString::from(b.name.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(CAPTION_SIZE))
                                    .text_color(rgba(t.text_secondary))
                                    .child(if b.known {
                                        size_short(b.bytes)
                                    } else {
                                        format!("{}…", size_short(b.bytes))
                                    }),
                            )
                        }),
                );
            if b.entry.is_some() {
                block = block.cursor_pointer().on_click(cx.listener(
                    move |view, ev: &ClickEvent, window, cx| {
                        view.set_map_cursor(k);
                        if ev.click_count() >= 2 {
                            view.open_block(k, window, cx);
                        }
                        cx.notify();
                    },
                ));
            }
            area = area.child(block);
        }
        let chosen = blocks.get(cursor).map(|b| {
            let share = if total > 0 {
                (b.bytes as f64 * 100.0 / total as f64).round() as u64
            } else {
                0
            };
            format!("{} · {} · {share}%", b.name, size_short(b.bytes))
        });
        Some(
            div()
                .id("disk-map")
                .absolute()
                .top(px(space(12)))
                .left_0()
                .right_0()
                .bottom_0()
                .flex()
                .flex_col()
                .bg(rgba(t.base))
                .border_t_1()
                .border_color(rgba(t.divider))
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(2)))
                        .flex()
                        .gap(px(space(3)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(
                            div()
                                .text_color(rgba(t.text))
                                .child(trf!("Mapa · {}", "Map · {}", size_short(total))),
                        )
                        .children(chosen)
                        .child(div().flex_1())
                        .child(status),
                )
                .child(area)
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!(
                            "Setas escolhem · Enter entra · Backspace sobe · Delete: Lixeira · Esc fecha",
                            "Arrows choose · Enter goes in · Backspace goes up · Delete: Recycle Bin · Esc closes"
                        )),
                ),
        )
    }

    /// The map as a screen reader reads it: the blocks in size order.
    pub(super) fn disk_map_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        let map = self.disk_map.as_ref()?;
        let (blocks, _) = self.map_blocks();
        let total: u64 = blocks.iter().map(|b| b.bytes).sum();
        let items = blocks
            .iter()
            .enumerate()
            .map(|(k, b)| {
                let share = (b.bytes * 100).checked_div(total).unwrap_or(0);
                Item::Choice {
                    label: b.name.clone(),
                    description: Some(format!("{}, {share}%", size_short(b.bytes))),
                    selected: k == map.cursor,
                }
            })
            .collect::<Vec<_>>();
        let focus = (!items.is_empty()).then(|| map.cursor.min(items.len() - 1));
        Some(Overlay {
            title: trf!("Mapa de {}", "Map of {}", self.path.display()),
            description: Some(
                tr!(
                    "O que ocupa espaço, do maior ao menor. Enter entra na pasta, Backspace sobe, Delete manda para a Lixeira, Esc fecha.",
                    "What takes space, largest first. Enter goes into a folder, Backspace goes up, Delete sends to the Recycle Bin, Esc closes."
                )
                .to_owned(),
            ),
            items,
            focus,
            positions: None,
            modal: true,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn color_groups_by_kind() {
        assert_eq!(super::group("Fotos", true), 0);
        assert_eq!(super::group("praia.JPG", false), 1);
        assert_eq!(super::group("filme.mkv", false), 2);
        assert_eq!(super::group("relatório.pdf", false), 4);
        assert_eq!(super::group("backup.7z", false), 5);
        assert_eq!(super::group("sem-extensao", false), 6);
    }
}
