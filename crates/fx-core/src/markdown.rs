//! Markdown for Quick Look (task T2.9): each source line becomes a styled
//! line (headings, list items, quotes, code blocks, rules), with the inline
//! markers (`**`, `*`, `` ` ``, links) reduced to their text. Enough to read
//! a README; not a full CommonMark renderer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Heading(u8),
    Item,
    Quote,
    Code,
    Rule,
    Text,
    Blank,
    /// A table row: its cells in `text`, separated by `\t`; `true` for the
    /// header (the row above the `|---|` line, which is left out).
    TableRow(bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdLine {
    pub block: Block,
    pub text: String,
}

pub fn render(lines: &[impl AsRef<str>]) -> Vec<MdLine> {
    let mut out = Vec::with_capacity(lines.len());
    let mut fenced = false;
    for (i, line) in lines.iter().enumerate() {
        let raw = line.as_ref();
        let t = raw.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            out.push(MdLine {
                block: Block::Code,
                text: raw.to_owned(),
            });
            continue;
        }
        let (block, text) = if t.is_empty() {
            (Block::Blank, String::new())
        } else if t.starts_with('|') {
            if table_separator(t) {
                continue;
            }
            let header = lines
                .get(i + 1)
                .is_some_and(|next| table_separator(next.as_ref().trim()));
            (Block::TableRow(header), cells(t))
        } else if let Some(level) = heading(t) {
            (
                Block::Heading(level),
                inline(t[level as usize..].trim().trim_end_matches('#').trim()),
            )
        } else if t.chars().all(|c| matches!(c, '-' | '*' | '_' | ' '))
            && t.chars().filter(|c| !c.is_whitespace()).count() >= 3
        {
            (Block::Rule, String::new())
        } else if let Some(rest) = t
            .strip_prefix("- ")
            .or_else(|| t.strip_prefix("* "))
            .or_else(|| t.strip_prefix("+ "))
        {
            let rest = rest
                .strip_prefix("[ ] ")
                .map(|r| format!("☐ {r}"))
                .or_else(|| rest.strip_prefix("[x] ").map(|r| format!("☑ {r}")))
                .unwrap_or_else(|| rest.to_owned());
            let depth = (raw.len() - t.len()) / 2;
            (
                Block::Item,
                format!("{}• {}", "  ".repeat(depth), inline(&rest)),
            )
        } else if let Some(n) = numbered(t) {
            (Block::Item, format!("{}{}", &t[..n], inline(&t[n..])))
        } else if let Some(rest) = t.strip_prefix('>') {
            (Block::Quote, inline(rest.trim_start()))
        } else if raw.starts_with("    ") || raw.starts_with('\t') {
            (Block::Code, raw.trim_start_matches('\t').to_owned())
        } else {
            (Block::Text, inline(t))
        };
        out.push(MdLine { block, text });
    }
    out
}

/// `|---|:--:|`: the line under a table header.
fn table_separator(t: &str) -> bool {
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// `| a | **b** |` → `a\tb`.
fn cells(t: &str) -> String {
    let inner = t.trim().trim_start_matches('|');
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner
        .split('|')
        .map(|c| inline(c.trim()))
        .collect::<Vec<_>>()
        .join("\t")
}

fn heading(t: &str) -> Option<u8> {
    let n = t.bytes().take_while(|&b| b == b'#').count();
    ((1..=6).contains(&n) && t.as_bytes().get(n) == Some(&b' ')).then_some(n as u8)
}

/// Length of a "1. " / "12) " prefix.
fn numbered(t: &str) -> Option<usize> {
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0
        && matches!(t.as_bytes().get(digits), Some(b'.' | b')'))
        && t.as_bytes().get(digits + 1) == Some(&b' '))
    .then_some(digits + 2)
}

/// Inline markers to plain text: `**a**`, `*a*`, `` `a` ``, `[a](url)`,
/// `![a](img)`.
fn inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if (c == '[' || rest.starts_with("!["))
            && let Some(text_end) = rest.find("](")
        {
            let start = if c == '!' { 2 } else { 1 };
            if let Some(url_end) = rest[text_end..].find(')') {
                out.push_str(&rest[start..text_end]);
                rest = &rest[text_end + url_end + 1..];
                continue;
            }
        }
        if matches!(c, '*' | '_' | '`') {
            rest = &rest[1..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_inline_markers() {
        let src = [
            "# fx",
            "Um **gerenciador** de `arquivos` ([site](https://x)).",
            "",
            "- item",
            "  - sub [x] não",
            "- [x] feito",
            "1. primeiro",
            "> citação",
            "---",
            "```rust",
            "let a = *b;",
            "```",
        ];
        let r = render(&src);
        let blocks: Vec<Block> = r.iter().map(|l| l.block).collect();
        assert_eq!(
            blocks,
            [
                Block::Heading(1),
                Block::Text,
                Block::Blank,
                Block::Item,
                Block::Item,
                Block::Item,
                Block::Item,
                Block::Quote,
                Block::Rule,
                Block::Code,
            ]
        );
        assert_eq!(r[0].text, "fx");
        assert_eq!(r[1].text, "Um gerenciador de arquivos (site).");
        assert_eq!(r[3].text, "• item");
        assert_eq!(r[4].text, "  • sub [x] não");
        assert_eq!(r[5].text, "• ☑ feito");
        assert_eq!(r[6].text, "1. primeiro");
        assert_eq!(r[9].text, "let a = *b;", "code keeps its markers");
    }

    #[test]
    fn tables_become_rows_of_cells() {
        let r = render(&[
            "| | Pathdash | Explorador |",
            "|---|:---:|---|",
            "| Abrir **100 mil** | ~10 ms | ~254 ms |",
            "texto",
        ]);
        let blocks: Vec<Block> = r.iter().map(|l| l.block).collect();
        assert_eq!(
            blocks,
            [Block::TableRow(true), Block::TableRow(false), Block::Text]
        );
        assert_eq!(r[0].text, "\tPathdash\tExplorador");
        assert_eq!(r[1].text, "Abrir 100 mil\t~10 ms\t~254 ms");
    }

    #[test]
    fn not_headings() {
        assert_eq!(render(&["#hashtag"])[0].block, Block::Text);
        assert_eq!(render(&["####### sete"])[0].block, Block::Text);
    }
}
