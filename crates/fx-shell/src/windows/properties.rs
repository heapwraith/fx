//! Properties of an item for the details panel (T6.9, D-068): the Windows
//! Property System names and formats each value in the system language
//! ("1920 x 1080", "00:03:25"), exactly as Explorer shows them. Read on the
//! caller's thread (a worker), never the UI thread.

use std::io;
use std::path::Path;

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree};
use windows::Win32::UI::Shell::PropertiesSystem::{
    GPS_BESTEFFORT, GPS_OPENSLOWITEM, IPropertyDescription, IPropertyStore, PDFF_DEFAULT,
    PSFormatPropertyValue, PSGetPropertyDescriptionByName, SHGetPropertyStoreFromParsingName,
};
use windows::core::{HSTRING, Interface, PWSTR};

/// The properties shown, by canonical name, in this order.
const NAMES: [&str; 18] = [
    "System.ItemTypeText",
    "System.Size",
    "System.DateCreated",
    "System.DateModified",
    "System.Image.Dimensions",
    "System.Photo.DateTaken",
    "System.Photo.CameraModel",
    "System.Media.Duration",
    "System.Video.FrameRate",
    "System.Audio.EncodingBitrate",
    "System.Music.Artist",
    "System.Music.AlbumTitle",
    "System.Title",
    "System.Author",
    "System.Document.PageCount",
    "System.Company",
    "System.FileVersion",
    "System.FileAttributes",
];

/// `(name, value)` of every property of `path` that has a value.
pub fn item_properties(path: &Path) -> io::Result<Vec<(String, String)>> {
    // SAFETY: COM for this (worker) thread; a second call is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // SAFETY: the path is a NUL-terminated string kept alive by HSTRING.
    let store: IPropertyStore = unsafe {
        SHGetPropertyStoreFromParsingName(
            &HSTRING::from(path.as_os_str()),
            None,
            GPS_BESTEFFORT | GPS_OPENSLOWITEM,
        )
    }
    .map_err(io::Error::from)?;
    let mut out = Vec::new();
    for name in NAMES {
        if let Some(pair) = one(&store, name) {
            out.push(pair);
        }
    }
    Ok(out)
}

/// The formatted values of `names` for a list column (T6.10), in that
/// order. Never opens a slow item (a cloud file is not downloaded for it).
pub fn property_values(path: &Path, names: &[&str]) -> io::Result<Vec<Option<String>>> {
    // SAFETY: COM for this (worker) thread; a second call is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // SAFETY: the path is a NUL-terminated string kept alive by HSTRING.
    let store: IPropertyStore = unsafe {
        SHGetPropertyStoreFromParsingName(&HSTRING::from(path.as_os_str()), None, GPS_BESTEFFORT)
    }
    .map_err(io::Error::from)?;
    Ok(names
        .iter()
        .map(|name| one(&store, name).map(|(_, value)| value))
        .collect())
}

/// When a photo was taken (`System.Photo.DateTaken`, UTC), for renaming by
/// date (T6.16). `None` when the file has no such date.
pub fn photo_taken(path: &Path) -> io::Result<Option<std::time::SystemTime>> {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::UI::Shell::PropertiesSystem::PSGetPropertyKeyFromName;
    use windows::Win32::UI::Shell::{IShellItem2, SHCreateItemFromParsingName};
    // SAFETY: COM for this (worker) thread; a second call is harmless.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    // SAFETY: the path is a NUL-terminated string kept alive by HSTRING.
    let item: IShellItem2 =
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None) }
            .map_err(io::Error::from)?;
    let mut key = PROPERTYKEY::default();
    // SAFETY: a NUL-terminated name and a valid out pointer.
    unsafe { PSGetPropertyKeyFromName(&HSTRING::from("System.Photo.DateTaken"), &mut key) }
        .map_err(io::Error::from)?;
    // SAFETY: `key` is a valid PROPERTYKEY for the call.
    let Ok(ft) = (unsafe { item.GetFileTime(&key) }) else {
        return Ok(None);
    };
    let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
    // FILETIME: 100 ns since 1601; Unix epoch is 11 644 473 600 s later.
    let secs = (ticks / 10_000_000).checked_sub(11_644_473_600);
    Ok(secs
        .filter(|s| *s > 0)
        .map(|s| std::time::UNIX_EPOCH + std::time::Duration::from_secs(s)))
}

fn one(store: &IPropertyStore, name: &str) -> Option<(String, String)> {
    let mut raw = std::ptr::null_mut();
    // SAFETY: the name is NUL-terminated (HSTRING); `raw` receives an
    // IPropertyDescription reference, owned by `desc` below.
    unsafe {
        PSGetPropertyDescriptionByName(&HSTRING::from(name), &IPropertyDescription::IID, &mut raw)
    }
    .ok()?;
    // SAFETY: a valid interface pointer from the call above, owned once.
    let desc = unsafe { IPropertyDescription::from_raw(raw) };
    // SAFETY: both interfaces are valid; the strings returned are
    // CoTaskMemAlloc'ed and freed by `take`.
    let value = take(unsafe { PSFormatPropertyValue(store, &desc, PDFF_DEFAULT) }.ok()?);
    // SAFETY: as above.
    let label = take(unsafe { desc.GetDisplayName() }.ok()?);
    let value = value
        .trim()
        .trim_matches('\u{200e}')
        .trim_matches('\u{200f}')
        .to_owned();
    (!value.is_empty() && !label.is_empty()).then_some((label, value))
}

/// The text of a COM string, freed.
fn take(s: PWSTR) -> String {
    let text = if s.is_null() {
        String::new()
    } else {
        // SAFETY: `s` is a valid NUL-terminated string from COM.
        unsafe { s.to_string() }.unwrap_or_default()
    };
    // SAFETY: allocated with CoTaskMemAlloc by the API, freed once.
    unsafe { CoTaskMemFree(Some(s.0.cast_const().cast())) };
    text
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_file_has_its_size_and_dates() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("notas.txt");
        std::fs::write(&f, b"0123456789").unwrap();
        let props = super::item_properties(&f).unwrap();
        assert!(props.len() >= 3, "{props:?}");
        assert!(props.iter().any(|(_, v)| v.contains("10")), "{props:?}");
        assert!(super::item_properties(&dir.path().join("nada")).is_err());
        let values = super::property_values(&f, &["System.Size", "System.Media.Duration"]).unwrap();
        assert!(
            values[0].as_deref().is_some_and(|v| v.contains("10")),
            "{values:?}"
        );
        assert_eq!(values[1], None, "a text file has no duration");
    }

    /// A JPEG made by WIC with an EXIF block written by hand
    /// (DateTimeOriginal 2024:07:15 14:30:22, local time).
    #[test]
    fn photo_date_taken_is_read() {
        use crate::windows::images::{ImageKind, convert_image};
        let dir = tempfile::tempdir().unwrap();
        let bmp = dir.path().join("x.bmp");
        let mut b = Vec::new();
        b.extend_from_slice(b"BM");
        b.extend_from_slice(&(54u32 + 32).to_le_bytes());
        b.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&[1, 0, 24, 0]);
        b.extend_from_slice(&[0; 24]);
        b.extend(std::iter::repeat_n(0x80u8, 32));
        std::fs::write(&bmp, &b).unwrap();
        let plain = dir.path().join("plain.jpg");
        convert_image(&bmp, &plain, ImageKind::Jpeg, 1.0).unwrap();
        assert_eq!(super::photo_taken(&plain).unwrap(), None);
        // TIFF (little endian): IFD0 with the Exif IFD pointer, then the
        // Exif IFD with DateTimeOriginal (ASCII, 20 bytes).
        let mut t: Vec<u8> = b"II*\0".to_vec();
        t.extend_from_slice(&8u32.to_le_bytes());
        let entry = |tag: u16, kind: u16, count: u32, value: u32| {
            let mut e = Vec::new();
            e.extend_from_slice(&tag.to_le_bytes());
            e.extend_from_slice(&kind.to_le_bytes());
            e.extend_from_slice(&count.to_le_bytes());
            e.extend_from_slice(&value.to_le_bytes());
            e
        };
        // IFD0 at 8: 1 entry (2 + 12 + 4 = 18 bytes) → Exif IFD at 26.
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend(entry(0x8769, 4, 1, 26));
        t.extend_from_slice(&0u32.to_le_bytes());
        // Exif IFD at 26: 1 entry → the string at 44.
        t.extend_from_slice(&1u16.to_le_bytes());
        t.extend(entry(0x9003, 2, 20, 44));
        t.extend_from_slice(&0u32.to_le_bytes());
        t.extend_from_slice(b"2024:07:15 14:30:22\0");
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(t);
        let jpeg = std::fs::read(&plain).unwrap();
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        out.extend(app1);
        out.extend_from_slice(&jpeg[2..]);
        let photo = dir.path().join("foto.jpg");
        std::fs::write(&photo, out).unwrap();
        let taken = super::photo_taken(&photo).unwrap().expect("a date taken");
        let secs = taken
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let local = secs + crate::utc_offset_at(taken);
        assert_eq!(local, 1_721_053_822, "2024-07-15 14:30:22 local");
    }
}
