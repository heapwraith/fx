//! The temporary name a copy is written to (`.name.fx-<pid>-<job>.tmp`),
//! renamed to the real name at the end. One definition for the copier
//! (fx-ops), the cleanup after a crash (its journal) and the lists, which
//! hide these files (review, D-066: there were two rules, one too loose).

use std::ffi::{OsStr, OsString};

/// Longest file name NTFS accepts, in UTF-16 units.
const MAX_NAME: usize = 255;

/// The temporary name for a copy to `name`. Always at most 255 UTF-16 units
/// (a long name is shortened), and built without converting `name` with loss:
/// a name that is not valid Unicode is not used at all (`_` stands in).
pub fn temp_name(name: &OsStr, pid: u32, job: u64) -> OsString {
    let suffix = format!(".fx-{pid}-{job}.tmp");
    // "." + stem + suffix; the suffix is ASCII (bytes = UTF-16 units).
    let room = MAX_NAME.saturating_sub(1 + suffix.len());
    let mut units = 0;
    let stem: String = name
        .to_str()
        .unwrap_or_default()
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= room
        })
        .collect();
    let stem = if stem.is_empty() { "_" } else { stem.as_str() };
    OsString::from(format!(".{stem}{suffix}"))
}

/// Whether `name` is one of those temporary names (exactly that shape, so a
/// user's `.notas.fx-backup.tmp` is not taken for one).
pub fn is_temp_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.').and_then(|r| r.strip_suffix(".tmp")) else {
        return false;
    };
    let Some(at) = rest.rfind(".fx-") else {
        return false;
    };
    let (stem, ids) = (&rest[..at], &rest[at + 4..]);
    let mut parts = ids.split('-');
    let digits =
        |p: Option<&str>| p.is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    !stem.is_empty() && digits(parts.next()) && digits(parts.next()) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_temporary_names_qualify() {
        for ok in [
            ".foto.jpg.fx-1234-5.tmp",
            ".a.fx-1-1.tmp",
            ".nome com espaço.txt.fx-99-12.tmp",
        ] {
            assert!(is_temp_name(ok), "{ok}");
        }
        for bad in [
            "foto.jpg",
            "foto.jpg.fx-1-2.tmp",
            ".foto.jpg.fx-1-2.txt",
            ".foto.jpg.fx-a-2.tmp",
            ".foto.jpg.fx-1-2-3.tmp",
            ".fx-1-2.tmp",
            ".foto.fx--2.tmp",
            ".notas.fx-backup.tmp",
            "relatorio.tmp",
            ".gitignore",
        ] {
            assert!(!is_temp_name(bad), "{bad}");
        }
    }

    #[test]
    fn names_fit_ntfs_and_are_recognized() {
        let short = temp_name(OsStr::new("foto.jpg"), 12, 3);
        assert_eq!(short, OsStr::new(".foto.jpg.fx-12-3.tmp"));
        // 250 characters, some outside the BMP (2 UTF-16 units each).
        let long: String = "😀".repeat(20) + &"a".repeat(230);
        let temp = temp_name(OsStr::new(&long), u32::MAX, u64::MAX);
        let text = temp.to_str().unwrap();
        assert!(text.encode_utf16().count() <= MAX_NAME, "{}", text.len());
        assert!(is_temp_name(text));
        assert!(text.starts_with(".😀"));
    }

    #[cfg(windows)]
    #[test]
    fn a_name_that_is_not_unicode_is_not_converted() {
        use std::os::windows::ffi::OsStringExt;
        // A lone surrogate: valid on NTFS, not Unicode.
        let odd = OsString::from_wide(&[0x61, 0xD800, 0x62]);
        let temp = temp_name(&odd, 1, 2);
        assert_eq!(temp, OsStr::new("._.fx-1-2.tmp"));
        assert!(is_temp_name(temp.to_str().unwrap()));
    }
}
