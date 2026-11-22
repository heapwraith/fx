//! Windows thumbnails (task T1.7): `IShellItemImageFactory::GetImage` on a
//! low-priority COM STA thread. The cache pass uses `SIIGBF_INCACHEONLY`
//! (Windows' own thumbnail cache, no extraction); items without a thumbnail
//! (folders, documents without a preview) get their large icon.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY, SIIGBF_INCACHEONLY, SIIGBF_THUMBNAILONLY,
};
use windows::core::PCWSTR;

use super::icons::bitmap_bgra;
use crate::icons::{encode_bmp, encode_png};
use crate::thumbs::{
    ShellPreview, THUMB_SIZE, THUMB_TIME_LIMIT, ThumbReady, ThumbService, fit_rgba,
};

/// Starts the thumbnail service.
pub fn start_thumb_service(replies: Sender<ThumbReady>) -> std::io::Result<ThumbService> {
    ThumbService::with_extractor(Arc::new(thumbnail), init, THUMB_TIME_LIMIT, replies)
}

fn init() {
    // SAFETY: first COM call of a fresh worker thread; never uninitialized.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    // Background mode lowers CPU, I/O and memory priority of this thread, so
    // extraction never competes with the UI or with folder reads.
    // SAFETY: pseudo-handle of the current thread; no other effect.
    let _ = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
}

fn thumbnail(path: &Path, cache_only: bool) -> Option<Vec<u8>> {
    thumbnail_sized(path, cache_only, THUMB_SIZE)
}

/// A large shell thumbnail for Quick Look (T1.22), at most `max` pixels:
/// photos (decoded by WIC at a reduced size), videos, documents with a
/// thumbnail handler. Without a thumbnail: the type's icon at `icon` pixels,
/// or `None` when `icon` is `None`. Initializes COM on the calling thread (a
/// dedicated worker).
pub fn preview_image(path: &Path, max: u32, icon: Option<u32>) -> Option<ShellPreview> {
    // SAFETY: COM on this worker thread; harmless if already initialized.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if let Some((w, h, bgra)) = image_bgra(path, SIIGBF_THUMBNAILONLY, max) {
        return Some(ShellPreview {
            bmp: encode_bmp(w, h, &bgra),
            is_icon: false,
        });
    }
    let (w, h, bgra) = image_bgra(path, SIIGBF_ICONONLY, icon?)?;
    Some(ShellPreview {
        bmp: encode_bmp(w, h, &bgra),
        is_icon: true,
    })
}

fn thumbnail_sized(path: &Path, cache_only: bool, max: u32) -> Option<Vec<u8>> {
    let extra = if cache_only {
        SIIGBF_INCACHEONLY
    } else {
        SIIGBF(0)
    };
    image_png(path, extra, max)
}

/// `IShellItemImageFactory::GetImage` with `extra` flags, as a PNG of at most
/// `max` pixels.
fn image_png(path: &Path, extra: SIIGBF, max: u32) -> Option<Vec<u8>> {
    let (w, h, mut px) = image_bgra(path, extra, max)?;
    for p in px.as_chunks_mut::<4>().0 {
        p.swap(0, 2); // BGRA -> RGBA
    }
    Some(encode_png(w, h, &px))
}

/// `IShellItemImageFactory::GetImage` with `extra` flags: straight-alpha BGRA
/// pixels, at most `max` pixels on a side.
fn image_bgra(path: &Path, extra: SIIGBF, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    let wide: Vec<u16> = OsStr::new(path).encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; no bind context.
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.ok()?;
    let flags = SIIGBF(SIIGBF_BIGGERSIZEOK.0 | extra.0);
    let size = SIZE {
        cx: max as i32,
        cy: max as i32,
    };
    // SAFETY: `factory` is a valid COM object of this STA thread.
    let bitmap = unsafe { factory.GetImage(size, flags) }.ok()?;
    let pixels = bitmap_bgra(bitmap);
    // SAFETY: GetImage gave us ownership of the bitmap; deleted once.
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    let (w, h, mut bgra) = pixels?;
    if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        for p in bgra.as_chunks_mut::<4>().0 {
            p[3] = 255; // photos come without alpha
        }
    }
    // Box-filters the 4 channels alike, so BGRA works as well as RGBA.
    Some(fit_rgba(w, h, bgra, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn real_thumbnail_or_icon_for_a_system_file() {
        let (tx, rx) = channel();
        let service = start_thumb_service(tx).unwrap();
        let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
        let target = Path::new(&windir).join("notepad.exe");
        service.want(vec![target.clone()]);
        let r = rx.recv_timeout(Duration::from_secs(15)).unwrap();
        assert_eq!(r.path, target);
        let png = r.png.expect("an image (icon at least)");
        let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
        assert!(w > 16 && w <= THUMB_SIZE, "{w}");
    }
}
