//! Quick Look (task T1.22): Space shows the item under the cursor in a large
//! panel over the list; it follows the arrows and closes with Space or Esc.
//!
//! Each preview loads on its own worker thread (file read, image decode,
//! shell thumbnail), so the UI never waits; a newer request makes older
//! results stale (dropped by ticket). Images: decoded by fx up to
//! `IMAGE_LIMIT`; text and code: the first 256 KB in a virtualized list;
//! everything else: the shell's large thumbnail (or its big icon).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use fx_shell::preview::{PreviewProcess, ScreenRect};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::time::Instant;

use fx_core::format::size_short;
use fx_core::highlight::{Kind, highlight, language_for};
use fx_core::markdown::{self, Block, MdLine};
use fx_core::preview::{
    IMAGE_LIMIT, ImageKind, PreviewKind, TEXT_LIMIT, preview_kind, text_preview,
};
use fx_core::zip_list;
use gpui::{
    Context, Image, ImageFormat, ImageSource, MouseButton, MouseDownEvent, ObjectFit, RenderImage,
    SharedString, Window, canvas, deferred, div, img, prelude::*, px, uniform_list,
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

type Spans = Vec<Vec<(std::ops::Range<usize>, Kind)>>;

/// Most archive entries listed.
const ZIP_MAX: usize = 5000;

/// How long a preview handler may take to show something.
const HANDLER_TIMEOUT: Duration = Duration::from_secs(5);

/// The `fx-preview` process of the open preview and its state.
#[derive(Default)]
pub(super) struct Host {
    process: Mutex<Option<PreviewProcess>>,
    started: AtomicBool,
    closed: AtomicBool,
    failed: Mutex<Option<String>>,
    last: Mutex<Option<ScreenRect>>,
    /// Last `raise`, to repeat it a few times a second while fx is active
    /// (a click on fx brings fx above the preview), and whether it is hidden.
    raised: Mutex<Option<std::time::Instant>>,
    hidden: AtomicBool,
}

impl Host {
    /// The preview went away: end the process (now, or when it starts).
    fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        drop(self.process.lock().ok().and_then(|mut p| p.take()));
    }

    /// Called every frame with the panel body in screen pixels: starts the
    /// process once, then keeps its window on the panel.
    fn place(self: &Arc<Self>, owner: isize, rect: ScreenRect, path: &Path, active: bool) {
        if !self.started.swap(true, Ordering::SeqCst) {
            let exe = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("fx-preview.exe")));
            let Some(exe) = exe.filter(|p| p.exists()) else {
                *lock(&self.failed) = Some("fx-preview.exe não está ao lado do fx.exe".into());
                return;
            };
            let mut cmd = std::process::Command::new(exe);
            cmd.arg(owner.to_string())
                .args([rect.x, rect.y, rect.width, rect.height].map(|v| v.to_string()))
                .arg(path);
            *lock(&self.last) = Some(rect);
            let host = self.clone();
            let _ = std::thread::Builder::new()
                .name("fx-preview-start".into())
                .spawn(move || match PreviewProcess::start(cmd, HANDLER_TIMEOUT) {
                    Ok(mut p) => {
                        if host.closed.load(Ordering::SeqCst) {
                            return; // dropped here: the process ends
                        }
                        if let Some(r) = *lock(&host.last) {
                            p.move_to(r);
                        }
                        *lock(&host.process) = Some(p);
                    }
                    Err(e) => *lock(&host.failed) = Some(e.to_string()),
                });
            return;
        }
        let mut last = lock(&self.last);
        let mut process = lock(&self.process);
        let Some(p) = process.as_mut() else { return };
        if *last != Some(rect) {
            *last = Some(rect);
            p.move_to(rect);
        }
        if !active {
            if !self.hidden.swap(true, Ordering::SeqCst) {
                p.set_visible(false);
            }
            return;
        }
        let mut raised = lock(&self.raised);
        let due = raised.is_none_or(|t| t.elapsed() >= Duration::from_millis(250));
        if due || self.hidden.swap(false, Ordering::SeqCst) {
            p.set_visible(true);
            *raised = Some(std::time::Instant::now());
        }
    }

    fn waiting(&self) -> bool {
        lock(&self.process).is_none() && lock(&self.failed).is_none()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

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
        /// Syntax colors per line, for code (T2.9).
        spans: Option<Arc<Spans>>,
    },
    /// Markdown, rendered (T2.9).
    Markdown(Arc<Vec<MdLine>>, bool),
    /// An archive's file list (T2.9): (name, size) rows and a summary.
    Zip(Arc<Vec<(SharedString, SharedString)>>, SharedString),
    /// A Windows preview handler (PDF...) shown by `fx-preview` over the
    /// panel (T2.9).
    Handler(PathBuf, Arc<Host>),
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
            && let Content::Image(image) | Content::Icon(image, _) = &q.content
        {
            self.stale_images.push(image.clone());
        }
        if let Some(q) = &self.quick_look
            && let Content::Handler(_, host) = &q.content
        {
            host.close();
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
                let handler = matches!(content, Content::Handler(..));
                q.content = content;
                let asked = q.asked;
                if let Some(trace) = &view.trace {
                    trace.borrow_mut().input_started(asked, "preview");
                }
                if handler {
                    view.tick_preview_host(ticket, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// While a Windows preview is open: repaint 4 times a second, which
    /// keeps its window on the panel, above fx, or hidden when fx is not
    /// active (see `Host::place`). Stops when the preview changes.
    fn tick_preview_host(&self, ticket: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(250))
                    .await;
                let open = this.update(cx, |view, cx| {
                    let open = view.quick_look.as_ref().is_some_and(|q| {
                        q.ticket == ticket && matches!(q.content, Content::Handler(..))
                    });
                    if open {
                        cx.notify();
                    }
                    open
                });
                if !matches!(open, Ok(true)) {
                    return;
                }
            }
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
            Content::Handler(path, host) => {
                let failed = lock(&host.failed).clone();
                match failed {
                    Some(why) => message_owned(format!("Sem prévia: {why}"), t.text_secondary)
                        .into_any_element(),
                    None => {
                        let waiting = host.waiting();
                        div()
                            .size_full()
                            .relative()
                            .child(message(
                                "Abrindo o visualizador do Windows…",
                                t.text_secondary,
                            ))
                            .child(
                                canvas(
                                    move |bounds, window, _| {
                                        let Some(owner) = fx_shell::window_id(window) else {
                                            return;
                                        };
                                        let scale = window.scale_factor();
                                        let px =
                                            |v: gpui::Pixels| (f32::from(v) * scale).round() as i32;
                                        let (x, y) = fx_shell::preview_host::client_to_screen(
                                            owner,
                                            px(bounds.origin.x),
                                            px(bounds.origin.y),
                                        );
                                        let rect = ScreenRect {
                                            x,
                                            y,
                                            width: px(bounds.size.width),
                                            height: px(bounds.size.height),
                                        };
                                        let active = window.is_window_active();
                                        host.place(owner, rect, &path, active);
                                        // Until it shows (or fails), check every frame;
                                        // afterwards a 250 ms tick keeps it in place.
                                        if waiting {
                                            window.request_animation_frame();
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                            .into_any_element()
                    }
                }
            }
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
            Content::Markdown(lines, truncated) => markdown_view(&lines, truncated, t, cx),
            Content::Zip(rows, summary) => zip_view(rows, summary, t),
            Content::Text {
                lines,
                truncated,
                spans,
            } => {
                let count = lines.len();
                let rows = lines.clone();
                let colors = [t.accent, t.syntax_string, t.text_secondary, t.syntax_number]
                    .map(|c| gpui::Hsla::from(rgba(c)));
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        uniform_list("quick-look-lines", count, move |range, _, _| {
                            range
                                .map(|i| {
                                    let line = div().h(px(LINE_HEIGHT)).whitespace_nowrap();
                                    match spans.as_ref().and_then(|s| s.get(i)) {
                                        Some(line_spans) if !line_spans.is_empty() => {
                                            let styles = line_spans.iter().map(|(r, k)| {
                                                let color = colors[match k {
                                                    Kind::Keyword => 0,
                                                    Kind::String => 1,
                                                    Kind::Comment => 2,
                                                    Kind::Number => 3,
                                                }];
                                                (
                                                    r.clone(),
                                                    gpui::HighlightStyle {
                                                        color: Some(color),
                                                        ..Default::default()
                                                    },
                                                )
                                            });
                                            line.child(
                                                gpui::StyledText::new(rows[i].clone())
                                                    .with_highlights(styles),
                                            )
                                        }
                                        _ => line.child(rows[i].clone()),
                                    }
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

fn message_owned(text: String, color: crate::theme::Color) -> gpui::Div {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgba(color))
        .child(text)
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
        PreviewKind::Zip => zip_listing(path).unwrap_or_else(|| shell(path, decode)),
        PreviewKind::Text => match read_start(path) {
            Ok((data, more)) => match text_preview(&data, more) {
                Some(t) => {
                    let ext = path
                        .extension()
                        .map(|e| e.to_string_lossy().to_ascii_lowercase())
                        .unwrap_or_default();
                    if ext == "md" || ext == "markdown" {
                        Content::Markdown(Arc::new(markdown::render(&t.lines)), t.truncated)
                    } else {
                        let spans = language_for(&ext).map(|l| Arc::new(highlight(&t.lines, l)));
                        Content::Text {
                            lines: Arc::new(t.lines.into_iter().map(SharedString::from).collect()),
                            truncated: t.truncated,
                            spans,
                        }
                    }
                }
                // Binary after all (a file without extension): a thumbnail.
                None => shell(path, decode),
            },
            Err(_) => Content::Nothing("Não foi possível ler este arquivo."),
        },
        // Folders and other files: a Windows preview handler when the type
        // has one (PDF...), else the shell's thumbnail.
        PreviewKind::Other if !is_dir && fx_shell::preview_host::handler_for(path).is_some() => {
            Content::Handler(path.to_path_buf(), Arc::new(Host::default()))
        }
        _ => shell(path, decode),
    }
}

/// The file list of a ZIP archive, from its central directory (nothing is
/// decompressed). `None` if it is not a readable ZIP.
fn zip_listing(path: &Path) -> Option<Content> {
    use std::io::{Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let tail_len = len.min(66_000);
    let tail_off = len - tail_len;
    let mut tail = vec![0u8; tail_len as usize];
    f.seek(SeekFrom::Start(tail_off)).ok()?;
    f.read_exact(&mut tail).ok()?;
    let mut read_at = |off: u64, n: usize| {
        let mut b = vec![0u8; n];
        f.seek(SeekFrom::Start(off)).ok()?;
        f.read_exact(&mut b).ok()?;
        Some(b)
    };
    let (dir_off, dir_len, total) = zip_list::central_directory(&tail, tail_off, &mut read_at)?;
    // A huge central directory is cut (the listing stops at ZIP_MAX anyway).
    let dir = read_at(dir_off, dir_len.min(32 << 20) as usize)?;
    let listing = zip_list::parse_central(&dir, total, ZIP_MAX)?;
    let unpacked: u64 = listing.entries.iter().map(|e| e.size).sum();
    let rows: Vec<(SharedString, SharedString)> = listing
        .entries
        .iter()
        .map(|e| {
            let size = if e.is_dir {
                String::new()
            } else {
                fx_core::format::size_kb(e.size)
            };
            (e.name.clone().into(), size.into())
        })
        .collect();
    let mut summary = format!(
        "{} · {} descompactado",
        fx_core::format::items(listing.total as usize),
        size_short(unpacked)
    );
    if listing.truncated {
        summary.push_str(&format!(" · mostrando os primeiros {ZIP_MAX}"));
    }
    Some(Content::Zip(Arc::new(rows), summary.into()))
}

fn zip_view(
    rows: Arc<Vec<(SharedString, SharedString)>>,
    summary: SharedString,
    t: crate::theme::Theme,
) -> gpui::AnyElement {
    let secondary = t.text_secondary;
    let count = rows.len();
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(
            uniform_list("quick-look-zip", count, move |range, _, _| {
                range
                    .map(|i| {
                        let (name, size) = rows[i].clone();
                        div()
                            .w_full()
                            .h(px(LINE_HEIGHT + 4.0))
                            .flex()
                            .items_center()
                            .gap(px(space(3)))
                            .child(div().flex_1().min_w_0().truncate().child(name))
                            .child(div().flex_none().text_color(rgba(secondary)).child(size))
                    })
                    .collect()
            })
            .flex_1()
            .px(px(space(3))),
        )
        .child(
            div()
                .px(px(space(3)))
                .py(px(space(1)))
                .border_t_1()
                .border_color(rgba(t.divider))
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .child(summary),
        )
        .into_any_element()
}

/// Rendered Markdown: headings larger and bold, code in a monospace band,
/// quotes in secondary text. Not virtualized (at most `LINE_LIMIT` lines).
fn markdown_view(
    lines: &[MdLine],
    truncated: bool,
    t: crate::theme::Theme,
    _cx: &mut Context<FolderView>,
) -> gpui::AnyElement {
    let mut body = div()
        .id("quick-look-md")
        .size_full()
        .overflow_y_scroll()
        .px(px(space(5)))
        .py(px(space(3)))
        .flex()
        .flex_col();
    for l in lines {
        let row = div().w_full().min_w_0();
        let row = match l.block {
            Block::Heading(level) => row
                .pt(px(if level <= 2 { space(3) } else { space(2) }))
                .pb(px(space(1)))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_size(px(match level {
                    1 => 24.0,
                    2 => 20.0,
                    3 => 17.0,
                    _ => 15.0,
                }))
                .child(l.text.clone()),
            Block::Code => row
                .px(px(space(2)))
                .bg(rgba(t.hover))
                .font_family(MONO_FONT)
                .text_size(px(13.0))
                .whitespace_nowrap()
                .child(l.text.clone()),
            Block::Quote => row
                .pl(px(space(3)))
                .border_l_2()
                .border_color(rgba(t.divider))
                .text_color(rgba(t.text_secondary))
                .child(l.text.clone()),
            Block::Rule => row.my(px(space(2))).h(px(1.0)).bg(rgba(t.divider)),
            Block::Blank => row.h(px(space(2))),
            Block::Item | Block::Text => row.child(l.text.clone()),
        };
        body = body.child(row);
    }
    if truncated {
        body = body.child(
            div()
                .pt(px(space(2)))
                .text_size(px(CAPTION_SIZE))
                .text_color(rgba(t.text_secondary))
                .child("Mostrando o começo do arquivo (256 KB)."),
        );
    }
    body.into_any_element()
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
        let Content::Text {
            lines, truncated, ..
        } = content
        else {
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

    #[test]
    fn code_markdown_and_zip_previews() {
        let t = tempfile::tempdir().unwrap();
        let rs = t.path().join("main.rs");
        std::fs::write(&rs, "fn main() { let x = \"oi\"; } // fim\n").unwrap();
        let Content::Text {
            spans: Some(spans), ..
        } = load(&rs, PreviewKind::Text, 0, false, &*no_decoder())
        else {
            panic!("code with colors")
        };
        assert!(spans[0].len() >= 4);
        let md = t.path().join("LEIA.md");
        std::fs::write(&md, "# Título\n- item\n").unwrap();
        let Content::Markdown(lines, false) =
            load(&md, PreviewKind::Text, 0, false, &*no_decoder())
        else {
            panic!("rendered markdown")
        };
        assert_eq!(lines[0].block, Block::Heading(1));
        // A stored ZIP with one file, written by hand.
        let zip = t.path().join("a.zip");
        let mut z = Vec::new();
        z.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        z.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&3u32.to_le_bytes());
        z.extend_from_slice(&3u32.to_le_bytes());
        z.extend_from_slice(&5u16.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes());
        z.extend_from_slice(b"x.txt");
        z.extend_from_slice(b"abc");
        let cd = z.len() as u32;
        z.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        z.extend_from_slice(&[20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        z.extend_from_slice(&3u32.to_le_bytes());
        z.extend_from_slice(&3u32.to_le_bytes());
        z.extend_from_slice(&5u16.to_le_bytes());
        z.extend_from_slice(&[0; 12]);
        z.extend_from_slice(&0u32.to_le_bytes());
        z.extend_from_slice(b"x.txt");
        let cd_len = z.len() as u32 - cd;
        z.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        z.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend_from_slice(&cd_len.to_le_bytes());
        z.extend_from_slice(&cd.to_le_bytes());
        z.extend_from_slice(&[0, 0]);
        std::fs::write(&zip, z).unwrap();
        let Content::Zip(rows, summary) = load(&zip, PreviewKind::Zip, 0, false, &*no_decoder())
        else {
            panic!("zip listing")
        };
        assert_eq!(rows[0].0.as_ref(), "x.txt");
        assert!(summary.starts_with("1 item"), "{summary}");
    }
}
