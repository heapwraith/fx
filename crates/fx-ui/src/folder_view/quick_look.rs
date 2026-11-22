//! Quick Look (task T1.22): Space shows the item under the cursor in a large
//! panel over the list; it follows the arrows and closes with Space or Esc.
//!
//! Each preview loads on its own worker thread (file read, image decode,
//! shell thumbnail), so the UI never waits; a newer request makes older
//! results stale (dropped by ticket). Images: decoded by fx up to
//! `IMAGE_LIMIT`; text and code: the first 256 KB in a virtualized list;
//! everything else: the shell's large thumbnail (or its big icon).

use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::Instant;

use fx_core::format::size_short;
use fx_core::preview::{
    IMAGE_LIMIT, ImageKind, PreviewKind, TEXT_LIMIT, preview_kind, text_preview,
};
use gpui::{
    Context, Image, ImageFormat, ImageSource, MouseButton, MouseDownEvent, ObjectFit, RenderImage,
    SharedString, Window, deferred, div, img, prelude::*, px, uniform_list,
};

use super::FolderView;
use crate::theme::{CAPTION_SIZE, RADIUS, rgba, space};

/// Largest side of the shell thumbnail used for "other" files.
const SHELL_PREVIEW: u32 = 512;
/// Types without a thumbnail show their icon at this size (not stretched).
const ICON_PREVIEW: u32 = 128;
/// JPEG and TIFF photos above this size are shown from the shell's thumbnail
/// (WIC decodes a JPEG straight at a reduced size): decoding a 12 MP photo
/// in full takes ~140 ms, the thumbnail ~45 ms. PNG decodes faster in fx
/// (10 MB: ~40 ms, via the shell ~55 ms); GIF keeps its animation.
const LARGE_IMAGE: u64 = 2 * 1024 * 1024;
/// Largest side of that thumbnail: covers the panel of a maximized 1080p
/// window.
const LARGE_PREVIEW: u32 = 1920;
const MONO_FONT: &str = "Consolas";
const LINE_HEIGHT: f32 = 18.0;

/// Whether the shell's photo decoder (WIC and its JPEG thumbnail provider)
/// was warmed up in this process. Its first use costs ~50 ms more (88 ms
/// instead of 39 for a 12 MP JPEG), which would land on the first Space.
static PHOTO_DECODER_WARM: AtomicBool = AtomicBool::new(false);

/// The open preview.
pub(super) struct QuickLook {
    entry: u32,
    ticket: u64,
    name: String,
    detail: String,
    content: Content,
    /// When the key that asked for it was handled (trace: key → preview shown).
    asked: Instant,
}

#[derive(Clone)]
enum Content {
    Loading,
    Image(Arc<RenderImage>),
    /// The file type's icon (no thumbnail handler for it), with a caption.
    Icon(Arc<RenderImage>, &'static str),
    Text {
        lines: Arc<Vec<SharedString>>,
        truncated: bool,
    },
    Nothing(&'static str),
}

impl FolderView {
    pub(super) fn quick_look_open(&self) -> bool {
        self.quick_look.is_some()
    }

    /// Space: opens on the cursor item, or closes.
    pub(super) fn toggle_quick_look(&mut self, asked: Instant, cx: &mut Context<Self>) {
        if self.quick_look.is_some() {
            self.close_quick_look();
        } else if let Some(c) = self.selection.cursor() {
            self.load_quick_look(c, asked, cx);
        }
    }

    pub(super) fn close_quick_look(&mut self) {
        if let Some(q) = self.quick_look.take()
            && let Content::Image(image) | Content::Icon(image, _) = q.content
        {
            self.stale_images.push(image);
        }
    }

    /// Called every frame while open: follows the cursor, and frees the GPU
    /// copies of images no longer shown.
    pub(super) fn sync_quick_look(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for image in self.stale_images.drain(..) {
            let _ = window.drop_image(image);
        }
        self.warm_photo_decoder();
        let Some(q) = &self.quick_look else { return };
        match self.selection.cursor() {
            Some(c) if c != q.entry => self.load_quick_look(c, Instant::now(), cx),
            Some(_) => {}
            None => self.close_quick_look(),
        }
    }

    /// The first time the cursor rests on a large photo, decodes it once in
    /// the background and throws the result away: the decoder is then loaded
    /// when Space comes. Only users who look at photos pay its memory.
    fn warm_photo_decoder(&self) {
        if PHOTO_DECODER_WARM.load(Ordering::Relaxed) {
            return;
        }
        let Some(c) = self.selection.cursor() else {
            return;
        };
        let is_dir = self.list.attributes(c).is_dir();
        let photo = matches!(
            preview_kind(self.list.name(c), is_dir),
            PreviewKind::Image(ImageKind::Jpeg | ImageKind::Tiff)
        );
        if !photo || self.list.size(c) <= LARGE_IMAGE {
            return;
        }
        PHOTO_DECODER_WARM.store(true, Ordering::Relaxed);
        let path = self.path.join(self.list.os_name(c));
        let _ = std::thread::Builder::new()
            .name("fx-preview-warm".into())
            .spawn(move || drop(fx_shell::preview_image(&path, LARGE_PREVIEW, None)));
    }

    fn load_quick_look(&mut self, entry: u32, asked: Instant, cx: &mut Context<Self>) {
        self.close_quick_look();
        self.quick_look_ticket += 1;
        let ticket = self.quick_look_ticket;
        let name = self.list.name(entry).to_owned();
        let is_dir = self.list.attributes(entry).is_dir();
        let bytes = self.list.size(entry);
        let kind = preview_kind(&name, is_dir);
        let detail = if is_dir {
            "Pasta".to_owned()
        } else {
            format!(
                "{} · {}",
                fx_core::type_label(&name, false),
                size_short(bytes)
            )
        };
        let path = self.path.join(self.list.os_name(entry));
        self.quick_look = Some(QuickLook {
            entry,
            ticket,
            name,
            detail,
            content: Content::Loading,
            asked,
        });
        // GPUI's decoder needs its SVG renderer (a type GPUI does not name
        // publicly): the closure carries it to the worker.
        let svg = cx.svg_renderer();
        let decode = move |image: Image| image.to_image_data(svg.clone()).ok();
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("fx-preview".into())
            .spawn(move || {
                let _ = tx.send(load(&path, kind, bytes, is_dir, &decode));
            });
        if spawned.is_err() {
            if let Some(q) = &mut self.quick_look {
                q.content = Content::Nothing("Não foi possível abrir a pré-visualização.");
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            // A stuck thumbnail handler only leaves its worker behind.
            let content = cx.background_spawn(async move { rx.recv() }).await;
            let Ok(content) = content else { return };
            let _ = this.update(cx, |view, cx| {
                let Some(q) = view.quick_look.as_mut().filter(|q| q.ticket == ticket) else {
                    if let Content::Image(image) | Content::Icon(image, _) = content {
                        view.stale_images.push(image);
                    }
                    return;
                };
                q.content = content;
                if let Some(trace) = &view.trace {
                    trace.borrow_mut().input_started(q.asked, "preview");
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn render_quick_look(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let q = self.quick_look.as_ref()?;
        let t = self.look.theme;
        let body = match q.content.clone() {
            Content::Loading => message("Carregando…", t.text_secondary).into_any_element(),
            Content::Nothing(why) => message(why, t.text_secondary).into_any_element(),
            Content::Icon(image, caption) => div()
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(space(4)))
                .child(
                    img(ImageSource::Render(image))
                        .size(px(ICON_PREVIEW as f32))
                        .object_fit(ObjectFit::Contain),
                )
                .child(div().text_color(rgba(t.text_secondary)).child(caption))
                .into_any_element(),
            Content::Image(image) => div()
                .size_full()
                .p(px(space(2)))
                .child(
                    img(ImageSource::Render(image))
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            Content::Text { lines, truncated } => {
                let count = lines.len();
                let rows = lines.clone();
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        uniform_list("quick-look-lines", count, move |range, _, _| {
                            range
                                .map(|i| {
                                    div()
                                        .h(px(LINE_HEIGHT))
                                        .whitespace_nowrap()
                                        .child(rows[i].clone())
                                })
                                .collect()
                        })
                        .flex_1()
                        .px(px(space(3)))
                        .font_family(MONO_FONT)
                        .text_size(px(13.0)),
                    )
                    .when(truncated, |d| {
                        d.child(
                            div()
                                .px(px(space(3)))
                                .py(px(space(1)))
                                .border_t_1()
                                .border_color(rgba(t.divider))
                                .text_size(px(CAPTION_SIZE))
                                .text_color(rgba(t.text_secondary))
                                .child("Mostrando o começo do arquivo (256 KB)."),
                        )
                    })
                    .into_any_element()
            }
        };
        let card = div()
            .id("quick-look")
            .absolute()
            .top(px(space(12)))
            .bottom(px(space(4)))
            .left(px(space(6)))
            .right(px(space(6)))
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
                    .flex()
                    .items_center()
                    .gap(px(space(3)))
                    .px(px(space(4)))
                    .py(px(space(2)))
                    .border_b_1()
                    .border_color(rgba(t.divider))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(q.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(CAPTION_SIZE))
                            .text_color(rgba(t.text_secondary))
                            .child(format!("{} · Espaço ou Esc fecha", q.detail)),
                    ),
            )
            .child(div().flex_1().min_h_0().child(body));
        Some(
            deferred(
                div()
                    .id("quick-look-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _: &MouseDownEvent, _, cx| {
                            view.close_quick_look();
                            cx.notify();
                        }),
                    )
                    .child(card),
            )
            .with_priority(1),
        )
    }
}

fn message(text: &'static str, color: crate::theme::Color) -> gpui::Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgba(color))
        .child(text)
}

/// Image bytes to a GPU-ready image (GPUI's decoder), on the worker.
type Decode = dyn Fn(Image) -> Option<Arc<RenderImage>>;

/// Runs on the preview worker.
fn load(path: &Path, kind: PreviewKind, bytes: u64, is_dir: bool, decode: &Decode) -> Content {
    let caption = if is_dir {
        "Enter abre a pasta."
    } else {
        "Sem pré-visualização para este tipo de arquivo."
    };
    let shell = |path: &Path, decode: &Decode| shell(path, decode, caption);
    match kind {
        PreviewKind::Image(k)
            if bytes > LARGE_IMAGE
                && matches!(k, ImageKind::Jpeg | ImageKind::Tiff)
                && let Some(image) = fx_shell::preview_image(path, LARGE_PREVIEW, None)
                    .and_then(|p| decode(Image::from_bytes(ImageFormat::Bmp, p.bmp))) =>
        {
            Content::Image(image)
        }
        PreviewKind::Image(k) if bytes <= IMAGE_LIMIT => match std::fs::read(path) {
            Ok(data) => match decode(Image::from_bytes(format_of(k), data)) {
                Some(image) => Content::Image(image),
                None => shell(path, decode),
            },
            Err(_) => Content::Nothing("Não foi possível ler este arquivo."),
        },
        PreviewKind::Text => match read_start(path) {
            Ok((data, more)) => match text_preview(&data, more) {
                Some(t) => Content::Text {
                    lines: Arc::new(t.lines.into_iter().map(SharedString::from).collect()),
                    truncated: t.truncated,
                },
                // Binary after all (a file without extension): a thumbnail.
                None => shell(path, decode),
            },
            Err(_) => Content::Nothing("Não foi possível ler este arquivo."),
        },
        _ => shell(path, decode),
    }
}

/// The first `TEXT_LIMIT` bytes, and whether the file goes on.
fn read_start(path: &Path) -> std::io::Result<(Vec<u8>, bool)> {
    let mut data = Vec::with_capacity(TEXT_LIMIT + 1);
    std::fs::File::open(path)?
        .take(TEXT_LIMIT as u64 + 1)
        .read_to_end(&mut data)?;
    let more = data.len() > TEXT_LIMIT;
    data.truncate(TEXT_LIMIT);
    Ok((data, more))
}

fn shell(path: &Path, decode: &Decode, caption: &'static str) -> Content {
    let Some(preview) = fx_shell::preview_image(path, SHELL_PREVIEW, Some(ICON_PREVIEW)) else {
        return Content::Nothing("Sem pré-visualização para este tipo de arquivo.");
    };
    match decode(Image::from_bytes(ImageFormat::Bmp, preview.bmp)) {
        Some(image) if preview.is_icon => Content::Icon(image, caption),
        Some(image) => Content::Image(image),
        None => Content::Nothing("Sem pré-visualização para este tipo de arquivo."),
    }
}

fn format_of(k: ImageKind) -> ImageFormat {
    match k {
        ImageKind::Png => ImageFormat::Png,
        ImageKind::Jpeg => ImageFormat::Jpeg,
        ImageKind::Gif => ImageFormat::Gif,
        ImageKind::Webp => ImageFormat::Webp,
        ImageKind::Bmp => ImageFormat::Bmp,
        ImageKind::Tiff => ImageFormat::Tiff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn no_decoder() -> Box<Decode> {
        Box::new(|_| None)
    }

    #[test]
    fn text_preview_reads_only_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        // ~1 MB of lines: only the first 256 KB are read.
        let line = "2026-11-21 04:31:00 INFO fx: uma linha de log com acentuação\n";
        std::fs::write(&path, line.repeat(1_000_000 / line.len())).unwrap();
        let (data, more) = read_start(&path).unwrap();
        assert_eq!(data.len(), TEXT_LIMIT);
        assert!(more);

        let started = Instant::now();
        let content = load(&path, PreviewKind::Text, 0, false, &*no_decoder());
        let took = started.elapsed();
        let Content::Text { lines, truncated } = content else {
            panic!("text expected");
        };
        assert!(truncated);
        assert!(lines.len() > 1_000);
        assert!(lines[0].starts_with("2026-11-21"));
        println!("text preview of 1 MB file: {took:?}");
        // Generous in debug builds; the release number goes to the report.
        assert!(took < Duration::from_millis(500), "{took:?}");
    }

    #[test]
    fn unreadable_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("sumiu.txt");
        let content = load(&missing, PreviewKind::Text, 0, false, &*no_decoder());
        assert!(matches!(content, Content::Nothing(m) if m.contains("ler")));
    }
}
