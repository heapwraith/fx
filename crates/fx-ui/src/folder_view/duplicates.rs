//! Duplicate files (T6.4, D-068): files with the same content in this folder
//! and below (`fx_fs::duplicates`, on a thread of its own, with real progress;
//! closing cancels). Space marks a file, A marks every copy but the first of
//! each group, Delete sends the marked ones to the Recycle Bin (confirmed,
//! undoable), Enter shows a file in its folder. While open, the list checks
//! every few seconds that its files still exist, so what it shows is true.
//! Ctrl+Shift+D opens and closes.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use fx_core::format::size_short;
use fx_core::{tr, trf};
use fx_fs::duplicates::{DupGroup, DupProgress, find_duplicates};
use gpui::{ClickEvent, Context, Keystroke, SharedString, Window, div, prelude::*, px};

use super::{FolderView, Restore};
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

/// Smaller files are left out (they waste little and are many).
const MIN_SIZE: u64 = 4 << 10;

enum State {
    Scanning(Option<DupProgress>),
    Ready(Vec<DupGroup>),
}

pub(super) struct Duplicates {
    root: PathBuf,
    state: State,
    /// Index into the files of all groups, in order.
    cursor: usize,
    marked: HashSet<PathBuf>,
    cancel: Arc<AtomicBool>,
}

enum Msg {
    Progress(DupProgress),
    Done(Vec<DupGroup>),
}

impl FolderView {
    /// Ctrl+Shift+D: looks for duplicates here and below, or closes.
    pub(super) fn toggle_duplicates(&mut self, cx: &mut Context<Self>) {
        if let Some(d) = self.duplicates.take() {
            d.cancel.store(true, Ordering::Relaxed);
            cx.notify();
            return;
        }
        let root = self.path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.duplicates = Some(Duplicates {
            root: root.clone(),
            state: State::Scanning(None),
            cursor: 0,
            marked: HashSet::new(),
            cancel: cancel.clone(),
        });
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Msg>();
        let _ = std::thread::Builder::new()
            .name("fx-duplicates".into())
            .spawn(move || {
                let mut last = Instant::now();
                let groups = find_duplicates(&root, MIN_SIZE, &cancel, &mut |p| {
                    if last.elapsed() >= Duration::from_millis(100) {
                        last = Instant::now();
                        let _ = tx.unbounded_send(Msg::Progress(p));
                    }
                });
                if !cancel.load(Ordering::Relaxed) {
                    let _ = tx.unbounded_send(Msg::Done(groups));
                }
            });
        cx.spawn(async move |this, cx| {
            while let Some(msg) = rx.next().await {
                let done = matches!(msg, Msg::Done(_));
                let alive = this.update(cx, |view, cx| {
                    if let Some(d) = view.duplicates.as_mut() {
                        d.state = match msg {
                            Msg::Progress(p) => State::Scanning(Some(p)),
                            Msg::Done(groups) => State::Ready(groups),
                        };
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    return;
                }
                if done {
                    let _ = this.update(cx, |view, cx| view.keep_duplicates_true(cx));
                    return;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// While open: every 3 s, drops the files that no longer exist (checked
    /// on a thread) and the groups left with one file.
    fn keep_duplicates_true(&mut self, cx: &mut Context<Self>) {
        let Some(cancel) = self.duplicates.as_ref().map(|d| d.cancel.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let files: Vec<PathBuf> = match this.update(cx, |view, _| {
                    match view.duplicates.as_ref().map(|d| &d.state) {
                        Some(State::Ready(groups)) => {
                            groups.iter().flat_map(|g| g.files.clone()).collect()
                        }
                        _ => Vec::new(),
                    }
                }) {
                    Ok(files) => files,
                    Err(_) => return,
                };
                let (tx, rx) = futures::channel::oneshot::channel();
                let _ = std::thread::Builder::new()
                    .name("fx-duplicates-check".into())
                    .spawn(move || {
                        let gone: HashSet<PathBuf> = files
                            .into_iter()
                            .filter(|f| std::fs::symlink_metadata(f).is_err())
                            .collect();
                        let _ = tx.send(gone);
                    });
                let Ok(gone) = rx.await else { continue };
                if gone.is_empty() {
                    continue;
                }
                let _ = this.update(cx, |view, cx| {
                    if let Some(d) = view.duplicates.as_mut()
                        && let State::Ready(groups) = &mut d.state
                    {
                        for g in groups.iter_mut() {
                            g.files.retain(|f| !gone.contains(f));
                        }
                        groups.retain(|g| g.files.len() > 1);
                        d.marked.retain(|f| !gone.contains(f));
                        let n: usize = groups.iter().map(|g| g.files.len()).sum();
                        d.cursor = d.cursor.min(n.saturating_sub(1));
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn duplicate_files(&self) -> Vec<PathBuf> {
        match self.duplicates.as_ref().map(|d| &d.state) {
            Some(State::Ready(groups)) => groups.iter().flat_map(|g| g.files.clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Keys while the panel is open. Returns whether it took them.
    pub(super) fn duplicates_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.duplicates.is_none() || ks.modifiers.control || ks.modifiers.alt {
            return false;
        }
        let files = self.duplicate_files();
        let Some(d) = self.duplicates.as_mut() else {
            return false;
        };
        let n = files.len();
        match ks.key.as_str() {
            "escape" => {
                d.cancel.store(true, Ordering::Relaxed);
                self.duplicates = None;
            }
            "down" if n > 0 => d.cursor = (d.cursor + 1).min(n - 1),
            "up" => d.cursor = d.cursor.saturating_sub(1),
            "space" => {
                if let Some(f) = files.get(d.cursor)
                    && !d.marked.remove(f)
                {
                    d.marked.insert(f.clone());
                }
            }
            "a" => {
                if let State::Ready(groups) = &d.state {
                    for g in groups {
                        d.marked.extend(g.files.iter().skip(1).cloned());
                    }
                }
            }
            "delete" => {
                let mut items: Vec<PathBuf> = d.marked.iter().cloned().collect();
                if items.is_empty() {
                    items.extend(files.get(d.cursor).cloned());
                }
                items.sort();
                if !items.is_empty() {
                    d.marked.clear();
                    self.recycle_confirmed(items, window, cx);
                }
            }
            "enter" => {
                if let Some(f) = files.get(d.cursor).cloned()
                    && let Some(folder) = f.parent().map(|p| p.to_path_buf())
                {
                    d.cancel.store(true, Ordering::Relaxed);
                    self.duplicates = None;
                    let restore = f.file_name().map(|n| Restore::cursor_on(n.to_os_string()));
                    self.go_to(folder, restore, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }

    pub(super) fn render_duplicates(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let d = self.duplicates.as_ref()?;
        let t = self.look.theme;
        let mut list = div()
            .id("dup-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(space(2)));
        let header = match &d.state {
            State::Scanning(p) => match p {
                None | Some(DupProgress::Listing(0)) => {
                    tr!("Listando arquivos…", "Listing files…").to_owned()
                }
                Some(DupProgress::Listing(n)) => {
                    trf!("Listando: {} arquivos…", "Listing: {} files…", n)
                }
                Some(DupProgress::Comparing(a, b)) => trf!(
                    "Comparando {} de {} candidatos…",
                    "Comparing {} of {} candidates…",
                    a,
                    b
                ),
            },
            State::Ready(groups) if groups.is_empty() => String::new(),
            State::Ready(groups) => {
                let wasted: u64 = groups.iter().map(DupGroup::wasted).sum();
                trf!(
                    "{} grupos · {} em cópias",
                    "{} groups · {} in copies",
                    groups.len(),
                    size_short(wasted)
                )
            }
        };
        if let State::Ready(groups) = &d.state {
            if groups.is_empty() {
                list = list.child(
                    div()
                        .p(px(space(4)))
                        .text_color(rgba(t.text_secondary))
                        .child(trf!(
                            "Nenhum arquivo repetido (de {} ou mais) em {} e subpastas.",
                            "No repeated files ({} or more) in {} and below.",
                            size_short(MIN_SIZE),
                            d.root.display()
                        )),
                );
            }
            let mut k = 0usize;
            for (g_index, g) in groups.iter().enumerate() {
                list = list.child(
                    div()
                        .pt(px(space(3)))
                        .pb(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(trf!(
                            "{} cópias de {} · {} a liberar",
                            "{} copies of {} · {} to free",
                            g.files.len(),
                            size_short(g.size),
                            size_short(g.wasted())
                        )),
                );
                for f in &g.files {
                    let marked = d.marked.contains(f);
                    let row = k;
                    list = list.child(
                        div()
                            .id(("dup", g_index * 10_000 + row))
                            .h(px(32.0))
                            .px(px(space(2)))
                            .flex()
                            .items_center()
                            .gap(px(space(2)))
                            .rounded(px(RADIUS))
                            .cursor_pointer()
                            .when(row == d.cursor, |x| x.bg(rgba(t.selected)))
                            .when(row != d.cursor, |x| x.hover(|h| h.bg(rgba(t.hover))))
                            .child(
                                div()
                                    .flex_none()
                                    .w(px(16.0))
                                    .font_family(self.look.icon_font)
                                    .text_size(px(CAPTION_SIZE))
                                    .child(if marked { "\u{E74D}" } else { "" }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(SharedString::from(f.display().to_string())),
                            )
                            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                                if let Some(d) = view.duplicates.as_mut() {
                                    d.cursor = row;
                                }
                                cx.notify();
                            })),
                    );
                    k += 1;
                }
            }
        }
        Some(
            div()
                .id("duplicates")
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
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(tr!("Arquivos repetidos", "Repeated files")))
                        .child(div().text_size(px(CAPTION_SIZE)).text_color(rgba(t.text_secondary)).child(header))
                        .child(div().flex_1())
                        .when(!d.marked.is_empty(), |x| {
                            x.child(div().text_size(px(CAPTION_SIZE)).child(trf!("{} marcados", "{} marked", d.marked.len())))
                        }),
                )
                .child(list)
                .child(
                    div()
                        .flex_none()
                        .px(px(space(3)))
                        .py(px(space(1)))
                        .text_size(px(CAPTION_SIZE))
                        .text_color(rgba(t.text_secondary))
                        .child(tr!(
                            "Espaço marca · A marca as cópias · Delete: Lixeira (com desfazer) · Enter mostra na pasta · Esc fecha",
                            "Space marks · A marks the copies · Delete: Recycle Bin (undoable) · Enter shows in folder · Esc closes"
                        )),
                ),
        )
    }

    /// The panel as a screen reader reads it.
    pub(super) fn duplicates_overlay(&self) -> Option<crate::a11y::Overlay> {
        use crate::a11y::{Item, Overlay};
        let d = self.duplicates.as_ref()?;
        let items: Vec<Item> = self
            .duplicate_files()
            .iter()
            .enumerate()
            .map(|(k, f)| Item::Choice {
                label: f.display().to_string(),
                description: d
                    .marked
                    .contains(f)
                    .then(|| tr!("marcado", "marked").to_owned()),
                selected: k == d.cursor,
            })
            .collect();
        let focus = (!items.is_empty()).then(|| d.cursor.min(items.len() - 1));
        Some(Overlay {
            title: tr!("Arquivos repetidos", "Repeated files").to_owned(),
            description: Some(
                tr!(
                    "Espaço marca, A marca as cópias, Delete manda para a Lixeira, Enter mostra na pasta, Esc fecha.",
                    "Space marks, A marks the copies, Delete sends to the Recycle Bin, Enter shows in folder, Esc closes."
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
