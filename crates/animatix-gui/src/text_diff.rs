//! Minimal text diff for editor updates.
//!
//! Computes a single non-overlapping byte-range edit for the common case where
//! only a small middle section changed.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start_byte: usize,
    pub end_byte: usize,
    pub replacement: String,
}

/// Compute minimal text edits using a common-prefix/common-suffix diff.
///
/// Returns edits in forward order (sorted by `start_byte`).
pub fn diff_text(old: &str, new: &str) -> Vec<TextEdit> {
    if old == new {
        return vec![];
    }

    let old_chars: Vec<(usize, char)> = old.char_indices().collect();
    let new_chars: Vec<(usize, char)> = new.char_indices().collect();

    let prefix_chars =
        old_chars.iter().zip(&new_chars).take_while(|((_, a), (_, b))| a == b).count();

    let mut suffix_chars = 0usize;
    while prefix_chars + suffix_chars < old_chars.len()
        && prefix_chars + suffix_chars < new_chars.len()
        && old_chars[old_chars.len() - 1 - suffix_chars].1
            == new_chars[new_chars.len() - 1 - suffix_chars].1
    {
        suffix_chars += 1;
    }

    let start_byte = old_chars.get(prefix_chars).map(|(byte, _)| *byte).unwrap_or(old.len());
    let end_byte = if suffix_chars == 0 {
        old.len()
    } else {
        old_chars[old_chars.len() - suffix_chars].0
    };

    let new_start_byte = new_chars.get(prefix_chars).map(|(byte, _)| *byte).unwrap_or(new.len());
    let new_end_byte = if suffix_chars == 0 {
        new.len()
    } else {
        new_chars[new_chars.len() - suffix_chars].0
    };

    vec![TextEdit {
        start_byte,
        end_byte,
        replacement: new[new_start_byte..new_end_byte].to_string(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_has_no_edits() {
        assert!(diff_text("abc", "abc").is_empty());
    }

    #[test]
    fn replaces_middle_span() {
        let edits = diff_text("hello world", "hello brave world");
        assert_eq!(
            edits,
            vec![TextEdit {
                start_byte: 6,
                end_byte: 6,
                replacement: "brave ".to_string(),
            }]
        );
    }

    #[test]
    fn handles_unicode_boundaries() {
        let edits = diff_text("héllo", "héyllo");
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].replacement, "y");
    }
}
