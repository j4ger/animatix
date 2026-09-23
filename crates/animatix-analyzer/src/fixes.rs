//! Code-action inputs: spelling suggestions for likely typos.
//!
//! The two most common diagnostics in real content are `undefined-label` (a
//! reference to a name nothing declares) and `unused-label` (a declaration
//! nothing references). They are usually two views of the same typo, so the
//! useful fix is "did you mean X?" over the names that *do* exist in the same
//! role. This module produces those candidates; the LSP turns them into edits.

use animatix_syntax::symbol_table::SymbolTable;

/// A suggested replacement for a misspelled name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellingCandidate {
    /// The existing name the author probably meant.
    pub name: String,
    /// Edit distance from the misspelling.
    pub distance: usize,
}

/// Maximum edit distance considered a plausible typo.
///
/// Scaled by name length: short names need an exact-ish match (one edit on a
/// three-letter name is usually coincidence), longer names tolerate two.
fn max_distance_for(len: usize) -> usize {
    match len {
        0..=3 => 1,
        4..=8 => 2,
        _ => 3,
    }
}

/// Levenshtein distance, capped at `cap` for early exit.
fn edit_distance(a: &str, b: &str, cap: usize) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len().abs_diff(b.len()) > cap {
        return cap + 1;
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Suggest declared label names close to `misspelled`.
///
/// Only names present in the symbol table are considered, so every suggestion
/// is a name that actually exists in this file (or its imports). Results are
/// sorted by distance then name, and capped at `limit`.
pub fn suggest_label_names(
    symbols: &SymbolTable,
    misspelled: &str,
    limit: usize,
) -> Vec<SpellingCandidate> {
    let cap = max_distance_for(misspelled.chars().count());
    let mut candidates: Vec<SpellingCandidate> = symbols
        .labels
        .keys()
        .filter(|name| name.as_str() != misspelled)
        .filter_map(|name| {
            let distance = edit_distance(misspelled, name, cap);
            (distance <= cap).then(|| SpellingCandidate {
                name: name.clone(),
                distance,
            })
        })
        .collect();
    candidates.sort_by(|a, b| a.distance.cmp(&b.distance).then_with(|| a.name.cmp(&b.name)));
    candidates.truncate(limit);
    candidates
}

/// Suggest action verbs close to `misspelled`.
pub fn suggest_action_names(
    symbols: &SymbolTable,
    misspelled: &str,
    limit: usize,
) -> Vec<SpellingCandidate> {
    let cap = max_distance_for(misspelled.chars().count());
    let mut candidates: Vec<SpellingCandidate> = symbols
        .actions
        .iter()
        .filter(|name| name.as_str() != misspelled)
        .filter_map(|name| {
            let distance = edit_distance(misspelled, name, cap);
            (distance <= cap).then(|| SpellingCandidate {
                name: name.clone(),
                distance,
            })
        })
        .collect();
    candidates.sort_by(|a, b| a.distance.cmp(&b.distance).then_with(|| a.name.cmp(&b.name)));
    candidates.truncate(limit);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Analyzer;

    fn symbol_table(source: &str) -> SymbolTable {
        Analyzer::new(source).symbols().clone()
    }

    #[test]
    fn suggests_close_label_names() {
        let symbols = symbol_table("header: Text, text: \"h\"\nbody: Text, text: \"b\"\n");
        let picks = suggest_label_names(&symbols, "haeder", 3);
        assert_eq!(picks.first().map(|c| c.name.as_str()), Some("header"));
    }

    #[test]
    fn ignores_unrelated_names() {
        let symbols = symbol_table("header: Text, text: \"h\"\n");
        assert!(
            suggest_label_names(&symbols, "zzzzzzzz", 3).is_empty(),
            "a name nothing resembles must yield no suggestion"
        );
    }

    #[test]
    fn short_names_need_a_near_match() {
        let symbols = symbol_table("ab: Text, text: \"x\"\n");
        // One edit from a two-letter name is still plausible (`abc` -> `ab`).
        let picks = suggest_label_names(&symbols, "abc", 3);
        assert_eq!(picks.first().map(|c| c.name.as_str()), Some("ab"));
    }

    #[test]
    fn suggests_close_action_verbs() {
        let symbols = symbol_table("box: Rect, size: (1, 1)\n");
        let picks = suggest_action_names(&symbols, "fadein", 3);
        assert!(
            picks.iter().any(|c| c.name == "fade-in"),
            "hyphenated verb is suggested for its unhyphenated typo: {picks:?}"
        );
    }

    #[test]
    fn results_are_ranked_and_capped() {
        let symbols = symbol_table(
            "card: Text, text: \"a\"\ncard2: Text, text: \"b\"\ncart: Text, text: \"c\"\n",
        );
        let picks = suggest_label_names(&symbols, "card", 2);
        assert!(picks.len() <= 2, "limit respected");
        // Sorted ascending by distance.
        for pair in picks.windows(2) {
            assert!(pair[0].distance <= pair[1].distance);
        }
    }
}
