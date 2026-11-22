//! Disk search (task T1.23): Ctrl+F opens a search panel over the tab;
//! results appear while typing and every key cancels the previous search.
//!
//! Unelevated (the normal case until `fx-indexd`, Phase 2) it walks the
//! current folder and its subfolders on a worker thread (`fx_fs::search_tree`),
//! results arriving in batches. Elevated, it reads the volume's NTFS MFT once
//! (`fx-index`, kept for the whole process) and searches the index; until the
//! index is ready, the folder walk is used.
//!
//! Enter opens the result (a folder opens in this tab); Ctrl+Enter shows it
//! selected in its folder.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use fx_fs::{SearchHit, SearchSummary};
use fx_index::Index;
use fx_shell::launch::Launch;
use gpui::{
    Context, Keystroke, MouseButton, MouseDownEvent, ScrollStrategy, SharedString,
    UniformListScrollHandle, Window, deferred, div, prelude::*, px, uniform_list,
};

use super::{FolderView, Restore};
use crate::theme::{CAPTION_SIZE, ICON_FOLDER, ICON_SEARCH, RADIUS, rgba, space};

/// The folder walk stops after this many results (type more to narrow).
const TREE_LIMIT: usize = 2_000;
const PANEL_WIDTH: f32 = 680.0;
const ROW_HEIGHT: f32 = 40.0;
const ICON_PAGE: &str = "\u{E7C3}";

/// One result.
#[derive(Debug, Clone)]
struct Found {
    path: PathBuf,
    is_dir: bool,
    name: SharedString,
    /// The folder it is in, for the second line.
    folder: SharedString,
}

impl Found {
    fn new(path: PathBuf, is_dir: bool) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let folder = path
            .parent()
            .map_or_else(String::new, |p| p.display().to_string());
        Self {
            path,
            is_dir,
            name: name.into(),
            folder: folder.into(),
        }
    }
}

/// The open search panel.
pub(super) struct Search {
    query: String,
    root: PathBuf,
    hits: Vec<Found>,
    cursor: usize,
    running: bool,
    summary: Option<String>,
    ticket: u64,
    cancel: Arc<AtomicBool>,
    scroll: UniformListScrollHandle,
}

/// MFT indexes by volume ("C:"), for the whole process (elevated only).
enum IndexState {
    Loading,
    Ready(Arc<Index>),
    Failed,
}

fn indexes() -> &'static Mutex<HashMap<String, IndexState>> {
    static INDEXES: OnceLock<Mutex<HashMap<String, IndexState>>> = OnceLock::new();
    INDEXES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// "C:" for a path on a lettered drive (also `\\?\C:\...`).
fn volume_of(path: &Path) -> Option<String> {
    match path.components().next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                Some(format!("{}:", char::from(letter).to_ascii_uppercase()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// The ready index of `volume`, starting its read (once) when elevated.
fn index_for(volume: &str) -> Option<Arc<Index>> {
    if !fx_shell::is_elevated() {
        return None;
    }
    let mut map = indexes().lock().unwrap_or_else(PoisonError::into_inner);
    match map.get(volume) {
        Some(IndexState::Ready(index)) => return Some(index.clone()),
        Some(IndexState::Loading | IndexState::Failed) => return None,
        None => {}
    }
    map.insert(volume.to_owned(), IndexState::Loading);
    let vol = volume.to_owned();
    let _ = std::thread::Builder::new()
        .name("fx-index-read".into())
        .spawn(move || {
            let state = match fx_index::read_volume(&vol) {
                Ok(index) => IndexState::Ready(Arc::new(index)),
                Err(_) => IndexState::Failed,
            };
            indexes()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(vol, state);
        });
    None
}

impl FolderView {
    pub(super) fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search.is_none() {
            self.search = Some(Search {
                query: String::new(),
                root: self.path.clone(),
                hits: Vec::new(),
                cursor: 0,
                running: false,
                summary: None,
                ticket: 0,
                cancel: Arc::new(AtomicBool::new(false)),
                scroll: UniformListScrollHandle::new(),
            });
            // Elevated: start reading the MFT now, it takes a few seconds.
            if let Some(v) = volume_of(&self.path) {
                let _ = index_for(&v);
            }
        }
        cx.notify();
    }

    fn close_search(&mut self) {
        if let Some(s) = self.search.take() {
            s.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Keys while the panel is open (all of them stay here). Returns whether
    /// the panel is open.
    pub(super) fn search_key(
        &mut self,
        ks: &Keystroke,
        started: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        let m = ks.modifiers;
        let mut query = search.query.clone();
        match ks.key.as_str() {
            "escape" => {
                self.close_search();
                return true;
            }
            "enter" => {
                let reveal = m.control;
                if let Some(found) = search.hits.get(search.cursor).cloned() {
                    self.close_search();
                    self.open_found(found, reveal, window, cx);
                }
                return true;
            }
            "down" | "up" => {
                let n = search.hits.len();
                if n > 0 {
                    search.cursor = if ks.key == "down" {
                        (search.cursor + 1).min(n - 1)
                    } else {
                        search.cursor.saturating_sub(1)
                    };
                    search
                        .scroll
                        .scroll_to_item(search.cursor, ScrollStrategy::Top);
                }
                return true;
            }
            "backspace" => {
                query.pop();
            }
            "space" => query.push(' '),
            _ => {
                if m.control || m.alt || m.platform || m.function {
                    return true;
                }
                match ks.key_char.as_deref() {
                    Some(c) if !c.chars().any(char::is_control) => query.push_str(c),
                    _ => return true,
                }
            }
        }
        if query != search.query {
            search.query = query;
            if let Some(trace) = &self.trace {
                trace.borrow_mut().input_started(started, "search");
            }
            self.run_search(cx);
        }
        true
    }

    /// Cancels the running search and starts one for the current text.
    fn run_search(&mut self, cx: &mut Context<Self>) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        search.cancel.store(true, Ordering::Relaxed);
        search.cancel = Arc::new(AtomicBool::new(false));
        search.ticket += 1;
        search.hits.clear();
        search.cursor = 0;
        search.summary = None;
        search.scroll.scroll_to_item(0, ScrollStrategy::Top);
        let query = search.query.trim().to_owned();
        search.running = !query.is_empty();
        if query.is_empty() {
            return;
        }
        let (ticket, cancel, root) = (search.ticket, search.cancel.clone(), search.root.clone());
        let index = volume_of(&root).and_then(|v| index_for(&v).map(|i| (v, i)));
        let (tx, rx) = channel::<Vec<Found>>();
        let (done_tx, done_rx) = channel::<String>();
        let spawned = std::thread::Builder::new()
            .name("fx-search".into())
            .spawn(move || {
                let summary = match index {
                    Some((volume, index)) => index_search(&index, &volume, &query, &cancel, &tx),
                    None => tree_search(&root, &query, &cancel, &tx),
                };
                drop(tx);
                let _ = done_tx.send(summary);
            });
        if spawned.is_err() {
            search.running = false;
            search.summary = Some("não foi possível iniciar a busca".into());
            return;
        }
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                let (back, batch) = background
                    .spawn(async move {
                        let b = rx.recv();
                        (rx, b)
                    })
                    .await;
                rx = back;
                let Ok(batch) = batch else { break };
                let current = this.update(cx, |view, cx| {
                    let Some(s) = view.search.as_mut().filter(|s| s.ticket == ticket) else {
                        return false;
                    };
                    s.hits.extend(batch);
                    cx.notify();
                    true
                });
                if !matches!(current, Ok(true)) {
                    return;
                }
            }
            let summary = background.spawn(async move { done_rx.recv() }).await;
            let _ = this.update(cx, |view, cx| {
                if let Some(s) = view.search.as_mut().filter(|s| s.ticket == ticket) {
                    s.running = false;
                    s.summary = summary.ok();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_found(
        &mut self,
        found: Found,
        reveal: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let parent = found.path.parent().map(Path::to_path_buf);
        match (reveal || !found.is_dir, parent) {
            // Ctrl+Enter, or a file with Enter... Enter on a file opens it.
            (true, Some(parent)) if reveal => {
                let name: Option<OsString> = found.path.file_name().map(|n| n.to_os_string());
                let restore = Restore {
                    selected: name.iter().cloned().collect(),
                    cursor: name,
                    scroll: None,
                };
                self.go_to(parent, Some(restore), window, cx);
            }
            (true, _) => self.launch(Launch::Open(found.path), cx),
            (false, _) => self.go_to(found.path, None, window, cx),
        }
    }

    pub(super) fn render_search(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let s = self.search.as_ref()?;
        let t = self.look.theme;
        let icon_font = self.look.icon_font;
        let where_ = s.root.file_name().map_or_else(
            || s.root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let indexed = volume_of(&s.root).is_some_and(|v| {
            matches!(
                indexes()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(&v),
                Some(IndexState::Ready(_))
            )
        });
        let placeholder = if indexed {
            "Buscar no disco inteiro (índice da MFT)".to_owned()
        } else {
            format!("Buscar em \"{where_}\" e subpastas")
        };
        let status = match (&s.summary, s.running) {
            (_, true) => format!("procurando… {}", fx_core::format::items(s.hits.len())),
            (Some(text), false) => text.clone(),
            (None, false) => "Enter abre · Ctrl+Enter mostra na pasta · Esc fecha".to_owned(),
        };
        let hits: Arc<Vec<Found>> = Arc::new(s.hits.clone());
        let cursor = s.cursor;
        let accent = t.selected;
        let secondary = t.text_secondary;
        let list = uniform_list("search-results", hits.len(), move |range, _, _| {
            range
                .map(|i| {
                    let f = &hits[i];
                    div()
                        .w_full()
                        .h(px(ROW_HEIGHT))
                        .px(px(space(3)))
                        .flex()
                        .items_center()
                        .gap(px(space(3)))
                        .rounded(px(RADIUS))
                        .when(i == cursor, |d| d.bg(rgba(accent)))
                        .child(
                            div()
                                .flex_none()
                                .font_family(icon_font)
                                .text_color(rgba(secondary))
                                .child(if f.is_dir { ICON_FOLDER } else { ICON_PAGE }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().truncate().child(f.name.clone()))
                                .child(
                                    div()
                                        .truncate()
                                        .text_size(px(CAPTION_SIZE))
                                        .text_color(rgba(secondary))
                                        .child(f.folder.clone()),
                                ),
                        )
                })
                .collect()
        })
        .track_scroll(s.scroll.clone())
        .flex_1()
        .px(px(space(1)));
        let card = div()
            .id("search-panel")
            .absolute()
            .top(px(space(12)))
            .left(px(space(4)))
            .right(px(space(4)))
            .mx_auto()
            .max_w(px(PANEL_WIDTH))
            .h(px(440.0))
            .flex()
            .flex_col()
            .rounded(px(RADIUS * 2.0))
            .border_1()
            .border_color(rgba(t.divider))
            .bg(rgba(t.base))
            .shadow_lg()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .h(px(44.0))
                    .px(px(space(4)))
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .border_b_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .flex_none()
                            .font_family(icon_font)
                            .text_color(rgba(t.text_secondary))
                            .child(ICON_SEARCH),
                    )
                    .child(if s.query.is_empty() {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgba(t.text_secondary))
                            .child(placeholder)
                    } else {
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(format!("{}|", s.query))
                    }),
            )
            .child(list)
            .child(
                div()
                    .flex_none()
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_t_1()
                    .border_color(rgba(t.divider))
                    .text_size(px(CAPTION_SIZE))
                    .text_color(rgba(t.text_secondary))
                    .truncate()
                    .child(status),
            );
        Some(
            deferred(
                div()
                    .id("search-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.close_search();
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
            .with_priority(1),
        )
    }
}

/// Folder walk; returns the summary line.
fn tree_search(
    root: &Path,
    query: &str,
    cancel: &AtomicBool,
    tx: &std::sync::mpsc::Sender<Vec<Found>>,
) -> String {
    let (htx, hrx) = channel::<Vec<SearchHit>>();
    let summary = std::thread::scope(|scope| {
        let forward = scope.spawn(|| {
            for batch in hrx {
                let found = batch
                    .into_iter()
                    .map(|h| Found::new(h.path, h.is_dir))
                    .collect();
                if tx.send(found).is_err() {
                    break;
                }
            }
        });
        let summary = fx_fs::search_tree(root, query, TREE_LIMIT, cancel, &htx);
        drop(htx);
        let _ = forward.join();
        summary
    });
    tree_summary(&summary)
}

fn tree_summary(s: &SearchSummary) -> String {
    let mut text = format!(
        "{} em {} pastas",
        fx_core::format::items(s.hits),
        fx_core::format::count(s.folders)
    );
    if s.capped {
        text.push_str(" (parou no limite: digite mais para refinar)");
    }
    if s.skipped > 0 {
        text.push_str(&format!(
            " · {} sem acesso",
            fx_core::format::count(s.skipped)
        ));
    }
    text
}

/// MFT index search (elevated); returns the summary line.
fn index_search(
    index: &Index,
    volume: &str,
    query: &str,
    cancel: &AtomicBool,
    tx: &std::sync::mpsc::Sender<Vec<Found>>,
) -> String {
    let Some(hits) = fx_index::search(index, query, cancel, None) else {
        return "cancelada".into();
    };
    let root = PathBuf::from(format!("{volume}\\"));
    let found: Vec<Found> = hits
        .iter()
        .filter_map(|h| {
            let rel = index.path(h.slot)?;
            Some(Found::new(root.join(rel), index.is_dir(h.slot)))
        })
        .collect();
    let n = found.len();
    let _ = tx.send(found);
    format!(
        "{} no disco {volume} (índice da MFT, os {} melhores)",
        fx_core::format::items(n),
        fx_index::MAX_RESULTS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes() {
        assert_eq!(volume_of(Path::new(r"c:\Users")).as_deref(), Some("C:"));
        assert_eq!(volume_of(Path::new(r"\\?\D:\x")).as_deref(), Some("D:"));
        assert_eq!(volume_of(Path::new(r"\\server\share\x")), None);
    }

    #[test]
    fn tree_search_reports_results_and_summary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::write(dir.path().join("a/b/Relatório.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("nada.txt"), b"x").unwrap();
        let (tx, rx) = channel();
        let summary = tree_search(dir.path(), "relatorio", &AtomicBool::new(false), &tx);
        drop(tx);
        let found: Vec<Found> = rx.into_iter().flatten().collect();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name.as_ref(), "Relatório.txt");
        assert!(found[0].folder.ends_with('b'));
        assert_eq!(summary, "1 item em 3 pastas");
    }
}
