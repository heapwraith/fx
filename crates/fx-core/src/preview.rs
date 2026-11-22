//! What Quick Look shows for a file (task T1.22): the kind of preview, and
//! the text excerpt for text and code. The UI reads and decodes off its
//! thread; this module is the pure part.

/// Text and code previews read at most this much of the file.
pub const TEXT_LIMIT: usize = 256 * 1024;
/// Images larger than this use the shell's large thumbnail instead of a
/// full decode.
pub const IMAGE_LIMIT: u64 = 64 * 1024 * 1024;
/// Very long lines are cut (shaping a megabyte-long line would stall).
pub const LINE_LIMIT: usize = 2_000;
/// Bytes looked at to tell text from binary.
const SNIFF: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
    Tiff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// Decoded by fx (the formats GPUI decodes).
    Image(ImageKind),
    /// Text and code: the first [`TEXT_LIMIT`] bytes. Files without an
    /// extension are sniffed as text too (and fall back to a thumbnail when
    /// binary).
    Text,
    /// ZIP-based archives: their file list (T2.9).
    Zip,
    /// Folders and everything else: the shell's large thumbnail.
    Other,
}

const TEXT_EXT: &[&str] = &[
    "txt",
    "md",
    "markdown",
    "log",
    "csv",
    "tsv",
    "ini",
    "cfg",
    "conf",
    "toml",
    "yaml",
    "yml",
    "json",
    "jsonc",
    "xml",
    "svg",
    "html",
    "htm",
    "css",
    "scss",
    "js",
    "mjs",
    "cjs",
    "ts",
    "tsx",
    "jsx",
    "vue",
    "rs",
    "py",
    "rb",
    "go",
    "c",
    "h",
    "cc",
    "cpp",
    "hpp",
    "cs",
    "java",
    "kt",
    "swift",
    "php",
    "lua",
    "sql",
    "sh",
    "bash",
    "zsh",
    "ps1",
    "psm1",
    "bat",
    "cmd",
    "reg",
    "gitignore",
    "gitattributes",
    "editorconfig",
    "env",
    "lock",
    "patch",
    "diff",
    "srt",
    "nfo",
];

pub fn preview_kind(name: &str, is_dir: bool) -> PreviewKind {
    if is_dir {
        return PreviewKind::Other;
    }
    let ext = match name.rfind('.') {
        // ".gitignore": the whole name is the "extension".
        Some(0) => name[1..].to_ascii_lowercase(),
        Some(dot) => name[dot + 1..].to_ascii_lowercase(),
        None => return PreviewKind::Text,
    };
    let image = match ext.as_str() {
        "png" => Some(ImageKind::Png),
        "jpg" | "jpeg" | "jpe" | "jfif" => Some(ImageKind::Jpeg),
        "gif" => Some(ImageKind::Gif),
        "webp" => Some(ImageKind::Webp),
        "bmp" | "dib" => Some(ImageKind::Bmp),
        "tif" | "tiff" => Some(ImageKind::Tiff),
        _ => None,
    };
    match image {
        Some(kind) => PreviewKind::Image(kind),
        None if TEXT_EXT.contains(&ext.as_str()) => PreviewKind::Text,
        None if matches!(
            ext.as_str(),
            "zip" | "jar" | "apk" | "nupkg" | "vsix" | "whl"
        ) =>
        {
            PreviewKind::Zip
        }
        None => PreviewKind::Other,
    }
}

/// The lines of a text preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextPreview {
    pub lines: Vec<String>,
    /// The file goes on past what is shown.
    pub truncated: bool,
}

/// Lines of `bytes` (the start of a file; `more` when the file is longer).
/// UTF-8 (BOM or not) and UTF-16 with a BOM; anything else is shown as
/// UTF-8 with replacement characters. `None` for binary content (a NUL byte
/// in the first 8 KB, outside UTF-16).
pub fn text_preview(bytes: &[u8], more: bool) -> Option<TextPreview> {
    let mut truncated = more || bytes.len() > TEXT_LIMIT;
    let bytes = &bytes[..bytes.len().min(TEXT_LIMIT)];
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, u16::from_be_bytes)
    } else {
        if bytes[..bytes.len().min(SNIFF)].contains(&0) {
            return None;
        }
        let rest = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
        String::from_utf8_lossy(rest).into_owned()
    };
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| {
            let line = line
                .strip_suffix('\r')
                .unwrap_or(line)
                .replace('\t', "    ");
            match line.char_indices().nth(LINE_LIMIT) {
                Some((cut, _)) => {
                    truncated = true;
                    format!("{}…", &line[..cut])
                }
                None => line,
            }
        })
        .collect();
    // A final newline does not make an extra empty line.
    if lines.len() > 1 && lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    Some(TextPreview { lines, truncated })
}

fn utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| read(*c)).collect();
    String::from_utf16_lossy(&units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(
            preview_kind("Foto.JPG", false),
            PreviewKind::Image(ImageKind::Jpeg)
        );
        assert_eq!(
            preview_kind("a.webp", false),
            PreviewKind::Image(ImageKind::Webp)
        );
        assert_eq!(preview_kind("main.rs", false), PreviewKind::Text);
        assert_eq!(preview_kind(".gitignore", false), PreviewKind::Text);
        assert_eq!(preview_kind("LICENSE", false), PreviewKind::Text);
        assert_eq!(preview_kind("video.mp4", false), PreviewKind::Other);
        assert_eq!(preview_kind("docs.pdf", false), PreviewKind::Other);
        assert_eq!(preview_kind("fotos.png", true), PreviewKind::Other);
    }

    #[test]
    fn text_lines_and_encodings() {
        let t = text_preview(b"a\r\nb\tc\n", false).unwrap();
        assert_eq!(t.lines, ["a", "b    c"]);
        assert!(!t.truncated);
        let bom = text_preview(b"\xEF\xBB\xBFol\xC3\xA1", false).unwrap();
        assert_eq!(bom.lines, ["olá"]);
        // UTF-16 LE with BOM ("hi\n" then "ç").
        let utf16 = [0xFF, 0xFE, b'h', 0, b'i', 0, b'\n', 0, 0xE7, 0];
        assert_eq!(text_preview(&utf16, false).unwrap().lines, ["hi", "ç"]);
        // Invalid UTF-8 is shown, not refused.
        assert_eq!(
            text_preview(b"x\xFFy", false).unwrap().lines,
            ["x\u{FFFD}y"]
        );
    }

    #[test]
    fn binary_is_refused() {
        assert_eq!(text_preview(b"MZ\x90\x00\x03", false), None);
    }

    #[test]
    fn limits() {
        let big = vec![b'a'; TEXT_LIMIT + 10];
        let t = text_preview(&big, false).unwrap();
        assert!(t.truncated);
        assert!(t.lines[0].chars().count() <= LINE_LIMIT + 1);
        assert!(
            text_preview(b"short", true).unwrap().truncated,
            "file goes on"
        );
        let empty = text_preview(b"", false).unwrap();
        assert_eq!(empty.lines, [""]);
    }
}
