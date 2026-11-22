//! Folder sizes in the Size column (task T2.7): once a folder is listed, a
//! background thread sums each subfolder (`fx_fs::folder_size`), in display
//! order, and the totals fill in as they arrive. Leaving the folder cancels
//! it. Totals are kept for the session (by path), so coming back is instant;
//! a folder being summed shows "…".

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
        let dirs: Vec<(u32, PathBuf)> = rows_of(&self.filtered, &self.list)
            .iter()
            .copied()
            .filter(|&i| self.list.attributes(i).is_dir())
            .map(|i| (i, self.path.join(self.list.os_name(i))))
            .collect();
        let mut todo = Vec::new();
        {
            let known = cache().lock().unwrap_or_else(PoisonError::into_inner);
            for (i, path) in dirs {
                match known.get(&path) {
                    Some(&bytes) => {
                        Arc::make_mut(&mut self.list).set_size(i, bytes);
                        self.dir_sizes.done.insert(i);
                    }
                    None => todo.push((i, path)),
                }
            }
        }
        if todo.is_empty() {
            self.resort_if_by_size(cx);
            return;
        }
        self.dir_sizes.running = true;
        let cancel = self.dir_sizes.cancel.clone();
        let generation = self.generation;
        let (tx, rx) = channel::<(u32, PathBuf, u64)>();
        let spawned = std::thread::Builder::new()
            .name("fx-dir-sizes".into())
            .spawn(move || {
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
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                let (back, first) = background
                    .spawn(async move {
                        let r = rx.recv();
                        (rx, r)
                    })
                    .await;
                rx = back;
                let Ok(first) = first else { break };
                // Apply everything already waiting in one update.
                let batch: Vec<_> = std::iter::once(first).chain(rx.try_iter()).collect();
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
