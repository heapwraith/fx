//! Filters typed in the disk search (task T2.6): `tipo:`, `ext:`, `tamanho:`
//! and `data:` words mixed with the name being searched.
//!
//! ```text
//! relatório tipo:documento data:semana     documents changed this week
//! tamanho:>1gb                             files over 1 GB (no name needed)
//! ext:pdf,docx 2026                        PDF or Word files with "2026"
//! ```
//!
//! English keys work too (`type:`, `size:`, `date:`). A word with a known key
//! but a value fx does not understand stays visible as a problem instead of
//! silently matching everything.

/// What the search should keep, besides the name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// The words that are not filters: matched against names.
    pub text: String,
    /// Lowercase extensions without the dot; empty: any.
    pub exts: Vec<String>,
    /// `Some(true)`: folders only; `Some(false)`: files only.
    pub dirs: Option<bool>,
    /// Size in bytes, inclusive bounds.
    pub size_min: Option<u64>,
    pub size_max: Option<u64>,
    /// Modification time in Unix seconds: `[min, max)`.
    pub modified_min: Option<i64>,
    pub modified_max: Option<i64>,
    /// Color tags every result must have (T3.4); checked by the caller,
    /// who knows the tags of each path.
    pub tags: u8,
    /// Text the file must contain (T6.8, `conteúdo:`), as typed; checked by
    /// the caller, who reads the file.
    pub content: Option<String>,
    /// One short label per filter understood ("imagens", "> 10 MB"), for the UI.
    pub labels: Vec<String>,
    /// Filter words fx could not read ("tamanho:muito").
    pub problems: Vec<String>,
}

const IMAGES: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff", "heic", "heif", "avif", "svg",
    "ico", "raw", "cr2", "nef", "arw", "dng",
];
const VIDEOS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v", "flv", "mpg", "mpeg", "3gp", "ts",
];
const AUDIO: &[&str] = &[
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "wma", "opus", "aiff", "mid",
];
const DOCUMENTS: &[&str] = &[
    "pdf", "doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ods", "csv", "ppt", "pptx",
    "odp", "epub",
];
const CODE: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "c", "h", "cpp", "hpp", "cs", "java", "go", "rb", "php",
    "html", "css", "json", "toml", "yaml", "yml", "xml", "sh", "ps1", "bat", "sql", "kt", "swift",
];
const ARCHIVES: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "bz2", "xz", "tgz", "iso", "cab",
];

const DAY: i64 = 86_400;

/// The words of a query; a double-quoted part stays one word with its
/// spaces (`conteúdo:"nota fiscal"`).
fn words(query: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    for (i, c) in query.char_indices() {
        match c {
            '"' => {
                quoted = !quoted;
                start.get_or_insert(i);
            }
            c if c.is_whitespace() && !quoted => {
                if let Some(s) = start.take() {
                    out.push(&query[s..i]);
                }
            }
            _ => {
                start.get_or_insert(i);
            }
        }
    }
    if let Some(s) = start {
        out.push(&query[s..]);
    }
    out
}

impl Filters {
    /// Reads `query`. `now` (Unix seconds) and `utc_offset` (seconds east of
    /// UTC) anchor relative dates like "hoje" to the local calendar.
    pub fn parse(query: &str, now: i64, utc_offset: i64) -> Self {
        let mut f = Filters::default();
        let mut text: Vec<&str> = Vec::new();
        for word in words(query) {
            let Some((key, value)) = word.split_once(':') else {
                text.push(word);
                continue;
            };
            if matches!(
                strip_accents(&key.to_lowercase()).as_str(),
                "conteudo" | "contem" | "content" | "contains"
            ) {
                let value = value.trim_matches('"');
                if value.is_empty() {
                    f.problems.push(word.to_owned());
                } else {
                    f.labels
                        .push(crate::trf!("contém \"{}\"", "contains \"{}\"", value));
                    f.content = Some(value.to_owned());
                }
                continue;
            }
            let value = value.to_lowercase();
            let ok = match key.to_lowercase().as_str() {
                "tipo" | "type" => f.kind(&value),
                "ext" => f.ext(&value),
                "tamanho" | "size" => f.size(&value).is_some(),
                "etiqueta" | "tag" => match crate::tags::bit_for(&value) {
                    Some(bit) => {
                        f.tags |= bit;
                        f.labels
                            .push(crate::trf!("etiqueta {value}", "tag {value}"));
                        true
                    }
                    None => false,
                },
                "data" | "date" | "modificado" | "modified" => {
                    f.date(&value, now, utc_offset).is_some()
                }
                // Not a filter key (e.g. "C:" or "10:30"): part of the name.
                _ => {
                    text.push(word);
                    continue;
                }
            };
            if !ok {
                f.problems.push(word.to_owned());
            }
        }
        f.text = text.join(" ");
        f
    }

    /// Any filter besides the name?
    pub fn is_active(&self) -> bool {
        !self.exts.is_empty()
            || self.dirs.is_some()
            || self.size_min.is_some()
            || self.size_max.is_some()
            || self.modified_min.is_some()
            || self.modified_max.is_some()
            || self.tags != 0
            || self.content.is_some()
    }

    /// Whether an entry passes the filters (the name is matched elsewhere).
    pub fn matches(&self, name: &str, is_dir: bool, size: u64, modified: i64) -> bool {
        if let Some(dirs) = self.dirs
            && dirs != is_dir
        {
            return false;
        }
        if !self.exts.is_empty() {
            if is_dir {
                return false;
            }
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase());
            if !ext.is_some_and(|e| self.exts.contains(&e)) {
                return false;
            }
        }
        if self.size_min.is_some() || self.size_max.is_some() {
            if is_dir {
                return false; // folders have no size of their own
            }
            if self.size_min.is_some_and(|m| size < m) || self.size_max.is_some_and(|m| size > m) {
                return false;
            }
        }
        !(self.modified_min.is_some_and(|m| modified < m)
            || self.modified_max.is_some_and(|m| modified >= m))
    }

    fn kind(&mut self, value: &str) -> bool {
        let (label, exts, dirs): (&str, &[&str], Option<bool>) = match strip_accents(value).as_str()
        {
            "imagem" | "imagens" | "foto" | "fotos" | "image" | "images" => {
                (crate::tr!("imagens", "images"), IMAGES, None)
            }
            "video" | "videos" => (crate::tr!("vídeos", "videos"), VIDEOS, None),
            "audio" | "audios" | "musica" | "musicas" | "music" => {
                (crate::tr!("áudio", "audio"), AUDIO, None)
            }
            "documento" | "documentos" | "doc" | "docs" | "document" => {
                (crate::tr!("documentos", "documents"), DOCUMENTS, None)
            }
            "codigo" | "code" => (crate::tr!("código", "code"), CODE, None),
            "compactado" | "compactados" | "zip" | "archive" => {
                (crate::tr!("compactados", "archives"), ARCHIVES, None)
            }
            "pasta" | "pastas" | "folder" | "folders" => {
                (crate::tr!("pastas", "folders"), &[], Some(true))
            }
            "arquivo" | "arquivos" | "file" | "files" => {
                (crate::tr!("arquivos", "files"), &[], Some(false))
            }
            _ => return false,
        };
        self.exts.extend(exts.iter().map(|e| (*e).to_owned()));
        if dirs.is_some() {
            self.dirs = dirs;
        }
        self.labels.push(label.to_owned());
        true
    }

    fn ext(&mut self, value: &str) -> bool {
        let exts: Vec<String> = value
            .split(',')
            .map(|e| e.trim().trim_start_matches('.').to_owned())
            .filter(|e| !e.is_empty())
            .collect();
        if exts.is_empty() {
            return false;
        }
        self.labels.push(format!(".{}", exts.join(", .")));
        self.exts.extend(exts);
        true
    }

    fn size(&mut self, value: &str) -> Option<()> {
        let (min, max) = if let Some(v) = value.strip_prefix('>') {
            (Some(parse_size(v.trim_start_matches('='))?), None)
        } else if let Some(v) = value.strip_prefix('<') {
            (None, Some(parse_size(v.trim_start_matches('='))?))
        } else if let Some((a, b)) = value.split_once("..") {
            (Some(parse_size(a)?), Some(parse_size(b)?))
        } else {
            return None;
        };
        self.size_min = min.or(self.size_min);
        self.size_max = max.or(self.size_max);
        self.labels.push(match (min, max) {
            (Some(a), Some(b)) => crate::trf!(
                "{} a {}",
                "{} to {}",
                crate::format::size_short(a),
                crate::format::size_short(b)
            ),
            (Some(a), None) => format!("> {}", crate::format::size_short(a)),
            (None, Some(b)) => format!("< {}", crate::format::size_short(b)),
            (None, None) => return None,
        });
        Some(())
    }

    fn date(&mut self, value: &str, now: i64, offset: i64) -> Option<()> {
        // Start of today, local time, in Unix seconds.
        let today = (now + offset).div_euclid(DAY) * DAY - offset;
        let (min, max, label): (Option<i64>, Option<i64>, String) =
            match strip_accents(value).as_str() {
                "hoje" | "today" => (Some(today), None, crate::tr!("hoje", "today").into()),
                "ontem" | "yesterday" => (
                    Some(today - DAY),
                    Some(today),
                    crate::tr!("ontem", "yesterday").into(),
                ),
                "semana" | "week" => (
                    Some(today - 6 * DAY),
                    None,
                    crate::tr!("últimos 7 dias", "last 7 days").into(),
                ),
                "mes" | "month" => (
                    Some(today - 29 * DAY),
                    None,
                    crate::tr!("últimos 30 dias", "last 30 days").into(),
                ),
                "ano" | "year" => (
                    Some(today - 364 * DAY),
                    None,
                    crate::tr!("últimos 365 dias", "last 365 days").into(),
                ),
                v => {
                    let day = |s: &str| parse_day(s).map(|d| d * DAY - offset);
                    if let Some(d) = v.strip_prefix('>') {
                        let d = day(d.trim_start_matches('='));
                        (
                            Some(d?),
                            None,
                            crate::trf!("desde {}", "since {}", v[1..].trim_start_matches('=')),
                        )
                    } else if let Some(d) = v.strip_prefix('<') {
                        let d = day(d.trim_start_matches('='));
                        (
                            None,
                            Some(d?),
                            crate::trf!("antes de {}", "before {}", v[1..].trim_start_matches('=')),
                        )
                    } else if let Some((a, b)) = v.split_once("..") {
                        // Both ends included: up to the end of the last day.
                        (
                            Some(day(a)?),
                            Some(day(b)? + DAY),
                            crate::trf!("{a} a {b}", "{a} to {b}"),
                        )
                    } else {
                        let d = day(v)?;
                        (Some(d), Some(d + DAY), v.to_owned())
                    }
                }
            };
        self.modified_min = min.or(self.modified_min);
        self.modified_max = max.or(self.modified_max);
        self.labels.push(label);
        Some(())
    }
}

/// "10mb", "1.5gb", "500k", "200" (bytes). Powers of 1024, like Explorer.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))?;
    let (number, unit) = if split == 0 {
        return None;
    } else {
        s.split_at(split)
    };
    let n: f64 = number.replace(',', ".").parse().ok()?;
    let mult: f64 = match unit {
        "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    let bytes = n * mult;
    (bytes.is_finite() && bytes >= 0.0).then_some(bytes as u64)
}

/// `2026-01-31` (or `31/01/2026`) as days since 1970-01-01.
fn parse_day(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(['-', '/']).collect();
    let [a, b, c] = parts.as_slice() else {
        return None;
    };
    let (y, m, d) = if a.len() == 4 {
        (*a, *b, *c)
    } else {
        (*c, *b, *a)
    };
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Days since 1970-01-01 of a proleptic Gregorian date (inverse of
/// `format::civil_from_days`).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' => 'u',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn content_filter_keeps_quoted_phrases() {
        let f = Filters::parse(r#"conteúdo:"Nota Fiscal" ext:pdf 2026"#, 0, 0);
        assert_eq!(f.content.as_deref(), Some("Nota Fiscal"));
        assert_eq!(f.text, "2026");
        assert_eq!(f.exts, ["pdf"]);
        assert!(f.is_active());
        let g = Filters::parse("content:todo", 0, 0);
        assert_eq!(g.content.as_deref(), Some("todo"));
        assert_eq!(g.text, "");
        assert_eq!(Filters::parse("conteudo:", 0, 0).problems, ["conteudo:"]);
    }

    use super::*;

    const BRT: i64 = -3 * 3600;

    /// 2026-11-22 15:00 in São Paulo (UTC-3).
    fn now() -> i64 {
        days_from_civil(2026, 11, 22) * DAY + 18 * 3600
    }

    fn p(q: &str) -> Filters {
        Filters::parse(q, now(), BRT)
    }

    #[test]
    fn words_that_are_not_filters_stay_in_the_text() {
        let f = p("relatório final C:\\x 10:30");
        assert_eq!(f.text, "relatório final C:\\x 10:30");
        assert!(!f.is_active());
        assert!(f.problems.is_empty());
    }

    #[test]
    fn kinds_and_extensions() {
        let f = p("férias tipo:imagem");
        assert_eq!(f.text, "férias");
        assert_eq!(f.labels, ["imagens"]);
        assert!(f.matches("praia.JPG", false, 10, 0));
        assert!(!f.matches("praia.pdf", false, 10, 0));
        assert!(
            !f.matches("fotos", true, 0, 0),
            "a folder has no image extension"
        );

        let f = p("ext:.pdf,docx");
        assert_eq!(f.exts, ["pdf", "docx"]);
        assert!(f.matches("a.docx", false, 1, 0));
        assert!(!f.matches("a.doc", false, 1, 0));
        assert!(!f.matches("semextensao", false, 1, 0));

        let f = p("type:pastas");
        assert!(f.matches("Projetos", true, 0, 0));
        assert!(!f.matches("a.txt", false, 0, 0));
        assert!(p("tipo:arquivo").matches("a.txt", false, 0, 0));
        assert!(
            p("tipo:vídeo").matches("a.mkv", false, 0, 0),
            "accents are optional"
        );
    }

    #[test]
    fn sizes() {
        let f = p("tamanho:>10mb");
        assert_eq!(f.size_min, Some(10 << 20));
        assert_eq!(f.labels, ["> 10 MB"]);
        assert!(f.matches("a", false, 11 << 20, 0));
        assert!(!f.matches("a", false, 9 << 20, 0));
        assert!(
            !f.matches("pasta", true, 0, 0),
            "folders do not pass a size filter"
        );
        let f = p("size:<1,5k");
        assert_eq!(f.size_max, Some(1536));
        let f = p("tamanho:1mb..1gb");
        assert_eq!((f.size_min, f.size_max), (Some(1 << 20), Some(1 << 30)));
        assert_eq!(p("tamanho:muito").problems, ["tamanho:muito"]);
        assert_eq!(p("tamanho:>10xb").problems.len(), 1);
    }

    #[test]
    fn relative_and_absolute_dates() {
        let today = (now() + BRT).div_euclid(DAY) * DAY - BRT; // 00:00 local
        let f = p("data:hoje");
        assert_eq!(f.modified_min, Some(today));
        assert!(f.matches("a", false, 0, now() - 3600));
        assert!(!f.matches("a", false, 0, today - 1));
        let f = p("data:ontem");
        assert_eq!(
            (f.modified_min, f.modified_max),
            (Some(today - DAY), Some(today))
        );
        assert_eq!(p("data:semana").modified_min, Some(today - 6 * DAY));
        assert_eq!(p("data:mês").labels, ["últimos 30 dias"]);

        let f = p("data:>2026-01-01");
        assert_eq!(
            f.modified_min,
            Some(days_from_civil(2026, 1, 1) * DAY - BRT)
        );
        let f = p("data:01/11/2026..30/11/2026");
        assert_eq!(
            f.modified_min,
            Some(days_from_civil(2026, 11, 1) * DAY - BRT)
        );
        assert_eq!(
            f.modified_max,
            Some(days_from_civil(2026, 12, 1) * DAY - BRT)
        );
        assert_eq!(p("data:2026-13-01").problems.len(), 1);
    }

    #[test]
    fn days_from_civil_matches_the_formatter() {
        for days in [-1, 0, 365, 19_000, 20_779] {
            let (y, m, d) = crate::format::civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn tag_filter() {
        let f = p("relatório etiqueta:verde tag:red");
        assert_eq!(f.text, "relatório");
        assert_eq!(f.tags, (1 << 3) | 1);
        assert!(f.is_active());
        assert_eq!(p("etiqueta:rosa").problems.len(), 1);
    }

    #[test]
    fn filters_combine() {
        let f = p("relatório tipo:documento tamanho:>1mb data:ano");
        assert_eq!(f.text, "relatório");
        assert_eq!(f.labels.len(), 3);
        assert!(f.matches("r.pdf", false, 2 << 20, now() - DAY));
        assert!(!f.matches("r.pdf", false, 2 << 20, now() - 400 * DAY));
        assert!(!f.matches("r.png", false, 2 << 20, now() - DAY));
    }
}
