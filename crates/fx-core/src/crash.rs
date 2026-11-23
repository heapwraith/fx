//! Crash report text (task T3.10): what a panic leaves on disk, with no
//! personal data. Paths become `<path>`, quoted text (file names) becomes
//! `<text>`, the user name becomes `<user>`; the stack keeps only
//! function names. Pure: the UI crate gathers the inputs and writes the file.

/// Most stack frames kept.
const MAX_FRAMES: usize = 25;

/// Removes what could identify the user from `text`.
pub fn scrub(text: &str, user: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        // Quoted text: usually a file or folder name. An apostrophe inside a
        // word (can't, it's, Ana's) is not a quote: ' opens only after a
        // non-letter and closes only before one (review, D-066).
        let opens =
            matches!(c, '"' | '`') || (c == '\'' && (i == 0 || !chars[i - 1].is_alphanumeric()));
        let closes = |j: usize| {
            chars[j] == c && (c != '\'' || chars.get(j + 1).is_none_or(|d| !d.is_alphanumeric()))
        };
        if opens && let Some(end) = (i + 1..chars.len()).find(|&j| closes(j)) {
            out.push(c);
            out.push_str("<text>");
            out.push(c);
            i = end + 1;
            continue;
        }
        // C:\..., \\server\..., \\?\..., /a/b
        let drive = c.is_ascii_alphabetic()
            && next == Some(':')
            && matches!(chars.get(i + 2), Some('\\') | Some('/'))
            && (i == 0 || !chars[i - 1].is_alphanumeric());
        let unc = c == '\\' && next == Some('\\');
        let unix = c == '/'
            && (i == 0 || chars[i - 1].is_whitespace() || chars[i - 1] == '(')
            && chars[i + 1..]
                .iter()
                .take_while(|d| !d.is_whitespace())
                .filter(|&&d| d == '/')
                .count()
                >= 1;
        if drive || unc || unix {
            out.push_str("<path>");
            while i < chars.len() && !chars[i].is_whitespace() && !matches!(chars[i], ')' | ',') {
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    match user.filter(|u| u.chars().count() >= 2) {
        Some(u) => replace_ignoring_case(&out, u, "<user>"),
        None => out,
    }
}

fn replace_ignoring_case(text: &str, needle: &str, with: &str) -> String {
    let lower = text.to_lowercase();
    let needle = needle.to_lowercase();
    // Lower-casing can change byte lengths; fall back to an exact replace.
    if lower.len() != text.len() {
        return text.replace(&needle, with);
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (at, _) in lower.match_indices(&needle) {
        out.push_str(&text[last..at]);
        out.push_str(with);
        last = at + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

/// The source file of a panic, without the folders above the crate:
/// `crates\fx-ui\src\folder_view.rs` → `fx-ui/src/folder_view.rs`; a file
/// of a dependency in the cargo registry keeps `crate-x.y/src/file.rs`.
pub fn short_location(file: &str) -> String {
    let parts: Vec<&str> = file.split(['\\', '/']).filter(|p| !p.is_empty()).collect();
    match parts.iter().rposition(|p| *p == "src") {
        Some(src) if src > 0 => parts[src - 1..].join("/"),
        _ => parts.last().copied().unwrap_or("?").to_owned(),
    }
}

/// The function names of a `std::backtrace::Backtrace` text (the lines
/// `N: name`; the `at path:line` lines are dropped), without the panic
/// machinery, at most `MAX_FRAMES`.
pub fn frames(backtrace: &str) -> Vec<String> {
    backtrace
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let (num, name) = l.split_once(": ")?;
            num.parse::<u32>().ok()?;
            Some(name.trim().to_owned())
        })
        .filter(|name| {
            !(name.starts_with("std::")
                || name.starts_with("core::")
                || name.starts_with("alloc::")
                || name.contains("rust_begin_unwind")
                || name.contains("__rust")
                || name.starts_with("BaseThreadInitThunk")
                || name.starts_with("RtlUserThreadStart"))
        })
        .take(MAX_FRAMES)
        .collect()
}

#[derive(Debug)]
pub struct Panic<'a> {
    pub version: &'a str,
    pub message: &'a str,
    pub file: &'a str,
    pub line: u32,
    pub backtrace: &'a str,
    /// The Windows user name, to be removed wherever it appears.
    pub user: Option<&'a str>,
}

/// The report text.
pub fn report(p: &Panic<'_>) -> String {
    let location = short_location(p.file);
    let module = location.split('/').next().unwrap_or("?").replace('-', "_");
    let mut out = format!(
        "Pathdash {}\nmodule: {module}\nlocation: {location}:{}\nmessage: {}\nstack:\n",
        p.version,
        p.line,
        scrub(p.message, p.user)
    );
    for f in frames(p.backtrace) {
        out.push_str("  ");
        out.push_str(&scrub(&f, p.user));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKTRACE: &str = r"   0: std::backtrace::Backtrace::force_capture
             at /rustc/abc/library/std/src/backtrace.rs:331
   1: fx::log_panics::{{closure}}
             at C:\Users\ana.silva\projetos\fx\crates\fx-ui\src\main.rs:85
   2: std::panicking::rust_panic_with_hook
   3: fx::folder_view::FolderView::open_selected
             at C:\Users\ana.silva\projetos\fx\crates\fx-ui\src\folder_view.rs:1200
   4: gpui::window::Window::dispatch_event
             at C:\Users\ana.silva\.cargo\registry\src\gpui-0.2.2\src\window.rs:10
   5: BaseThreadInitThunk";

    #[test]
    fn no_user_name_or_paths() {
        let r = report(&Panic {
            version: "0.2.0-beta+abc123",
            message: r"falha ao abrir C:\Users\ana.silva\Documents\salário 2026.xlsx: 'salário 2026.xlsx' em \\servidor\ana.silva\x e /home/ana.silva/y (Ana.Silva)",
            file: r"crates\fx-ui\src\folder_view.rs",
            line: 1200,
            backtrace: BACKTRACE,
            user: Some("ana.silva"),
        });
        let lower = r.to_lowercase();
        assert!(!lower.contains("ana.silva"), "{r}");
        assert!(
            !r.contains("C:\\") && !r.contains("\\\\") && !r.contains("/home"),
            "{r}"
        );
        assert!(!r.contains("salário"), "file names removed: {r}");
        assert!(!r.contains("projetos"), "{r}");
        assert!(r.contains("Pathdash 0.2.0-beta+abc123"));
        assert!(r.contains("location: fx-ui/src/folder_view.rs:1200"), "{r}");
        assert!(r.contains("module: fx_ui"));
        assert!(r.contains("fx::folder_view::FolderView::open_selected"));
        assert!(r.contains("gpui::window::Window::dispatch_event"));
        assert!(!r.contains("std::panicking"), "panic machinery dropped");
        assert!(!r.contains("BaseThreadInitThunk"));
    }

    #[test]
    fn scrub_keeps_plain_text() {
        assert_eq!(
            scrub("index out of bounds: len 3, index 5", None),
            "index out of bounds: len 3, index 5"
        );
        assert_eq!(
            scrub("ratio 3/4", None),
            "ratio 3/4",
            "not a path: no leading slash"
        );
        assert_eq!(scrub("a (/tmp/x) b", None), "a (<path>) b");
        // Apostrophes in words are not quotes.
        assert_eq!(scrub("can't open 'x'", None), "can't open '<text>'");
        assert_eq!(scrub("doesn't exist", None), "doesn't exist");
        assert_eq!(
            scrub("file 'Ana's notes.txt' missing", None),
            "file '<text>' missing"
        );
        assert_eq!(scrub("say \"hi\" now", None), "say \"<text>\" now");
        assert_eq!(short_location("src/main.rs"), "main.rs");
        assert_eq!(
            short_location(r"C:\x\gpui-0.2.2\src\window.rs"),
            "gpui-0.2.2/src/window.rs"
        );
    }
}
