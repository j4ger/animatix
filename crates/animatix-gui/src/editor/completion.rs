//! Completion popup support for the code editor.

use crate::cell_editor::parse_cells;
use crate::cell_editor::CellEditorState;
use crate::editor::EditorBuffer;

impl EditorBuffer {
    /// Trigger completion at current cursor position.
    pub(super) fn trigger_completion(&mut self) {
        let cursor_pos = self.text.len();
        let (line, col) = self.byte_to_line_col(cursor_pos);

        let items = self.analyzer.completions_at(line, col);
        let trigger_text = self.get_current_word();
        self.completion.show(items, trigger_text);
    }

    /// Insert completion text at cursor position.
    pub(super) fn insert_completion(&mut self, insert_text: &str) {
        self.text.push_str(insert_text);
        self.cells = parse_cells(&self.text);
        self.cell_state = CellEditorState::default();
        self.cells_dirty = false;
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
    }

    /// Get the current word being typed (for completion filtering).
    pub(super) fn get_current_word(&self) -> String {
        let text = &self.text;
        let mut word = String::new();

        for ch in text.chars().rev() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                word.insert(0, ch);
            } else {
                break;
            }
        }

        word
    }
}