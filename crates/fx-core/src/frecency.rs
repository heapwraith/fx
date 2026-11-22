//! "Ir para" que aprende (T6.5, D-068): folders ranked by how often and how
//! recently they were opened, with zoxide's rules ("frecency" and keyword
//! matching), so a few letters in the palette take to the right folder.
//!
//! Pure: the interface records the visits and keeps them in `state.db`.

use std::path::{Path, PathBuf};

use crate::natural::fold;

/// When the ranks add up to more than this, all of them shrink by 10% and
/// the ones below 1 are dropped (old habits fade; zoxide's aging).
const MAX_TOTAL: f64 = 10_000.0;
/// Most folders remembered (the least used go first).
const MAX_ENTRIES: usize = 500;

/// One remembered folder.
#[derive(Debug, Clone, PartialEq)]
pub struct Visit {
    pub path: PathBuf,
    /// Grows by 1 at each visit; shrinks with aging.
    pub rank: f64,
    /// Last visit, in Unix seconds.
    pub last: i64,
}

/// The folders opened, with their ranks.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Frecency {
    visits: Vec<Visit>,
}

impl Frecency {
    pub const fn new() -> Self {
        Self { visits: Vec::new() }
    }

    pub fn from_visits(visits: Vec<Visit>) -> Self {
        let mut f = Self { visits };
        f.trim();
        f
    }

    pub fn visits(&self) -> &[Visit] {
        &self.visits
    }

    /// `path` was opened at `now`.
    pub fn visit(&mut self, path: &Path, now: i64) {
        match self.visits.iter_mut().find(|v| same_folder(&v.path, path)) {
            Some(v) => {
                v.rank += 1.0;
                v.last = now;
            }
            None => self.visits.push(Visit {
                path: path.to_path_buf(),
                rank: 1.0,
                last: now,
            }),
        }
        let total: f64 = self.visits.iter().map(|v| v.rank).sum();
        if total > MAX_TOTAL {
            for v in &mut self.visits {
                v.rank *= 0.9;
            }
            self.visits.retain(|v| v.rank >= 1.0);
        }
        self.trim();
    }

    /// Forgets `path` (it no longer exists).
    pub fn forget(&mut self, path: &Path) {
        self.visits.retain(|v| !same_folder(&v.path, path));
    }

    /// The most used folders now, best first.
    pub fn top(&self, now: i64, n: usize) -> Vec<&Visit> {
        let mut all: Vec<&Visit> = self.visits.iter().collect();
        all.sort_by(|a, b| score(b, now).total_cmp(&score(a, now)));
        all.truncate(n);
        all
    }

    /// The folders that match `query`, best first (see [`keywords_match`]).
    pub fn matches(&self, query: &str, now: i64) -> Vec<&Visit> {
        let mut found: Vec<&Visit> = self
            .visits
            .iter()
            .filter(|v| keywords_match(query, &v.path))
            .collect();
        found.sort_by(|a, b| score(b, now).total_cmp(&score(a, now)));
        found
    }

    /// Keeps at most `MAX_ENTRIES`, dropping the lowest ranks (the oldest
    /// first among equal ones).
    fn trim(&mut self) {
        if self.visits.len() > MAX_ENTRIES {
            self.visits
                .sort_by(|a, b| b.rank.total_cmp(&a.rank).then(b.last.cmp(&a.last)));
            self.visits.truncate(MAX_ENTRIES);
        }
    }
}

/// zoxide's score: the rank, weighted by how recent the last visit is
/// (within the hour ×4, the day ×2, the week ×½, older ×¼).
pub fn score(v: &Visit, now: i64) -> f64 {
    let age = now.saturating_sub(v.last);
    let weight = if age < 3_600 {
        4.0
    } else if age < 86_400 {
        2.0
    } else if age < 604_800 {
        0.5
    } else {
        0.25
    };
    v.rank * weight
}

/// zoxide's rule: every word of `query` appears in the path, in order, and
/// the last word is in the folder's own name. Case and accents do not matter
/// ("proj fx" finds `C:\Users\ana\Projetos\fx`). A path that is not valid
/// Unicode never matches (nothing is converted with loss).
pub fn keywords_match(query: &str, path: &Path) -> bool {
    let words: Vec<String> = query.split_whitespace().map(fold).collect();
    let Some(last_word) = words.last() else {
        return false;
    };
    let Some(text) = path.to_str() else {
        return false;
    };
    let hay = fold(text);
    let mut at = 0;
    for w in &words {
        match hay.get(at..).and_then(|rest| rest.find(w.as_str())) {
            Some(i) => at += i + w.len(),
            None => return false,
        }
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| fold(name).contains(last_word.as_str()))
}

/// The same folder for NTFS (case does not matter).
fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.to_str(), b.to_str()) {
        (Some(x), Some(y)) => x.to_lowercase() == y.to_lowercase(),
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600;
    const DAY: i64 = 86_400;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn visits_raise_the_rank_and_case_does_not_matter() {
        let mut f = Frecency::default();
        f.visit(Path::new(r"C:\Projetos\fx"), 100);
        f.visit(Path::new(r"c:\projetos\FX"), 200);
        assert_eq!(f.visits().len(), 1);
        assert_eq!(f.visits()[0].rank, 2.0);
        assert_eq!(f.visits()[0].last, 200);
    }

    #[test]
    fn recent_visits_weigh_more() {
        let v = |last| Visit {
            path: p(r"C:\a"),
            rank: 4.0,
            last,
        };
        let now = 10 * 7 * DAY;
        assert_eq!(score(&v(now - 60), now), 16.0, "within the hour");
        assert_eq!(score(&v(now - 2 * HOUR), now), 8.0, "within the day");
        assert_eq!(score(&v(now - 3 * DAY), now), 2.0, "within the week");
        assert_eq!(score(&v(now - 30 * DAY), now), 1.0, "older");
        assert_eq!(score(&v(now + 50), now), 16.0, "clock went back");
    }

    #[test]
    fn words_in_order_and_the_last_in_the_folder_name() {
        let fx = Path::new(r"C:\Users\ana\Projetos\fx");
        assert!(keywords_match("proj fx", fx));
        assert!(keywords_match("PROJ FX", fx));
        assert!(keywords_match("fx", fx));
        assert!(!keywords_match("fx proj", fx), "order matters");
        assert!(
            !keywords_match("proj", fx),
            "the last word must be in the folder's name"
        );
        assert!(
            keywords_match("relat", Path::new(r"D:\Relatórios")),
            "accents fold"
        );
        assert!(keywords_match("relatorios", Path::new(r"D:\Relatórios")));
        assert!(!keywords_match("", fx));
        assert!(!keywords_match("   ", fx));
    }

    #[test]
    fn matches_come_most_frecent_first() {
        let now = 100 * DAY;
        let mut f = Frecency::default();
        f.visit(Path::new(r"C:\Antigo\fx"), now - 60 * DAY);
        for _ in 0..3 {
            f.visit(Path::new(r"C:\Antigo\fx"), now - 60 * DAY);
        }
        f.visit(Path::new(r"C:\Projetos\fx"), now - 10);
        let found: Vec<&Path> = f
            .matches("fx", now)
            .iter()
            .map(|v| v.path.as_path())
            .collect();
        // 4 visits two months ago (×¼ = 1) lose to 1 visit just now (×4 = 4).
        assert_eq!(
            found,
            [Path::new(r"C:\Projetos\fx"), Path::new(r"C:\Antigo\fx")]
        );
        assert_eq!(f.top(now, 1)[0].path, p(r"C:\Projetos\fx"));
    }

    #[test]
    fn aging_keeps_the_total_bounded_and_drops_the_forgotten() {
        let mut f = Frecency::default();
        f.visit(Path::new(r"C:\raro"), 0);
        for k in 0..10_100 {
            f.visit(Path::new(r"C:\sempre"), k);
        }
        let total: f64 = f.visits().iter().map(|v| v.rank).sum();
        assert!(total <= MAX_TOTAL, "{total}");
        assert!(
            f.visits().iter().all(|v| v.path != p(r"C:\raro")),
            "a rank below 1 is dropped"
        );
    }

    #[test]
    fn at_most_500_folders_and_forget() {
        let mut f = Frecency::default();
        for k in 0..600 {
            f.visit(&p(&format!(r"C:\pasta{k}")), k);
        }
        assert_eq!(f.visits().len(), MAX_ENTRIES);
        assert!(
            f.visits().iter().all(|v| v.path != p(r"C:\pasta0")),
            "the oldest go"
        );
        assert!(f.visits().iter().any(|v| v.path == p(r"C:\pasta599")));
        f.forget(Path::new(r"C:\PASTA599"));
        assert!(f.visits().iter().all(|v| v.path != p(r"C:\pasta599")));
    }
}
