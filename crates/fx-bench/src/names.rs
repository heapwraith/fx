//! Deterministic, Windows-valid file names for fixtures.
//!
//! Every name embeds its index, so names are unique even under Windows'
//! case-insensitive comparison. Fragments never contain characters forbidden by
//! Windows (`<>:"/\|?*`, control chars), and names never end in a dot or space,
//! nor collide with reserved device names (CON, NUL, COM1...), because they always
//! contain a `-` followed by digits.

use crate::rng::Rng;

const ASCII_WORDS: &[&str] = &[
    "report", "IMG", "backup", "Notes", "data", "final", "draft", "invoice", "song", "clip",
    "photo", "scan", "log", "README", "budget", "v2", "copy", "new", "old", "temp",
];

const UNICODE_WORDS: &[&str] = &[
    "relatório",
    "ação",
    "café",
    "naïve",
    "Ærø",
    "straße",
    "Ελληνικά",
    "русский",
    "日本語",
    "中文文件",
    "한국어",
    "עברית",
    "العربية",
    "📁",
    "🎵音乐",
    "😀",
    "👨‍👩‍👧",
];

const EXTENSIONS: &[&str] = &[
    "txt", "pdf", "docx", "xlsx", "jpg", "png", "mp3", "mp4", "zip", "rs", "json", "md", "exe",
    "dll", "tar.gz", "",
];

/// Name for item `index` of a flat fixture: numbers, Unicode and mixed extensions.
pub fn flat_name(rng: &mut Rng, index: usize) -> String {
    let word = if rng.below(3) == 0 {
        *rng.pick(UNICODE_WORDS)
    } else {
        *rng.pick(ASCII_WORDS)
    };
    let ext = *rng.pick(EXTENSIONS);
    let mut name = match rng.below(4) {
        0 => format!("{index:06}"),
        1 => format!("{word}-{index}"),
        2 => format!("{word} ({}) -{index}", rng.below(100)),
        _ => format!("{word}_{}-{index:06}", rng.below(10_000)),
    };
    if !ext.is_empty() {
        name.push('.');
        name.push_str(ext);
    }
    name
}

/// Unicode-heavy name (accents, emoji, CJK) for item `index`.
pub fn unicode_name(rng: &mut Rng, index: usize) -> String {
    let a = *rng.pick(UNICODE_WORDS);
    let b = *rng.pick(UNICODE_WORDS);
    format!("{a} {b}-{index}.{}", rng.pick(&["txt", "md", "jpg"]))
}

/// Directory name with `target_units` UTF-16 code units (approximately, never more
/// than `target_units`), used to build paths longer than 260 characters.
/// `target_units` must stay well below 255, the NTFS component limit.
pub fn long_dir_name(rng: &mut Rng, index: usize, target_units: usize) -> String {
    let mut name = format!("long-{index}-");
    loop {
        let word = *rng.pick(UNICODE_WORDS);
        if name.encode_utf16().count() + word.encode_utf16().count() + 1 > target_units {
            break;
        }
        name.push_str(word);
        name.push('_');
    }
    name.push('x');
    name
}

/// True if `name` is a valid Windows file name component.
#[cfg(test)]
pub fn is_valid_windows_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    !name.is_empty()
        && name.encode_utf16().count() <= 255
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        && !RESERVED.contains(&stem.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::FIXTURE_SEED;
    use std::collections::HashSet;

    fn flat_names(n: usize) -> Vec<String> {
        let mut rng = Rng::new(FIXTURE_SEED);
        (0..n).map(|i| flat_name(&mut rng, i)).collect()
    }

    #[test]
    fn same_seed_same_names() {
        assert_eq!(flat_names(1000), flat_names(1000));
    }

    #[test]
    fn names_are_valid_and_unique_case_insensitively() {
        let names = flat_names(20_000);
        let mut seen = HashSet::new();
        for n in &names {
            assert!(is_valid_windows_name(n), "invalid: {n:?}");
            assert!(seen.insert(n.to_lowercase()), "duplicate: {n:?}");
        }
        let mut rng = Rng::new(1);
        for i in 0..500 {
            let n = unicode_name(&mut rng, i);
            assert!(is_valid_windows_name(&n), "invalid: {n:?}");
        }
    }

    #[test]
    fn flat_names_mix_numbers_unicode_and_extensions() {
        let names = flat_names(1000);
        assert!(names.iter().any(|n| !n.is_ascii()));
        assert!(names.iter().any(|n| n.ends_with(".tar.gz")));
        assert!(names.iter().any(|n| !n.contains('.')));
        assert!(
            names
                .iter()
                .any(|n| n.chars().all(|c| c.is_ascii_digit() || c == '.'))
        );
    }

    #[test]
    fn long_dir_name_respects_limit() {
        let mut rng = Rng::new(7);
        let n = long_dir_name(&mut rng, 3, 120);
        let units = n.encode_utf16().count();
        assert!((100..=120).contains(&units), "{units}");
        assert!(is_valid_windows_name(&n));
    }

    #[test]
    fn validator_rejects_bad_names() {
        for bad in [
            "",
            "a:b",
            "x?",
            "con",
            "NUL.txt",
            "trailing.",
            "space ",
            "a\u{1}",
        ] {
            assert!(!is_valid_windows_name(bad), "{bad:?}");
        }
    }
}
