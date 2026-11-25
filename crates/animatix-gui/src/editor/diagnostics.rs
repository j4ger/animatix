//! Convert compiler and analyzer diagnostics to source-byte highlight ranges.

use crate::editor::EditorBuffer;
use crate::highlighting::{HighlightDiagnostic, HighlightSeverity};

impl EditorBuffer {
    /// Keep parser/build diagnostics and analyzer diagnostics on the same
    /// document coordinate system so one TextEdit can decorate every line.
    pub fn set_diagnostics(&mut self, diagnostics: &[animatix_syntax::diagnostics::Diagnostic]) {
        if self.analysis_due.is_some_and(|due| due <= std::time::Instant::now()) {
            self.finish_analysis();
        }
        let ready = self.analyzer.get().map(|analyzer| analyzer.diagnostics()).unwrap_or_default();
        let line_index = animatix_syntax::token::LineIndex::new(&self.text);
        let mut ranges = Vec::with_capacity(diagnostics.len() + ready.len());
        for diagnostic in diagnostics {
            let Some(line) = diagnostic.location.line else {
                continue;
            };
            let start =
                diagnostic.location.span.as_ref().map(|span| span.start).unwrap_or_else(|| {
                    line_index.line_col_to_byte(
                        line.saturating_sub(1),
                        diagnostic.location.column.unwrap_or(1).saturating_sub(1),
                    )
                });
            let end = diagnostic.location.span.as_ref().map(|span| span.end).unwrap_or_else(|| {
                let end_line = diagnostic.location.end_line.unwrap_or(line);
                let start_col = diagnostic.location.column.unwrap_or(1);
                let end_col = diagnostic
                    .location
                    .end_col
                    .unwrap_or_else(|| {
                        if end_line == line {
                            start_col.saturating_add(4)
                        } else {
                            1
                        }
                    })
                    .saturating_sub(1);
                line_index.line_col_to_byte(end_line.saturating_sub(1), end_col)
            });
            if let Some((start, end)) = valid_range(&self.text, start, end) {
                ranges.push(HighlightDiagnostic {
                    start,
                    end,
                    severity: syntax_severity(diagnostic.severity),
                });
            }
        }
        for diagnostic in ready {
            let start = line_index.line_col_to_byte(diagnostic.line, diagnostic.col);
            let end_col =
                if diagnostic.end_line == diagnostic.line && diagnostic.end_col <= diagnostic.col {
                    diagnostic.col.saturating_add(1)
                } else {
                    diagnostic.end_col
                };
            let end = line_index.line_col_to_byte(diagnostic.end_line, end_col);
            if let Some((start, end)) = valid_range(&self.text, start, end) {
                ranges.push(HighlightDiagnostic {
                    start,
                    end,
                    severity: analyzer_severity(diagnostic.severity),
                });
            }
        }
        ranges.sort_by_key(|range| (range.start, range.end, severity_order(range.severity)));
        ranges.dedup();
        if self.diagnostics != ranges {
            self.diagnostics = ranges;
        }
    }
}

fn valid_range(source: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let mut start = start.min(source.len());
    let mut end = end.min(source.len());
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    while end > start && !source.is_char_boundary(end) {
        end -= 1;
    }
    (start < end).then_some((start, end))
}

fn syntax_severity(
    severity: animatix_syntax::diagnostics::DiagnosticSeverity,
) -> HighlightSeverity {
    match severity {
        animatix_syntax::diagnostics::DiagnosticSeverity::Error => HighlightSeverity::Error,
        animatix_syntax::diagnostics::DiagnosticSeverity::Warning => HighlightSeverity::Warning,
        animatix_syntax::diagnostics::DiagnosticSeverity::Info => HighlightSeverity::Info,
        animatix_syntax::diagnostics::DiagnosticSeverity::Hint => HighlightSeverity::Hint,
    }
}

fn analyzer_severity(severity: animatix_analyzer::DiagnosticSeverity) -> HighlightSeverity {
    match severity {
        animatix_analyzer::DiagnosticSeverity::Error => HighlightSeverity::Error,
        animatix_analyzer::DiagnosticSeverity::Warning => HighlightSeverity::Warning,
        animatix_analyzer::DiagnosticSeverity::Info => HighlightSeverity::Info,
        animatix_analyzer::DiagnosticSeverity::Hint => HighlightSeverity::Hint,
    }
}

fn severity_order(severity: HighlightSeverity) -> u8 {
    match severity {
        HighlightSeverity::Error => 0,
        HighlightSeverity::Warning => 1,
        HighlightSeverity::Info => 2,
        HighlightSeverity::Hint => 3,
    }
}
