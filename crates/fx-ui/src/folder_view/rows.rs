//! Drawing the list: the rows (details and grid) and the scrollbar.

use super::*;

impl FolderView {
    pub(super) fn render_rows(
        &mut self,
        range: Range<usize>,
        focused: bool,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        if let Some(trace) = &self.trace {
            trace.borrow_mut().record_rows_built(range.len());
        }
        // Lines are rows or group headers (T6.10).
        let slots: Vec<groups::Slot> = range.map(|line| self.slot(line)).collect();
        let t = self.look.theme;
        let row_h = self.look.density.row_height();
        let columns = self.columns;
        let widths = self.widths;
        let first = slots
            .iter()
            .find_map(|s| match s {
                groups::Slot::Row(p) => Some(*p),
                groups::Slot::Header(_) => None,
            })
            .unwrap_or(0);
        let shown_entries: Vec<u32> = slots
            .iter()
            .filter_map(|s| match s {
                groups::Slot::Row(p) => self.rows().get(*p).copied(),
                groups::Slot::Header(_) => None,
            })
            .collect();
        self.want_media(&shown_entries, cx);
        // Icons of the rows drawn; unknown ones are requested after this pass.
        let icons = self.icons.read(cx);
        let show_icons = icons.enabled();
        let mut missing = Vec::new();
        let rows: Vec<(u32, Option<Arc<Image>>)> = shown_entries
            .iter()
            .copied()
            .map(|i| {
                if !show_icons {
                    return (i, None);
                }
                let key = self.icon_key_of(i);
                let image = icons.get(&key);
                if image.is_none() && icons.is_unknown(&key) {
                    missing.push(key);
                }
                (i, image)
            })
            .collect();
        if !missing.is_empty() {
            self.icons.update(cx, |icons, _| icons.request(missing));
        }
        let shown: Vec<u32> = rows.iter().map(|&(i, _)| i).collect();
        self.publish_a11y(first, &shown);
        let mut built = rows
            .into_iter()
            .map(|(i, icon)| {
                let is_dir = self.list.attributes(i).is_dir();
                let selected = self.selection.is_selected(i);
                let is_cursor = self.selection.cursor() == Some(i);
                let dim = self.look.show_hidden && self.list.is_hidden(i);
                let size: SharedString = if is_dir {
                    self.dir_sizes.label(i, self.list.size(i)).into()
                } else {
                    size_kb(self.list.size(i)).into()
                };
                div()
                    .id(i as usize)
                    .w_full()
                    .h(px(row_h))
                    .px(px(space(1)))
                    .child(
                        div()
                            .relative()
                            .size_full()
                            .flex()
                            .items_center()
                            .gap(px(space(3)))
                            .px(px(space(3)))
                            .rounded(px(RADIUS))
                            .when(selected, |d| d.bg(rgba(t.selected)))
                            .when(!selected, |d| d.hover(|s| s.bg(rgba(t.hover))))
                            // Always 1 px of border (transparent off the cursor),
                            // so the focus ring does not shift the text.
                            .border_1()
                            .border_color(if is_cursor && focused {
                                rgba(t.focus)
                            } else {
                                rgba(0x00000000)
                            })
                            .when(selected, |d| {
                                d.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top(px(row_h / 2.0 - 8.0))
                                        .w(px(INDICATOR_WIDTH))
                                        .h(px(16.0))
                                        .rounded(px(INDICATOR_WIDTH / 2.0))
                                        .bg(rgba(t.accent)),
                                )
                            })
                            // Fixed slot: the icon arriving later moves nothing.
                            .when(show_icons, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .when(dim, |d| d.opacity(HIDDEN_OPACITY))
                                        .size(px(ICON_SIZE))
                                        .when_some(icon, |d, image| {
                                            d.child(img(image).size(px(ICON_SIZE)))
                                        }),
                                )
                            })
                            .child(match self.renaming.as_ref().filter(|r| r.entry == i) {
                                Some(r) => self.render_rename_box(r, t).into_any_element(),
                                None => {
                                    let tags =
                                        crate::tags::mask_of(&self.path.join(self.list.os_name(i)));
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(px(space(2)))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .truncate()
                                                .when(dim, |d| d.opacity(HIDDEN_OPACITY))
                                                .text_color(rgba(t.text))
                                                .child(SharedString::from(
                                                    self.list.name(i).to_owned(),
                                                )),
                                        )
                                        .children(fx_core::tags::colors(tags).map(|(_, c)| {
                                            div()
                                                .flex_none()
                                                .size(px(8.0))
                                                .rounded_full()
                                                .bg(rgba(c))
                                        }))
                                        .children(self.git.states.get(self.list.name(i)).map(
                                            |state| {
                                                div()
                                                    .flex_none()
                                                    .text_size(px(CAPTION_SIZE))
                                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                                    .text_color(rgba(match state {
                                                        fx_core::git_status::GitState::Untracked => {
                                                            t.text_secondary
                                                        }
                                                        _ => t.accent,
                                                    }))
                                                    .child(state.letter())
                                            },
                                        ))
                                        .children(cloud_glyph(self.list.attributes(i)).map(
                                            |(glyph, strong)| {
                                                div()
                                                    .flex_none()
                                                    .font_family(self.look.icon_font)
                                                    .text_size(px(12.0))
                                                    .text_color(rgba(if strong {
                                                        t.accent
                                                    } else {
                                                        t.text_secondary
                                                    }))
                                                    .child(glyph)
                                            },
                                        ))
                                        .into_any_element()
                                }
                            })
                            .when(columns.kind, |d| {
                                d.child(
                                    div()
                                        .w(px(widths.kind))
                                        .flex_none()
                                        .truncate()
                                        .text_color(rgba(t.text_secondary))
                                        .child(type_label(
                                            self.list.name(i),
                                            self.list.attributes(i).is_dir(),
                                        )),
                                )
                            })
                            .when(columns.size, |d| {
                                d.child(
                                    div()
                                        .w(px(widths.size))
                                        .flex_none()
                                        .flex()
                                        .justify_end()
                                        .text_color(rgba(t.text_secondary))
                                        .child(size),
                                )
                            })
                            .when(columns.date, |d| {
                                d.child(
                                    div()
                                        .w(px(widths.date))
                                        .flex_none()
                                        .truncate()
                                        .text_color(rgba(t.text_secondary))
                                        .child(local_date(self.list.modified(i))),
                                )
                            })
                            .children(
                                MediaCol::ALL
                                    .into_iter()
                                    .filter(|m| columns.media[m.index()])
                                    .map(|m| {
                                        div()
                                            .w(px(widths.media[m.index()]))
                                            .flex_none()
                                            .truncate()
                                            .text_color(rgba(t.text_secondary))
                                            .children(self.media_value(i, m))
                                    }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, ev: &MouseDownEvent, window, cx| {
                            view.focus.focus(window);
                            view.click(i, ev, window, cx);
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |view, _: &MouseUpEvent, _, cx| {
                            view.release_on(i);
                            cx.notify();
                        }),
                    )
                    // Right click: the row joins (or becomes) the selection,
                    // then the shell menu opens for it.
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |view, _: &MouseDownEvent, window, cx| {
                            view.focus.focus(window);
                            if !view.selection.is_selected(i) {
                                view.selection.select_only(i);
                            }
                            let items = view.targets();
                            view.open_menu(items, window, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    // Files dropped on a folder row go into that folder.
                    .when(is_dir, |d| {
                        let target = self.path.join(self.list.os_name(i));
                        d.drag_over::<ExternalPaths>(move |s, _, _, _| s.bg(rgba(t.selected)))
                            .on_drop(cx.listener(move |view, paths: &ExternalPaths, window, cx| {
                                view.drop_into(paths.paths().to_vec(), target.clone(), window, cx);
                                cx.stop_propagation();
                            }))
                    })
                    .into_any_element()
            })
            .collect::<Vec<_>>()
            .into_iter();
        slots
            .into_iter()
            .filter_map(|s| match s {
                groups::Slot::Row(_) => built.next(),
                groups::Slot::Header(k) => {
                    Some(self.render_group_header(k, row_h).into_any_element())
                }
            })
            .collect()
    }

    /// The header of group `k`: its name over a line, as tall as a row.
    fn render_group_header(&self, k: usize, row_h: f32) -> impl IntoElement + use<> {
        let t = self.look.theme;
        div()
            .id(SharedString::from(format!("group-{k}")))
            .w_full()
            .h(px(row_h))
            .px(px(space(4)))
            .flex()
            .items_end()
            .child(
                div()
                    .w_full()
                    .pb(px(space(1)))
                    .flex()
                    .items_center()
                    .gap(px(space(2)))
                    .text_size(px(CAPTION_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgba(t.text_secondary))
                    .child(div().flex_none().child(self.group_label(k)))
                    .child(div().flex_1().h(px(1.0)).bg(rgba(t.divider))),
            )
    }

    pub(super) fn render_scrollbar(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let state = self.scroll.0.borrow();
        let viewport = state.base_handle.bounds();
        let view_h = f32::from(viewport.size.height);
        let content_h = state
            .last_item_size
            .map_or(0.0, |s| f32::from(s.contents.height));
        let offset = -f32::from(state.base_handle.offset().y);
        let base = state.base_handle.clone();
        drop(state);

        if view_h <= 0.0 || content_h <= view_h {
            return div().id("scrollbar");
        }
        let thumb_h = (view_h * view_h / content_h).max(THUMB_MIN);
        let max_scroll = content_h - view_h;
        let thumb_top = (offset / max_scroll).clamp(0.0, 1.0) * (view_h - thumb_h);
        let entity = cx.entity();

        div()
            .id("scrollbar")
            .absolute()
            .top(px(thumb_top))
            .right_0()
            .w(px(SCROLLBAR_WIDTH))
            .h(px(thumb_h))
            .p(px(2.0))
            .child(
                div()
                    .size_full()
                    .rounded(px(RADIUS))
                    .bg(rgba(self.look.theme.thumb))
                    .hover(|s| s.bg(rgba(self.look.theme.thumb_hover))),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |thumb, _, window, _| {
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |ev: &MouseDownEvent, _, _, cx| {
                                if thumb.contains(&ev.position) {
                                    entity.update(cx, |view, _| {
                                        view.drag_offset = Some(ev.position.y - thumb.origin.y);
                                    });
                                }
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |_: &MouseUpEvent, _, _, cx| {
                                entity.update(cx, |view, _| view.drag_offset = None);
                            }
                        });
                        window.on_mouse_event(move |ev: &MouseMoveEvent, _, _, cx| {
                            let Some(grab) = entity.read(cx).drag_offset else {
                                return;
                            };
                            if !ev.dragging() {
                                return;
                            }
                            let top = f32::from(ev.position.y - viewport.origin.y - grab);
                            let fraction = (top / (view_h - thumb_h)).clamp(0.0, 1.0);
                            base.set_offset(point(px(0.0), px(-fraction * max_scroll)));
                            cx.notify(entity.entity_id());
                        });
                    },
                )
                // Pinned to the thumb's corner: without top/left an absolute child
                // sits at its static position, below the visible thumb.
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}
