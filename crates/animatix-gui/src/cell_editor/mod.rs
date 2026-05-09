mod cell;
mod parser;
mod render;

pub use cell::{Cell, format_duration_s};
pub use parser::{cells_to_source, parse_cells};
pub use render::render_cell_editor;

/// A lightweight diagnostic attached to a cell for error display.
#[derive(Debug, Clone)]
pub struct CellDiagnostic {
    pub line: usize,
    pub message: String,
    pub severity: animatix::diagnostics::DiagnosticSeverity,
    /// Which cell this diagnostic belongs to.
    pub cell_index: usize,
    /// Cell-body-relative position for token-level underlining.
    pub rel_line: usize,
    pub rel_col: usize,
    pub rel_end_line: usize,
    pub rel_end_col: usize,
}

/// Persistent state for the cell editor (scroll position, focused cell, etc.).
#[derive(Debug, Clone)]
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
    /// The cell that had focus on the previous frame. Used to detect when
    /// the user moved focus away from a cell so we can auto-remove it if empty.
    pub prev_focused_cell: Option<usize>,
    /// Diagnostics mapped to this cell's line range, for showing error indicators.
    pub diagnostics: Vec<CellDiagnostic>,
    /// Set of cell indices that have at least one diagnostic error.
    pub error_cells: std::collections::HashSet<usize>,
    /// Set of cell indices that have at least one diagnostic warning (but no errors).
    pub warning_cells: std::collections::HashSet<usize>,
}

impl Default for CellEditorState {
    fn default() -> Self {
        Self {
            focused_cell: None,
            scroll_to_cell: None,
            highlighted_cell: None,
            pending_delete_cell: None,
            pending_duplicate_cell: None,
            pending_insert_after: None,
            prev_focused_cell: None,
            diagnostics: Vec::new(),
            error_cells: std::collections::HashSet::new(),
            warning_cells: std::collections::HashSet::new(),
        }
    }
}


