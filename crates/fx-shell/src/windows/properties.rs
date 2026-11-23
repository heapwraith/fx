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
}
