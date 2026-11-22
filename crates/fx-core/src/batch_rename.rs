//! Batch rename rules (task T2.8): what each selected name becomes, and
//! what is wrong with the result, before anything touches the disk.
//!
//! For each name (in display order): find/replace on the name without its
//! extension (plain text or a regular expression), then the template
//! (`{nome}` is the result so far, `{n}` / `{n:3}` the position, from
//! `start`), then the letter case. The extension is kept.

use regex::RegexBuilder;

/// Letter case applied last (to the name without its extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Case {
    #[default]
    Keep,
    Lower,
    Upper,
    /// First letter of each word in upper case.
    Title,
}

impl Case {
    pub fn next(self) -> Self {
        match self {
            Case::Keep => Case::Lower,
            Case::Lower => Case::Upper,
            Case::Upper => Case::Title,
            Case::Title => Case::Keep,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Case::Keep => "manter",
            Case::Lower => "minúsculas",
            Case::Upper => "MAIÚSCULAS",
            Case::Title => "Título",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// `{nome}` keeps the name; e.g. `Férias {n:3}` numbers the files.
    pub template: String,
    pub start: u64,
    pub case: Case,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            find: String::new(),
            replace: String::new(),
            regex: false,
            case_sensitive: false,
            template: "{nome}".into(),
            start: 1,
            case: Case::Keep,
        }
    }
}

/// Why a new name cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Not a valid Windows file name (with the reason).
    Invalid(&'static str),
    /// Two items of the batch would get the same name.
    Duplicate,
    /// An item outside the batch already has this name.
    Exists,
}

impl Problem {
    pub fn text(self) -> &'static str {
        match self {
            Problem::Invalid(why) => why,
            Problem::Duplicate => "repetido no lote",
            Problem::Exists => "já existe na pasta",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub old: String,
    pub new: String,
    pub problem: Option<Problem>,
}

impl Planned {
    pub fn changes(&self) -> bool {
        self.new != self.old
    }
}

/// The new names of `names`. `taken(name)` says whether a name is used by an
/// item of the folder outside the batch (compare without case, like NTFS).
/// Fails only for an invalid regular expression.
pub fn plan(
    names: &[String],
    rules: &Rules,
    taken: &dyn Fn(&str) -> bool,
) -> Result<Vec<Planned>, String> {
    let pattern = if rules.find.is_empty() {
        None
    } else {
        let source = if rules.regex {
            rules.find.clone()
        } else {
            regex::escape(&rules.find)
        };
        Some(
            RegexBuilder::new(&source)
                .case_insensitive(!rules.case_sensitive)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| first_line(&e.to_string()))?,
        )
    };
    let mut out: Vec<Planned> = names
        .iter()
        .enumerate()
        .map(|(k, old)| {
            let (stem, ext) = split_ext(old);
            let mut stem = match &pattern {
                Some(p) if rules.regex => p.replace_all(stem, rules.replace.as_str()).into_owned(),
                Some(p) => p
                    .replace_all(stem, regex::NoExpand(&rules.replace))
                    .into_owned(),
                None => stem.to_owned(),
            };
            stem = apply_template(&rules.template, &stem, rules.start + k as u64);
            stem = apply_case(&stem, rules.case);
            let new = match ext {
                Some(ext) => format!("{stem}.{ext}"),
                None => stem,
            };
            let problem = invalid(&new).map(Problem::Invalid);
            Planned {
                old: old.clone(),
                new,
                problem,
            }
        })
        .collect();
    // Duplicates inside the batch (case-insensitive).
    let lower: Vec<String> = out.iter().map(|p| p.new.to_lowercase()).collect();
    for i in 0..out.len() {
        if out[i].problem.is_none() && lower.iter().filter(|l| **l == lower[i]).count() > 1 {
            out[i].problem = Some(Problem::Duplicate);
        }
    }
    // Names taken by items outside the batch. A name freed by another item
    // of the batch is fine (two-phase rename).
    let olds: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    for (i, p) in out.iter_mut().enumerate() {
        if p.problem.is_none() && p.changes() && !olds.contains(&lower[i]) && taken(&p.new) {
            p.problem = Some(Problem::Exists);
        }
    }
    Ok(out)
}

fn first_line(s: &str) -> String {
    s.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or(s)
        .trim()
        .to_owned()
}

/// `("foto", Some("jpg"))`; dot-files and names without a dot keep everything.
fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}

fn apply_template(template: &str, name: &str, n: u64) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = &after[..close];
        match token {
            "nome" | "name" => out.push_str(name),
            "n" => out.push_str(&n.to_string()),
            t if t.starts_with("n:") => match t[2..].parse::<usize>() {
                Ok(width) if width <= 12 => out.push_str(&format!("{n:0width$}")),
                _ => out.push_str(&rest[open..open + close + 2]),
            },
            _ => out.push_str(&rest[open..open + close + 2]),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn apply_case(s: &str, case: Case) -> String {
    match case {
        Case::Keep => s.to_owned(),
        Case::Lower => s.to_lowercase(),
        Case::Upper => s.to_uppercase(),
        Case::Title => {
            let mut out = String::with_capacity(s.len());
            let mut start = true;
            for c in s.chars() {
                if start {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                start = matches!(c, ' ' | '-' | '_' | '.' | '(' | '[');
            }
            out
        }
    }
}

/// Why `name` is not a valid Windows file name.
fn invalid(name: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        return Some("nome vazio");
    }
    if name
        .chars()
        .any(|c| c.is_control() || r#"<>:"/\|?*"#.contains(c))
    {
        return Some("caractere não permitido");
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Some("termina com ponto ou espaço");
    }
    if name.encode_utf16().count() > 255 {
        return Some("nome longo demais");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or(name)
        .trim_end()
        .to_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0');
    reserved.then_some("nome reservado do Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    fn news(p: &[Planned]) -> Vec<&str> {
        p.iter().map(|p| p.new.as_str()).collect()
    }

    const NONE: &dyn Fn(&str) -> bool = &|_| false;

    #[test]
    fn find_and_replace_keeps_the_extension() {
        let rules = Rules {
            find: "IMG_".into(),
            replace: "Férias ".into(),
            ..Rules::default()
        };
        let p = plan(
            &names(&["img_001.JPG", "IMG_002.jpg", "notas.img_.txt"]),
            &rules,
            NONE,
        )
        .unwrap();
        assert_eq!(
            news(&p),
            ["Férias 001.JPG", "Férias 002.jpg", "notas.Férias .txt"]
        );
        let exact = Rules {
            case_sensitive: true,
            ..rules
        };
        assert_eq!(
            plan(&names(&["img_1.jpg"]), &exact, NONE).unwrap()[0].new,
            "img_1.jpg"
        );
    }

    #[test]
    fn regular_expressions_with_groups() {
        let rules = Rules {
            find: r"(\d{4})-(\d{2})-(\d{2})".into(),
            replace: "$3.$2.$1".into(),
            regex: true,
            ..Rules::default()
        };
        let p = plan(&names(&["relatorio 2026-11-22.pdf"]), &rules, NONE).unwrap();
        assert_eq!(p[0].new, "relatorio 22.11.2026.pdf");
        // In plain-text mode "$1" is literal.
        let plain = Rules {
            find: "a".into(),
            replace: "$1".into(),
            ..Rules::default()
        };
        assert_eq!(
            plan(&names(&["a.txt"]), &plain, NONE).unwrap()[0].new,
            "$1.txt"
        );
        let bad = Rules {
            find: "(".into(),
            regex: true,
            ..Rules::default()
        };
        assert!(plan(&names(&["a"]), &bad, NONE).is_err());
    }

    #[test]
    fn template_numbers_in_display_order() {
        let rules = Rules {
            template: "Viagem {n:3} - {nome}".into(),
            start: 9,
            ..Rules::default()
        };
        let p = plan(&names(&["praia.jpg", "serra.png", "README"]), &rules, NONE).unwrap();
        assert_eq!(
            news(&p),
            [
                "Viagem 009 - praia.jpg",
                "Viagem 010 - serra.png",
                "Viagem 011 - README"
            ]
        );
        let rules = Rules {
            template: "{n}{x}{".into(),
            ..Rules::default()
        };
        assert_eq!(
            plan(&names(&["a.b"]), &rules, NONE).unwrap()[0].new,
            "1{x}{.b"
        );
    }

    #[test]
    fn letter_case() {
        let mut rules = Rules {
            case: Case::Title,
            ..Rules::default()
        };
        assert_eq!(
            plan(&names(&["meu ARQUIVO-final.TXT"]), &rules, NONE).unwrap()[0].new,
            "Meu Arquivo-Final.TXT"
        );
        rules.case = Case::Upper;
        assert_eq!(
            plan(&names(&["ação.pdf"]), &rules, NONE).unwrap()[0].new,
            "AÇÃO.pdf"
        );
        assert_eq!(Case::Title.next(), Case::Keep);
    }

    #[test]
    fn problems_are_found_before_renaming() {
        // Everything becomes "x": duplicates.
        let rules = Rules {
            template: "x".into(),
            ..Rules::default()
        };
        let p = plan(&names(&["a.txt", "B.TXT"]), &rules, NONE).unwrap();
        assert!(
            p.iter().all(|p| p.problem == Some(Problem::Duplicate)),
            "{p:?}"
        );
        // Invalid names.
        for (template, why) in [
            ("a:b", "caractere não permitido"),
            ("  ", "nome vazio"),
            ("CON", "nome reservado do Windows"),
            ("com1", "nome reservado do Windows"),
        ] {
            let rules = Rules {
                template: template.into(),
                ..Rules::default()
            };
            let p = plan(&names(&["a"]), &rules, NONE).unwrap();
            assert_eq!(p[0].problem, Some(Problem::Invalid(why)), "{template}");
        }
        let rules = Rules {
            template: "{nome}.".into(),
            ..Rules::default()
        };
        assert!(
            plan(&names(&["a"]), &rules, NONE).unwrap()[0]
                .problem
                .is_some()
        );
        assert!(invalid("com0").is_none() && invalid("console").is_none());
    }

    #[test]
    fn names_taken_outside_the_batch_but_not_inside() {
        let taken = |n: &str| n.eq_ignore_ascii_case("c.txt");
        let rules = Rules {
            find: "a".into(),
            replace: "c".into(),
            ..Rules::default()
        };
        let p = plan(&names(&["a.txt"]), &rules, &taken).unwrap();
        assert_eq!(p[0].problem, Some(Problem::Exists));
        // A name another batch item frees is fine (f1 -> f2 while f2 -> f3).
        let rules = Rules {
            find: r"f(\d)".into(),
            replace: "g".into(),
            regex: true,
            ..Rules::default()
        };
        let p = plan(&names(&["f1.txt"]), &rules, &|n: &str| n == "g.txt").unwrap();
        assert_eq!(p[0].problem, Some(Problem::Exists));
        let rules = Rules {
            template: "{nome}".into(),
            find: "1".into(),
            replace: "2".into(),
            ..Rules::default()
        };
        let p = plan(&names(&["f1.txt", "f2.txt"]), &rules, &|n: &str| {
            n == "f2.txt"
        })
        .unwrap();
        // f1 -> f2 while f2 stays f2: duplicate inside the batch.
        assert_eq!(p[0].problem, Some(Problem::Duplicate));
    }
}
