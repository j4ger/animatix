//! Diagnostics mapping — converts AST/analyzer diagnostics into cell-level
//! decorations for the cell-based notebook UI.

use crate::cell_editor::CellDiagnostic;
use crate::editor::EditorBuffer;

impl EditorBuffer {
    /// Set diagnostics (e.g. parse errors) on the buffer, mapping them to cell
    /// positions and updating cell-level decoration state.
    pub fn set_diagnostics(&mut self, diagnostics: &[animatix::diagnostics::Diagnostic]) {
        self.cell_state.diagnostics.clear();
        self.cell_state.error_cells.clear();
        self.cell_state.warning_cells.clear();

        for d in diagnostics {
            let Some(doc_line) = d.location.line else { continue };
            let doc_line = doc_line.saturating_sub(1); // 0-indexed

            let Some(cell_idx) = self.cell_index_for_source_line(doc_line) else { continue };
            let Some(cell_start_line) = self.source_line_for_cell(cell_idx) else { continue };

            let cell = &self.cells[cell_idx];

            // How many header lines before the editable body?
            let header_lines = match cell {
                crate::cell_editor::Cell::Code { .. } => 0,
                crate::cell_editor::Cell::Keyframe { attached_comment, .. } => {
                    let comment_lines =
                        attached_comment.as_ref().map(|c| c.lines().count()).unwrap_or(0);
                    comment_lines + 1 // +1 for the #timestamp line
                }
            };

            let body_start_line = cell_start_line + header_lines;

            // Track cell border color by severity.
            match d.severity {
                animatix::diagnostics::DiagnosticSeverity::Error => {
                    self.cell_state.error_cells.insert(cell_idx);
                }
                animatix::diagnostics::DiagnosticSeverity::Warning => {
                    self.cell_state.warning_cells.insert(cell_idx);
                }
            }

            // Skip diagnostics that sit on the cell header (not in the body text).
            if doc_line < body_start_line {
                continue;
            }

            let rel_line = doc_line - body_start_line;
            let rel_col = d.location.column.map(|c| c.saturating_sub(1)).unwrap_or(0);
            let rel_end_col = rel_col + 5; // approximate token width for underline

            self.cell_state.diagnostics.push(CellDiagnostic {
                line: doc_line,
                message: d.message.clone(),
                severity: d.severity,
                cell_index: cell_idx,
                rel_line,
                rel_col,
                rel_end_line: rel_line,
                rel_end_col,
            });
        }

        // Merge analyzer semantic diagnostics
        self.refresh_analyzer_diagnostics();
    }

    /// Refresh analyzer diagnostics and merge them into cell_state.
    fn refresh_analyzer_diagnostics(&mut self) {
        let analyzer_diagnostics = self.analyzer.diagnostics();
        for d in analyzer_diagnostics {
            // Analyzer diagnostics use 0-based tree-sitter positions
            let doc_line = d.line;

            let Some(cell_idx) = self.cell_index_for_source_line(doc_line) else { continue };
            let Some(cell_start_line) = self.source_line_for_cell(cell_idx) else { continue };

            let cell = &self.cells[cell_idx];

            let header_lines = match cell {
                crate::cell_editor::Cell::Code { .. } => 0,
                crate::cell_editor::Cell::Keyframe { attached_comment, .. } => {
                    let comment_lines =
                        attached_comment.as_ref().map(|c| c.lines().count()).unwrap_or(0);
                    comment_lines + 1
                }
            };

            let body_start_line = cell_start_line + header_lines;

            let severity = match d.severity {
                animatix_analyzer::DiagnosticSeverity::Error => {
                    animatix::diagnostics::DiagnosticSeverity::Error
                }
                animatix_analyzer::DiagnosticSeverity::Warning => {
                    animatix::diagnostics::DiagnosticSeverity::Warning
                }
                animatix_analyzer::DiagnosticSeverity::Info => {
                    animatix::diagnostics::DiagnosticSeverity::Warning
                }
                animatix_analyzer::DiagnosticSeverity::Hint => {
                    animatix::diagnostics::DiagnosticSeverity::Warning
                }
            };

            match severity {
                animatix::diagnostics::DiagnosticSeverity::Error => {
                    self.cell_state.error_cells.insert(cell_idx);
                }
                animatix::diagnostics::DiagnosticSeverity::Warning => {
                    if !self.cell_state.error_cells.contains(&cell_idx) {
                        self.cell_state.warning_cells.insert(cell_idx);
                    }
                }
            }

            if doc_line < body_start_line {
                continue;
            }

            let rel_line = doc_line - body_start_line;
            let rel_col = d.col;
            let rel_end_col = d.end_col.max(rel_col + 1);

            self.cell_state.diagnostics.push(CellDiagnostic {
                line: doc_line,
                message: d.message,
                severity,
                cell_index: cell_idx,
                rel_line,
                rel_col,
                rel_end_line: d.end_line.max(body_start_line) - body_start_line,
                rel_end_col,
            });
        }
    }
}