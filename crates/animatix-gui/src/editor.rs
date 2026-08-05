//! Code editor with cell-based notebook UI, tree-sitter syntax highlighting,
//! line numbers, and auto-complete.

use std::path::{Path, PathBuf};

use animatix_analyzer::Analyzer;
use egui::Key;
use egui::text::LayoutJob;

use crate::cell_editor::{Cell, CellEditorState, CellType, parse_cells, render_cell_editor};
use crate::completion_popup::CompletionPopup;

mod completion;
mod diagnostics;

pub struct EditorBuffer {
    text: String,
    document_path: PathBuf,
    /// Cached highlight result to avoid re-parsing every frame.
    cached_highlight: Option<(String, LayoutJob)>,
    /// Language analyzer for completions and diagnostics.
    analyzer: Analyzer,
    /// Completion popup state.
    completion: CompletionPopup,
    /// Whether completion was just confirmed (to avoid re-triggering).
    completion_confirmed: bool,
    /// Cell-based editor state.
    cells: Vec<Cell>,
    cell_state: CellEditorState,
    /// Line to scroll to on next frame (0-indexed, consumed by caller in workspace_ui).
    pub pending_scroll_to_line: Option<usize>,
    /// Line to highlight as "current" (0-indexed, for timeline sync).
    pub highlighted_line: Option<usize>,
    /// Absolute time in seconds for each keyframe line (line → time).
    pub keyframe_times_s: std::collections::HashMap<usize, f64>,
    /// Current cursor line (0-indexed), updated each frame.
    pub cursor_line: Option<usize>,
    /// When the user clicks the ▶ play button on a keyframe cell, this is set
    /// to the target time. The workspace layer reads it and scrubs the timeline.
    pub pending_scrub_to_time: Option<f64>,
}

impl EditorBuffer {
    pub fn new(path: &Path, text: String) -> Self {
        let analyzer = Analyzer::new(&text);
        let cells = parse_cells(&text);
        Self {
            text: text.clone(),
            document_path: path.to_path_buf(),
            cached_highlight: None,
            analyzer,
            completion: CompletionPopup::new(),
            completion_confirmed: false,
            cells,
            cell_state: CellEditorState::default(),
            pending_scroll_to_line: None,
            highlighted_line: None,
            keyframe_times_s: std::collections::HashMap::new(),
            cursor_line: None,
            pending_scrub_to_time: None,
        }
    }

    pub fn set_document(&mut self, path: &Path, text: String) {
        self.text = text.clone();
        self.document_path = path.to_path_buf();
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
        self.completion.hide();
        self.cells = parse_cells(&text);
        self.cell_state = CellEditorState::default();
        self.pending_scroll_to_line = None;
        self.highlighted_line = None;
        self.keyframe_times_s.clear();
        self.cursor_line = None;
        self.pending_scrub_to_time = None;
    }

    pub fn text(&self) -> &str {
        // text is kept in sync with cells inside show() / replace_text() / set_document(),
        // so we can always return the cached string.
        &self.text
    }

    pub fn replace_text(&mut self, text: String) {
        self.text = text.clone();
        self.cells = parse_cells(&text);
        self.cell_state = CellEditorState::default();
        self.cached_highlight = None;
        self.analyzer.update(&self.text);
        self.cursor_line = None;
        self.keyframe_times_s.clear();
        self.pending_scrub_to_time = None;
    }

    pub fn scroll_to_line(&mut self, line: usize) {
        self.pending_scroll_to_line = Some(line);
    }

    /// Current focused cell index, if any.
    pub fn focused_cell(&self) -> Option<usize> {
        self.cell_state.focused_cell
    }

    /// Type of the currently focused cell, if any.
    pub fn focused_cell_type(&self) -> Option<crate::cell_editor::CellType> {
        self.cell_state
            .focused_cell
            .and_then(|idx| self.cells.get(idx).map(|c| c.cell_type()))
    }

    /// Override the focused cell index.
    pub fn set_focused_cell(&mut self, cell: Option<usize>) {
        self.cell_state.focused_cell = cell;
    }

    /// Focus the cell that contains `line` and place the cursor at the end
    /// of the word/token that starts at `column`.
    pub fn focus_diagnostic(&mut self, line: usize, column: usize) {
        let cell_idx = self.cell_index_for_source_line(line);
        let Some(idx) = cell_idx else { return };
        let Some(cell_start_line) = self.source_line_for_cell(idx) else {
            return;
        };

        // Determine how many header lines precede the editable body.
        let cell = &self.cells[idx];
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

        // If the diagnostic points to a header line, focus the cell and scroll
        // to it, but do not place a cursor in the body (headers are not editable).
        if line < body_start_line {
            self.cell_state.focused_cell = Some(idx);
            self.cell_state.scroll_to_cell = Some(idx);
            return;
        }

        let line_in_body = line - body_start_line;
        let col_0based = column;

        let body = cell.body();
        let mut char_idx = 0usize;
        for (i, body_line) in body.lines().enumerate() {
            if i == line_in_body {
                char_idx += col_0based.min(body_line.chars().count());
                break;
            }
            char_idx += body_line.chars().count() + 1; // +1 for '\n'
        }

        // Advance to the end of the current word/token so the user is
        // positioned right after the problematic identifier/value.
        let body_chars: Vec<char> = body.chars().collect();
        // Skip any whitespace immediately after the start position.
        while char_idx < body_chars.len() && body_chars[char_idx].is_whitespace() {
            char_idx += 1;
        }
        // Consume the word/token.
        while char_idx < body_chars.len() && !body_chars[char_idx].is_whitespace() {
            char_idx += 1;
        }

        self.cell_state.focused_cell = Some(idx);
        self.cell_state.scroll_to_cell = Some(idx);
        self.cell_state.pending_cursor_cell = Some(idx);
        self.cell_state.pending_cursor_char = Some(char_idx);
    }

    pub fn set_highlighted_line(&mut self, line: Option<usize>) {
        self.highlighted_line = line;
        // Map source line to cell index for cell-level highlighting
        self.cell_state.highlighted_cell = line.and_then(|l| self.cell_index_for_source_line(l));
        // Timeline sync takes precedence over manual cell focus
        self.cell_state.focused_cell = None;
    }

    /// Get the analyzer for diagnostics, hover, etc.
    pub fn analyzer(&self) -> &Analyzer {
        &self.analyzer
    }

    /// Build a mapping from source line to cell index.
    fn cell_index_for_source_line(&self, target_line: usize) -> Option<usize> {
        let mut current_line = 0usize;
        for (idx, cell) in self.cells.iter().enumerate() {
            let cell_lines = cell.to_source().lines().count();
            if target_line < current_line + cell_lines {
                return Some(idx);
            }
            current_line += cell_lines;
        }
        None
    }

    /// Find which source line a given cell starts at.
    fn source_line_for_cell(&self, cell_idx: usize) -> Option<usize> {
        let mut current_line = 0usize;
        for (idx, cell) in self.cells.iter().enumerate() {
            if idx == cell_idx {
                return Some(current_line);
            }
            current_line += cell.to_source().lines().count();
        }
        None
    }

    /// Compute a new keyframe cell to insert after `after_idx`.
    ///
    /// Timestamp logic:
    /// - Between two keyframes at T₁ and T₂: default to `#+(T₂−T₁)/2`.
    /// - After the last keyframe: `#+1s`.
    fn compute_insert_keyframe(&self, after_idx: usize) -> Cell {
        use crate::cell_editor::format_duration_s;

        let prev_time_s =
            self.cells[..=after_idx].iter().rev().find_map(|c| c.time_s()).unwrap_or(0.0);

        let next_time_s = self.cells[after_idx + 1..].iter().find_map(|c| c.time_s());

        let (timestamp, is_relative, new_time_s) = if let Some(next) = next_time_s {
            let delta = (next - prev_time_s) / 2.0;
            let ts = format!("+{}", format_duration_s(delta));
            let t = prev_time_s + delta;
            (ts, true, t)
        } else {
            ("+1s".to_string(), true, prev_time_s + 1.0)
        };

        Cell::Keyframe {
            timestamp,
            is_relative,
            time_s: new_time_s,
            body: String::new(),
            attached_comment: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> egui::Response {
        // Handle pending scroll by mapping line to cell
        if let Some(target_line) = self.pending_scroll_to_line.take() {
            if let Some(cell_idx) = self.cell_index_for_source_line(target_line) {
                self.cell_state.scroll_to_cell = Some(cell_idx);
            }
        }

        // Track source changes from the cell editor
        let mut source_changed = false;
        let mut new_source = String::new();

        // Render the cell editor
        let mut scrub_to_time: Option<f64> = None;
        {
            let cells = &mut self.cells;
            let state = &mut self.cell_state;
            let on_source_changed = &mut |s: String| {
                source_changed = true;
                new_source = s;
            };
            let on_scrub_to_time = &mut |t: f64| {
                scrub_to_time = Some(t);
            };
            render_cell_editor(ui, cells, state, on_source_changed, on_scrub_to_time);
        }

        // Defensive: egui TextEdit handles Ctrl-Z internally and may not fire
        // response.changed(). Reconstruct source from cells and compare with our
        // cached text. If they diverge, something changed internally (undo, etc.)
        // and we must sync.
        let reconstructed = crate::cell_editor::cells_to_source(&self.cells);
        if !source_changed && reconstructed != self.text {
            source_changed = true;
            new_source = reconstructed;
        }

        // Build response manually since render_cell_editor doesn't return one
        let response = ui.response();

        // Handle play-button scrub
        if let Some(time_s) = scrub_to_time {
            self.pending_scrub_to_time = Some(time_s);
        }

        // Handle structural edits (delete / duplicate / insert) requested from the cell menu
        let mut structurally_changed = false;
        if let Some(idx) = self.cell_state.pending_delete_cell.take() {
            if idx < self.cells.len() {
                self.cells.remove(idx);
                structurally_changed = true;
                // Adjust focus if needed
                if self.cell_state.focused_cell == Some(idx) {
                    self.cell_state.focused_cell = None;
                } else if self.cell_state.focused_cell.map(|f| f > idx).unwrap_or(false) {
                    self.cell_state.focused_cell = self.cell_state.focused_cell.map(|f| f - 1);
                }
            }
        }
        if let Some(idx) = self.cell_state.pending_duplicate_cell.take() {
            if idx < self.cells.len() {
                let cloned = self.cells[idx].duplicate();
                self.cells.insert(idx + 1, cloned);
                structurally_changed = true;
            }
        }
        if let Some(idx) = self.cell_state.pending_insert_after.take() {
            let new_cell = self.compute_insert_keyframe(idx);
            let insert_at = idx + 1;
            if insert_at <= self.cells.len() {
                self.cells.insert(insert_at, new_cell);
                structurally_changed = true;
                self.cell_state.focused_cell = Some(insert_at);
            }
        }
        if let Some(idx) = self.cell_state.pending_insert_code_after.take() {
            let insert_at = idx + 1;
            if insert_at <= self.cells.len() {
                self.cells.insert(
                    insert_at,
                    Cell::Code {
                        body: String::new(),
                        expanded: true,
                    },
                );
                structurally_changed = true;
                self.cell_state.focused_cell = Some(insert_at);
            }
        }
        if let Some(idx) = self.cell_state.pending_move_up.take() {
            if idx > 0 && idx < self.cells.len() {
                self.cells.swap(idx, idx - 1);
                self.cell_state.swap_cell_indices(idx, idx - 1);
                structurally_changed = true;
            }
        }
        if let Some(idx) = self.cell_state.pending_move_down.take() {
            if idx + 1 < self.cells.len() {
                self.cells.swap(idx, idx + 1);
                self.cell_state.swap_cell_indices(idx, idx + 1);
                structurally_changed = true;
            }
        }

        ui.horizontal(|ui| {
            if ui.button("+ Keyframe").clicked() {
                self.cell_state.pending_append_at_end = Some(CellType::Keyframe);
            }
            if ui.button("+ Code").clicked() {
                self.cell_state.pending_append_at_end = Some(CellType::Code);
            }
        });
        if let Some(cell_type) = self.cell_state.pending_append_at_end.take() {
            let new_cell = match cell_type {
                CellType::Keyframe => {
                    self.compute_insert_keyframe(self.cells.len().saturating_sub(1))
                },
                CellType::Code => Cell::Code {
                    body: String::new(),
                    expanded: true,
                },
            };
            self.cells.push(new_cell);
            structurally_changed = true;
            self.cell_state.focused_cell = Some(self.cells.len() - 1);
        }

        // ── Auto-remove empty cells that lost focus ────────────────────────
        // When focus moves from cell A to cell B (or leaves the editor),
        // cell A is removed if it became empty while it was focused.
        let prev = self.cell_state.prev_focused_cell;
        let curr = self.cell_state.focused_cell;
        if prev != curr {
            if let Some(prev_idx) = prev {
                if prev_idx < self.cells.len() && self.cells[prev_idx].is_empty() {
                    self.cells.remove(prev_idx);
                    structurally_changed = true;
                    // Adjust current focus if it was after the removed cell
                    if let Some(ref mut c) = self.cell_state.focused_cell {
                        if *c > prev_idx {
                            *c -= 1;
                        }
                    }
                }
            }
        }
        self.cell_state.prev_focused_cell = self.cell_state.focused_cell;

        if source_changed || structurally_changed {
            if structurally_changed {
                new_source = crate::cell_editor::cells_to_source(&self.cells);
            }
            self.text = new_source;
            self.cached_highlight = None;
            self.analyzer.update(&self.text);

            // Update cursor_line from focused cell
            if let Some(focused_idx) = self.cell_state.focused_cell {
                if let Some(start_line) = self.source_line_for_cell(focused_idx) {
                    // Estimate cursor line within cell body
                    self.cursor_line = Some(start_line);
                }
            }

            // Mark response as changed so the workspace knows the source text was modified.
            let mut r =
                ui.interact(response.rect, ui.id().with("cell_editor"), egui::Sense::click());
            r.mark_changed();
            return r;
        }

        // Update cursor_line from focused cell even if not changed
        if let Some(focused_idx) = self.cell_state.focused_cell {
            if let Some(start_line) = self.source_line_for_cell(focused_idx) {
                self.cursor_line = Some(start_line);
            }
        }

        // Handle completion keyboard input
        let completion_consumed = self.completion.handle_input(ui.ctx());

        // If completion consumed the input, don't process further
        if completion_consumed {
            let insert_text = self
                .completion
                .selected_item()
                .map(|item| item.insert_text.as_deref().unwrap_or(&item.label).to_string());
            if let Some(text) = insert_text {
                self.insert_completion(&text);
                self.completion.hide();
                self.completion_confirmed = true;
            }
        }

        // Trigger completion on Ctrl+Space
        if response.has_focus() {
            ui.input(|i| {
                if i.key_pressed(Key::Space) && i.modifiers.ctrl {
                    self.trigger_completion();
                }
            });
        }

        // Reset completion_confirmed flag
        if self.completion_confirmed {
            self.completion_confirmed = false;
        }

        // Show completion popup if visible
        if self.completion.is_visible() {
            let cursor_rect = response.rect;
            if let Some(insert_text) = self.completion.ui(ui, cursor_rect) {
                self.insert_completion(&insert_text);
                self.completion.hide();
            }
        }

        response
    }

    /// Convert byte offset to (line, col).
    pub(super) fn byte_to_line_col(&self, byte_offset: usize) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;

        for (i, ch) in self.text.char_indices() {
            if i >= byte_offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        (line, col)
    }
}
