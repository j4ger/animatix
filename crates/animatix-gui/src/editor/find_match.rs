//! Find-highlight ranges in document byte coordinates.

use crate::editor::EditorBuffer;
use crate::highlighting::FindHighlight;

impl EditorBuffer {
    /// Refresh find decorations for the current query and options.
    ///
    /// `current_byte_offset` is the end offset of the previous Find Next match.
    /// The containing/next match is marked current, with wraparound.
    pub fn set_find_state(
        &mut self,
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
        is_regex: bool,
        current_byte_offset: Option<usize>,
    ) {
        self.find_highlights.clear();
        if query.is_empty() {
            return;
        }
        let re = match crate::app::shell::find_replace::build_pattern(
            query,
            case_sensitive,
            whole_word,
            is_regex,
        ) {
            Ok(re) => re,
            Err(err) => {
                tracing::debug!("find: invalid pattern, clearing highlights: {err}");
                return;
            },
        };

        let matches: Vec<(usize, usize)> = re
            .find_iter(&self.text)
            .map(|matched| (matched.start(), matched.end()))
            .collect();
        let current = current_byte_offset.and_then(|offset| pick_current_match(&matches, offset));
        self.find_highlights = matches
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| FindHighlight {
                start,
                end,
                current: current == Some(index),
            })
            .collect();
    }
}

/// Pick the current match by containing offset, exact previous-match end,
/// next match, then wrap to the first match.
fn pick_current_match(entries: &[(usize, usize)], offset: usize) -> Option<usize> {
    if let Some(index) = entries.iter().position(|(start, end)| offset >= *start && offset < *end) {
        return Some(index);
    }
    if let Some(index) = entries.iter().position(|(_, end)| *end == offset) {
        return Some(index);
    }
    if let Some(index) = entries.iter().position(|(start, _)| *start >= offset) {
        return Some(index);
    }
    (!entries.is_empty()).then_some(0)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::editor::EditorBuffer;

    fn editor() -> EditorBuffer {
        let source = "box: Rect, size: (100, 100)\n#0s\nbox.pos = (0, 0)\nbox.color = red\n";
        EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string())
    }

    fn matches(editor: &EditorBuffer) -> Vec<(usize, usize, bool)> {
        editor
            .find_highlights
            .iter()
            .map(|matched| (matched.start, matched.end, matched.current))
            .collect()
    }

    #[test]
    fn matches_are_document_byte_ranges_including_headers() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, None);
        assert_eq!(matches(&editor), vec![(0, 3, false), (32, 35, false), (49, 52, false)]);

        editor.set_find_state("0s", true, false, false, None);
        assert_eq!(matches(&editor), vec![(29, 31, false)]);
    }

    #[test]
    fn current_match_wraps_and_options_are_honoured() {
        let mut editor = editor();
        editor.set_find_state("box", true, true, false, Some(3));
        assert_eq!(matches(&editor), vec![(0, 3, true), (32, 35, false), (49, 52, false)]);

        editor.set_find_state("box", true, true, false, Some(usize::MAX));
        assert!(matches(&editor)[0].2);
        editor.set_find_state("BOX", true, false, false, None);
        assert!(editor.find_highlights.is_empty());
        editor.set_find_state("box", false, true, false, None);
        assert_eq!(editor.find_highlights.len(), 3);
    }

    #[test]
    fn empty_or_invalid_pattern_clears_matches() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, None);
        assert!(!editor.find_highlights.is_empty());
        editor.set_find_state("[", true, false, true, None);
        assert!(editor.find_highlights.is_empty());
        editor.set_find_state("", true, false, false, None);
        assert!(editor.find_highlights.is_empty());
    }

    #[test]
    fn current_match_advances_between_hits() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, Some(35));
        assert_eq!(matches(&editor), vec![(0, 3, false), (32, 35, true), (49, 52, false)]);
    }
}
