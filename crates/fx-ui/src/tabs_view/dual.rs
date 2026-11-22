//! Dual pane (task T3.2): F3 splits the active tab into two folder views
//! side by side. Tab moves the focus between them (unless the focused one
//! uses Tab itself: path field, search, batch rename); F5 copies and F6
//! moves its selection into the other pane's folder, through the normal
//! queue (conflicts, undo). Clicking a pane makes it the active one, shown
//! by an accent bar. Commands (palette, sidebar) act on the active pane.

use fx_ops::Job;
use gpui::{Context, Entity, EntityId, Window, div, prelude::*, px};

use super::TabsView;
use crate::folder_view::FolderView;
use crate::theme::rgba;

/// The second pane of a tab.
pub(super) struct Second {
    pub(super) view: Entity<FolderView>,
    /// The second pane has the focus (else the tab's own view).
    pub(super) focused: bool,
}

impl TabsView {
    /// The pane that commands act on: the active tab's view, or its second
    /// pane when that one has the focus.
    pub(crate) fn current(&self) -> Entity<FolderView> {
        let primary = self.tabs.active().clone();
        match self.seconds.get(&primary.entity_id()) {
            Some(s) if s.focused => s.view.clone(),
            _ => primary,
        }
    }

    fn other(&self) -> Option<Entity<FolderView>> {
        let primary = self.tabs.active().clone();
        let s = self.seconds.get(&primary.entity_id())?;
        Some(if s.focused { primary } else { s.view.clone() })
    }

    /// F3: opens or closes the second pane of the active tab.
    pub(super) fn toggle_dual(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let primary = self.tabs.active().clone();
        let id: EntityId = primary.entity_id();
        if self.seconds.remove(&id).is_none() {
            let path = primary.read(cx).path().to_path_buf();
            let view = Self::new_tab(
                path,
                self.look,
                self.trace.clone(),
                None,
                self.media.clone(),
                None,
                cx,
            );
            self.seconds.insert(
                id,
                Second {
                    view,
                    focused: true,
                },
            );
        }
        self.focus_active(window, cx);
    }

    /// Tab with two panes: the other one gets the focus. Returns whether it
    /// handled the key.
    pub(super) fn switch_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let current = self.current();
        if current.read(cx).uses_tab() {
            return false;
        }
        let id = self.tabs.active().entity_id();
        let Some(s) = self.seconds.get_mut(&id) else {
            return false;
        };
        s.focused = !s.focused;
        self.focus_active(window, cx);
        true
    }

    /// F5 (copy) / F6 (move): the focused pane's selection into the other
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
            if s.view.read(cx).focus_handle().contains_focused(window, cx) {
                s.focused = true;
            } else if primary.read(cx).focus_handle().contains_focused(window, cx) {
                s.focused = false;
            }
        }
    }

    /// The active tab's content: one view, or two side by side.
    pub(super) fn render_panes(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let primary = self.tabs.active().clone();
        let Some(s) = self.seconds.get(&primary.entity_id()) else {
            return div().flex_1().min_w_0().child(primary).into_any_element();
        };
        let t = self.look.theme;
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
        let _ = cx;
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .child(pane(primary, !s.focused))
            .child(div().flex_none().w(px(1.0)).bg(rgba(t.divider)))
            .child(pane(s.view.clone(), s.focused))
            .into_any_element()
    }
}
