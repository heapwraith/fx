//! Windows icon extraction (task T1.6): `SHGetFileInfoW` on a COM STA thread,
//! HICON converted to RGBA through GDI, then PNG.
//!
//! Extension and folder icons use `SHGFI_USEFILEATTRIBUTES` (no disk access:
//! the shell answers from the registry); per-file icons (.exe, .lnk, .ico)
//! read the file and may run third-party shell extensions, hence the time
//! limit in `crate::icons`.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows::Win32::UI::Shell::{
    SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::PCWSTR;

use crate::icons::{IconKey, IconReady, IconService, TIME_LIMIT, encode_png};

/// Starts the icon service (STA worker, time limit per request).
pub fn start_icon_service(replies: Sender<IconReady>) -> std::io::Result<IconService> {
    IconService::with_extractor(Arc::new(extract), com_init, TIME_LIMIT, replies)
}

fn com_init() {
    // SAFETY: called once at the start of a fresh worker thread, before any
    // other COM call on it; the thread never uninitializes (it lives until
    // the process ends or is abandoned).
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
}

fn wide_z(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn extract(key: &IconKey) -> Option<Vec<u8>> {
    let (target, attributes, extra) = match key {
        IconKey::Folder => (
            wide_z(OsStr::new("folder")),
            FILE_ATTRIBUTE_DIRECTORY,
            SHGFI_USEFILEATTRIBUTES,
        ),
        IconKey::Extension(ext) => (
            wide_z(OsStr::new(&format!("file.{ext}"))),
            FILE_ATTRIBUTE_NORMAL,
            SHGFI_USEFILEATTRIBUTES,
        ),
        IconKey::File(path) => (
            wide_z(path.as_os_str()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Default::default(),
        ),
    };
    let mut info = SHFILEINFOW::default();
    // SAFETY: `target` is NUL-terminated and `info` a writable SHFILEINFOW whose
    // size is passed; both outlive the call.
    let ok = unsafe {
        SHGetFileInfoW(
            PCWSTR(target.as_ptr()),
            attributes,
            Some(&mut info),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON | extra,
        )
    };
    if ok == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let rgba = icon_rgba(info.hIcon);
    // SAFETY: SHGFI_ICON hands us ownership of the icon; it is destroyed once.
    let _ = unsafe { DestroyIcon(info.hIcon) };
    let (w, h, pixels) = rgba?;
    Some(encode_png(w, h, &pixels))
}

/// Pixels of an icon as straight-alpha RGBA.
fn icon_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let mut ii = ICONINFO::default();
    // SAFETY: `icon` is a valid icon and `ii` a writable ICONINFO.
    unsafe { GetIconInfo(icon, &mut ii) }.ok()?;
    let result = if ii.hbmColor.is_invalid() {
        None // monochrome icon: rare, skipped
    } else {
        bitmap_bgra(ii.hbmColor).map(|(w, h, mut bgra)| {
            // Old icons have no alpha channel: take it from the AND mask.
            if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0)
                && let Some((_, _, mask)) = bitmap_bgra(ii.hbmMask)
            {
                let masks = mask.as_chunks::<4>().0;
                for (p, m) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(masks) {
                    p[3] = if m[0] == 0 { 255 } else { 0 };
                }
            }
            for p in bgra.as_chunks_mut::<4>().0 {
                p.swap(0, 2); // BGRA -> RGBA
            }
            (w, h, bgra)
        })
    };
    // SAFETY: GetIconInfo created both bitmaps for us; each is deleted once
    // (deleting a null handle is harmless).
    unsafe {
        let _ = DeleteObject(HGDIOBJ(ii.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(ii.hbmMask.0));
    }
    result
}

/// A bitmap's pixels as 32-bit top-down BGRA (at most 1024 px a side).
/// Largest bitmap side read back (Quick Look asks the shell for up to 1920).
const MAX_SIDE: i32 = 4096;

pub(super) fn bitmap_bgra(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a writable BITMAP and its size is passed.
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some((&raw mut bm).cast()),
        )
    };
    if got == 0
        || bm.bmWidth <= 0
        || bm.bmHeight <= 0
        || bm.bmWidth > MAX_SIDE
        || bm.bmHeight > MAX_SIDE
    {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    // SAFETY: plain call creating a memory DC; deleted below.
    let dc = unsafe { CreateCompatibleDC(None) };
    // SAFETY: `pixels` holds exactly w*h 32-bit pixels as described by `bmi`;
    // both outlive the call; `dc` is a valid memory DC.
    let lines = unsafe {
        GetDIBits(
            dc,
            bitmap,
            0,
            h as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    // SAFETY: `dc` was created above and is deleted once.
    let _ = unsafe { DeleteDC(dc) };
    (lines == h).then_some((w as u32, h as u32, pixels))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn extracts_real_icons_on_an_sta_thread() {
        let (tx, rx) = channel();
        let service = start_icon_service(tx).unwrap();
        let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".into());
        let keys = [
            IconKey::Folder,
            IconKey::Extension("txt".into()),
            IconKey::Extension(String::new()),
            IconKey::File(std::path::Path::new(&windir).join("notepad.exe")),
        ];
        for k in &keys {
            service.request(k.clone());
        }
        for k in &keys {
            let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(&r.key, k);
            let png = r.png.unwrap_or_else(|| panic!("no icon for {k:?}"));
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            // Small icons are 16x16 at 100% (bigger at higher DPI).
            let w = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
            assert!((16..=64).contains(&w), "{k:?}: {w}");
        }
    }
}
