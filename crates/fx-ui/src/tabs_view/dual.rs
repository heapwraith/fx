//! Split panes (tasks T3.2, T6.14): F3 splits the active tab into two folder
//! views side by side and closes the split again; Ctrl+F3 adds one more
//! pane (up to four) and Ctrl+Shift+F3 closes the focused extra pane. Tab
//! moves the focus to the next pane (unless the focused one uses Tab
//! itself: path field, search, batch rename); F5 copies and F6 moves its
//! selection into the next pane's folder, through the normal queue
//! (conflicts, undo). Clicking a pane makes it the active one, shown by an
//! accent bar. Commands (palette, sidebar) act on the active pane.
//!
//! A tab dropped on the panes (dragged from the tab row) moves to a new
//! window, with its folder, sort and view; "Mover aba para nova janela" in
//! the palette does the same.

use fx_core::tr;
use fx_ops::Job;
use gpui::{Context, Entity, EntityId, Window, div, prelude::*, px};

use super::{DraggedTab, TabsView};
use crate::folder_view::FolderView;
use crate::store::WindowState;
use crate::theme::rgba;

/// The most panes in a tab.
pub(super) const MAX_PANES: usize = 4;

/// The extra panes of a tab (the tab's own view is pane 0).
pub(super) struct Second {
    pub(super) views: Vec<Entity<FolderView>>,
    /// The pane with the focus: 0 is the tab's own view, `k` is `views[k - 1]`.
    pub(super) focused: usize,
}

impl Second {
    fn pane(&self, primary: &Entity<FolderView>, k: usize) -> Entity<FolderView> {
        if k == 0 {
            primary.clone()
        } else {
            self.views[k - 1].clone()
        }
    }

    fn count(&self) -> usize {
        self.views.len() + 1
    }
}

impl TabsView {
    /// The pane that commands act on: the focused pane of the active tab.
    pub(crate) fn current(&self) -> Entity<FolderView> {
        let primary = self.tabs.active().clone();
        match self.seconds.get(&primary.entity_id()) {
            Some(s) => s.pane(&primary, s.focused),
            None => primary,
        }
    }

    /// The pane after the focused one (F5/F6 target, compare).
    pub(super) fn other(&self) -> Option<Entity<FolderView>> {
        let primary = self.tabs.active().clone();
        let s = self.seconds.get(&primary.entity_id())?;
        Some(s.pane(&primary, (s.focused + 1) % s.count()))
    }

    fn new_pane(&self, cx: &mut Context<Self>) -> Entity<FolderView> {
        let path = self.current().read(cx).path().to_path_buf();
        Self::new_tab(
            path,
            self.look,
            self.trace.clone(),
            None,
            self.media.clone(),
            None,
            cx,
        )
    }

    /// F3: splits the active tab in two, or closes the split.
    pub(super) fn toggle_dual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id: EntityId = self.tabs.active().entity_id();
        if self.seconds.remove(&id).is_none() {
            let view = self.new_pane(cx);
            self.seconds.insert(
                id,
                Second {
                    views: vec![view],
                    focused: 1,
                },
            );
        }
        self.focus_active(window, cx);
    }

    /// Ctrl+F3: one more pane (up to `MAX_PANES`), focused.
    pub(super) fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id: EntityId = self.tabs.active().entity_id();
        let full = self
            .seconds
            .get(&id)
            .is_some_and(|s| s.count() >= MAX_PANES);
        if full {
            let current = self.current();
            current.update(cx, |v, cx| {
                v.show_notice(
                    tr!("No máximo 4 painéis por aba", "At most 4 panes per tab").to_owned(),
                    cx,
                );
            });
            return;
        }
        let view = self.new_pane(cx);
        let s = self.seconds.entry(id).or_insert(Second {
            views: Vec::new(),
            focused: 0,
        });
        s.views.push(view);
        s.focused = s.views.len();
        self.focus_active(window, cx);
    }

    /// Ctrl+Shift+F3: closes the focused pane (the tab's own view stays:
    /// with it focused, the last extra pane closes).
    pub(super) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id: EntityId = self.tabs.active().entity_id();
        let Some(s) = self.seconds.get_mut(&id) else {
            return;
        };
        let k = if s.focused == 0 {
            s.views.len()
        } else {
            s.focused
        };
        s.views.remove(k - 1);
        s.focused = k - 1;
        if s.views.is_empty() {
            self.seconds.remove(&id);
        }
        self.focus_active(window, cx);
    }

    /// Tab with several panes: the next one gets the focus. Returns whether
    /// it handled the key.
    pub(super) fn switch_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let current = self.current();
        if current.read(cx).uses_tab() {
            return false;
        }
        let id = self.tabs.active().entity_id();
        let Some(s) = self.seconds.get_mut(&id) else {
            return false;
        };
        s.focused = (s.focused + 1) % s.count();
        self.focus_active(window, cx);
        true
    }

    /// F5 (copy) / F6 (move): the focused pane's selection into the next
    /// pane's folder.
    pub(super) fn transfer(
        &mut self,
        copy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(other) = self.other() else {
            return false;
        };
        let current = self.current();
        let dest = other.read(cx).path().to_path_buf();
        let paths = current.read(cx).selected_paths();
        if paths.is_empty() {
            return true;
        }
        let job = if copy {
            Job::copy(paths, dest)
        } else {
            Job::move_to(paths, dest)
        };
        current.update(cx, |view, cx| view.queue_job(job, window, cx));
        true
    }

    /// Keeps `focused` in step with clicks (a click focuses a pane's view).
    pub(super) fn sync_pane_focus(&mut self, window: &Window, cx: &Context<Self>) {
        let primary = self.tabs.active().clone();
        if let Some(s) = self.seconds.get_mut(&primary.entity_id()) {
            for k in 0..s.count() {
                if s.pane(&primary, k)
                    .read(cx)
                    .focus_handle()
                    .contains_focused(window, cx)
                {
                    s.focused = k;
                    break;
                }
            }
        }
    }

    /// Moves tab `i` to a new window (its folder, sort and view); the last
    /// tab of a window stays, it already is a window of its own.
    pub(super) fn tab_to_window(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            let current = self.current();
            current.update(cx, |v, cx| {
                v.show_notice(
                    tr!(
                        "Esta já é a única aba da janela",
                        "This already is the window's only tab"
                    )
                    .to_owned(),
                    cx,
                );
            });
            return;
        }
        let Some(tab) = self.tabs.iter().nth(i).cloned() else {
            return;
        };
        let state = tab.read(cx).export();
        let path = state.path.clone();
        let opener = cx.try_global::<crate::WindowOpener>().map(|o| o.0.clone());
        let Some(open) = opener else {
            return;
        };
        open(
            path,
            Some(WindowState {
                bounds: None,
                active: 0,
                tabs: vec![state],
            }),
            cx,
        );
        self.close_tab(i, window, cx);
    }

    /// The active tab's content: one view, or the panes side by side. A tab
    /// dropped here moves to a new window.
    pub(super) fn render_panes(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let primary = self.tabs.active().clone();
        let t = self.look.theme;
        let drop_target = |d: gpui::Div, cx: &mut Context<Self>| {
            d.drag_over::<DraggedTab>(move |s, _, _, _| s.bg(rgba(t.selected)))
                .on_drop(cx.listener(|view, d: &DraggedTab, window, cx| {
                    view.tab_to_window(d.index, window, cx);
                }))
        };
        let Some(s) = self.seconds.get(&primary.entity_id()) else {
            let d = div().flex_1().min_w_0().child(primary);
            return drop_target(d, cx).into_any_element();
        };
        let pane = |view: Entity<FolderView>, active: bool| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().flex_none().h(px(2.0)).bg(rgba(if active {
                    t.accent
                } else {
                    t.divider
                })))
                .child(div().flex_1().min_h_0().child(view))
        };
        let mut row = div().flex_1().min_w_0().flex();
        for k in 0..s.count() {
            if k > 0 {
                row = row.child(div().flex_none().w(px(1.0)).bg(rgba(t.divider)));
            }
            row = row.child(pane(s.pane(&primary, k), k == s.focused));
        }
        drop_target(row, cx).into_any_element()
    }
}
