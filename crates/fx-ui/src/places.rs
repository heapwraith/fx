//! Places shown in the sidebar of every window (task T1.5): pinned folders
//! (quick access), known user folders and drives. One per app.
//!
//! Every fx-shell call runs off the UI thread: known folders on the background
//! executor, drives on fx-shell's watcher thread, which sends a new list within
//! ~250 ms of a drive letter appearing or disappearing.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::channel;

use fx_core::Pinned;
use fx_shell::places::{Drive, DriveKind, KnownFolder};
use gpui::{AppContext, Context, SharedString};

use crate::trace::Trace;

pub struct Places {
    pub drives: Vec<Drive>,
    known: Vec<(KnownFolder, PathBuf)>,
    pub pinned: Pinned,
    trace: Option<Rc<RefCell<Trace>>>,
}

/// Folders pinned on first run, like Explorer's quick access.
const DEFAULT_PINS: [KnownFolder; 6] = [
    KnownFolder::Desktop,
    KnownFolder::Downloads,
    KnownFolder::Documents,
    KnownFolder::Pictures,
    KnownFolder::Music,
    KnownFolder::Videos,
];

impl Places {
    pub fn new(trace: Option<Rc<RefCell<Trace>>>, cx: &mut Context<Self>) -> Self {
        Self::load_known(cx);
        Self::watch(cx);
        Self {
            drives: Vec::new(),
            known: Vec::new(),
            pinned: Pinned::new(),
            trace,
        }
    }

    fn load_known(cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let known = cx
                .background_spawn(async { fx_shell::known_folders() })
                .await;
            let _ = this.update(cx, |places, cx| {
                // Until settings persist (T1.17), every start pins the defaults.
                if places.pinned.is_empty() {
                    for f in DEFAULT_PINS {
                        if let Some((_, p)) = known.iter().find(|(k, _)| *k == f) {
                            places.pinned.pin(p.clone());
                        }
                    }
                }
                places.known = known;
                cx.notify();
            });
        })
        .detach();
    }

    fn watch(cx: &mut Context<Self>) {
        let (tx, rx) = channel();
        if fx_shell::watch_drives(tx).is_err() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let background = cx.background_executor().clone();
            let mut rx = rx;
            loop {
                // Wait on a background thread, never on the UI thread.
                let (back, drives) = background
                    .spawn(async move {
                        let d = rx.recv();
                        (rx, d)
                    })
                    .await;
                rx = back;
                let Ok(drives) = drives else { break };
                let alive = this.update(cx, |places, cx| {
                    if places.drives != drives {
                        if let Some(t) = &places.trace {
                            t.borrow_mut().record_drives(drives.len());
                        }
                        places.drives = drives;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Home folder, if known yet.
    pub fn home(&self) -> Option<&Path> {
        self.known
            .iter()
            .find(|(k, _)| *k == KnownFolder::Home)
            .map(|(_, p)| p.as_path())
    }

    /// Sidebar label of a folder: the Portuguese name of a known folder, else
    /// its own name.
    pub fn label(&self, path: &Path) -> SharedString {
        let known = self.known.iter().find(|(_, p)| p == path).map(|(k, _)| *k);
        let name = match known {
            Some(KnownFolder::Desktop) => "Área de Trabalho",
            Some(KnownFolder::Downloads) => "Downloads",
            Some(KnownFolder::Documents) => "Documentos",
            Some(KnownFolder::Pictures) => "Imagens",
            Some(KnownFolder::Music) => "Músicas",
            Some(KnownFolder::Videos) => "Vídeos",
            Some(KnownFolder::Home) | None => {
                return path
                    .file_name()
                    .map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    )
                    .into();
            }
        };
        name.into()
    }

    pub fn known_kind(&self, path: &Path) -> Option<KnownFolder> {
        self.known.iter().find(|(_, p)| p == path).map(|(k, _)| *k)
    }
}

/// "Disco Local (C:)", "Pendrive (E:)".
pub fn drive_label(d: &Drive) -> SharedString {
    let letter = d.root.to_string_lossy();
    let letter = letter.trim_end_matches('\\');
    let name = if d.label.is_empty() {
        match d.kind {
            DriveKind::Fixed => "Disco Local",
            DriveKind::Removable => "Unidade removível",
            DriveKind::Network => "Unidade de rede",
            DriveKind::Optical => "Unidade de DVD",
            DriveKind::Other => "Unidade",
        }
    } else {
        d.label.as_str()
    };
    format!("{name} ({letter})").into()
}
