//! Archives as folders (T6.7, D-068): which files are archives, the items
//! of a folder inside a ZIP (from its central directory, with the folders
//! that only exist as prefixes), and which entries to extract for a
//! selection. A name with a part that could escape the folder it is
//! extracted to (`..`, a drive, a separator) is never listed nor extracted.

use std::collections::BTreeMap;

use crate::zip_list::ZipEntry;

/// Whether `name` is an archive the fx opens as a folder.
pub fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    const ENDINGS: [&str; 9] = [
        ".zip", ".7z", ".tar", ".tgz", ".tbz2", ".txz", ".tar.gz", ".tar.bz2", ".tar.xz",
    ];
    ENDINGS
        .iter()
        .any(|e| lower.len() > e.len() && lower.ends_with(e))
}

/// Whether a name listed inside an archive is a plain item name: not empty,
/// not `.` or `..`, no separators, no drive or stream colon, no control
/// characters. Anything else is left out of the listing, so it can never
/// be part of a path built from it.
pub fn is_safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|c| matches!(c, '/' | '\\' | ':' | '\0') || c.is_control())
}

/// Whether `name` is a ZIP (browsed as a folder; other archives are
/// extracted whole).
pub fn is_zip(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.len() > 4 && lower.ends_with(".zip")
}

/// The parts of an entry's path, if every part is safe.
fn parts(name: &str) -> Option<Vec<&str>> {
    let p: Vec<&str> = name.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    (!p.is_empty() && p.iter().all(|s| is_safe_name(s))).then_some(p)
}

/// One item of a folder inside a ZIP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZipItem {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// MS-DOS date and time (0 for folders known only by their contents).
    pub dos_time: u32,
}

/// The items directly inside the folder `inner` (parts below the archive,
/// empty for the top level), sorted by name. Unsafe entries are skipped.
pub fn zip_children(entries: &[ZipEntry], inner: &[&str]) -> Vec<ZipItem> {
    let mut out: BTreeMap<String, ZipItem> = BTreeMap::new();
    for e in entries {
        let Some(p) = parts(&e.name) else { continue };
        if p.len() <= inner.len() || !p.iter().zip(inner).all(|(a, b)| a == b) {
            continue;
        }
        let name = p[inner.len()];
        let direct_file = p.len() == inner.len() + 1 && !e.is_dir;
        let item = out.entry(name.to_owned()).or_insert(ZipItem {
            name: name.to_owned(),
            is_dir: !direct_file,
            size: 0,
            dos_time: 0,
        });
        if p.len() == inner.len() + 1 {
            item.dos_time = e.dos_time;
            if direct_file {
                item.size = e.size;
            }
        }
    }
    out.into_values().collect()
}

/// Entries to extract for the items `selected` of the folder `inner`:
/// `(entry index, path parts below inner)`; folders bring everything in
/// them. Unsafe entries are skipped.
pub fn zip_plan(
    entries: &[ZipEntry],
    inner: &[&str],
    selected: &[&str],
) -> Vec<(usize, Vec<String>)> {
    entries
        .iter()
        .enumerate()
        .filter_map(|(k, e)| {
            let p = parts(&e.name)?;
            if p.len() <= inner.len() || !p.iter().zip(inner).all(|(a, b)| a == b) {
                return None;
            }
            if !selected.contains(&p[inner.len()]) {
                return None;
            }
            Some((
                k,
                p[inner.len()..].iter().map(|s| (*s).to_owned()).collect(),
            ))
        })
        .collect()
}

/// Local Unix seconds of an MS-DOS date and time (0 if unset).
pub fn dos_to_local_secs(dos: u32) -> i64 {
    if dos == 0 {
        return 0;
    }
    let (date, time) = (dos >> 16, dos & 0xFFFF);
    let year = 1980 + i64::from(date >> 9);
    let month = i64::from((date >> 5) & 0xF).clamp(1, 12);
    let day = i64::from(date & 0x1F).max(1);
    // Days from civil (Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400
        + i64::from(time >> 11) * 3600
        + i64::from((time >> 5) & 0x3F) * 60
        + i64::from(time & 0x1F) * 2
}

/// The MS-DOS date and time of local Unix seconds (1980-01-01 for earlier
/// instants, 2107-12-31 for later ones; seconds rounded down to even).
pub fn local_secs_to_dos(local: i64) -> u32 {
    let (y, m, d) = crate::format::civil_from_days(local.div_euclid(86_400));
    if y < 1980 {
        return ((1 << 5) | 1) << 16;
    }
    if y > 2107 {
        return (((127 << 9) | (12 << 5) | 31) << 16) | ((23 << 11) | (59 << 5) | 29);
    }
    let t = local.rem_euclid(86_400) as u32;
    let date = (((y - 1980) as u32) << 9) | (m << 5) | d;
    let time = ((t / 3600) << 11) | ((t / 60 % 60) << 5) | ((t % 60) / 2);
    (date << 16) | time
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, size: u64) -> ZipEntry {
        ZipEntry {
            name: name.to_owned(),
            is_dir: name.ends_with('/'),
            size,
            compressed: size,
            dos_time: 0x5AEF_73AB,
        }
    }

    #[test]
    fn folders_inside_a_zip_and_what_to_extract() {
        let entries = vec![
            e("a.txt", 1),
            e("docs/", 0),
            e("docs/b.md", 2),
            e("fotos/2024/x.jpg", 3),
            e("../fora.txt", 4),
            e("docs/../../mau.txt", 5),
            e("C:/win.ini", 6),
        ];
        let top = zip_children(&entries, &[]);
        let names: Vec<&str> = top.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a.txt", "docs", "fotos"]);
        assert!(!top[0].is_dir && top[1].is_dir && top[2].is_dir);
        assert_eq!(top[0].size, 1);
        let fotos = zip_children(&entries, &["fotos"]);
        assert_eq!(fotos.len(), 1);
        assert!(fotos[0].is_dir, "2024 exists only as a prefix");
        assert_eq!(
            zip_plan(&entries, &[], &["docs", "a.txt"]),
            vec![
                (0, vec!["a.txt".to_owned()]),
                (1, vec!["docs".to_owned()]),
                (2, vec!["docs".to_owned(), "b.md".to_owned()]),
            ]
        );
        assert_eq!(
            zip_plan(&entries, &["fotos"], &["2024"]),
            vec![(3, vec!["2024".to_owned(), "x.jpg".to_owned()])]
        );
        // 2025-07-15 14:29:22 → 0x5AEF (date) 0x73AB (time).
        assert_eq!(dos_to_local_secs(0x5AEF_73AB), 1_752_589_762);
        assert_eq!(local_secs_to_dos(1_752_589_762), 0x5AEF_73AB);
        assert_eq!(local_secs_to_dos(1_752_589_763), 0x5AEF_73AB, "odd second");
        assert_eq!(dos_to_local_secs(local_secs_to_dos(0)), 315_532_800, "1980");
    }

    #[test]
    fn archives_and_safe_names() {
        assert!(is_archive("Fotos.ZIP"));
        assert!(is_archive("src.tar.gz"));
        assert!(is_archive("x.7z"));
        assert!(!is_archive(".zip"));
        assert!(!is_archive("notas.gz"));
        assert!(!is_archive("x.rar"), "no free Rust reader");
        assert!(!is_archive("relatorio.pdf"));
        assert!(is_safe_name("ação 日本.txt"));
        for bad in [
            "", ".", "..", "../x", r"..\x", "C:x", "a/b", "a\u{1}b", "x:stream",
        ] {
            assert!(!is_safe_name(bad), "{bad:?}");
        }
    }
}
