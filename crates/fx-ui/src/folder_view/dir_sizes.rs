//! Folder sizes in the Size column (task T2.7): once a folder is listed, a
//! background thread asks the `fx-indexd` service for every subfolder total
//! at once (live, from its index). Without the service (or for folders it
//! does not know) it sums each subfolder itself (`fx_fs::folder_size`), in
//! display order, and the totals fill in as they arrive. Leaving the folder
//! cancels it. Self-computed totals are kept for the session (by path), so
//! coming back is instant; a folder being summed shows "…".

use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use fx_core::SortKey;
use gpui::Context;

use super::{FolderView, rows_of};

#[derive(Default)]
pub(super) struct DirSizes {
    cancel: Arc<AtomicBool>,
    /// Entries whose total is known.
    done: HashSet<u32>,
    running: bool,
}

impl DirSizes {
    /// The total of `entry` is final.
    pub(super) fn is_known(&self, entry: u32) -> bool {
        self.done.contains(&entry)
    }

    /// Totals are still being summed.
    pub(super) fn running(&self) -> bool {
        self.running
    }

    pub(super) fn label(&self, entry: u32, bytes: u64) -> String {
        if self.done.contains(&entry) {
            fx_core::format::size_kb(bytes)
        } else if self.running {
            "…".to_owned()
        } else {
            String::new()
        }
    }
}

/// Totals computed this session, by folder path.
fn cache() -> &'static Mutex<HashMap<PathBuf, u64>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

impl FolderView {
    /// Leaving the folder (or reloading it): stop and forget.
    pub(super) fn stop_dir_sizes(&mut self) {
        self.dir_sizes.cancel.store(true, Ordering::Relaxed);
        self.dir_sizes = DirSizes::default();
    }

    /// The folder finished loading: sum its subfolders in the background.
    pub(super) fn start_dir_sizes(&mut self, cx: &mut Context<Self>) {
        self.stop_dir_sizes();
        let folder = self.path.clone();
        let dirs: Vec<(u32, PathBuf)> = rows_of(&self.filtered, &self.list)
            .iter()
            .copied()
            .filter(|&i| self.list.attributes(i).is_dir())
            .map(|i| (i, self.path.join(self.list.os_name(i))))
            .collect();
        if dirs.is_empty() {
            return;
        }
        let mut uncached = Vec::new();
        {
            let known = cache().lock().unwrap_or_else(PoisonError::into_inner);
            for (i, path) in &dirs {
                match known.get(path) {
                    Some(&bytes) => {
                        Arc::make_mut(&mut self.list).set_size(*i, bytes);
                        self.dir_sizes.done.insert(*i);
                    }
                    None => uncached.push((*i, path.clone())),
                }
            }
        }
        self.dir_sizes.running = true;
        let cancel = self.dir_sizes.cancel.clone();
        let generation = self.generation;
        let (tx, rx) = channel::<(u32, PathBuf, u64)>();
        let spawned = std::thread::Builder::new()
            .name("fx-dir-sizes".into())
            .spawn(move || {
                // The service knows every total at once (and always current).
                let todo = match service_totals(&folder) {
                    Some(totals) => {
                        let mut rest = Vec::new();
                        for (i, path) in dirs {
                            match totals.get(&path) {
                                Some(&bytes) => {
                                    if tx.send((i, path, bytes)).is_err() {
                                        return;
                                    }
                                }
                                None => rest.push((i, path)),
                            }
                        }
                        rest
                    }
                    None => uncached,
                };
                for (i, path) in todo {
                    let total = fx_fs::folder_size(&path, &cancel);
                    if total.cancelled || tx.send((i, path, total.bytes)).is_err() {
                        return;
                    }
                }
            });
        if spawned.is_err() {
            self.dir_sizes.running = false;
            return;
        }
        cx.spawn(async move |this, cx| {
            let mut rx = crate::chan::stream(rx);
            while let Some(first) = rx.next().await {
                // Apply everything already waiting in one update.
                let batch: Vec<_> = std::iter::once(first)
                    .chain(std::iter::from_fn(|| rx.try_recv().ok()))
                    .collect();
                let alive = this.update(cx, |view, cx| {
                    if view.generation != generation {
                        return false;
                    }
                    let mut known = cache().lock().unwrap_or_else(PoisonError::into_inner);
                    let list = Arc::make_mut(&mut view.list);
                    for (i, path, bytes) in batch {
                        list.set_size(i, bytes);
                        view.dir_sizes.done.insert(i);
                        known.insert(path, bytes);
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |view, cx| {
                if view.generation == generation {
                    view.dir_sizes.running = false;
                    view.resort_if_by_size(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn resort_if_by_size(&mut self, cx: &mut Context<Self>) {
        if self.sort.0 == SortKey::Size {
            self.resort(cx);
        }
    }
}

/// Subfolder totals of `folder` from `fx-indexd`, by path; `None` when the
/// service is not running (the pipe is missing: fails at once) or not ready.
fn service_totals(folder: &std::path::Path) -> Option<HashMap<PathBuf, u64>> {
    use fx_index::protocol::{Request, Status};
    let r = fx_index::pipe::query(&Request::folder_sizes(folder), 200).ok()?;
    if r.status != Status::Ready {
        return None;
    }
    Some(r.hits.iter().map(|h| (h.path_buf(), h.size)).collect())
}
