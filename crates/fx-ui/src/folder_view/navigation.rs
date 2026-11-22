//! Moving between folders: open, up, back, forward, and putting the cursor
//! and scroll back where they were.

use super::*;

impl FolderView {
    /// Enter / double click: a folder opens in place; files open with their
    /// default app (the selected ones when the cursor is among them, at most
    /// `MAX_OPEN`). Returns whether something happened.
    pub(super) fn open_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(i) = self.selection.cursor() else {
            return false;
        };
        if self.list.attributes(i).is_dir() {
            let path = self.path.join(self.list.os_name(i));
            self.go_to(path, None, window, cx);
            return true;
        }
        let rows = rows_of(&self.filtered, &self.list);
        let files: Vec<u32> = if self.selection.is_selected(i) {
            self.selection
                .selected_in(rows)
                .filter(|&j| !self.list.attributes(j).is_dir())
                .take(MAX_OPEN)
                .collect()
        } else {
            vec![i]
        };
        for j in files {
            let path = self.path.join(self.list.os_name(j));
            self.launch(Launch::Open(path), cx);
        }
        true
    }

    /// Starts a launch off the UI thread; a failure shows in the status bar.
    /// Opens a web page in the default browser (off the UI thread).
    pub fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        self.launch(Launch::Open(PathBuf::from(url)), cx);
    }

    pub(super) fn launch(&mut self, what: Launch, cx: &mut Context<Self>) {
        let label = match &what {
            Launch::Open(p) | Launch::OpenWith(p) | Launch::Terminal(p) => {
                p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                )
            }
            Launch::Properties(_) => tr!("as Propriedades", "Properties").to_owned(),
            Launch::Explore(name) => name.to_string_lossy().into_owned(),
            Launch::Elevated { .. } => tr!("o serviço de índice", "the index service").to_owned(),
        };
        let rx = fx_shell::launch(what);
        cx.spawn(async move |this, cx| {
            let result = crate::chan::next(rx).await.unwrap_or(Ok(()));
            if let Err(e) = result {
                let _ = this.update(cx, |view, cx| {
                    view.notice = Some(trf!(
                        "não foi possível abrir {label}: {e}",
                        "could not open {label}: {e}"
                    ));
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Alt+Up / Backspace: opens the parent folder and selects the one we left.
    pub(super) fn go_up(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(parent) = self.path.parent().map(Path::to_path_buf) else {
            return false;
        };
        let child = self.path.file_name().map(OsString::from);
        self.go_to(parent, child.map(Restore::cursor_on), window, cx);
        true
    }

    /// Opens another folder in this window, as a new history entry.
    pub(super) fn go_to(
        &mut self,
        path: PathBuf,
        restore: Option<Restore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.crumb_menu = false;
        self.history.current_mut().restore = self.snapshot();
        self.history.push(Visit {
            path: path.clone(),
            restore: Restore::default(),
        });
        self.load(path, restore, window, cx);
    }

    /// Alt+Left / mouse button 4.
    pub(super) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let snapshot = self.snapshot();
        if !self.history.can_go_back() {
            return false;
        }
        self.history.current_mut().restore = snapshot;
        if let Some(v) = self.history.go_back().cloned() {
            self.load(v.path, Some(v.restore), window, cx);
        }
        true
    }

    /// Alt+Right / mouse button 5.
    pub(super) fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let snapshot = self.snapshot();
        if !self.history.can_go_forward() {
            return false;
        }
        self.history.current_mut().restore = snapshot;
        if let Some(v) = self.history.go_forward().cloned() {
            self.load(v.path, Some(v.restore), window, cx);
        }
        true
    }

    /// Selection and scroll position of the folder shown now.
    pub(super) fn snapshot(&self) -> Restore {
        let selected = if self.selection.len() <= MAX_RESTORED {
            self.selection
                .selected()
                .map(|i| self.list.os_name(i))
                .collect()
        } else {
            Vec::new()
        };
        Restore {
            selected,
            cursor: self.selection.cursor().map(|i| self.list.os_name(i)),
            scroll: Some(f32::from(self.scroll.0.borrow().base_handle.offset().y)),
        }
    }

    /// Shows another folder in this window (history is the caller's job).
    pub(super) fn load(
        &mut self,
        path: PathBuf,
        restore: Option<Restore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit = None;
        self.generation += 1;
        self.stop_dir_sizes();
        self.path = path;
        self.list = Arc::new(FileList::new());
        self.filter_text.clear();
        self.filtered = None;
        self.selection.clear();
        self.restore = restore;
        self.scroll
            .0
            .borrow()
            .base_handle
            .set_offset(point(px(0.0), px(0.0)));
        window.set_window_title(&window_title(&self.path));
        self.start_loading(cx);
        cx.notify();
    }

    /// Once loaded, restores the selection and scroll position saved for this
    /// folder (unless the user selected something meanwhile). Names that no
    /// longer exist are skipped.
    pub(super) fn apply_restore(&mut self) {
        let Some(r) = self.restore.take() else {
            return;
        };
        if !self.selection.is_empty() {
            return;
        }
        let exact: HashSet<&OsString> = r.selected.iter().chain(&r.cursor).collect();
        let shown: HashSet<String> = exact
            .iter()
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        let mut selected = Vec::new();
        let mut cursor = None;
        if !shown.is_empty() {
            for i in self.list.order().to_vec() {
                if !shown.contains(self.list.name(i)) {
                    continue;
                }
                let name = self.list.os_name(i);
                if r.cursor.as_ref() == Some(&name) {
                    cursor = Some(i);
                }
                if r.selected.contains(&name) {
                    selected.push(i);
                }
            }
        }
        self.selection.restore(selected, cursor);
        match (r.scroll, cursor) {
            (Some(y), _) => self
                .scroll
                .0
                .borrow()
                .base_handle
                .set_offset(point(px(0.0), px(y))),
            (None, Some(i)) => {
                if let Some(pos) = self.selection.position(self.list.order(), i) {
                    self.scroll.scroll_to_item(pos, ScrollStrategy::Center);
                }
            }
            (None, None) => {}
        }
    }
}
