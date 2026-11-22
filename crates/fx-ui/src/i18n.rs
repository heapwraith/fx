//! Interface language (task T3.7). The texts themselves are pairs at the
//! place they are used (`fx_core::tr!` / `fx_core::trf!`, D-053), so the
//! compiler refuses a text without its English version; this module picks
//! the language and holds the test that no interface text escapes the pairs.
//!
//! Order: `idioma` in `settings.toml` ("pt"/"en"), then `FX_LANG`, then the
//! Windows display language (Portuguese → Portuguese, anything else →
//! English). The palette switches it live; the choice is saved with the
//! session (`persist.rs`).

use std::sync::Mutex;

use fx_core::locale::{english, set_english};

/// The language picked in the palette this session, to be saved.
static CHOSEN: Mutex<Option<&'static str>> = Mutex::new(None);

fn parse(value: &str) -> Option<bool> {
    match value.trim().to_lowercase().as_str() {
        "pt" | "pt-br" | "português" | "portugues" => Some(false),
        "en" | "en-us" | "english" | "inglês" | "ingles" => Some(true),
        _ => None,
    }
}

/// Sets the language at startup (before any window).
pub fn init(saved: &str) {
    let en = parse(saved)
        .or_else(|| std::env::var("FX_LANG").ok().and_then(|v| parse(&v)))
        .unwrap_or_else(|| !fx_shell::system_language_is_portuguese());
    set_english(en);
}

/// Palette: Portuguese ↔ English.
pub fn switch() {
    let en = !english();
    set_english(en);
    if let Ok(mut c) = CHOSEN.lock() {
        *c = Some(if en { "en" } else { "pt" });
    }
}

/// What to save in `idioma`, if the user switched this session.
pub fn chosen() -> Option<&'static str> {
    CHOSEN.lock().ok().and_then(|c| *c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn language_names() {
        assert_eq!(parse("pt"), Some(false));
        assert_eq!(parse(" EN "), Some(true));
        assert_eq!(parse("English"), Some(true));
        assert_eq!(parse(""), None);
        assert_eq!(parse("fr"), None);
    }

    /// Literals that look Portuguese but are not interface text: values
    /// stored in `settings.toml` or read from the environment, and names of
    /// things that are the same in both languages.
    const NOT_INTERFACE: &[&str] = &[
        "padrão",
        "confortável",
        "compacta",
        "português",
        "portugues",
        "inglês",
        "ingles",
    ];

    fn looks_portuguese(text: &str) -> bool {
        // SQL is not interface text.
        if ["SELECT ", "INSERT ", "CREATE ", "UPDATE ", "DELETE "]
            .iter()
            .any(|k| text.contains(k))
        {
            return false;
        }
        const WORDS: &[&str] = &[
            " de ", " da ", " do ", " para ", " não ", " um ", " uma ", " com ", " sem ", " na ",
            " em ", " os ", " é ",
        ];
        // Character tables (the CP437 map of the zip reader) are not text.
        if !text.contains(' ') && text.chars().filter(|c| !c.is_ascii()).count() > 20 {
            return false;
        }
        let accents = text.chars().any(|c| "ãõçáéíóúâêôàÃÕÇÁÉÍÓÚÂÊÔ".contains(c));
        let padded = format!(" {} ", text.to_lowercase());
        // Single words count only with accents ("as", "do" are also code).
        accents || text.trim().contains(' ') && WORDS.iter().any(|w| padded.contains(w))
    }

    /// The literals of a source file, outside comments and the test module,
    /// each with whether it is the first half of a pair: `tr!("pt", "en")`,
    /// `trf!("pt", "en", ...)` or `("pt", "en")` (palette commands).
    fn literals(src: &str) -> Vec<(usize, String, bool)> {
        // Cut at the test module (`#[cfg(test)]`, `#[cfg(all(test, windows))]`).
        let cut = src
            .match_indices("\n#[cfg(")
            .find(|(at, _)| {
                let rest = &src[at + 1..];
                let line = rest.lines().next().unwrap_or("");
                line.contains("test")
                    && rest
                        .lines()
                        .nth(1)
                        .is_some_and(|l| l.starts_with("mod tests"))
            })
            .map_or(src.len(), |(at, _)| at);
        let src = &src[..cut];
        let bytes = src.as_bytes();
        let mut out = Vec::new();
        let (mut i, mut line) = (0, 1);
        while i < bytes.len() {
            match bytes[i] {
                b'\n' => line += 1,
                b'/' if bytes.get(i + 1) == Some(&b'/') => {
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                b'\'' if bytes.get(i + 2) == Some(&b'\'') => i += 2, // '"'
                b'\'' if bytes.get(i + 1) == Some(&b'\\') => {
                    while i + 1 < bytes.len() && bytes[i + 1] != b'\'' {
                        i += 1;
                    }
                    i += 1;
                }
                // Raw strings: r"…", r#"…"# (no escapes inside).
                b'r' if (i == 0
                    || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_')
                    && matches!(bytes.get(i + 1), Some(b'"' | b'#')) =>
                {
                    let mut j = i + 1;
                    let mut hashes = 0;
                    while bytes.get(j) == Some(&b'#') {
                        hashes += 1;
                        j += 1;
                    }
                    if bytes.get(j) != Some(&b'"') {
                        i += 1;
                        continue;
                    }
                    let start = j + 1;
                    let close = format!("\"{}", "#".repeat(hashes));
                    let end = src[start..].find(&close).map_or(src.len(), |e| start + e);
                    let text = &src[start..end];
                    line += text.matches('\n').count();
                    out.push((line, text.to_owned(), false));
                    i = end + close.len();
                    continue;
                }
                b'"' => {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && bytes[i] != b'"' {
                        if bytes[i] == b'\\' {
                            i += 1;
                        }
                        if bytes.get(i) == Some(&b'\n') {
                            line += 1;
                        }
                        i += 1;
                    }
                    let text = &src[start + 1..i.min(src.len())];
                    let before = src[..start].trim_end();
                    let after = src[(i + 1).min(src.len())..].trim_start();
                    let opens = before.ends_with("tr!(")
                        || before.ends_with("trf!(")
                        || before.ends_with('(');
                    let pair =
                        opens && after.starts_with(',') && after[1..].trim_start().starts_with('"');
                    out.push((line, text.to_owned(), pair));
                }
                _ => {}
            }
            i += 1;
        }
        out
    }

    fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                sources(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs")
                && p.file_name().is_none_or(|n| n != "tests.rs")
            {
                out.push(p);
            }
        }
    }

    #[test]
    fn no_interface_text_outside_the_pairs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut files = Vec::new();
        sources(&root, &mut files);
        // Libraries whose messages reach the interface (T4.2).
        for lib in ["fx-core", "fx-ops", "fx-shell", "fx-fs"] {
            sources(&crates.join(lib).join("src"), &mut files);
        }
        assert!(files.len() > 20, "sources found: {}", files.len());
        let mut loose = Vec::new();
        for f in &files {
            // A Windows checkout (CI) may have CRLF line endings.
            let src = std::fs::read_to_string(f).unwrap().replace("\r\n", "\n");
            let lits = literals(&src);
            for (k, (line, text, pair)) in lits.iter().enumerate() {
                // The English half of a pair is the literal right after one.
                let english_half = k > 0 && lits[k - 1].2;
                if *pair || english_half || NOT_INTERFACE.contains(&text.as_str()) {
                    continue;
                }
                if looks_portuguese(text) {
                    let name = f.strip_prefix(&crates).unwrap_or(f).display();
                    loose.push(format!("{name}:{line}: \"{text}\""));
                }
            }
        }
        assert!(
            loose.is_empty(),
            "texts outside tr!/trf!:\n{}",
            loose.join("\n")
        );
    }

    #[test]
    fn crlf_sources_skip_the_test_module_too() {
        let src =
            "let a = 1;\r\n#[cfg(test)]\r\nmod tests {\r\n    let b = \"relatório\";\r\n}\r\n"
                .replace("\r\n", "\n");
        assert!(literals(&src).is_empty());
    }

    #[test]
    fn the_scanner_sees_pairs_and_loose_texts() {
        let src = r#"let a = tr!("Copiar", "Copy"); let b = "Nova pasta de testes"; let c = trf!(
            "{} de {}", "{} of {}", 1, 2);"#;
        let lits = literals(src);
        assert!(lits[0].2 && !lits[1].2, "pair and its English half");
        assert!(!lits[2].2 && looks_portuguese(&lits[2].1), "loose text");
        assert!(lits[3].2, "multi-line trf!");
    }
}
