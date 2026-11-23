//! Image quick actions from the palette (T6.16, D-068): copies of the
//! selected images as JPG or PNG, or at half size, next to them. Originals
//! are never touched; a copy never replaces anything (a temporary name, then
//! a rename that refuses to replace, "keep both" on a clash). Runs on a
//! thread; the status says how far it really is.

use std::path::{Path, PathBuf};

use futures::StreamExt;
use fx_core::{tr, trf};
use fx_shell::images::{ImageKind, convert_image};
use gpui::{Context, Window};

use super::FolderView;

/// What to make of each image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageAction {
    To(ImageKind),
    Half,
    /// Rename by the date taken (`2024-07-15 14.30.22.jpg`), as one
    /// operation that undo reverts.
    RenameByDate,
}

fn is_image(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    matches!(
        ext.as_str(),
        "jpg" | "jpeg" | "png" | "bmp" | "gif" | "tif" | "tiff" | "webp" | "ico" | "jfif"
    )
}

/// The free name of the copy of `src`.
fn copy_name(src: &Path, action: ImageAction) -> (PathBuf, ImageKind) {
    let dir = src.parent().unwrap_or(Path::new("."));
    let stem = src
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let (kind, name) = match action {
        ImageAction::To(kind) => (kind, format!("{stem}.{}", kind.extension())),
        ImageAction::Half | ImageAction::RenameByDate => {
            let png = src
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"));
            let kind = if png { ImageKind::Png } else { ImageKind::Jpeg };
            (kind, format!("{stem} (50%).{}", kind.extension()))
        }
    };
    let target = dir.join(&name);
    let free = if target.exists() {
        fx_core::naming::keep_both_name(dir, &name, |p| p.exists())
    } else {
        target
    };
    (free, kind)
}

/// Makes one copy: a temporary file, then a rename that never replaces.
fn make_copy(src: &Path, action: ImageAction) -> std::io::Result<PathBuf> {
    let (target, kind) = copy_name(src, action);
    let dir = target.parent().unwrap_or(Path::new(".")).to_path_buf();
    let dir = dir.as_path();
    let temp = dir.join(fx_core::temp_name::temp_name(
        target.file_name().unwrap_or_default(),
        std::process::id(),
        0,
    ));
    convert_image(src, &temp, kind, action == ImageAction::Half)?;
    // Copies made in parallel may pick the same free name: try the next.
    let name = target.file_name().unwrap_or_default().to_os_string();
    let mut candidate = target;
    let mut result = fx_fs::rename_no_replace(&temp, &candidate);
    for _ in 0..8 {
        match &result {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                candidate = fx_core::naming::keep_both_name(dir, &name, |p| p.exists());
                result = fx_fs::rename_no_replace(&temp, &candidate);
            }
            _ => break,
        }
    }
    let result = result.map(|()| candidate);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

impl FolderView {
    /// Runs `action` on the selected images (palette).
    pub fn image_action(
        &mut self,
        action: ImageAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let images: Vec<PathBuf> = self
            .selected_paths()
            .into_iter()
            .filter(|p| is_image(p))
            .collect();
        if images.is_empty() {
            self.show_notice(
                tr!("Selecione uma ou mais imagens", "Select one or more images").to_owned(),
                cx,
            );
            return;
        }
        if action == ImageAction::RenameByDate {
            self.rename_by_date(images, window, cx);
            return;
        }
        let total = images.len();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<(usize, usize)>();
        let _ = std::thread::Builder::new()
            .name("fx-images".into())
            .spawn(move || {
                use std::sync::atomic::{AtomicUsize, Ordering};
                // Up to 4 images at once (a 12 MP photo takes ~100 MB while
                // it is converted).
                let workers = std::thread::available_parallelism()
                    .map_or(1, |n| n.get())
                    .clamp(1, 4)
                    .min(images.len());
                let (next, done, failed) = (
                    AtomicUsize::new(0),
                    AtomicUsize::new(0),
                    AtomicUsize::new(0),
                );
                std::thread::scope(|scope| {
                    for _ in 0..workers {
                        scope.spawn(|| {
                            while let Some(src) = images.get(next.fetch_add(1, Ordering::Relaxed)) {
                                let bad = usize::from(make_copy(src, action).is_err());
                                let failed = failed.fetch_add(bad, Ordering::Relaxed) + bad;
                                let done = done.fetch_add(1, Ordering::Relaxed) + 1;
                                let _ = tx.unbounded_send((done, failed));
                            }
                        });
                    }
                });
            });
        cx.spawn(async move |this, cx| {
            let mut last = (0, 0);
            while let Some(step) = rx.next().await {
                // Threads report in any order: keep the highest counts.
                last = (last.0.max(step.0), last.1.max(step.1));
                let (done, _) = last;
                let _ = this.update(cx, |view, cx| {
                    if done < total {
                        view.show_notice(
                            trf!("Imagens: {} de {}…", "Images: {} of {}…", done, total),
                            cx,
                        );
                    }
                });
            }
            let (done, failed) = last;
            let _ = this.update(cx, |view, cx| {
                let made = done - failed;
                let text = if failed == 0 {
                    trf!(
                        "{} cópias feitas ao lado das originais",
                        "{} copies made next to the originals",
                        made
                    )
                } else {
                    trf!(
                        "{} cópias feitas; {} imagens não puderam ser lidas",
                        "{} copies made; {} images could not be read",
                        made,
                        failed
                    )
                };
                view.show_notice(text, cx);
            });
        })
        .detach();
    }
}

/// The renames by date (worker thread: checks the folder for free names).
fn plan_renames(read: Vec<(PathBuf, Option<i64>)>) -> (Vec<(PathBuf, std::ffi::OsString)>, usize) {
    let without = read.iter().filter(|(_, t)| t.is_none()).count();
    let photos: Vec<(std::ffi::OsString, Option<i64>)> = read
        .iter()
        .map(|(p, t)| (p.file_name().unwrap_or_default().to_os_string(), *t))
        .collect();
    let dir = read
        .first()
        .and_then(|(p, _)| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let names = fx_core::photo_names::plan(&photos, |name| dir.join(name).exists());
    let pairs = read
        .into_iter()
        .zip(names)
        .filter_map(|((path, _), new)| Some((path, new?)))
        .collect();
    (pairs, without)
}

impl FolderView {
    /// Reads the dates on a thread (with real progress), then renames the
    /// photos that have one through the engine (never replacing; undo).
    fn rename_by_date(
        &mut self,
        images: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        enum Step {
            Read(usize),
            /// The renames, and how many photos have no date.
            Done(Vec<(PathBuf, std::ffi::OsString)>, usize),
        }
        let total = images.len();
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Step>();
        let _ = std::thread::Builder::new()
            .name("fx-exif".into())
            .spawn(move || {
                let mut out = Vec::with_capacity(images.len());
                for (k, path) in images.into_iter().enumerate() {
                    let local = fx_shell::images::photo_taken(&path).ok().flatten();
                    out.push((path, local));
                    if (k + 1) % 8 == 0 {
                        let _ = tx.unbounded_send(Step::Read(k + 1));
                    }
                }
                let (pairs, without) = plan_renames(out);
                let _ = tx.unbounded_send(Step::Done(pairs, without));
            });
        let window_handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            while let Some(step) = rx.next().await {
                match step {
                    Step::Read(done) => {
                        let _ = this.update(cx, |view, cx| {
                            view.show_notice(
                                trf!(
                                    "Lendo datas das fotos: {} de {}…",
                                    "Reading photo dates: {} of {}…",
                                    done,
                                    total
                                ),
                                cx,
                            );
                        });
                    }
                    Step::Done(pairs, without) => {
                        let _ = window_handle.update(cx, |_, window, cx| {
                            let _ = this.update(cx, |view, cx| {
                                view.apply_dates(pairs, without, window, cx)
                            });
                        });
                    }
                }
            }
        })
        .detach();
    }

    fn apply_dates(
        &mut self,
        pairs: Vec<(PathBuf, std::ffi::OsString)>,
        without: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pairs.is_empty() {
            self.show_notice(
                if without > 0 {
                    tr!(
                        "Nenhuma das imagens tem data da foto",
                        "None of the images has a date taken"
                    )
                } else {
                    tr!(
                        "As fotos já têm o nome pela data",
                        "The photos are already named by date"
                    )
                }
                .to_owned(),
                cx,
            );
            return;
        }
        let renamed = pairs.len();
        self.submit_job(fx_ops::Job::batch_rename(pairs), window, cx);
        let text = if without == 0 {
            trf!(
                "Renomeando {} fotos pela data; Ctrl+Z desfaz",
                "Renaming {} photos by date; Ctrl+Z undoes",
                renamed
            )
        } else {
            trf!(
                "Renomeando {} fotos pela data; {} sem data ficam como estão",
                "Renaming {} photos by date; {} without a date stay as they are",
                renamed,
                without
            )
        };
        self.show_notice(text, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_never_clash_and_only_images_count() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("praia.jpeg");
        std::fs::write(&src, b"x").unwrap();
        let (png, kind) = copy_name(&src, ImageAction::To(ImageKind::Png));
        assert_eq!(png, dir.path().join("praia.png"));
        assert_eq!(kind, ImageKind::Png);
        std::fs::write(&png, b"x").unwrap();
        assert_eq!(
            copy_name(&src, ImageAction::To(ImageKind::Png)).0,
            dir.path().join("praia (2).png")
        );
        assert_eq!(
            copy_name(&src, ImageAction::Half).0,
            dir.path().join("praia (50%).jpg")
        );
        assert!(is_image(Path::new("A.WEBP")));
        assert!(!is_image(Path::new("A.HEIC")), "no free Rust HEIC decoder");
        assert!(!is_image(Path::new("notas.txt")));
    }
}
