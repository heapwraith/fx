//! Renaming photos by the date they were taken (T6.16, D-068):
//! `2024-07-15 14.30.22.jpg`, like Lightroom and Windows Photos import.
//! Photos taken in the same second get ` (2)`, ` (3)`…; a name already in
//! the folder is never taken. Photos without a date, or already named so,
//! are left alone.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};

use crate::format::civil_from_days;

/// The name stem for a local time (Unix seconds already shifted to local).
pub fn stem_for(local_secs: i64) -> String {
    let days = local_secs.div_euclid(86_400);
    let rem = local_secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}.{:02}.{:02}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

/// The new name of each photo (`None`: left alone). `photos` are
/// `(current name, local time taken)`; `taken` says whether a name is
/// already used in the folder (case-insensitive, as on Windows).
pub fn plan(
    photos: &[(OsString, Option<i64>)],
    taken: impl Fn(&str) -> bool,
) -> Vec<Option<OsString>> {
    let own: HashSet<String> = photos
        .iter()
        .filter_map(|(n, _)| n.to_str().map(str::to_lowercase))
        .collect();
    let mut used: HashSet<String> = HashSet::new();
    photos
        .iter()
        .map(|(name, when)| {
            let when = (*when)?;
            let ext = std::path::Path::new(name)
                .extension()
                .map(OsStr::to_string_lossy)
                .map(|e| format!(".{}", e.to_lowercase()))
                .unwrap_or_default();
            let stem = stem_for(when);
            let current = name.to_string_lossy().to_lowercase();
            for n in 1u32.. {
                let candidate = if n == 1 {
                    format!("{stem}{ext}")
                } else {
                    format!("{stem} ({n}){ext}")
                };
                let key = candidate.to_lowercase();
                if key == current {
                    used.insert(key);
                    return None;
                }
                // A name of another selected photo counts as taken too: the
                // renames run one by one and never replace.
                if used.contains(&key) || own.contains(&key) || taken(&candidate) {
                    continue;
                }
                used.insert(key);
                return Some(OsString::from(candidate));
            }
            None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2024-07-15 14:30:22.
    const T: i64 = 1_721_053_822;

    #[test]
    fn names_by_date_without_clashes() {
        assert_eq!(stem_for(T), "2024-07-15 14.30.22");
        let photos = vec![
            (OsString::from("IMG_001.JPG"), Some(T)),
            (OsString::from("IMG_002.jpg"), Some(T)),
            (OsString::from("sem-data.png"), None),
            (OsString::from("2024-07-15 14.30.23.jpg"), Some(T + 1)),
            (OsString::from("IMG_003.heic"), Some(T + 60)),
        ];
        let got = plan(&photos, |n| n == "2024-07-15 14.31.22.heic");
        assert_eq!(
            got,
            vec![
                Some("2024-07-15 14.30.22.jpg".into()),
                Some("2024-07-15 14.30.22 (2).jpg".into()),
                None,
                None,
                Some("2024-07-15 14.31.22 (2).heic".into()),
            ]
        );
    }
}
