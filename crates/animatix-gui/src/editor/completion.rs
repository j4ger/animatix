//! Completion popup support for the code editor.
//!
//! Completion is anchored to the live caret of the focused cell (tracked by
//! the cell renderer), so confirming an item splices text in place instead of
//! appending to the document.

use animatix_analyzer::CompletionItem;

use crate::editor::EditorBuffer;

/// Where a completion session applies: which cell, the caret offset within its
/// body, and the start of the partial word being replaced.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompletionAnchor {
    pub cell: usize,
    pub caret: usize,
    pub word_start: usize,
}

impl EditorBuffer {
    /// Trigger completion at the caret of the currently focused cell.
    pub(super) fn trigger_completion(&mut self) {
        let Some((cell, caret, word_start)) = self.caret_context() else {
            return;
        };
        let (items, trigger_text) = self.completion_items_for(cell, caret, word_start);
        self.completion_anchor = Some(CompletionAnchor {
            cell,
            caret,
            word_start,
        });
        self.completion.show(items, trigger_text);
    }

    /// Splice a confirmed completion into the focused cell at the caret,
    /// replacing the partial word that was being typed.
    pub(super) fn insert_completion(&mut self, insert_text: &str) {
        let Some(anchor) = self.completion_anchor.take() else {
            // Without an anchor there is no safe insertion point; appending to
            // the document (the old behaviour) corrupted the source.
            tracing::debug!("completion confirmed without an anchor; ignoring");
            return;
        };
        let Some(cell) = self.cells.get_mut(anchor.cell) else {
            return;
        };

        let chars: Vec<char> = cell.body().chars().collect();
        let caret = anchor.caret.min(chars.len());
        let start = anchor.word_start.min(caret);
        let before: String = chars[..start].iter().collect();
        let after: String = chars[caret..].iter().collect();
        let new_caret = start + insert_text.chars().count();
        cell.set_body(format!("{before}{insert_text}{after}"));

        self.text = crate::cell_editor::cells_to_source(&self.cells);
        self.cached_highlight = None;
        self.analyzer.update(&self.text);

        // Return focus and caret to the edited cell on the next frame.
        self.cell_state.focused_cell = Some(anchor.cell);
        self.cell_state.pending_cursor_cell = Some(anchor.cell);
        self.cell_state.pending_cursor_char = Some(new_caret);
    }

    /// Keep the completion session anchored to a live caret and auto-open it
    /// after a `.` (member access). Refines the open popup rather than
    /// re-showing it, so the selected entry survives continued typing.
    pub(super) fn sync_completion_session(&mut self) {
        let Some((cell, caret, word_start)) = self.caret_context() else {
            return;
        };

        if self.completion.is_visible() {
            self.completion_anchor = Some(CompletionAnchor {
                cell,
                caret,
                word_start,
            });
            let (items, trigger_text) = self.completion_items_for(cell, caret, word_start);
            self.completion.refine(items, trigger_text);
            return;
        }

        let signature = (cell, caret);
        if self.completion_auto_armed == Some(signature) {
            return;
        }
        let ch_before = self
            .cells
            .get(cell)
            .and_then(|c| caret.checked_sub(1).and_then(|i| c.body().chars().nth(i)));
        if ch_before == Some('.') {
            self.completion_auto_armed = Some(signature);
            self.trigger_completion();
        }
    }

    /// Resolve the focused cell and the caret/word offsets inside its body.
    fn caret_context(&self) -> Option<(usize, usize, usize)> {
        let cell_idx = self.cell_state.focused_cell?;
        let cell = self.cells.get(cell_idx)?;
        let chars: Vec<char> = cell.body().chars().collect();
        let caret = self.cell_state.focused_cursor_char.unwrap_or(chars.len()).min(chars.len());

        let mut word_start = caret;
        while word_start > 0 && is_completion_word_char(chars[word_start - 1]) {
            word_start -= 1;
        }
        Some((cell_idx, caret, word_start))
    }

    /// Query the analyzer for completions at the given body-relative caret,
    /// translating it to a document-absolute position. Returns the items and
    /// the partial word used to filter them.
    fn completion_items_for(
        &self,
        cell_idx: usize,
        caret: usize,
        word_start: usize,
    ) -> (Vec<CompletionItem>, String) {
        let prefix = crate::cell_editor::cells_to_source(&self.cells[..cell_idx]);
        let abs_char = prefix.chars().count() + caret;
        let abs_byte = char_to_byte_offset(&self.text, abs_char);
        let (line, _char_col) = self.byte_to_line_col(abs_byte);
        // The analyzer expects LSP UTF-16 code-unit columns, while the editor
        // tracks Unicode scalar (char) offsets. Emoji and other astral
        // characters occupy two UTF-16 units, so derive the column from the
        // actual line prefix instead of passing the char count through.
        let col_utf16 = utf16_column_at(&self.text, abs_byte);
        let items = self.analyzer.completions_at(line, col_utf16);

        let trigger_text: String = self
            .cells
            .get(cell_idx)
            .map(|c| c.body().chars().skip(word_start).take(caret - word_start).collect())
            .unwrap_or_default();
        (items, trigger_text)
    }
}

/// Characters that make up a completion-triggering word.
fn is_completion_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

/// Convert a char offset into a byte offset (clamped to the string end).
fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices().nth(char_offset).map(|(byte, _)| byte).unwrap_or(text.len())
}

/// UTF-16 code-unit column of `byte_offset` within its source line.
fn utf16_column_at(text: &str, byte_offset: usize) -> usize {
    let byte_offset = byte_offset.min(text.len());
    let line_start = text[..byte_offset].rfind('\n').map(|index| index + 1).unwrap_or(0);
    text[line_start..byte_offset].chars().map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::EditorBuffer;
    use super::utf16_column_at;

    #[test]
    fn utf16_column_counts_emoji_as_two_code_units() {
        // "😀" is one char / four UTF-8 bytes but two UTF-16 code units.
        let text = "box: Text, text: \"😀\" tail";
        let tail_byte = text.find(" tail").expect("tail");
        let char_count = text[..tail_byte].chars().count();

        assert_eq!(utf16_column_at(text, tail_byte), char_count + 1);
    }

    #[test]
    fn completion_query_uses_utf16_column_after_emoji() {
        let source = "let x = \"😀\"; box.\n#0s\n";
        let editor = EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string());
        let body = editor.cells()[0].body().to_string();
        let caret = body.chars().count();
        let mut expected_col = 0usize;
        let mut line = 0usize;
        for ch in body.chars() {
            if ch == '\n' {
                line += 1;
                expected_col = 0;
            } else {
                expected_col += ch.len_utf16();
            }
        }

        let (items, _) = editor.completion_items_for(0, caret, caret);
        let expected = editor.analyzer.completions_at(line, expected_col);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        let expected_labels: Vec<_> = expected.iter().map(|item| item.label.as_str()).collect();

        // The source contains astral characters before the caret; using the
        // char count as the analyzer column would land before the final `.`.
        assert_eq!(labels, expected_labels);
        assert_eq!(expected_col, caret + 1, "emoji adds one extra UTF-16 unit");
    }
}
