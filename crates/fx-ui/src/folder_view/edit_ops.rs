//! File operations from the list (task T1.12): Delete sends to the Recycle
//! Bin, Shift+Delete deletes for good after a confirmation, F2 renames in
//! place, Ctrl+Shift+N makes a folder (and starts renaming it), Ctrl+Z /
//! Ctrl+Y undo and redo. Operations on a drive root or a system folder ask
//! for an extra confirmation first (SAFETY.md §2.7).
//!
//! Clipboard (task T1.14): Ctrl+C / Ctrl+X put the files on the clipboard
//! like the Explorer (`CF_HDROP` + "Preferred DropEffect"); Ctrl+V pastes
//! files from fx or any app through the operations queue (conflicts asked,
//! undo recorded), and empties the clipboard after a successful cut-paste.

#[allow(unused_imports)]
use fx_core::{tr, trf};
use std::path::PathBuf;

use fx_ops::{Job, JobId, Resolution};
use gpui::{AppContext, Context, Keystroke, PromptLevel, Window};

use super::FolderView;

/// Inline rename of one entry. Right after F2 the name without its extension
/// is selected, so typing replaces it and keeps the extension.
#[derive(Debug, Clone)]
pub(super) struct Rename {
    pub entry: u32,
    pub text: String,
    /// Selected prefix length in bytes (0 = no selection).
    pub selected: usize,
    /// Caret position in bytes (always on a char boundary).
    pub caret: usize,
}

impl Rename {
    fn new(entry: u32, name: &str, is_dir: bool) -> Self {
        let stem = match name.rfind('.') {
            Some(i) if i > 0 && !is_dir => i,
            _ => name.len(),
        };
        Self {
            entry,
            text: name.to_owned(),
            selected: stem,
            caret: stem,
        }
    }

    /// Replaces the selection (if any) with `s`, or inserts it at the caret.
    fn insert(&mut self, s: &str) {
        if self.selected > 0 {
            self.text.replace_range(..self.selected, s);
            self.caret = s.len();
            self.selected = 0;
        } else {
            self.text.insert_str(self.caret, s);
            self.caret += s.len();
        }
    }

    fn backspace(&mut self) {
        if self.selected > 0 {
            self.insert("");
        } else if let Some(c) = self.text[..self.caret].chars().next_back() {
            self.caret -= c.len_utf8();
            self.text.remove(self.caret);
        }
    }

    fn delete(&mut self) {
        if self.selected > 0 {
            self.insert("");
        } else if self.caret < self.text.len() {
            self.text.remove(self.caret);
        }
    }

    fn left(&mut self) {
        self.selected = 0;
        if let Some(c) = self.text[..self.caret].chars().next_back() {
            self.caret -= c.len_utf8();
        }
    }

    fn right(&mut self) {
        self.selected = 0;
        if let Some(c) = self.text[self.caret..].chars().next() {
            self.caret += c.len_utf8();
        }
    }
}

fn count(n: usize) -> String {
    fx_core::format::items(n)
}

impl FolderView {
    /// Entries an operation applies to: the selection when the cursor is in
    /// it, otherwise the cursor entry.
    pub(super) fn targets(&self) -> Vec<PathBuf> {
        let Some(c) = self.selection.cursor() else {
            return Vec::new();
        };
        let rows = super::rows_of(&self.filtered, &self.list);
        // Never act on an entry that is not shown (hidden file, filtered out).
        if !rows.contains(&c) {
            return Vec::new();
        }
        let entries: Vec<u32> = if self.selection.is_selected(c) {
            self.selection.selected_in(rows).collect()
        } else {
            vec![c]
        };
        entries
            .into_iter()
            .map(|i| self.path.join(self.list.os_name(i)))
            .collect()
    }

    /// Queues `job`; asks first when it touches a system folder or root.
    pub(super) fn submit_job(
        &mut self,
        job: Job,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<JobId> {
        let ops = self.ops.clone();
        match ops.update(cx, |ops, _| ops.submit(job.clone())) {
            Ok(id) => Some(id),
            Err(reason) => {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    tr!(
                        "Operação em pasta protegida",
                        "Operation in a protected folder"
                    ),
                    Some(&trf!(
                        "Atenção: {reason}. Continuar mesmo assim?",
                        "Warning: {reason}. Continue anyway?"
                    )),
                    &[tr!("Continuar", "Continue"), tr!("Cancelar", "Cancel")],
                    cx,
                );
                cx.spawn(async move |_, cx| {
                    if answer.await == Ok(0) {
                        let _ = ops.update(cx, |ops, _| ops.submit(job.confirm_system()));
                    }
                })
                .detach();
                None
            }
        }
    }

    /// Keys for file operations. Returns whether the key was used.
    pub(super) fn ops_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let m = ks.modifiers;
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        let only_ctrl = m.control && !m.alt && !m.shift;
        let ctrl_shift = m.control && m.shift && !m.alt;
        match ks.key.as_str() {
            "delete" if plain => {
                let items = self.targets();
                if items.is_empty() {
                    return true;
                }
                if !crate::prefs::confirm_recycle() {
                    self.submit_job(Job::recycle(items), window, cx);
                    return true;
                }
                let detail = match items.len() {
                    1 => trf!(
                        "\"{}\" vai para a Lixeira. Dá para desfazer com Ctrl+Z.",
                        "\"{}\" goes to the Recycle Bin. Ctrl+Z undoes it.",
                        items[0]
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    ),
                    n => trf!(
                        "{} itens vão para a Lixeira. Dá para desfazer com Ctrl+Z.",
                        "{} items go to the Recycle Bin. Ctrl+Z undoes it.",
                        count(n)
                    ),
                };
                let answer = window.prompt(
                    PromptLevel::Warning,
                    tr!("Enviar para a Lixeira?", "Send to the Recycle Bin?"),
                    Some(&detail),
                    &[
                        tr!("Enviar para a Lixeira", "Send to Recycle Bin"),
                        tr!("Cancelar", "Cancel"),
                    ],
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if answer.await == Ok(0) {
                        let _ = this.update_in(cx, |view, window, cx| {
                            view.submit_job(Job::recycle(items), window, cx);
                        });
                    }
                })
                .detach();
                true
            }
            "delete" if m.shift && !m.control && !m.alt => {
                let items = self.targets();
                if items.is_empty() {
                    return true;
                }
                let detail = match items.len() {
                    1 => trf!(
                        "\"{}\" será excluído para sempre, sem passar pela Lixeira.",
                        "\"{}\" will be deleted forever, without going to the Recycle Bin.",
                        items[0]
                            .file_name()
                            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                    ),
                    n => trf!(
                        "{} serão excluídos para sempre, sem passar pela Lixeira.",
                        "{} will be deleted forever, without going to the Recycle Bin.",
                        count(n)
                    ),
                };
                let answer = window.prompt(
                    PromptLevel::Critical,
                    tr!("Excluir permanentemente?", "Delete permanently?"),
                    Some(&detail),
                    &[tr!("Excluir", "Delete"), tr!("Cancelar", "Cancel")],
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if answer.await == Ok(0) {
                        let _ = this.update_in(cx, |view, window, cx| {
                            view.submit_job(Job::delete_permanently(items), window, cx);
                        });
                    }
                })
                .detach();
                true
            }
            "f2" if plain => {
                // Several items: the batch rename panel (T2.8).
                if !self.open_batch_rename()
                    && let Some(c) = self.selection.cursor()
                {
                    self.begin_rename(c);
                }
                true
            }
            "n" if ctrl_shift => {
                let job =
                    Job::new_folder(self.path.clone(), tr!("Nova pasta", "New folder").into());
                self.awaiting_folder = self.submit_job(job, window, cx);
                true
            }
            "c" | "x" if only_ctrl => {
                self.put_on_clipboard(ks.key == "x", cx);
                true
            }
            // Like Explorer's "Copy as path".
            "c" if ctrl_shift => {
                let items = self.targets();
                self.copy_paths(items, cx);
                true
            }
            "v" if only_ctrl => {
                self.paste(window, cx);
                true
            }
            "menu" if plain => {
                self.open_menu(self.targets(), window, cx);
                true
            }
            "f10" if m.shift && !m.control && !m.alt => {
                self.open_menu(self.targets(), window, cx);
                true
            }
            "z" if only_ctrl => {
                self.ops.update(cx, |ops, cx| ops.undo(cx));
                true
            }
            "y" if only_ctrl => {
                self.ops.update(cx, |ops, cx| ops.redo(cx));
                true
            }
            "z" if ctrl_shift => {
                self.ops.update(cx, |ops, cx| ops.redo(cx));
                true
            }
            _ => false,
        }
    }

    /// Ctrl+C / Ctrl+X: the targets on the clipboard (off the UI thread).
    pub(super) fn put_on_clipboard(&mut self, cut: bool, cx: &mut Context<Self>) {
        let items = self.targets();
        if items.is_empty() {
            return;
        }
        let n = items.len();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_spawn(async move { fx_shell::set_clipboard_files(&items, cut) })
                .await;
            let _ = this.update(cx, |view, cx| {
                view.notice = Some(match done {
                    Ok(()) if cut => trf!(
                        "recortado: {} (cole com Ctrl+V)",
                        "cut: {} (paste with Ctrl+V)",
                        count(n)
                    ),
                    Ok(()) => trf!(
                        "copiado: {} (cole com Ctrl+V)",
                        "copied: {} (paste with Ctrl+V)",
                        count(n)
                    ),
                    Err(e) => trf!(
                        "área de transferência indisponível: {e}",
                        "clipboard unavailable: {e}"
                    ),
                });
                cx.notify();
            });
        })
        .detach();
    }

    /// Ctrl+V: files on the clipboard (from fx or any app) into this folder.
    pub(super) fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dest = self.path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let clip = cx
                .background_spawn(async { fx_shell::clipboard_files() })
                .await;
            let _ = this.update_in(cx, |view, window, cx| match clip {
                Ok(Some(files)) if !files.paths.is_empty() => {
                    let job = if files.cut {
                        Job::move_to(files.paths, dest)
                    } else {
                        Job::copy(files.paths, dest)
                    };
                    let id = view.submit_job(job.on_conflict(Resolution::Ask), window, cx);
                    if files.cut
                        && let Some(id) = id
                    {
                        view.ops.update(cx, |ops, _| ops.clear_clipboard_after(id));
                    }
                }
                Ok(_) => {
                    view.notice = Some(tr!("nada para colar", "nothing to paste").into());
                    cx.notify();
                }
                Err(e) => {
                    view.notice = Some(trf!(
                        "área de transferência indisponível: {e}",
                        "clipboard unavailable: {e}"
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The shell context menu for `items`, or for this folder when empty
    /// (task T1.16). fx's items come first; shell verbs fx performs itself
    /// come back here so they use the queue, the Recycle Bin and undo.
    pub(super) fn open_menu(
        &mut self,
        items: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let folder = self.path.clone();
        self.open_menu_in(items, folder, window, cx);
    }

    /// [`Self::open_menu`] for items of another folder (search results).
    pub(super) fn open_menu_in(
        &mut self,
        items: Vec<PathBuf>,
        folder: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = fx_shell::window_id(window) else {
            return;
        };
        let request = fx_shell::menu::MenuRequest {
            items: items.clone(),
            folder,
            fx_items: vec![
                tr!("Abrir no Terminal aqui", "Open in Terminal here").into(),
                tr!("Copiar caminho", "Copy path").into(),
            ],
            owner,
            started: std::time::Instant::now(),
            warm_up: false,
        };
        let rx = fx_shell::show_context_menu(request);
        cx.spawn_in(window, async move |this, cx| {
            let reply = cx.background_spawn(async move { rx.recv() }).await;
            let Ok(reply) = reply else { return };
            let _ = this.update_in(cx, |view, window, cx| {
                view.on_menu(reply, items, window, cx)
            });
        })
        .detach();
    }

    fn on_menu(
        &mut self,
        reply: fx_shell::menu::MenuReply,
        items: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use fx_shell::menu::MenuOutcome;
        if let (Some(trace), Some(ready), Some(built)) =
            (&self.trace, reply.ready_after, reply.built_in)
        {
            trace.borrow_mut().record_menu(ready, built);
        }
        match reply.outcome {
            MenuOutcome::Fx(0) => {
                self.launch(fx_shell::launch::Launch::Terminal(self.path.clone()), cx);
            }
            MenuOutcome::Fx(_) => self.copy_paths(items, cx),
            MenuOutcome::Verb(verb) => match verb.as_str() {
                "delete" if !items.is_empty() => {
                    self.submit_job(Job::recycle(items), window, cx);
                }
                "rename" => {
                    if let Some(c) = self.selection.cursor() {
                        self.begin_rename(c);
                    }
                }
                "copy" => self.put_on_clipboard(false, cx),
                "cut" => self.put_on_clipboard(true, cx),
                "paste" => self.paste(window, cx),
                "properties" => {
                    let items = if items.is_empty() {
                        vec![self.path.clone()]
                    } else {
                        items
                    };
                    self.launch(fx_shell::launch::Launch::Properties(items), cx);
                }
                _ => {}
            },
            MenuOutcome::Unavailable(why) => self.notice = Some(why),
            MenuOutcome::Shell | MenuOutcome::Cancelled => {}
        }
        cx.notify();
    }

    /// Full paths of `items` (or of this folder) as text, one per line.
    pub(super) fn copy_paths(&mut self, items: Vec<PathBuf>, cx: &mut Context<Self>) {
        let paths = if items.is_empty() {
            vec![self.path.clone()]
        } else {
            items
        };
        let text = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\r\n");
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        self.notice = Some(if paths.len() == 1 {
            tr!("caminho copiado", "path copied").into()
        } else {
            trf!("{} caminhos copiados", "{} paths copied", paths.len())
        });
    }

    pub(super) fn begin_rename(&mut self, entry: u32) {
        let name = self.list.name(entry).to_owned();
        let is_dir = self.list.attributes(entry).is_dir();
        self.renaming = Some(Rename::new(entry, &name, is_dir));
    }

    /// Ends the inline rename; `commit` queues the rename when the name changed.
    pub(super) fn end_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.renaming.take() else {
            return;
        };
        let new_name = r.text.trim().to_owned();
        if commit && !new_name.is_empty() && new_name != self.list.name(r.entry) {
            let path = self.path.join(self.list.os_name(r.entry));
            self.submit_job(Job::rename(path, new_name), window, cx);
        }
    }

    /// Keys while renaming. Returns false when not renaming.
    pub(super) fn rename_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(r) = &mut self.renaming else {
            return false;
        };
        let m = ks.modifiers;
        match ks.key.as_str() {
            "escape" => self.end_rename(false, window, cx),
            "enter" => self.end_rename(true, window, cx),
            "backspace" => r.backspace(),
            "delete" => r.delete(),
            "left" => r.left(),
            "right" => r.right(),
            "home" => {
                r.selected = 0;
                r.caret = 0;
            }
            "end" => {
                r.selected = 0;
                r.caret = r.text.len();
            }
            "a" if m.control => {
                r.selected = r.text.len();
                r.caret = r.text.len();
            }
            // GPUI sends no character for the space bar.
            "space" if !(m.control || m.alt || m.platform) => r.insert(" "),
            _ => {
                if !(m.control || m.alt || m.platform || m.function)
                    && let Some(c) = ks.key_char.as_deref()
                    && !c.chars().any(char::is_control)
                {
                    r.insert(c);
                }
            }
        }
        true
    }

    /// A "new folder" job finished: rename the folder once the list has it.
    pub(super) fn check_new_folder(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.awaiting_folder else {
            return;
        };
        let created = self.ops.update(cx, |ops, _| ops.created_by(id));
        if let Some(path) = created {
            self.awaiting_folder = None;
            self.rename_when_listed = path.file_name().map(|n| n.to_os_string());
            self.try_rename_listed();
        }
    }

    /// Starts renaming the new folder if it is in the list already.
    pub(super) fn try_rename_listed(&mut self) {
        let Some(name) = self.rename_when_listed.clone() else {
            return;
        };
        let shown = name.to_string_lossy();
        let found = self
            .list
            .order()
            .iter()
            .copied()
            .find(|&i| self.list.name(i) == shown);
        if let Some(i) = found {
            self.rename_when_listed = None;
            self.selection.select_only(i);
            if let Some(pos) = self
                .selection
                .position(super::rows_of(&self.filtered, &self.list), i)
            {
                let item = if self.grid { pos / self.grid_cols } else { pos };
                self.scroll
                    .scroll_to_item(item, gpui::ScrollStrategy::Center);
            }
            self.begin_rename(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_selects_the_stem_and_typing_keeps_the_extension() {
        let mut r = Rename::new(0, "relatório.pdf", false);
        assert_eq!(r.selected, "relatório".len());
        for c in ["n", "o", "v", "o"] {
            r.insert(c);
        }
        assert_eq!(r.text, "novo.pdf", "typing continues before the extension");
        r.insert(" ");
        r.insert("2");
        assert_eq!(r.text, "novo 2.pdf");
        r.backspace();
        r.left();
        r.delete();
        assert_eq!(r.text, "novo.pdf");
        r.right();
        r.right();
        r.insert("x");
        assert_eq!(r.text, "novo.pxdf");
        let mut d = Rename::new(0, "pasta.v2", true);
        d.insert("nova");
        assert_eq!(d.text, "nova", "folders: the whole name");
        let mut dot = Rename::new(0, ".gitignore", false);
        dot.insert("z");
        assert_eq!(dot.text, "z");
        let mut u = Rename::new(0, "ção.txt", false);
        u.left();
        u.backspace();
        assert_eq!(u.text, "ço.txt", "char boundaries respected");
    }
}
