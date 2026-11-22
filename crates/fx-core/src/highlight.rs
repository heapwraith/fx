//! Syntax colors for Quick Look (task T2.9): a small tokenizer per language
//! family (keywords, strings, comments, numbers), enough to read code at a
//! glance. Not a parser: unusual syntax just shows as plain text.

use std::ops::Range;

/// What a piece of a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keyword,
    String,
    Comment,
    Number,
}

/// A language family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lang {
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    keywords: &'static [&'static str],
    /// Quote characters that start strings.
    quotes: &'static [char],
}

const RUST: Lang = Lang {
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while", "Some", "None", "Ok", "Err",
    ],
    quotes: &['"'],
};

const C_LIKE: Lang = Lang {
    line_comments: &["//"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "abstract",
        "async",
        "await",
        "bool",
        "break",
        "case",
        "catch",
        "char",
        "class",
        "const",
        "continue",
        "default",
        "delete",
        "do",
        "double",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "final",
        "finally",
        "float",
        "for",
        "func",
        "function",
        "go",
        "if",
        "implements",
        "import",
        "int",
        "interface",
        "let",
        "long",
        "namespace",
        "new",
        "null",
        "nullptr",
        "package",
        "private",
        "protected",
        "public",
        "return",
        "short",
        "static",
        "struct",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "using",
        "var",
        "void",
        "while",
        "yield",
        "string",
        "auto",
        "unsigned",
        "include",
        "define",
        "type",
        "chan",
        "defer",
        "range",
        "undefined",
        "of",
        "from",
    ],
    quotes: &['"', '\'', '`'],
};

const PYTHON: Lang = Lang {
    line_comments: &["#"],
    block_comment: None,
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
        "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
        "try", "while", "with", "yield", "self",
    ],
    quotes: &['"', '\''],
};

const SHELL: Lang = Lang {
    line_comments: &["#"],
    block_comment: None,
    keywords: &[
        "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "case", "esac",
        "function", "return", "export", "local", "echo", "param", "foreach", "true", "false",
    ],
    quotes: &['"', '\''],
};

const DATA: Lang = Lang {
    line_comments: &["#"],
    block_comment: None,
    keywords: &["true", "false", "null", "yes", "no"],
    quotes: &['"', '\''],
};

const SQL: Lang = Lang {
    line_comments: &["--"],
    block_comment: Some(("/*", "*/")),
    keywords: &[
        "select", "from", "where", "and", "or", "not", "insert", "into", "values", "update", "set",
        "delete", "create", "table", "drop", "join", "left", "right", "inner", "on", "group", "by",
        "order", "having", "limit", "as", "null", "primary", "key", "index",
    ],
    quotes: &['\''],
};

/// The language of a file extension, if fx colors it.
pub fn language_for(ext: &str) -> Option<Lang> {
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => RUST,
        "c" | "h" | "cpp" | "hpp" | "cc" | "cs" | "java" | "js" | "mjs" | "ts" | "tsx" | "jsx"
        | "go" | "kt" | "swift" | "css" | "scss" | "php" => C_LIKE,
        "py" | "pyw" => PYTHON,
        "sh" | "bash" | "zsh" | "ps1" | "psm1" | "bat" | "cmd" => SHELL,
        "json" | "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" => DATA,
        "sql" => SQL,
        _ => return None,
    })
}

/// Colored ranges (byte offsets) of each line; a block comment can span lines.
pub fn highlight(lines: &[impl AsRef<str>], lang: Lang) -> Vec<Vec<(Range<usize>, Kind)>> {
    let mut in_block = false;
    lines
        .iter()
        .map(|line| line_spans(line.as_ref(), lang, &mut in_block))
        .collect()
}

fn line_spans(line: &str, lang: Lang, in_block: &mut bool) -> Vec<(Range<usize>, Kind)> {
    let mut spans = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let rest = &line[i..];
        if *in_block {
            let (_, close) = lang.block_comment.unwrap_or(("", ""));
            let end = rest.find(close).map_or(line.len(), |e| i + e + close.len());
            spans.push((i..end, Kind::Comment));
            if end < line.len() || rest.contains(close) {
                *in_block = false;
            }
            i = end;
            continue;
        }
        if let Some((open, close)) = lang.block_comment
            && rest.starts_with(open)
        {
            match rest[open.len()..].find(close) {
                Some(e) => {
                    let end = i + open.len() + e + close.len();
                    spans.push((i..end, Kind::Comment));
                    i = end;
                }
                None => {
                    spans.push((i..line.len(), Kind::Comment));
                    *in_block = true;
                    i = line.len();
                }
            }
            continue;
        }
        if lang.line_comments.iter().any(|c| rest.starts_with(c)) {
            spans.push((i..line.len(), Kind::Comment));
            break;
        }
        let c = rest.chars().next().unwrap_or(' ');
        if lang.quotes.contains(&c) {
            let mut j = i + c.len_utf8();
            let mut escaped = false;
            while j < b.len() {
                let d = line[j..].chars().next().unwrap_or(' ');
                j += d.len_utf8();
                if escaped {
                    escaped = false;
                } else if d == '\\' {
                    escaped = true;
                } else if d == c {
                    break;
                }
            }
            spans.push((i..j, Kind::String));
            i = j;
            continue;
        }
        if c.is_ascii_digit() && (i == 0 || !is_word(b[i - 1] as char)) {
            let mut j = i;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'.' || b[j] == b'_') {
                j += 1;
            }
            spans.push((i..j, Kind::Number));
            i = j;
            continue;
        }
        if is_word(c) {
            let mut j = i;
            while j < b.len() && is_word(b[j] as char) {
                j += 1;
            }
            let word = &line[i..j];
            let keyword = if lang.keywords == SQL.keywords {
                lang.keywords.contains(&word.to_ascii_lowercase().as_str())
            } else {
                lang.keywords.contains(&word)
            };
            if keyword {
                spans.push((i..j, Kind::Keyword));
            }
            i = j;
            continue;
        }
        i += c.len_utf8();
    }
    spans
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str, lang: Lang) -> Vec<(&str, Kind)> {
        let spans = highlight(&[line], lang);
        spans[0]
            .iter()
            .map(|(r, k)| (&line[r.clone()], *k))
            .collect()
    }

    #[test]
    fn rust_line() {
        let got = kinds(r#"pub fn x() -> u32 { let s = "a\"b"; 42 } // fim"#, RUST);
        assert_eq!(
            got,
            [
                ("pub", Kind::Keyword),
                ("fn", Kind::Keyword),
                ("let", Kind::Keyword),
                (r#""a\"b""#, Kind::String),
                ("42", Kind::Number),
                ("// fim", Kind::Comment),
            ]
        );
    }

    #[test]
    fn block_comments_span_lines() {
        let lines = ["int a; /* começa", "meio", "fim */ return 1;"];
        let spans = highlight(&lines, C_LIKE);
        assert_eq!(spans[0].last().unwrap().1, Kind::Comment);
        assert_eq!(spans[1], [(0..4, Kind::Comment)]);
        assert_eq!(spans[2][0], (0..6, Kind::Comment));
        assert!(spans[2].iter().any(|(_, k)| *k == Kind::Keyword));
    }

    #[test]
    fn languages_by_extension() {
        assert_eq!(language_for("RS"), Some(RUST));
        assert_eq!(language_for("py"), Some(PYTHON));
        assert!(language_for("txt").is_none());
        assert_eq!(kinds("# nota", PYTHON), [("# nota", Kind::Comment)]);
        assert_eq!(
            kinds("SELECT x FROM t", SQL).len(),
            2,
            "SQL keywords ignore case"
        );
        assert!(
            kinds("x1 = a2b", PYTHON).is_empty(),
            "digits inside names are not numbers"
        );
    }
}
