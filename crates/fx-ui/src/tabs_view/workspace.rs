//! Workspaces from the palette (T6.13): this window's tabs saved under a
//! name, and a saved set opened in a window of its own (nothing open is
//! closed).

use std::path::PathBuf;

use fx_core::trf;
use gpui::Context;

use super::TabsView;
use crate::store::{TabState, WindowState, WorkspaceState};

/// "1 aba", "3 abas".
pub(super) fn tabs_text(n: usize) -> String {
    match n {
        1 => fx_core::tr!("1 aba", "1 tab").to_owned(),
        n => trf!("{n} abas", "{n} tabs"),
    }
}

impl TabsView {
    /// Saves this window's tabs as the workspace `name` (the same name, in
    /// any case, updates it).
    pub(super) fn save_workspace(&mut self, name: String, cx: &mut Context<Self>) {
        let tabs: Vec<PathBuf> = self
            .tabs
            .iter()
            .map(|t| t.read(cx).path().to_path_buf())
            .collect();
        let count = tabs.len();
        let replaced = crate::workspaces::save(WorkspaceState {
            name: name.clone(),
            tabs,
            active: self.tabs.active_index(),
        });
        let tabs = tabs_text(count);
        let text = if replaced {
            trf!(
                "Espaço \"{name}\" atualizado com {tabs}",
                "Workspace \"{name}\" updated with {tabs}"
            )
        } else {
            trf!(
                "Espaço \"{name}\" salvo com {tabs} · Ctrl+K abre de novo",
                "Workspace \"{name}\" saved with {tabs} · Ctrl+K opens it again"
            )
        };
        self.current()
            .update(cx, |view, cx| view.show_notice(text, cx));
    }

    /// Opens the workspace `name` in a new window.
    pub(super) fn open_workspace(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(ws) = crate::workspaces::list()
            .into_iter()
            .find(|w| w.name == name)
        else {
            return;
        };
        let Some(first) = ws.tabs.first().cloned() else {
            return;
        };
        let state = WindowState {
            bounds: None,
            active: ws.active.min(ws.tabs.len() - 1),
            tabs: ws
                .tabs
                .into_iter()
                .map(|path| TabState {
                    path,
                    ..Default::default()
                })
                .collect(),
        };
        let opener = cx.try_global::<crate::WindowOpener>().map(|o| o.0.clone());
        if let Some(open) = opener {
            open(first, Some(state), cx);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_tab_or_many() {
        let one = super::tabs_text(1);
        assert!(one == "1 aba" || one == "1 tab", "{one}");
        assert!(super::tabs_text(3).starts_with("3 "));
    }
}
