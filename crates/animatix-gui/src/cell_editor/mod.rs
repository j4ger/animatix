mod cell;
mod parser;
mod render;

pub use cell::{Cell, format_duration_s};
pub use parser::{cells_to_source, parse_cells};
pub use render::render_cell_editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellType {
    Keyframe,
    Code,
}

/// A lightweight diagnostic attached to a cell for error display.
#[derive(Debug, Clone)]
pub struct CellDiagnostic {
    pub line: usize,
    pub message: String,
    pub severity: animatix_syntax::diagnostics::DiagnosticSeverity,
    /// Which cell this diagnostic belongs to.
    pub cell_index: usize,
    /// Cell-body-relative position for token-level underlining.
    pub rel_line: usize,
    pub rel_col: usize,
    pub rel_end_line: usize,
    pub rel_end_col: usize,
}

/// A semantic highlight range within a cell body.
#[derive(Debug, Clone)]
pub struct SemanticHighlight {
    /// Which cell this highlight belongs to.
    pub cell_index: usize,
    /// Cell-relative line (0-indexed).
    pub rel_line: usize,
    /// Cell-relative start column (0-indexed).
    pub rel_col: usize,
    /// Cell-relative end line (0-indexed).
    pub rel_end_line: usize,
    /// Cell-relative end column (0-indexed).
    pub rel_end_col: usize,
    /// Semantic token kind.
    pub kind: SemanticTokenKind,
}

/// Kinds of semantic tokens for coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTokenKind {
    /// Actor label (declaration or reference).
    ActorName,
    /// Property name (e.g., `color`, `position`).
    PropertyName,
    /// Scene name (in `# SceneName` or `play SceneName`).
    SceneName,
    /// Component name.
    ComponentName,
}

/// Persistent state for the cell editor (scroll position, focused cell, etc.).
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct CellEditorState {
    pub focused_cell: Option<usize>,
    pub scroll_to_cell: Option<usize>,
    pub highlighted_cell: Option<usize>,
    /// Optional action requests that the caller can apply to the backing cell list.
    pub pending_delete_cell: Option<usize>,
    /// Optional duplicate request that the caller can apply to the backing cell list.
    pub pending_duplicate_cell: Option<usize>,
    /// Optional insert request: insert a new keyframe after this cell index.
    pub pending_insert_after: Option<usize>,
    /// Optional request to insert a code block after this cell index.
    pub pending_insert_code_after: Option<usize>,
    /// Optional request to append a cell at the very end.
    pub pending_append_at_end: Option<CellType>,
    /// The cell that had focus on the previous frame. Used to detect when
    /// the user moved focus away from a cell so we can auto-remove it if empty.
    pub prev_focused_cell: Option<usize>,
    /// Diagnostics mapped to this cell's line range, for showing error indicators.
    pub diagnostics: Vec<CellDiagnostic>,
    /// Semantic highlights for each cell.
    pub semantic_highlights: Vec<SemanticHighlight>,
    /// Set of cell indices that have at least one diagnostic error.
    pub error_cells: std::collections::HashSet<usize>,
    /// Set of cell indices that have at least one diagnostic warning (but no errors).
    pub warning_cells: std::collections::HashSet<usize>,
    /// When set, focus this cell and place the cursor at the given char index
    /// inside its body TextEdit. Consumed by the cell renderer on the next frame.
    pub pending_cursor_cell: Option<usize>,
    /// Char index within the cell body where the cursor should be placed.
    pub pending_cursor_char: Option<usize>,
    /// Set of cell indices that are collapsed (applies to keyframes; code cells
    /// store expansion on the `Cell` enum itself).
    pub collapsed_cells: std::collections::HashSet<usize>,
    /// Optional request to move the cell at this index up by one position.
    pub pending_move_up: Option<usize>,
    /// Optional request to move the cell at this index down by one position.
    pub pending_move_down: Option<usize>,
    /// Which cell is currently editing its timestamp inline.
    pub editing_timestamp_cell: Option<usize>,
    /// Cached highlight LayoutJobs per cell index, keyed by cell body content.
    /// Avoids re-highlighting unchanged cells every frame.
    pub cached_highlight_jobs: std::collections::HashMap<usize, (String, egui::text::LayoutJob)>,
}

impl CellEditorState {
    /// Swap all index-based state for two cell positions (e.g. after reordering).
    pub fn swap_cell_indices(&mut self, a: usize, b: usize) {
        let swap_opt = |opt: &mut Option<usize>| {
            if let Some(v) = *opt {
                if v == a {
                    *opt = Some(b);
                } else if v == b {
                    *opt = Some(a);
                }
            }
        };
        swap_opt(&mut self.focused_cell);
        swap_opt(&mut self.scroll_to_cell);
        swap_opt(&mut self.highlighted_cell);
        swap_opt(&mut self.prev_focused_cell);
        swap_opt(&mut self.pending_cursor_cell);
        swap_opt(&mut self.editing_timestamp_cell);

        let swap_set = |set: &mut std::collections::HashSet<usize>| {
            let has_a = set.contains(&a);
            let has_b = set.contains(&b);
            if has_a && !has_b {
                set.remove(&a);
                set.insert(b);
            } else if !has_a && has_b {
                set.remove(&b);
                set.insert(a);
            }
        };
        swap_set(&mut self.collapsed_cells);
        swap_set(&mut self.error_cells);
        swap_set(&mut self.warning_cells);
    }
}

