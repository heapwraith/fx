//! Archives as folders (T6.7, D-068). Enter on a ZIP opens it like a folder
//! (the same list, sorting, filter and breadcrumbs); inside, nothing can be
//! changed: Ctrl+E (or the palette) extracts the selection, or everything,
//! next to the archive; Ctrl+C extracts and puts the files on the clipboard;
//! Enter on a file extracts it and opens it. Outside, Ctrl+E on selected
//! archives (ZIP, 7z, RAR, tar) extracts each into a folder with its name;
//! "Compactar em ZIP" in the palette makes a ZIP of the selection.
//!
//! Extraction goes to a fresh folder in `%TEMP%` first (on a thread, with
//! real progress); what came out is then copied by the operations engine,
//! so conflicts are asked and undo works. Names that could escape a folder
//! are never listed nor extracted (`fx_core::archive`).

use std::path::{Path, PathBuf};

use futures::StreamExt;
use fx_core::{tr, trf};
use fx_ops::Job;
use gpui::{AppContext, Context, Keystroke, Window};

use super::FolderView;
use super::edit_ops::count;

/// What to do with what was extracted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    /// Copy into this folder (by the engine).
    CopyTo,
    /// Put on the clipboard.
    Clipboard,
    /// Open with the default app.
    Open,
}

enum Step {
    Progress(usize, usize),
    Done(std::io::Result<Vec<PathBuf>>),
}

impl FolderView {
    /// Keys inside an archive: changes are refused, Ctrl+C extracts.
    /// Returns whether the key was used.
    pub(super) fn archive_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.archive.is_none() {
            return false;
        }
        let m = ks.modifiers;
        let plain = !m.control && !m.alt && !m.shift && !m.platform;
        let only_ctrl = m.control && !m.alt && !m.shift;
        let ctrl_shift = m.control && m.shift && !m.alt;
        let refused = match ks.key.as_str() {
            "delete" | "f2" if !m.control && !m.alt => true,
            "x" | "v" if only_ctrl => true,
            "n" if ctrl_shift => true,
            "menu" if plain => true,
            "f10" if m.shift => true,
            "c" if only_ctrl => {
                self.extract_selection(Then::Clipboard, window, cx);
                return true;
            }
            _ => false,
        };
        if refused {
            self.show_notice(
                tr!(
                    "Arquivo compactado: só leitura. Ctrl+E extrai, Ctrl+C copia para colar em outro lugar.",
                    "Archive: read-only. Ctrl+E extracts, Ctrl+C copies to paste elsewhere."
                )
                .to_owned(),
                cx,
            );
        }
        refused
    }

    /// Ctrl+E and "Extrair" in the palette.
    pub fn extract(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.archive.is_some() {
            self.extract_selection(Then::CopyTo, window, cx);
            return;
        }
        let archives: Vec<PathBuf> = self
            .targets()
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(fx_core::archive::is_archive)
            })
            .collect();
        if archives.is_empty() {
            self.show_notice(
                tr!(
                    "Selecione um ou mais arquivos compactados (ZIP, 7z, RAR, tar)",
                    "Select one or more archives (ZIP, 7z, RAR, tar)"
                )
                .to_owned(),
                cx,
            );
            return;
        }
        let dest = self.path.clone();
        self.run_extraction(dest.clone(), Then::CopyTo, window, cx, move |progress| {
            extract_whole(&archives, progress)
        });
    }

    /// Enter on a file inside an archive: extract it and open it.
    pub(super) fn open_in_archive(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(zip) = self.archive.clone() else {
            return;
        };
        let path = self.path.clone();
        self.run_extraction(PathBuf::new(), Then::Open, window, cx, move |progress| {
            extract_from_zip(&zip, &path, &[name], progress)
        });
    }

    /// The selection (or the cursor, or everything) of the archive folder.
    fn extract_selection(&mut self, then: Then, window: &mut Window, cx: &mut Context<Self>) {
        let Some(zip) = self.archive.clone() else {
            return;
        };
        let mut names: Vec<String> = self
            .targets()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect();
        if names.is_empty() {
            names = self
                .list
                .order()
                .iter()
                .map(|&i| self.list.name(i).to_owned())
                .collect();
        }
        if names.is_empty() {
            return;
        }
        let dest = zip.parent().map(Path::to_path_buf).unwrap_or_default();
        let path = self.path.clone();
        self.run_extraction(dest, then, window, cx, move |progress| {
            extract_from_zip(&zip, &path, &names, progress)
        });
    }

    /// Runs `work` on a thread (it extracts into a fresh folder and returns
    /// the top items), showing its real progress, then does `then`.
    fn run_extraction(
        &mut self,
        dest: PathBuf,
        then: Then,
        window: &mut Window,
        cx: &mut Context<Self>,
        work: impl FnOnce(&mut dyn FnMut(usize, usize)) -> std::io::Result<Vec<PathBuf>>
        + Send
        + 'static,
    ) {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Step>();
        let spawned = std::thread::Builder::new()
            .name("fx-extract".into())
            .spawn(move || {
                let progress_tx = tx.clone();
                let result = work(&mut |done, total| {
                    let _ = progress_tx.unbounded_send(Step::Progress(done, total));
                });
                let _ = tx.unbounded_send(Step::Done(result));
            });
        if spawned.is_err() {
            return;
        }
        self.show_notice(tr!("Extraindo…", "Extracting…").to_owned(), cx);
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            while let Some(step) = rx.next().await {
                match step {
                    Step::Progress(done, total) => {
                        let _ = this.update(cx, |view, cx| {
                            let text = if total > 0 {
                                trf!("Extraindo: {} de {}…", "Extracting: {} of {}…", done, total)
                            } else {
                                trf!("Extraindo: {} itens…", "Extracting: {} items…", done)
                            };
                            view.show_notice(text, cx);
                        });
                    }
                    Step::Done(result) => {
                        let dest = dest.clone();
                        let _ = window_handle.update(cx, |_, window, cx| {
                            let _ = this.update(cx, |view, cx| {
                                view.extracted(result, dest, then, window, cx)
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }

    fn extracted(
        &mut self,
        result: std::io::Result<Vec<PathBuf>>,
        dest: PathBuf,
        then: Then,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items = match result {
            Ok(items) if !items.is_empty() => items,
            Ok(_) => {
                self.show_notice(
                    tr!("Nada para extrair", "Nothing to extract").to_owned(),
                    cx,
                );
                return;
            }
            Err(e) => {
                self.show_notice(
                    trf!("Não foi possível extrair: {e}", "Could not extract: {e}"),
                    cx,
                );
                return;
            }
        };
        let n = items.len();
        match then {
            Then::CopyTo => {
                let folder = dest.file_name().map_or_else(
                    || dest.display().to_string(),
                    |f| f.to_string_lossy().into_owned(),
                );
                self.submit_job(Job::copy(items, dest), window, cx);
                self.show_notice(
                    trf!(
                        "Extraído: {} em \"{}\" (Ctrl+Z desfaz)",
                        "Extracted: {} into \"{}\" (Ctrl+Z undoes)",
                        count(n),
                        folder
                    ),
                    cx,
                );
            }
            Then::Clipboard => {
                cx.spawn(async move |this, cx| {
                    let done = cx
                        .background_spawn(
                            async move { fx_shell::set_clipboard_files(&items, false) },
                        )
                        .await;
                    let _ = this.update(cx, |view, cx| {
                        view.show_notice(
                            match done {
                                Ok(()) => trf!(
                                    "copiado do arquivo compactado: {} (cole com Ctrl+V)",
                                    "copied from the archive: {} (paste with Ctrl+V)",
                                    count(n)
                                ),
                                Err(e) => trf!(
                                    "área de transferência indisponível: {e}",
                                    "clipboard unavailable: {e}"
                                ),
                            },
                            cx,
                        );
                    });
                })
                .detach();
            }
            Then::Open => {
                if let Some(file) = items.into_iter().next() {
                    self.launch(fx_shell::launch::Launch::Open(file), cx);
                }
            }
        }
    }

    /// "Compactar em ZIP" (palette): the selection into a new ZIP here.
    pub fn compress_zip(&mut self, cx: &mut Context<Self>) {
        if self.archive.is_some() {
            return;
        }
        let items = self.targets();
        if items.is_empty() {
            self.show_notice(
                tr!("Selecione o que compactar", "Select what to compress").to_owned(),
                cx,
            );
            return;
        }
        let folder = self.path.clone();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Step>();
        let spawned = std::thread::Builder::new()
            .name("fx-zip".into())
            .spawn(move || {
                let name = zip_name(&items, &folder);
                let target = fx_core::naming::keep_both_name(&folder, &name, |p| p.exists());
                let progress_tx = tx.clone();
                let offset = |utc: i64| {
                    fx_shell::utc_offset_at(
                        std::time::UNIX_EPOCH + std::time::Duration::from_secs(utc.max(0) as u64),
                    )
                };
                let result =
                    fx_fs::archive::make_zip(&items, &target, &offset, &mut |done, total| {
                        let _ = progress_tx.unbounded_send(Step::Progress(done, total));
                    })
                    .map(|()| vec![target]);
                let _ = tx.unbounded_send(Step::Done(result));
            });
        if spawned.is_err() {
            return;
        }
        self.show_notice(tr!("Compactando…", "Compressing…").to_owned(), cx);
        cx.spawn(async move |this, cx| {
            while let Some(step) = rx.next().await {
                let _ = this.update(cx, |view, cx| {
                    let text = match step {
                        Step::Progress(done, total) => trf!(
                            "Compactando: {} de {}…",
                            "Compressing: {} of {}…",
                            done,
                            total
                        ),
                        Step::Done(Ok(made)) => trf!(
                            "\"{}\" criado",
                            "\"{}\" created",
                            made.first()
                                .and_then(|p| p.file_name())
                                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                        ),
                        Step::Done(Err(e)) => {
                            trf!("Não foi possível compactar: {e}", "Could not compress: {e}")
                        }
                    };
                    view.show_notice(text, cx);
                });
            }
        })
        .detach();
    }
}

/// The ZIP's name: one item gives its name (a file without its extension),
/// several the folder's name.
fn zip_name(items: &[PathBuf], folder: &Path) -> String {
    let base = match items {
        [one] if one.is_dir() => one.file_name(),
        [one] => one.file_stem(),
        _ => folder.file_name(),
    };
    let base = base.map_or_else(
        || "arquivo".to_owned(),
        |b| b.to_string_lossy().into_owned(),
    );
    format!("{base}.zip")
}

/// Extracts `names` of the folder `path` inside `zip` into a fresh folder
/// (worker thread); returns the top items.
fn extract_from_zip(
    zip: &Path,
    path: &Path,
    names: &[String],
    progress: &mut dyn FnMut(usize, usize),
) -> std::io::Result<Vec<PathBuf>> {
    let entries = fx_fs::archive::read_zip(zip)?;
    let inner = fx_fs::archive::inner_parts(zip, path);
    let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
    let selected: Vec<&str> = names.iter().map(String::as_str).collect();
    let plan = fx_core::archive::zip_plan(&entries, &inner, &selected);
    let staging = fx_fs::archive::staging_dir()?;
    fx_fs::archive::extract_zip(zip, &plan, &staging, progress)?;
    top_items(&staging)
}

/// Extracts each archive whole into `<staging>\<its name>` (worker thread);
/// returns those folders.
fn extract_whole(
    archives: &[PathBuf],
    progress: &mut dyn FnMut(usize, usize),
) -> std::io::Result<Vec<PathBuf>> {
    let staging = fx_fs::archive::staging_dir()?;
    let mut out = Vec::new();
    for archive in archives {
        let name = archive
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        let stem = [".tar.gz", ".tar.bz2", ".tar.xz"]
            .iter()
            .find(|e| lower.ends_with(*e))
            .map_or_else(
                || {
                    archive
                        .file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
                },
                |e| name[..name.len() - e.len()].to_owned(),
            );
        let dest = staging.join(&stem);
        std::fs::create_dir_all(&dest)?;
        if fx_core::archive::is_zip(&name) {
            let entries = fx_fs::archive::read_zip(archive)?;
            let top: Vec<String> = fx_core::archive::zip_children(&entries, &[])
                .into_iter()
                .map(|i| i.name)
                .collect();
            let top: Vec<&str> = top.iter().map(String::as_str).collect();
            let plan = fx_core::archive::zip_plan(&entries, &[], &top);
            fx_fs::archive::extract_zip(archive, &plan, &dest, progress)?;
        } else {
            fx_fs::archive::extract_all_other(archive, &dest, &mut |n| progress(n, 0))?;
        }
        out.push(dest);
    }
    Ok(out)
}

fn top_items(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    v.sort();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_names() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        std::fs::create_dir(d.join("Fotos")).unwrap();
        std::fs::write(d.join("nota.txt"), b"x").unwrap();
        assert_eq!(zip_name(&[d.join("Fotos")], d), "Fotos.zip");
        assert_eq!(zip_name(&[d.join("nota.txt")], d), "nota.zip");
        let both = zip_name(&[d.join("Fotos"), d.join("nota.txt")], d);
        assert_eq!(
            both,
            format!("{}.zip", d.file_name().unwrap().to_string_lossy())
        );
    }
}
