//! "Agrupar por" (T6.10, D-068): the list split into groups by type, date or
//! size, like Explorer. Folders always come first. The grouped order is the
//! sorted order made stable by group (computed on a worker); the headers are
//! found by one pass over the shown rows, without allocating per row.

use crate::format::civil_from_days;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    #[default]
    None,
    Type,
    Date,
    Size,
}

/// What a row is grouped by.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    pub name: &'a str,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds.
    pub modified: i64,
}

/// "Now" for the date groups: Unix seconds and the local UTC offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub secs: i64,
    pub offset: i64,
}

/// A group, in display order (`Ord`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Folders,
    /// Lowercase extension, without the dot.
    Ext(String),
    NoExt,
    Date(DateGroup),
    Size(SizeGroup),
}

/// Newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DateGroup {
    Future,
    Today,
    Yesterday,
    Last7Days,
    Last30Days,
    ThisYear,
    Older,
}

/// Explorer's size groups, largest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SizeGroup {
    /// More than 4 GiB.
    Gigantic,
    /// 1 to 4 GiB.
    Huge,
    /// 128 MiB to 1 GiB.
    Large,
    /// 1 to 128 MiB.
    Medium,
    /// 16 KiB to 1 MiB.
    Small,
    /// Up to 16 KiB.
    Tiny,
    Empty,
}

/// The extension of a file name (after the last dot, not a leading one).
fn ext_of(name: &str) -> Option<&str> {
    match name.rfind('.') {
        Some(0) | None => None,
        Some(k) if k + 1 < name.len() => Some(&name[k + 1..]),
        _ => None,
    }
}

fn date_group(modified: i64, now: Now) -> DateGroup {
    let day = |t: i64| (t + now.offset).div_euclid(86_400);
    let (d, today) = (day(modified), day(now.secs));
    match today - d {
        k if k < 0 => DateGroup::Future,
        0 => DateGroup::Today,
        1 => DateGroup::Yesterday,
        2..7 => DateGroup::Last7Days,
        7..30 => DateGroup::Last30Days,
        _ if civil_from_days(d).0 == civil_from_days(today).0 => DateGroup::ThisYear,
        _ => DateGroup::Older,
    }
}

fn size_group(size: u64) -> SizeGroup {
    const KIB: u64 = 1 << 10;
    const MIB: u64 = 1 << 20;
    const GIB: u64 = 1 << 30;
    match size {
        0 => SizeGroup::Empty,
        s if s <= 16 * KIB => SizeGroup::Tiny,
        s if s <= MIB => SizeGroup::Small,
        s if s <= 128 * MIB => SizeGroup::Medium,
        s if s <= GIB => SizeGroup::Large,
        s if s <= 4 * GIB => SizeGroup::Huge,
        _ => SizeGroup::Gigantic,
    }
}

/// The group of a row (allocates only for the type of a file).
pub fn group_of(by: GroupBy, f: Facts<'_>, now: Now) -> Option<Group> {
    if by == GroupBy::None {
        return None;
    }
    if f.is_dir {
        return Some(Group::Folders);
    }
    Some(match by {
        GroupBy::Type => match ext_of(f.name) {
            Some(e) => Group::Ext(e.to_ascii_lowercase()),
            None => Group::NoExt,
        },
        GroupBy::Date => Group::Date(date_group(f.modified, now)),
        GroupBy::Size => Group::Size(size_group(f.size)),
        GroupBy::None => return None,
    })
}

/// Whether two rows are in the same group, without allocating.
pub fn same_group(by: GroupBy, a: Facts<'_>, b: Facts<'_>, now: Now) -> bool {
    if a.is_dir || b.is_dir {
        return a.is_dir == b.is_dir;
    }
    match by {
        GroupBy::None => true,
        GroupBy::Type => match (ext_of(a.name), ext_of(b.name)) {
            (Some(x), Some(y)) => x.eq_ignore_ascii_case(y),
            (None, None) => true,
            _ => false,
        },
        GroupBy::Date => date_group(a.modified, now) == date_group(b.modified, now),
        GroupBy::Size => size_group(a.size) == size_group(b.size),
    }
}

/// `order` made stable by group (folders first, then the groups in order),
/// keeping the sorted order inside each group.
pub fn grouped_order<'a>(
    order: Vec<u32>,
    by: GroupBy,
    now: Now,
    facts: impl Fn(u32) -> Facts<'a>,
) -> Vec<u32> {
    if by == GroupBy::None {
        return order;
    }
    let mut keyed: Vec<(Option<Group>, u32)> = order
        .into_iter()
        .map(|i| (group_of(by, facts(i), now), i))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, i)| i).collect()
}

/// Where each group starts in `rows`: `(position, group)`.
pub fn headers<'a>(
    rows: &[u32],
    by: GroupBy,
    now: Now,
    facts: impl Fn(u32) -> Facts<'a>,
) -> Vec<(usize, Group)> {
    let mut out = Vec::new();
    if by == GroupBy::None {
        return out;
    }
    let mut prev: Option<Facts<'a>> = None;
    for (pos, &i) in rows.iter().enumerate() {
        let f = facts(i);
        if prev.is_none_or(|p| !same_group(by, p, f, now))
            && let Some(g) = group_of(by, f, now)
        {
            out.push((pos, g));
        }
        prev = Some(f);
    }
    out
}

impl Group {
    /// The header's text.
    pub fn label(&self) -> String {
        use crate::tr;
        match self {
            Group::Folders => tr!("Pastas", "Folders").to_owned(),
            Group::Ext(e) => crate::trf!("Arquivos {}", "{} files", e.to_uppercase()),
            Group::NoExt => tr!("Sem extensão", "No extension").to_owned(),
            Group::Date(d) => match d {
                DateGroup::Future => tr!("No futuro", "In the future"),
                DateGroup::Today => tr!("Hoje", "Today"),
                DateGroup::Yesterday => tr!("Ontem", "Yesterday"),
                DateGroup::Last7Days => tr!("Últimos 7 dias", "Last 7 days"),
                DateGroup::Last30Days => tr!("Últimos 30 dias", "Last 30 days"),
                DateGroup::ThisYear => tr!("Este ano", "This year"),
                DateGroup::Older => tr!("Anos anteriores", "Earlier years"),
            }
            .to_owned(),
            Group::Size(s) => match s {
                SizeGroup::Gigantic => tr!("Gigantes (mais de 4 GB)", "Gigantic (over 4 GB)"),
                SizeGroup::Huge => tr!("Enormes (1 a 4 GB)", "Huge (1 to 4 GB)"),
                SizeGroup::Large => tr!("Grandes (128 MB a 1 GB)", "Large (128 MB to 1 GB)"),
                SizeGroup::Medium => tr!("Médios (1 a 128 MB)", "Medium (1 to 128 MB)"),
                SizeGroup::Small => tr!("Pequenos (16 KB a 1 MB)", "Small (16 KB to 1 MB)"),
                SizeGroup::Tiny => tr!("Minúsculos (até 16 KB)", "Tiny (up to 16 KB)"),
                SizeGroup::Empty => tr!("Vazios", "Empty"),
            }
            .to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    /// 2026-11-22 12:00 UTC.
    const NOW: Now = Now {
        secs: 1_795_348_800,
        offset: 0,
    };

    fn file(name: &str, size: u64, modified: i64) -> (String, bool, u64, i64) {
        (name.to_owned(), false, size, modified)
    }

    fn facts<'a>(items: &'a [(String, bool, u64, i64)]) -> impl Fn(u32) -> Facts<'a> {
        move |i| {
            let (name, is_dir, size, modified) = &items[i as usize];
            Facts {
                name,
                is_dir: *is_dir,
                size: *size,
                modified: *modified,
            }
        }
    }

    #[test]
    fn by_type_folders_first_then_extensions() {
        let items = vec![
            file("b.TXT", 1, 0),
            file("leia-me", 1, 0),
            ("fotos".to_owned(), true, 0, 0),
            file("a.pdf", 1, 0),
            file("c.txt", 1, 0),
            file(".gitignore", 1, 0),
        ];
        let order = grouped_order((0..6).collect(), GroupBy::Type, NOW, facts(&items));
        assert_eq!(order, [2, 3, 0, 4, 1, 5]);
        let h = headers(&order, GroupBy::Type, NOW, facts(&items));
        let labels: Vec<(usize, String)> = h.iter().map(|(p, g)| (*p, g.label())).collect();
        assert_eq!(labels.len(), 4, "{labels:?}");
        assert_eq!(h[0], (0, Group::Folders));
        assert_eq!(h[1], (1, Group::Ext("pdf".into())));
        assert_eq!(h[2], (2, Group::Ext("txt".into())), "case does not split");
        assert_eq!(h[3], (4, Group::NoExt));
    }

    #[test]
    fn by_date_newest_first() {
        let items = vec![
            file("velho", 1, NOW.secs - 400 * DAY),
            file("hoje", 1, NOW.secs - 60),
            file("ontem", 1, NOW.secs - DAY),
            file("semana", 1, NOW.secs - 3 * DAY),
            file("mes", 1, NOW.secs - 20 * DAY),
            file("ano", 1, NOW.secs - 100 * DAY),
        ];
        let order = grouped_order((0..6).collect(), GroupBy::Date, NOW, facts(&items));
        assert_eq!(order, [1, 2, 3, 4, 5, 0]);
        let h = headers(&order, GroupBy::Date, NOW, facts(&items));
        assert_eq!(h.len(), 6);
        assert_eq!(h[0].1, Group::Date(DateGroup::Today));
        assert_eq!(h[4].1, Group::Date(DateGroup::ThisYear));
        assert_eq!(h[5].1, Group::Date(DateGroup::Older));
    }

    #[test]
    fn by_size_largest_first_and_order_kept_inside() {
        let items = vec![
            file("vazio", 0, 0),
            file("a", 10, 0),
            file("grande", 200 << 20, 0),
            file("b", 20, 0),
        ];
        let order = grouped_order((0..4).collect(), GroupBy::Size, NOW, facts(&items));
        assert_eq!(order, [2, 1, 3, 0], "a before b, as sorted");
        let h = headers(&order, GroupBy::Size, NOW, facts(&items));
        assert_eq!(
            h.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
            [0, 1, 3],
            "{h:?}"
        );
        assert!(headers(&order, GroupBy::None, NOW, facts(&items)).is_empty());
    }

    #[test]
    fn grouping_100k_rows() {
        let items: Vec<(String, bool, u64, i64)> = (0..100_000u64)
            .map(|k| {
                file(
                    &format!("f{k}.{}", ["txt", "jpg", "pdf"][(k % 3) as usize]),
                    k * 997,
                    NOW.secs - (k as i64) * 600,
                )
            })
            .collect();
        for by in [GroupBy::Type, GroupBy::Date, GroupBy::Size] {
            let order = grouped_order((0..100_000).collect(), by, NOW, facts(&items));
            let started = std::time::Instant::now();
            let h = headers(&order, by, NOW, facts(&items));
            let took = started.elapsed();
            println!("{by:?}: {} headers in {took:?}", h.len());
            assert!(h.len() < 10, "{by:?}: {}", h.len());
        }
    }
}
