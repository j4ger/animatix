//! Diagnostics mapping — converts AST/analyzer diagnostics into cell-level
//! decorations for the cell-based notebook UI.

use crate::cell_editor::{CellDiagnostic, SemanticHighlight, SemanticTokenKind};
use crate::editor::EditorBuffer;

impl EditorBuffer {
    /// Set diagnostics (e.g. parse errors) on the buffer, mapping them to cell
    /// positions and updating cell-level decoration state.
    pub fn set_diagnostics(&mut self, diagnostics: &[animatix_syntax::diagnostics::Diagnostic]) {
        self.cell_state.diagnostics.clear();
        self.cell_state.error_cells.clear();
        self.cell_state.warning_cells.clear();

        for d in diagnostics {
            let Some(doc_line) = d.location.line else {
                continue;
            };
            let doc_line = doc_line.saturating_sub(1); // 0-indexed

            let Some((cell_idx, body_start_line)) = self.cell_body_start(doc_line) else {
                continue;
            };

            // Track cell border color by severity.
            match d.severity {
                animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                    self.cell_state.error_cells.insert(cell_idx);
                },
                animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                | animatix_syntax::diagnostics::DiagnosticSeverity::Info
                | animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                    self.cell_state.warning_cells.insert(cell_idx);
                },
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

        // Compute semantic highlights from analyzer symbol table
        self.refresh_semantic_highlights();
    }

    /// Map a 0-indexed document line to its cell and the first line of that
    /// cell's editable body. Keyframe cells carry their attached comment plus
    /// the `#timestamp` line as a non-body header; code cells have none.
    fn cell_body_start(&self, doc_line: usize) -> Option<(usize, usize)> {
        let cell_idx = self.cell_index_for_source_line(doc_line)?;
        let cell_start_line = self.source_line_for_cell(cell_idx)?;
        let header_lines = match &self.cells[cell_idx] {
            crate::cell_editor::Cell::Code { .. } => 0,
            crate::cell_editor::Cell::Keyframe {
                attached_comment, ..
            } => attached_comment.as_ref().map(|c| c.lines().count()).unwrap_or(0) + 1,
        };
        Some((cell_idx, cell_start_line + header_lines))
    }

    /// Compute semantic highlights from the analyzer's symbol table and map
    /// them to cell-relative positions.
    fn refresh_semantic_highlights(&mut self) {
        self.cell_state.semantic_highlights.clear();

        let symbols = self.analyzer.symbols();

        // Actor labels
        for info in symbols.labels.values() {
            if info.kind != animatix_analyzer::LabelKind::Actor {
                continue;
            }
            let Some(span) = &info.span else { continue };
            let doc_line = span.start_line.saturating_sub(1); // 0-indexed
            let doc_end_line = span.end_line.saturating_sub(1);

            let Some((cell_idx, body_start_line)) = self.cell_body_start(doc_line) else {
                continue;
            };

            if doc_line < body_start_line {
                continue;
            }

            self.cell_state.semantic_highlights.push(SemanticHighlight {
                cell_index: cell_idx,
                rel_line: doc_line - body_start_line,
                rel_col: span.start_col.saturating_sub(1),
                rel_end_line: doc_end_line - body_start_line,
                rel_end_col: span.end_col.saturating_sub(1),
                kind: SemanticTokenKind::ActorName,
            });
        }

        // Component names
        for info in symbols.components.values() {
            let Some(span) = &info.span else { continue };
            let doc_line = span.start_line.saturating_sub(1);
            let doc_end_line = span.end_line.saturating_sub(1);

            let Some((cell_idx, body_start_line)) = self.cell_body_start(doc_line) else {
                continue;
            };

            if doc_line < body_start_line {
                continue;
            }

            self.cell_state.semantic_highlights.push(SemanticHighlight {
                cell_index: cell_idx,
                rel_line: doc_line - body_start_line,
                rel_col: span.start_col.saturating_sub(1),
                rel_end_line: doc_end_line - body_start_line,
                rel_end_col: span.end_col.saturating_sub(1),
                kind: SemanticTokenKind::ComponentName,
            });
        }

        // Scene names (from AST)
        // We do a simple text search for `# Name` and `play Name` patterns.
        if !symbols.scenes.is_empty() {
            for (cell_idx, cell) in self.cells.iter().enumerate() {
                let cell_source = cell.to_source();

                for line in cell_source.lines().enumerate() {
                    let (rel_line_idx, line_text) = line;
                    let trimmed = line_text.trim_start();

                    // Scene declaration: `# SceneName`
                    if let Some(rest) = trimmed.strip_prefix('#') {
                        let name = rest.trim();
                        if symbols.scenes.contains_key(name) {
                            let start_col = line_text.find('#').unwrap_or(0) + 1;
                            let end_col = line_text.len();
                            self.cell_state.semantic_highlights.push(SemanticHighlight {
                                cell_index: cell_idx,
                                rel_line: rel_line_idx,
                                rel_col: start_col,
                                rel_end_line: rel_line_idx,
                                rel_end_col: end_col,
                                kind: SemanticTokenKind::SceneName,
                            });
                        }
                    }

                    // Play statement: `play SceneName [...]`
                    if let Some(rest) = trimmed.strip_prefix("play ") {
                        let name = rest.split_whitespace().next().unwrap_or("");
                        if symbols.scenes.contains_key(name) {
                            let start_col = line_text.find("play ").unwrap_or(0) + 5;
                            let end_col = start_col + name.len();
                            self.cell_state.semantic_highlights.push(SemanticHighlight {
                                cell_index: cell_idx,
                                rel_line: rel_line_idx,
                                rel_col: start_col,
                                rel_end_line: rel_line_idx,
                                rel_end_col: end_col,
                                kind: SemanticTokenKind::SceneName,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Refresh analyzer diagnostics and merge them into cell_state.
    fn refresh_analyzer_diagnostics(&mut self) {
        let analyzer_diagnostics = self.analyzer.diagnostics();
        for d in analyzer_diagnostics {
            // Analyzer diagnostics use 0-based positions
            let doc_line = d.line;

            let Some((cell_idx, body_start_line)) = self.cell_body_start(doc_line) else {
                continue;
            };

            let severity = match d.severity {
                animatix_analyzer::DiagnosticSeverity::Error => {
                    animatix_syntax::diagnostics::DiagnosticSeverity::Error
                },
                animatix_analyzer::DiagnosticSeverity::Warning => {
                    animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                },
                animatix_analyzer::DiagnosticSeverity::Info => {
                    animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                },
                animatix_analyzer::DiagnosticSeverity::Hint => {
                    animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                },
            };

            match severity {
                animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                    self.cell_state.error_cells.insert(cell_idx);
                },
                animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                | animatix_syntax::diagnostics::DiagnosticSeverity::Info
                | animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                    if !self.cell_state.error_cells.contains(&cell_idx) {
                        self.cell_state.warning_cells.insert(cell_idx);
                    }
                },
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
