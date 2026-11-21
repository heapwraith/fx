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
    SIIGBF_INCACHEONLY,
};
use windows::core::PCWSTR;

use super::icons::bitmap_bgra;
use crate::icons::encode_png;
use crate::thumbs::{THUMB_SIZE, THUMB_TIME_LIMIT, ThumbReady, ThumbService, fit_rgba};

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
    let wide: Vec<u16> = OsStr::new(path).encode_wide().chain([0]).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call; no bind context.
    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }.ok()?;
    let mut flags = SIIGBF_BIGGERSIZEOK;
    if cache_only {
        flags = SIIGBF(flags.0 | SIIGBF_INCACHEONLY.0);
    }
    let size = SIZE {
        cx: THUMB_SIZE as i32,
        cy: THUMB_SIZE as i32,
    };
    // SAFETY: `factory` is a valid COM object of this STA thread.
    let bitmap = unsafe { factory.GetImage(size, flags) }.ok()?;
    let pixels = bitmap_bgra(bitmap);
    // SAFETY: GetImage gave us ownership of the bitmap; deleted once.
    let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
    let (w, h, mut bgra) = pixels?;
    let opaque = bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0);
    for p in bgra.as_chunks_mut::<4>().0 {
        p.swap(0, 2); // BGRA -> RGBA
        if opaque {
            p[3] = 255; // photos come without alpha
        }
    }
    let (w, h, rgba) = fit_rgba(w, h, bgra, THUMB_SIZE);
    Some(encode_png(w, h, &rgba))
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
