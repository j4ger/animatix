//! Code-action inputs: spelling suggestions for likely typos, plus the source
//! ranges the structural fixes need.
//!
//! The two most common diagnostics in real content are `undefined-label` (a
//! reference to a name nothing declares) and `unused-label` (a declaration
//! nothing references). They are usually two views of the same typo, so the
//! useful fixes are "did you mean X?" over the names that *do* exist in the
//! same role, "remove the unused declaration", and "declare the missing
//! actor". This module produces the candidates and computes the ranges; the
//! LSP turns them into edits.

use animatix_syntax::occurrence::{Occurrence, OccurrenceKind};
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

/// True when `byte` sits at top level (outside every brace/paren block).
///
/// An *inline child* declaration (an effect stage inside a `Filter`, an actor
/// inside a `Row`) renders as part of its parent even when its label is never
/// referenced, so a "remove unused" fix must only ever be offered for
/// top-level declarations, which are invisible without an entrance action.
pub fn is_top_level_position(source: &str, byte: usize) -> bool {
    let bytes = source.as_bytes();
    let mut brace_depth: i64 = 0;
    let mut paren_depth: i64 = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut index = 0;
    while index < bytes.len() && index < byte {
        match bytes[index] {
            b'"' => {
                in_string = !in_string;
                escape = false;
            },
            b'\\' if in_string => escape = !escape,
            b'\n' => in_string = false,
            _ if in_string => {},
            b'{' => brace_depth += 1,
            b'}' => brace_depth -= 1,
            b'(' => paren_depth += 1,
            b')' => paren_depth -= 1,
            _ => {},
        }
        index += 1;
    }
    brace_depth <= 0 && paren_depth <= 0
}

/// Byte offset of the first declaration of `name`, if one was recorded.
pub fn first_declaration_byte(occurrences: &[Occurrence], name: &str) -> Option<usize> {
    occurrences
        .iter()
        .find(|o| o.declaration && o.name == name && matches!(o.kind, OccurrenceKind::Label))
        .map(|o| o.span.start)
}

/// The full source range of the statement containing `decl_byte`.
///
/// Returns `(start, end)` where `start` is the beginning of the declaration's
/// line (including indentation) and `end` is just past its terminating
/// newline, so a delete edit removes the whole statement cleanly.
///
/// The statement extends past its first line only through brace or parenthesis
/// nesting; string literals, `$$` typst blocks, and `//` comments on the way
/// are skipped so their braces and newlines do not end (or extend) the
/// statement early. Returns `None` when nesting goes negative (a stray closer)
/// — the source is not shaped like a removable statement.
pub fn statement_removal_range(source: &str, decl_byte: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    if decl_byte >= bytes.len() {
        return None;
    }
    let start = bytes[..decl_byte]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |newline| newline + 1);

    let mut brace_depth: i64 = 0;
    let mut paren_depth: i64 = 0;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        b'"' => {
                            index += 1;
                            break;
                        },
                        b'\n' => break, // unterminated: the newline ends the line
                        _ => index += 1,
                    }
                }
            },
            b'$' if bytes.get(index + 1) == Some(&b'$') => {
                // `$$` typst block: skip to the closing `$$`. The loop leaves
                // `index` just past the closer when it is found.
                index += 2;
                while index + 1 < bytes.len() {
                    if bytes[index] == b'$' && bytes[index + 1] == b'$' {
                        index += 2;
                        break;
                    }
                    index += 1;
                }
            },
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            },
            b'{' => {
                brace_depth += 1;
                index += 1;
            },
            b'}' => {
                brace_depth -= 1;
                if brace_depth < 0 {
                    return None;
                }
                index += 1;
            },
            b'(' => {
                paren_depth += 1;
                index += 1;
            },
            b')' => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return None;
                }
                index += 1;
            },
            b'\n' if brace_depth == 0 && paren_depth == 0 => {
                return Some((start, index + 1));
            },
            _ => index += 1,
        }
    }
    // End of input with balanced nesting: the statement runs to the end.
    Some((start, bytes.len()))
}

/// The line index a new top-level declaration should be inserted before.
///
/// Declarations must precede their uses, and uses live in keyframes
/// (`#0s …`) and `always` blocks — so insert before the first top-level
/// keyframe marker, else before the first top-level `always`, else at the end
/// of the file. Returns `source.lines().count()` for the append case.
pub fn declaration_insertion_line(source: &str) -> usize {
    let mut brace_depth: i64 = 0;
    let mut paren_depth: i64 = 0;
    let mut in_string = false;
    let mut escape = false;
    let first_keyframe: Option<usize> = None;
    let mut first_always: Option<usize> = None;

    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if brace_depth == 0 && paren_depth == 0 {
            if first_keyframe.is_none() && is_keyframe_marker(trimmed) {
                return index;
            }
            if first_always.is_none() && trimmed.starts_with("always") {
                first_always = Some(index);
            }
        }
        // Track nesting so a `#…` inside a block or string is not mistaken
        // for a top-level keyframe marker.
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_string {
                match c {
                    '\\' => escape = true,
                    '"' if !escape => in_string = false,
                    _ => escape = false,
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '/' if chars.peek() == Some(&'/') => break,
                '{' => brace_depth += 1,
                '}' => brace_depth -= 1,
                '(' => paren_depth += 1,
                ')' => paren_depth -= 1,
                _ => {},
            }
        }
    }

    first_keyframe.or(first_always).unwrap_or(source.lines().count())
}

/// True when a trimmed line starts an absolute or relative keyframe
/// (`#0s`, `# 0.5s`, `#+1s`), as opposed to a scene marker (`# Intro`).
fn is_keyframe_marker(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    let rest = rest.strip_prefix('+').unwrap_or(rest);
    let rest = rest.trim_start();
    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// The statement text to insert when declaring a missing actor named `name`.
///
/// `Rect` with an explicit size renders visibly and accepts every common
/// action as a target, which is what the author was trying to reference.
pub fn missing_actor_statement(name: &str) -> String {
    format!("{name}: Rect, size: (100, 100)\n")
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
    #[test]
    fn removal_range_covers_single_line_statement() {
        let source = "box: Rect, size: (100, 100)\nkeep: Text, text: \"k\"\n";
        let decl = source.find("box").unwrap();
        let (start, end) = statement_removal_range(source, decl).unwrap();
        assert_eq!(&source[start..end], "box: Rect, size: (100, 100)\n");
    }

    #[test]
    fn removal_range_covers_braced_statement() {
        let source = "panel: Filter, size: (1, 2) {\n  pix: Blur, radius: 4\n}\nkeep: Text\n";
        let decl = source.find("panel").unwrap();
        let (start, end) = statement_removal_range(source, decl).unwrap();
        assert_eq!(
            &source[start..end],
            "panel: Filter, size: (1, 2) {\n  pix: Blur, radius: 4\n}\n"
        );
    }

    #[test]
    fn removal_range_ignores_braces_in_strings_and_comments() {
        let source = "t: Text, text: \"a}b\" // } not a close\nkeep: Text\n";
        let decl = source.find("t:").unwrap();
        let (start, end) = statement_removal_range(source, decl).unwrap();
        assert_eq!(&source[start..end].lines().count(), &1);
        assert!(source[start..end].contains("a}b"));
    }

    #[test]
    fn removal_range_ignores_typst_blocks() {
        let source = "eq: Typst, content: $$ x { y } $$\nkeep: Text\n";
        let decl = source.find("eq:").unwrap();
        let (start, end) = statement_removal_range(source, decl).unwrap();
        assert_eq!(&source[start..end].lines().count(), &1);
    }

    #[test]
    fn insertion_line_lands_before_first_keyframe() {
        let source = "a: Text, text: \"x\"\n#0s\nfade-in a [1s]\n";
        assert_eq!(declaration_insertion_line(source), 1);
    }

    #[test]
    fn insertion_line_handles_spaced_keyframes_and_always() {
        assert_eq!(declaration_insertion_line("# 0s\n"), 0, "spaced marker");
        assert_eq!(declaration_insertion_line("#+1s\n"), 0, "relative marker");
        assert_eq!(declaration_insertion_line("# Intro\n"), 1, "scene marker is not a keyframe");
        assert_eq!(declaration_insertion_line("always {\n  x = 1\n}\n"), 0);
        assert_eq!(declaration_insertion_line(""), 0, "empty file appends at 0");
    }

    #[test]
    fn insertion_skips_keyframe_markers_inside_blocks() {
        // The `#` here sits inside a string inside a block: not a top-level
        // keyframe, so insertion falls back to the end.
        let source = "t: Text, text: \"#0s\"\n";
        assert_eq!(declaration_insertion_line(source), 1);
    }

    #[test]
    fn missing_actor_statement_is_parseable() {
        let statement = missing_actor_statement("ghost");
        assert_eq!(statement, "ghost: Rect, size: (100, 100)\n");
    }
}
