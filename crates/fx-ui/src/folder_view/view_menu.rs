//! The "Exibir" (View) menu of a tab (T1.21): hidden items, sort order and
//! density, so nothing is reachable only by a shortcut. Density and hidden
//! items are window-wide: the tab asks `TabsView` through a [`ViewEvent`].
//! Also the Properties sheet (Alt+Enter and the context menu).

use fx_core::tr;
use fx_core::{Direction, SortKey};
use fx_shell::launch::Launch;
use gpui::{
    ClickEvent, Context, EventEmitter, MouseButton, MouseDownEvent, SharedString, deferred, div,
    prelude::*, px,
};

use super::{FolderView, MediaCol};
use crate::theme::{CAPTION_SIZE, Density, RADIUS, rgba, space};
use fx_core::grouping::GroupBy;

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

impl FolderView {
    /// The View menu as a screen reader reads it.
    pub(super) fn view_menu_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item as A11yItem, Overlay};
        if !self.view_menu {
            return None;
        }
        let items = self
            .menu_items()
            .into_iter()
            .filter_map(|item| match item {
                Item::Caption(text) => Some(A11yItem::Text(text.to_owned())),
                Item::Choice {
                    label,
                    hint,
                    checked,
                    ..
                } => Some(A11yItem::Radio {
                    label: if hint.is_empty() {
                        label.to_owned()
                    } else {
                        format!("{label} ({hint})")
                    },
                    checked,
                }),
                Item::Divider => None,
            })
            .collect();
        Some(Overlay {
            title: tr!("Exibir", "View").to_owned(),
            description: Some(tr!("Esc fecha.", "Esc closes.").to_owned()),
            items,
            focus: None,
            positions: None,
            modal: true,
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Action {
    ToggleHidden,
    Sort(SortKey),
    Direction(Direction),
    Density(Density),
    Group(GroupBy),
    Column(MediaCol),
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
        let by = self.groups.by;
        let group = |id, label, g: GroupBy| Item::Choice {
            id,
            label,
            hint: "",
            checked: by == g,
            action: Action::Group(g),
        };
        let column = |id, m: MediaCol| Item::Choice {
            id,
            label: m.label(),
            hint: "",
            checked: self.media.shown[m.index()],
            action: Action::Column(m),
        };
        let mut items = vec![
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
        ];
        items.extend([
            Item::Divider,
            Item::Caption(tr!("Agrupar por", "Group by")),
            group(
                "group-none",
                tr!("Não agrupar", "Don't group"),
                GroupBy::None,
            ),
            group("group-type", tr!("Tipo", "Type"), GroupBy::Type),
            group(
                "group-date",
                tr!("Data de modificação", "Date modified"),
                GroupBy::Date,
            ),
            group("group-size", tr!("Tamanho", "Size"), GroupBy::Size),
            Item::Divider,
            Item::Caption(tr!("Colunas", "Columns")),
            column("col-dims", MediaCol::Dimensions),
            column("col-duration", MediaCol::Duration),
            column("col-taken", MediaCol::Taken),
        ]);
        items
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
            Action::Group(g) => self.set_group_by(g, cx),
            Action::Column(m) => self.toggle_media_column(m, cx),
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
            .max_h(gpui::relative(0.85))
            .overflow_y_scroll()
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
            deferred(crate::motion::enter(
                "view-menu-in",
                div()
                    .id("view-menu-backdrop")
                    // The closing click never reaches the list behind.
                    .occlude()
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
            ))
            .with_priority(1),
        )
    }
}
