//! The "Exibir" (View) menu of a tab (T1.21): hidden items, sort order and
//! density, so nothing is reachable only by a shortcut. Density and hidden
//! items are window-wide: the tab asks `TabsView` through a [`ViewEvent`].
//! Also the Properties sheet (Alt+Enter and the context menu).

use fx_core::{Direction, SortKey};
#[allow(unused_imports)]
use fx_core::{tr, trf};
use fx_shell::launch::Launch;
use gpui::{
    ClickEvent, Context, EventEmitter, MouseButton, MouseDownEvent, SharedString, deferred, div,
    prelude::*, px,
};

use super::FolderView;
use crate::theme::{CAPTION_SIZE, Density, RADIUS, rgba, space};

const MENU_WIDTH: f32 = 248.0;
const ICON_CHECK: &str = "\u{E73E}";

/// What a tab asks its window to change for every tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEvent {
    Density(Density),
    ToggleHidden,
}

impl EventEmitter<ViewEvent> for FolderView {}

/// One line of the menu.
enum Item {
    Caption(&'static str),
    Divider,
    Choice {
        id: &'static str,
        label: &'static str,
        hint: &'static str,
        checked: bool,
        action: Action,
    },
}

#[derive(Debug, Clone, Copy)]
enum Action {
    ToggleHidden,
    Sort(SortKey),
    Direction(Direction),
    Density(Density),
}

impl FolderView {
    /// Properties of the targets (selection or cursor), or of this folder
    /// when nothing is chosen. Off the UI thread (a launch).
    pub(super) fn show_properties(&mut self, cx: &mut Context<Self>) {
        let mut items = self.targets();
        if items.is_empty() {
            items.push(self.path.clone());
        }
        self.launch(Launch::Properties(items), cx);
    }

    pub(super) fn toggle_view_menu(&mut self, cx: &mut Context<Self>) {
        self.view_menu = !self.view_menu;
        cx.notify();
    }

    fn menu_items(&self) -> Vec<Item> {
        let (key, dir) = self.sort;
        let density = self.look.density;
        let sort = |id, label, k: SortKey| Item::Choice {
            id,
            label,
            hint: "",
            checked: key == k,
            action: Action::Sort(k),
        };
        let dens = |id, label, hint, d: Density| Item::Choice {
            id,
            label,
            hint,
            checked: density == d,
            action: Action::Density(d),
        };
        vec![
            Item::Choice {
                id: "hidden",
                label: tr!("Mostrar itens ocultos", "Show hidden items"),
                hint: "Ctrl+H",
                checked: self.look.show_hidden,
                action: Action::ToggleHidden,
            },
            Item::Divider,
            Item::Caption(tr!("Ordenar por", "Sort by")),
            sort("sort-name", tr!("Nome", "Name"), SortKey::Name),
            sort("sort-type", tr!("Tipo", "Type"), SortKey::Type),
            sort("sort-size", tr!("Tamanho", "Size"), SortKey::Size),
            sort(
                "sort-date",
                tr!("Modificado em", "Date modified"),
                SortKey::Modified,
            ),
            Item::Choice {
                id: "asc",
                label: tr!("Crescente", "Ascending"),
                hint: "",
                checked: dir == Direction::Ascending,
                action: Action::Direction(Direction::Ascending),
            },
            Item::Choice {
                id: "desc",
                label: tr!("Decrescente", "Descending"),
                hint: "",
                checked: dir == Direction::Descending,
                action: Action::Direction(Direction::Descending),
            },
            Item::Divider,
            Item::Caption(tr!("Densidade", "Density")),
            dens(
                "compact",
                tr!("Compacta", "Compact"),
                "Ctrl+1",
                Density::Compact,
            ),
            dens(
                "standard",
                tr!("Padrão", "Standard"),
                "Ctrl+2",
                Density::Standard,
            ),
            dens(
                "comfortable",
                tr!("Confortável", "Comfortable"),
                "Ctrl+3",
                Density::Comfortable,
            ),
        ]
    }

    fn run(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::ToggleHidden => cx.emit(ViewEvent::ToggleHidden),
            Action::Density(d) => cx.emit(ViewEvent::Density(d)),
            Action::Sort(k) if k != self.sort.0 => {
                self.sort = (k, k.first_direction());
                self.resort(cx);
            }
            Action::Direction(d) if d != self.sort.1 => {
                self.sort.1 = d;
                self.resort(cx);
            }
            Action::Sort(_) | Action::Direction(_) => {}
        }
        self.view_menu = false;
        cx.notify();
    }

    /// The open menu, under the path bar on the right; a click outside it
    /// (or Esc) closes it.
    pub(super) fn render_view_menu(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.view_menu {
            return None;
        }
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let mut card = div()
            .id("view-menu")
            .absolute()
            .top(px(space(11)))
            .right(px(space(2)))
            .w(px(MENU_WIDTH))
            .py(px(space(1)))
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for item in self.menu_items() {
            card = match item {
                Item::Caption(text) => card.child(
                    div()
                        .px(px(space(3)))
                        .pt(px(space(2)))
                        .pb(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(text),
                ),
                Item::Divider => card.child(div().my(px(space(1))).h(px(1.0)).bg(rgba(t.divider))),
                Item::Choice {
                    id,
                    label,
                    hint,
                    checked,
                    action,
                } => card.child(
                    div()
                        .id(SharedString::from(id))
                        .mx(px(space(1)))
                        .px(px(space(2)))
                        .h(px(32.0))
                        .flex()
                        .items_center()
                        .gap(px(space(2)))
                        .rounded(px(RADIUS))
                        .hover(|s| s.bg(rgba(t.hover)))
                        .child(
                            div()
                                .w(px(16.0))
                                .flex_none()
                                .font_family(icon_font)
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.accent))
                                .when(checked, |d| d.child(ICON_CHECK)),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(label))
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .child(hint),
                        )
                        .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                            view.run(action, cx);
                        })),
                ),
            };
        }
        Some(
            deferred(
                div()
                    .id("view-menu-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.view_menu = false;
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
            .with_priority(1),
        )
    }
}
