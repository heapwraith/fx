//! Path bar autocompletion (task T1.3): split what was typed into the parent
//! folder and the partial name, and match the partial name against the
//! parent's subfolders. Folder listing is the caller's job (background, cached
//! per parent).

use crate::natural::{fold, natural_cmp};

/// `C:\Users\an` → (`C:\Users\`, `an`). The parent keeps its trailing
/// separator; `None` when there is no separator at all.
pub fn split_typed(text: &str) -> Option<(&str, &str)> {
    let cut = text.rfind(['\\', '/'])? + 1;
    Some((&text[..cut], &text[cut..]))
}

/// Subfolder names of one folder, ready for fast prefix matching.
#[derive(Debug, Clone, Default)]
pub struct Candidates {
    names: Vec<String>,
    folded: Vec<String>,
}

impl Candidates {
    /// Sorts the names naturally; matching keeps that order.
    pub fn new(names: Vec<String>) -> Self {
        let mut pairs: Vec<(String, String)> = names.into_iter().map(|n| (fold(&n), n)).collect();
        pairs.sort_by(|a, b| natural_cmp(&a.0, &b.0).then_with(|| a.1.cmp(&b.1)));
        let (folded, names) = pairs.into_iter().unzip();
        Self { names, folded }
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    pub fn name(&self, i: usize) -> &str {
        &self.names[i]
    }

    /// Up to `max` names starting with `partial` (case and accents ignored),
    /// in natural order.
    pub fn matching(&self, partial: &str, max: usize) -> Vec<usize> {
        let p = fold(partial);
        self.folded
            .iter()
            .enumerate()
            .filter(|(_, f)| f.starts_with(&p))
            .map(|(i, _)| i)
            .take(max)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_parent_and_partial() {
        assert_eq!(split_typed(r"C:\Users\an"), Some((r"C:\Users\", "an")));
        assert_eq!(split_typed(r"C:\"), Some((r"C:\", "")));
        assert_eq!(split_typed("C:/a/b"), Some(("C:/a/", "b")));
        assert_eq!(
            split_typed(r"\\server\share\x"),
            Some((r"\\server\share\", "x"))
        );
        assert_eq!(split_typed("nothing"), None);
    }

    #[test]
    fn matches_prefix_ignoring_case_and_accents_in_natural_order() {
        let c = Candidates::new(
            ["pasta10", "Pasta2", "Música", "musicas", "outra", "pasta1"]
                .map(String::from)
                .to_vec(),
        );
        let names = |v: Vec<usize>| v.into_iter().map(|i| c.name(i)).collect::<Vec<_>>();
        assert_eq!(
            names(c.matching("pas", 10)),
            ["pasta1", "Pasta2", "pasta10"]
        );
        assert_eq!(names(c.matching("MUSI", 10)), ["Música", "musicas"]);
        assert_eq!(names(c.matching("mús", 10)), ["Música", "musicas"]);
        assert_eq!(c.matching("", 10).len(), 6);
        assert_eq!(c.matching("", 2).len(), 2);
        assert!(c.matching("zzz", 10).is_empty());
    }
}
