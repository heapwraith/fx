//! Fuzzy matching for the command palette (task T1.24): the query's
//! characters must appear in order in the candidate (case and accents
//! ignored); the score favours consecutive runs, word starts and an early
//! first match, so "nv pa" finds "Nova pasta" first.

use crate::natural::fold;

const MATCH: i32 = 16;
const CONSECUTIVE: i32 = 12;
const WORD_START: i32 = 10;
const FIRST_AT_START: i32 = 8;
const GAP: i32 = 1;

/// Score of `candidate` for `query`, or `None` when it does not match.
/// Spaces in the query are ignored (they only separate words).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = fold(query).chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let c: Vec<char> = fold(candidate).chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &ch) in c.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if ch != q[qi] {
            continue;
        }
        score += MATCH;
        let word_start = i == 0 || !c[i - 1].is_alphanumeric();
        if word_start {
            score += WORD_START;
        }
        match last {
            Some(prev) if prev + 1 == i => score += CONSECUTIVE,
            Some(prev) => score -= GAP * (i - prev - 1).min(8) as i32,
            None if i == 0 => score += FIRST_AT_START,
            None => score -= GAP * i.min(8) as i32,
        }
        last = Some(i);
        qi += 1;
    }
    (qi == q.len()).then_some(score)
}

/// Indices of the matching candidates, best first (ties: shorter text, then
/// the original order).
pub fn fuzzy_rank<'a>(query: &str, candidates: impl IntoIterator<Item = &'a str>) -> Vec<usize> {
    let mut scored: Vec<(usize, i32, usize)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(i, text)| fuzzy_score(query, text).map(|s| (i, s, text.len())))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_ignoring_case_and_accents() {
        assert!(fuzzy_score("nvpa", "Nova pasta").is_some());
        assert!(fuzzy_score("NOVA", "nova pasta").is_some());
        assert!(fuzzy_score("acao", "Ação").is_some());
        assert!(
            fuzzy_score("pasta nova", "Nova pasta").is_none(),
            "order matters"
        );
        assert!(fuzzy_score("xyz", "Nova pasta").is_none());
        assert_eq!(fuzzy_score("", "qualquer"), Some(0));
    }

    #[test]
    fn ranking_prefers_word_starts_and_runs() {
        let items = [
            "Fechar aba",
            "Nova aba",
            "Nova pasta",
            "Mostrar itens ocultos",
        ];
        let ranked = fuzzy_rank("nova", items);
        assert_eq!(&ranked[..2], &[1, 2], "both 'Nova…', shorter first");
        assert_eq!(fuzzy_rank("np", items)[0], 2, "N-ova P-asta: word starts");
        assert_eq!(fuzzy_rank("ocu", items), [3]);
        assert_eq!(
            fuzzy_rank("aba", items)[..2],
            [1, 0],
            "the earlier match first"
        );
    }

    #[test]
    fn many_candidates_rank_fast() {
        let names: Vec<String> = (0..10_000)
            .map(|i| format!("pasta de projeto {i}"))
            .collect();
        let started = std::time::Instant::now();
        let ranked = fuzzy_rank("pp9", names.iter().map(String::as_str));
        assert!(!ranked.is_empty());
        // The palette has a few hundred items; 10k must stay far below a frame
        // even in debug builds.
        assert!(
            started.elapsed().as_millis() < 200,
            "{:?}",
            started.elapsed()
        );
    }
}
