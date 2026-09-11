//! Find-match mapping — converts document-level find/replace regex hits into
//! cell-body byte ranges so the cell renderer can paint match backgrounds.
//!
//! Mirrors `diagnostics.rs`: document line/column positions are mapped to a
//! cell index plus cell-body-relative coordinates. Because the cell layouter
//! lays out the body string (not the document), matches are expressed as byte
//! offsets into the body.

use crate::cell_editor::CellFindMatch;
use crate::editor::EditorBuffer;

impl EditorBuffer {
    /// Recompute find-match decorations for the current query and options.
    ///
    /// `current_byte_offset` is the document byte offset of the find cursor
    /// (`UiStore::find_last_match`, the end of the previous Find Next hit). The
    /// match containing it, or the next match at/after it, is marked current;
    /// when the cursor is past the last match the search wraps to the first,
    /// matching `find_next_in_editor`. An empty query (or an invalid pattern)
    /// clears the list.
    pub fn set_find_state(
        &mut self,
        query: &str,
        case_sensitive: bool,
        whole_word: bool,
        is_regex: bool,
        current_byte_offset: Option<usize>,
    ) {
        self.cell_state.find_matches.clear();

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
                // Invalid pattern: the dialog already surfaces the error, so a
                // stale highlight set is worse than none.
                tracing::debug!("find: invalid pattern, clearing highlights: {err}");
                return;
            },
        };

        let text = self.text.clone();
        let mut mapped: Vec<(usize, usize, CellFindMatch)> = Vec::new();
        for m in re.find_iter(&text) {
            match self.map_match_to_cell(m.start(), m.end()) {
                Some((cell_index, rel_start_byte, rel_end_byte)) => {
                    mapped.push((
                        m.start(),
                        m.end(),
                        CellFindMatch {
                            cell_index,
                            rel_start_byte,
                            rel_end_byte,
                            is_current: false,
                        },
                    ));
                },
                None => {
                    // Header lines are not editable, so matches there have no
                    // body range to paint; log rather than dropping silently.
                    tracing::debug!(
                        "find: match at bytes {}..{} does not map to a cell body",
                        m.start(),
                        m.end()
                    );
                },
            }
        }

        let current = current_byte_offset.and_then(|off| pick_current_match(&mapped, off));
        for (index, (_, _, mut m)) in mapped.into_iter().enumerate() {
            m.is_current = current == Some(index);
            self.cell_state.find_matches.push(m);
        }
    }

    /// Map a document byte range to `(cell_index, rel_start_byte, rel_end_byte)`.
    ///
    /// Returns `None` when the match starts on a cell header line (not editable
    /// body text) or falls outside every cell.
    fn map_match_to_cell(
        &self,
        start_byte: usize,
        end_byte: usize,
    ) -> Option<(usize, usize, usize)> {
        let (doc_line, rel_col) = self.byte_to_line_col(start_byte);
        let cell_idx = self.cell_index_for_source_line(doc_line)?;
        let cell_start_line = self.source_line_for_cell(cell_idx)?;
        let cell = self.cells.get(cell_idx)?;

        // Number of header lines before the editable body (mirrors
        // diagnostics.rs).
        let header_lines = match cell {
            crate::cell_editor::Cell::Code { .. } => 0,
            crate::cell_editor::Cell::Keyframe {
                attached_comment, ..
            } => {
                let comment_lines =
                    attached_comment.as_ref().map(|c| c.lines().count()).unwrap_or(0);
                comment_lines + 1 // +1 for the #timestamp line
            },
        };
        let body_start_line = cell_start_line + header_lines;

        // Matches on the keyframe header/comment are not part of the body.
        if doc_line < body_start_line {
            return None;
        }

        let rel_line = doc_line - body_start_line;
        let body = cell.body();
        let rel_start = line_col_to_body_byte(body, rel_line, rel_col)?;
        // The body is a contiguous slice of the document from its first line, so
        // the match length carries over byte-for-byte. Clamp in case a regex
        // spans a cell boundary.
        let match_len = end_byte.saturating_sub(start_byte);
        let rel_end = (rel_start + match_len).min(body.len());
        if rel_start >= rel_end {
            return None;
        }
        Some((cell_idx, rel_start, rel_end))
    }
}

/// Convert a 0-indexed body line and char column to a byte offset in `body`.
fn line_col_to_body_byte(body: &str, line: usize, col: usize) -> Option<usize> {
    let mut line_start = 0usize;
    let mut current = 0usize;
    for (i, ch) in body.char_indices() {
        if current == line {
            break;
        }
        if ch == '\n' {
            current += 1;
            line_start = i + 1;
        }
    }
    if current != line {
        return None; // line is past the end of the body
    }

    let line_text = body[line_start..].split('\n').next().unwrap_or("");
    let byte_in_line = line_text.char_indices().nth(col).map(|(b, _)| b).unwrap_or(line_text.len());
    Some(line_start + byte_in_line)
}

/// Pick the index of the "current" match for a find cursor byte offset.
///
/// `entries` are `(document_start, document_end, match)` in document order.
/// Preference order: the match containing the offset, the match ending exactly
/// at it (the last Find Next hit), the next match starting at/after it, then
/// wrap to the first match.
fn pick_current_match(entries: &[(usize, usize, CellFindMatch)], offset: usize) -> Option<usize> {
    if let Some(i) = entries.iter().position(|(s, e, _)| offset >= *s && offset < *e) {
        return Some(i);
    }
    if let Some(i) = entries.iter().position(|(_, e, _)| *e == offset) {
        return Some(i);
    }
    if let Some(i) = entries.iter().position(|(s, _, _)| *s >= offset) {
        return Some(i);
    }
    if entries.is_empty() { None } else { Some(0) }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::cell_editor::Cell;

    /// Document layout used by the tests (line numbers in comments):
    /// ```text
    /// 0: box: Rect, size: (100, 100)
    /// 1: #0s
    /// 2: box.pos = (0, 0)
    /// 3: box.color = red
    /// ```
    fn editor() -> EditorBuffer {
        let source = "box: Rect, size: (100, 100)\n#0s\nbox.pos = (0, 0)\nbox.color = red\n";
        EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string())
    }

    fn find(editor: &EditorBuffer) -> Vec<(usize, usize, usize, bool)> {
        editor
            .cell_state
            .find_matches
            .iter()
            .map(|m| (m.cell_index, m.rel_start_byte, m.rel_end_byte, m.is_current))
            .collect()
    }

    #[test]
    fn matches_map_to_cell_body_byte_ranges() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, None);

        // One match in the code cell (line 0) and two in the keyframe body
        // (lines 2 and 3, i.e. body lines 0 and 1).
        assert_eq!(
            find(&editor),
            vec![
                (0, 0, 3, false),
                // "box.pos = (0, 0)" is 16 bytes, so line 1 starts at 17.
                (1, 0, 3, false),
                (1, 17, 20, false),
            ]
        );
        assert_eq!(editor.cells.len(), 2);
        assert!(matches!(editor.cells[1], Cell::Keyframe { .. }));
    }

    #[test]
    fn multiple_matches_in_one_cell_are_all_reported() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, None);

        let in_keyframe: Vec<_> = editor
            .cell_state
            .find_matches
            .iter()
            .filter(|m| m.cell_index == 1)
            .map(|m| (m.rel_start_byte, m.rel_end_byte))
            .collect();
        assert_eq!(in_keyframe, vec![(0, 3), (17, 20)]);
    }

    #[test]
    fn keyframe_header_match_is_excluded() {
        let mut editor = editor();
        // "0s" only occurs on the `#0s` header line of the keyframe cell.
        editor.set_find_state("0s", true, false, false, None);
        assert!(
            editor.cell_state.find_matches.is_empty(),
            "matches on non-editable keyframe headers must not be mapped: {:?}",
            find(&editor)
        );
    }

    #[test]
    fn current_match_marks_the_find_next_hit() {
        let mut editor = editor();
        // `find_last_match` holds the end of the last Find Next hit. The first
        // "box" match ends at byte 3.
        editor.set_find_state("box", true, false, false, Some(3));
        assert_eq!(find(&editor), vec![(0, 0, 3, true), (1, 0, 3, false), (1, 17, 20, false)]);

        // A cursor offset between hits advances to the next match (match 1 ends
        // at document byte 35: 28 (line 0 + newline) + 4 ("#0s\n") + 3).
        editor.set_find_state("box", true, false, false, Some(35));
        let current: Vec<_> =
            editor.cell_state.find_matches.iter().filter(|m| m.is_current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].cell_index, 1);
        assert_eq!(current[0].rel_start_byte, 0);
    }

    #[test]
    fn current_match_wraps_past_the_last_hit() {
        let mut editor = editor();
        // Past the end of the document: search wraps to the first match.
        editor.set_find_state("box", true, false, false, Some(usize::MAX));
        assert_eq!(find(&editor), vec![(0, 0, 3, true), (1, 0, 3, false), (1, 17, 20, false)]);
    }

    #[test]
    fn empty_query_clears_matches() {
        let mut editor = editor();
        editor.set_find_state("box", true, false, false, None);
        assert!(!editor.cell_state.find_matches.is_empty());
        editor.set_find_state("", true, false, false, None);
        assert!(editor.cell_state.find_matches.is_empty());
    }

    #[test]
    fn options_are_honoured() {
        let mut editor = editor();
        // Case-sensitive "Box" matches nothing; case-insensitive does not occur
        // either, so use whole-word to drop the "box.pos"/"box.color" hits.
        editor.set_find_state("box", true, true, false, None);
        let rels: Vec<_> = editor
            .cell_state
            .find_matches
            .iter()
            .map(|m| (m.cell_index, m.rel_start_byte))
            .collect();
        // Whole-word "box" still matches all three standalone occurrences.
        assert_eq!(rels, vec![(0, 0), (1, 0), (1, 17)]);

        editor.set_find_state("bo", true, false, false, None);
        assert!(editor.cell_state.find_matches.iter().all(|m| {
            let body = editor.cells[m.cell_index].body();
            &body[m.rel_start_byte..m.rel_end_byte] == "bo"
        }));
    }

    #[test]
    fn line_col_to_body_byte_maps_lines_and_columns() {
        let body = "box.pos = (0, 0)\nbox.color = red";
        assert_eq!(line_col_to_body_byte(body, 0, 0), Some(0));
        assert_eq!(line_col_to_body_byte(body, 1, 0), Some(17));
        assert_eq!(line_col_to_body_byte(body, 1, 4), Some(21));
        assert_eq!(line_col_to_body_byte(body, 2, 0), None, "line past end maps to None");
        // Columns past the end of a line clamp to the line end.
        assert_eq!(line_col_to_body_byte(body, 0, 999), Some(16));
    }

    #[test]
    fn match_byte_ranges_slice_back_to_the_query() {
        let mut editor = editor();
        editor.set_find_state("color", true, false, false, None);
        assert_eq!(editor.cell_state.find_matches.len(), 1);
        let m = editor.cell_state.find_matches[0];
        let body = editor.cells[m.cell_index].body();
        assert_eq!(&body[m.rel_start_byte..m.rel_end_byte], "color");
    }
}
