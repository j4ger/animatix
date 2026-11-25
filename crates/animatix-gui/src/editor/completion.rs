//! Analyzer completions for the single source buffer.

use animatix_analyzer::CompletionItem;

use crate::editor::EditorBuffer;

/// Where a completion session applies, using Unicode-scalar offsets into the
/// complete source buffer.
#[derive(Clone, Copy, Debug)]
pub(super) struct CompletionAnchor {
    pub caret: usize,
    pub word_start: usize,
}

impl EditorBuffer {
    /// Trigger completion at the current TextEdit caret.
    pub(super) fn trigger_completion(&mut self) {
        let Some((caret, word_start)) = self.caret_context() else {
            return;
        };
        let (items, trigger_text) = self.completion_items_for(caret, word_start);
        self.completion_anchor = Some(CompletionAnchor { caret, word_start });
        self.completion.show(items, trigger_text);
    }

    /// Insert the selected completion over the partial word at its saved caret.
    pub(super) fn insert_completion(&mut self, insert_text: &str) {
        let Some(anchor) = self.completion_anchor.take() else {
            tracing::debug!("completion confirmed without an anchor; ignoring");
            return;
        };
        let chars: Vec<char> = self.text.chars().collect();
        let caret = anchor.caret.min(chars.len());
        let start = anchor.word_start.min(caret);
        let before: String = chars[..start].iter().collect();
        let after: String = chars[caret..].iter().collect();
        let new_caret = start + insert_text.chars().count();
        self.text = format!("{before}{insert_text}{after}");
        self.after_source_replaced();
        self.pending_cursor_char = Some(new_caret);
        self.cursor_char = Some(new_caret);
        self.pending_cursor_focus = true;
        self.completion_auto_armed = Some(new_caret);
    }

    /// Keep the popup query aligned with the current caret and auto-open after
    /// member access. A dismissed popup stays closed until the caret moves.
    pub(super) fn sync_completion_session(&mut self) {
        let Some((caret, word_start)) = self.caret_context() else {
            return;
        };

        if self.completion.is_visible() {
            self.completion_anchor = Some(CompletionAnchor { caret, word_start });
            let (items, trigger_text) = self.completion_items_for(caret, word_start);
            self.completion.refine(items, trigger_text);
            return;
        }

        if self.completion_auto_armed == Some(caret) {
            return;
        }
        let previous = self.text.chars().take(caret).last();
        if previous == Some('.') {
            self.completion_auto_armed = Some(caret);
            self.trigger_completion();
        }
    }

    fn caret_context(&self) -> Option<(usize, usize)> {
        let caret = self.cursor_char?.min(self.text.chars().count());
        let chars: Vec<char> = self.text.chars().collect();
        let mut word_start = caret;
        while word_start > 0 && is_completion_word_char(chars[word_start - 1]) {
            word_start -= 1;
        }
        Some((caret, word_start))
    }

    /// Query analyzer completions at a document-absolute caret. Its columns
    /// use UTF-16 units; TextEdit cursor offsets use Unicode scalar values.
    fn completion_items_for(
        &self,
        caret: usize,
        word_start: usize,
    ) -> (Vec<CompletionItem>, String) {
        let abs_byte = char_to_byte_offset(&self.text, caret);
        let index = animatix_syntax::token::LineIndex::new(&self.text);
        let (line, col_utf16) = index.byte_to_line_col_utf16(abs_byte);
        let items = self.analyzer().completions_at(line, col_utf16);
        let trigger_text = self
            .text
            .chars()
            .skip(word_start)
            .take(caret.saturating_sub(word_start))
            .collect();
        (items, trigger_text)
    }
}

fn is_completion_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices().nth(char_offset).map(|(byte, _)| byte).unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::editor::EditorBuffer;

    #[test]
    fn completion_query_uses_utf16_column_after_emoji() {
        let source = "title: Text, content: \"😀\" { s }";
        let editor = EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string());
        let byte = source.find("{ s }").expect("property block") + 2;
        let caret = source[..byte].chars().count();
        let (items, _) = editor.completion_items_for(caret, caret);

        let utf16_col = source[..byte].encode_utf16().count();
        assert_eq!(utf16_col, caret + 1, "emoji adds one UTF-16 code unit");
        assert!(
            items
                .iter()
                .any(|item| item.kind == animatix_analyzer::CompletionKind::Property)
        );
        let expected = editor.analyzer().completions_at(0, utf16_col);
        let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
        let expected_labels: Vec<_> = expected.iter().map(|item| item.label.as_str()).collect();
        assert_eq!(labels, expected_labels);
    }

    #[test]
    fn completion_splices_at_absolute_caret_and_preserves_other_bytes() {
        let source = "before = 1\n😀box.po after = 2\n";
        let mut editor = EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string());
        let caret = source.find(" after").unwrap();
        let caret_char = source[..caret].chars().count();
        editor.completion_anchor = Some(CompletionAnchor {
            caret: caret_char,
            word_start: caret_char - "box.po".chars().count(),
        });

        editor.insert_completion("position");

        assert_eq!(editor.text(), "before = 1\n😀position after = 2\n");
        assert_eq!(editor.pending_cursor_char, Some("before = 1\n😀position".chars().count()));
    }

    #[test]
    fn completion_without_anchor_is_ignored() {
        let source = "box: Rect\n#0s\n";
        let mut editor = EditorBuffer::new(&PathBuf::from("test.amx"), source.to_string());
        editor.insert_completion("garbage");
        assert_eq!(editor.text(), source, "an unanchored completion must not touch the document");
    }
}
