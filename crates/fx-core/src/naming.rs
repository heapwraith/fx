//! Names for a new item next to one that already exists ("keep both").

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Free name in `dir` for `name`: `a.txt`, `a (2).txt`, `a (3).txt`... Built
/// without converting the name with loss: one that is not valid Unicode gets
/// the number at the end (`name (2)`), keeping every unit it had.
pub fn keep_both_name(
    dir: &Path,
    name: impl AsRef<OsStr>,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    let name = name.as_ref();
    let numbered = |n: u32| -> OsString {
        match name.to_str() {
            Some(text) => {
                let (stem, ext) = match text.rfind('.') {
                    Some(i) if i > 0 => (&text[..i], &text[i..]),
                    _ => (text, ""),
                };
                OsString::from(format!("{stem} ({n}){ext}"))
            }
            None => {
                let mut s = name.to_os_string();
                s.push(format!(" ({n})"));
                s
            }
        }
    };
    (2..)
        .map(|n| dir.join(numbered(n)))
        .find(|p| !exists(p))
        .unwrap_or_else(|| dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_before_the_extension() {
        let d = Path::new(r"C:\d");
        let taken = |p: &Path| p.ends_with("a (2).txt") || p.ends_with("pasta (2)");
        assert_eq!(keep_both_name(d, "a.txt", taken), d.join("a (3).txt"));
        assert_eq!(keep_both_name(d, "pasta", taken), d.join("pasta (3)"));
        assert_eq!(
            keep_both_name(d, ".gitignore", |_| false),
            d.join(".gitignore (2)")
        );
        assert_eq!(
            keep_both_name(d, "b.tar.gz", |_| false),
            d.join("b.tar (2).gz")
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_name_that_is_not_unicode_keeps_its_units() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let odd = OsString::from_wide(&[0x61, 0xD800, 0x2E, 0x62]);
        let got = keep_both_name(Path::new(r"C:\d"), &odd, |_| false);
        let units: Vec<u16> = got.file_name().unwrap().encode_wide().collect();
        let mut want: Vec<u16> = odd.encode_wide().collect();
        want.extend(" (2)".encode_utf16());
        assert_eq!(units, want);
    }
}
