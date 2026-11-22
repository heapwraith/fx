//! Grid view (task T1.7): tiles with a thumbnail (or large icon) and the name,
//! virtualized as rows of tiles in a `uniform_list`, so only the rows on screen
//! are built. Thumbnails are asked for the tiles drawn; until one arrives the
//! tile shows the small file icon, in a box of the final size.

use std::ops::Range;
use std::path::PathBuf;

use gpui::{Context, MouseButton, MouseDownEvent, SharedString, div, img, prelude::*, px};

use super::{FolderView, ICON_SIZE, SCROLLBAR_WIDTH};
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

pub const TILE_WIDTH: f32 = 120.0;
pub const TILE_HEIGHT: f32 = 136.0;
/// Side of the image box (thumbnails are at most 96 px).
const IMAGE_BOX: f32 = 96.0;

/// Tiles per row for a list `width` px wide.
pub fn columns(width: f32) -> usize {
    let usable = width - SCROLLBAR_WIDTH - 2.0 * space(1);
    if usable <= 0.0 {
        return 4; // before the first layout
    }
    ((usable / TILE_WIDTH).floor() as usize).max(1)
}

impl FolderView {
    pub(super) fn render_grid_rows(
        &mut self,
        range: Range<usize>,
        cols: usize,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Vec<impl IntoElement + use<>> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().record_rows_built(range.len());
        }
        let t = self.look.theme;
        let cursor = self.selection.cursor();
        let first = range.start * cols;
        let last = (range.end * cols).min(self.rows().len());
        let entries: Vec<u32> = self.rows()[first..last].to_vec();

        // Thumbnails of the tiles drawn (the queue follows this list).
        let paths: Vec<PathBuf> = entries
            .iter()
            .map(|&i| self.path.join(self.list.os_name(i)))
            .collect();
        // Never the content of an item only in the cloud (T3.9).
        let readable: Vec<PathBuf> = entries
            .iter()
            .zip(&paths)
            .filter(|(i, _)| super::may_read_content(self.list.attributes(**i)))
            .map(|(_, p)| p.clone())
            .collect();
        self.thumbs
            .update(cx, |thumbs, cx| thumbs.want(readable, cx));
        let thumbs = self.thumbs.read(cx);
        let icons = self.icons.read(cx);
        let mut missing_icons = Vec::new();
        let tiles: Vec<_> = entries
            .iter()
            .zip(&paths)
            .map(|(&i, path)| {
                let thumb = thumbs.get(path);
                let icon = if thumb.is_none() && icons.enabled() {
                    let key = self.icon_key_of(i);
                    let icon = icons.get(&key);
                    if icon.is_none() && icons.is_unknown(&key) {
                        missing_icons.push(key);
                    }
                    icon
                } else {
                    None
                };
                (i, thumb, icon)
            })
            .collect();
        if !missing_icons.is_empty() {
            self.icons
                .update(cx, |icons, _| icons.request(missing_icons));
        }

        self.publish_a11y(first, &entries);
        let mut rows = Vec::with_capacity(range.len());
        for chunk in tiles.chunks(cols) {
            let mut row = div().h(px(TILE_HEIGHT)).px(px(space(1))).flex();
            for (i, thumb, icon) in chunk.iter().cloned() {
                let selected = self.selection.is_selected(i);
                let is_cursor = cursor == Some(i);
                let name: SharedString = self.list.name(i).to_owned().into();
                let image_box = div()
                    .size(px(IMAGE_BOX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(thumb, |d, image| d.child(img(image)))
                    .when_some(icon, |d, image| d.child(img(image).size(px(ICON_SIZE))));
                row = row.child(
                    div()
                        .id(i as usize)
                        .w(px(TILE_WIDTH))
                        .h_full()
                        .p(px(space(1)))
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(space(1)))
                                .pt(px(space(1)))
                                .px(px(space(1)))
                                .rounded(px(RADIUS))
                                .border_1()
                                .border_color(if is_cursor && focused {
                                    rgba(t.focus)
                                } else {
                                    rgba(0x00000000)
                                })
                                .when(selected, |d| d.bg(rgba(t.selected)))
                                .when(!selected, |d| d.hover(|s| s.bg(rgba(t.hover))))
                                .child(image_box)
                                .child(
                                    div()
                                        .w_full()
                                        .text_center()
                                        .truncate()
                                        .text_size(px(CAPTION_SIZE))
                                        .text_color(rgba(t.text))
                                        .child(name),
                                ),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, ev: &MouseDownEvent, window, cx| {
                                view.focus.focus(window);
                                view.click(i, ev, window, cx);
                                cx.notify();
                            }),
                        ),
                );
            }
            rows.push(row);
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_fit_the_width() {
        assert_eq!(columns(0.0), 4);
        assert_eq!(columns(100.0), 1, "at least one");
        let w = SCROLLBAR_WIDTH + 2.0 * space(1) + 5.0 * TILE_WIDTH;
        assert_eq!(columns(w), 5);
        assert_eq!(columns(w - 1.0), 4);
    }
}
