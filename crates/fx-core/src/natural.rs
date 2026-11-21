//! Explorer-like name comparison: case-insensitive, accent-insensitive and
//! "natural" (digit runs compare as numbers, so `a2` < `a10`). `StrCmpLogicalW`
//! is the behavioural reference (docs/ARCHITECTURE.md, "Desenho da lista").

use std::cmp::Ordering;

/// Appends the comparison key of `name` to `out`: lowercase, without diacritics
/// for Latin letters. Used for sorting and for "type to filter".
pub fn fold_into(name: &str, out: &mut String) {
    for c in name.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
            continue;
        }
        for lower in c.to_lowercase() {
            match strip_accent(lower) {
                Folded::One(f) => out.push(f),
                Folded::Two(a, b) => {
                    out.push(a);
                    out.push(b);
                }
            }
        }
    }
}

/// Comparison key of `name` (see [`fold_into`]).
pub fn fold(name: &str) -> String {
    let mut s = String::with_capacity(name.len());
    fold_into(name, &mut s);
    s
}

enum Folded {
    One(char),
    Two(char, char),
}

/// Lowercase Latin letter with diacritic -> base letter. Covers Latin-1
/// Supplement and Latin Extended-A, which include Portuguese, Spanish, French,
/// German, Nordic and Central European letters.
fn strip_accent(c: char) -> Folded {
    use Folded::{One, Two};
    let base = match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'æ' => return Two('a', 'e'),
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' | 'ħ' => 'h',
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'ĳ' => return Two('i', 'j'),
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' | 'ŉ' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
        'œ' => return Two('o', 'e'),
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ß' => return Two('s', 's'),
        'ţ' | 'ť' | 'ŧ' => 't',
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    };
    One(base)
}

/// Natural comparison of two *folded* keys. Digit runs (ASCII) compare by
/// numeric value; equal values with more leading zeros sort after.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (ai, bj) = (digit_run_end(a, i), digit_run_end(b, j));
            let ord = cmp_numbers(&a[i..ai], &b[j..bj]);
            if ord != Ordering::Equal {
                return ord;
            }
            i = ai;
            j = bj;
        } else {
            // Byte order on UTF-8 equals code point order, so non-digit runs can be
            // compared bytewise.
            if a[i] != b[j] {
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

fn digit_run_end(s: &[u8], from: usize) -> usize {
    from + s[from..].iter().take_while(|c| c.is_ascii_digit()).count()
}

fn cmp_numbers(a: &[u8], b: &[u8]) -> Ordering {
    let strip = |s: &[u8]| -> usize { s.iter().take_while(|&&c| c == b'0').count() };
    let (za, zb) = (strip(a), strip(b));
    let (va, vb) = (&a[za..], &b[zb..]);
    va.len()
        .cmp(&vb.len())
        .then_with(|| va.cmp(vb))
        .then_with(|| za.cmp(&zb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<&str> = names.to_vec();
        v.sort_by(|a, b| natural_cmp(&fold(a), &fold(b)).then_with(|| a.cmp(b)));
        v.into_iter().map(String::from).collect()
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(&["a10", "a2", "a1", "a100", "a20"]),
            ["a1", "a2", "a10", "a20", "a100"]
        );
        assert_eq!(
            sorted(&["file10.txt", "file2.txt", "file1.txt"]),
            ["file1.txt", "file2.txt", "file10.txt"]
        );
        assert_eq!(natural_cmp("x2y10", "x2y9"), Ordering::Greater);
        assert_eq!(natural_cmp("a01", "a1"), Ordering::Greater);
        assert_eq!(natural_cmp("a1", "a1"), Ordering::Equal);
        assert_eq!(
            natural_cmp("n99999999999999999999999", "n100000000000000000000000"),
            Ordering::Less
        );
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(fold("ReadMe.TXT"), "readme.txt");
        assert_eq!(sorted(&["b", "A", "a", "B"])[..2], ["A", "a"]);
        assert_eq!(natural_cmp(&fold("ABC"), &fold("abc")), Ordering::Equal);
    }

    #[test]
    fn accents_sort_with_base_letter() {
        assert_eq!(fold("Relatório Ação"), "relatorio acao");
        assert_eq!(fold("Straße Æble Œuvre"), "strasse aeble oeuvre");
        assert_eq!(
            sorted(&["zebra", "école", "eagle", "fable"]),
            ["eagle", "école", "fable", "zebra"]
        );
        // Non-Latin scripts are kept (lowercased where applicable).
        assert_eq!(fold("ΕΛΛΗΝΙΚΆ 日本"), "ελληνικά 日本");
    }

    #[test]
    fn prefix_sorts_first() {
        assert_eq!(sorted(&["abc", "ab", "a"]), ["a", "ab", "abc"]);
        assert_eq!(sorted(&["a1b", "a1"]), ["a1", "a1b"]);
    }
}
