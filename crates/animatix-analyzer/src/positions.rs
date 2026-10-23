//! Position enrichment and diagnostic span conversion.

use animatix_syntax::ast::Span;
use animatix_syntax::token::{Token, TokenKind};

use crate::Analyzer;
use crate::diagnostics;
use crate::fixes;
use crate::symbol_table::SymbolTable;

impl Analyzer {
    /// Convert a syntax-level diagnostic to an analyzer diagnostic.
    ///
    /// Position resolution prefers a byte span (resolved against `source`),
    /// so producers that hold only a span — like the type checker, which sees
    /// a property's `value_span` but not the text — still land on the right
    /// line instead of the file start.
    pub(super) fn convert_type_diagnostic(
        source: &str,
        d: animatix_syntax::diagnostics::Diagnostic,
    ) -> diagnostics::Diagnostic {
        let severity = match d.severity {
            animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                diagnostics::DiagnosticSeverity::Error
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                diagnostics::DiagnosticSeverity::Warning
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Info => {
                diagnostics::DiagnosticSeverity::Info
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                diagnostics::DiagnosticSeverity::Hint
            },
        };
        let (line, col, end_line, end_col) = match &d.location.span {
            Some(span) if span.end <= source.len() => {
                let s = Span::from_range(source, span.clone());
                (
                    s.start_line.saturating_sub(1),
                    s.start_col.saturating_sub(1),
                    s.end_line.saturating_sub(1),
                    s.end_col.saturating_sub(1),
                )
            },
            _ => {
                let line = d.location.line.unwrap_or(1).saturating_sub(1);
                let col = d.location.column.unwrap_or(1).saturating_sub(1);
                (line, col, line, col + 1)
            },
        };
        diagnostics::Diagnostic {
            severity,
            line,
            col,
            end_line,
            end_col,
            message: d.message,
            code: Some(d.code.to_string()),
        }
    }

    /// Populate symbol table entries with declaration positions recorded by
    /// the parser, so declarations are distinguished from references.
    pub(super) fn enrich_positions(
        occurrences: &[animatix_syntax::occurrence::Occurrence],
        tokens: &[Token],
        source: &str,
        table: &mut SymbolTable,
    ) {
        use animatix_syntax::occurrence::OccurrenceKind;

        for occ in occurrences {
            if !occ.declaration {
                continue;
            }
            let span = Span::from_byte_span(source, occ.span);
            match occ.kind {
                OccurrenceKind::Label | OccurrenceKind::Variable => {
                    if let Some(info) = table.labels.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = span.start_line;
                            info.col = span.start_col;
                            info.span = Some(span);
                        }
                    }
                },
                OccurrenceKind::Component => {
                    if let Some(info) = table.components.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = span.start_line;
                            info.col = span.start_col;
                            info.span = Some(span);
                        }
                    }
                },
                OccurrenceKind::Scene => {
                    if let Some(info) = table.scenes.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = span.start_line;
                            info.col = span.start_col;
                            info.span = Some(span);
                        }
                    }
                },
                _ => {},
            }
        }

        // Import spans come from the string literal path token, not an
        // identifier occurrence, so resolve them from the token stream.
        for token in tokens {
            if let TokenKind::Str(path) = &token.kind {
                let span = Span::from_byte_span(source, token.span);
                for info in &mut table.imports {
                    if &info.path == path && info.span.is_none() {
                        info.span = Some(span);
                        break;
                    }
                }
            }
        }
    }

    /// Convert a parse-time warning into an analyzer diagnostic.
    ///
    /// Parse warnings carry a byte span with placeholder line/column values,
    /// so resolve the position from the span when available.
    pub(super) fn convert_parse_warning(
        source: &str,
        d: animatix_syntax::diagnostics::Diagnostic,
    ) -> diagnostics::Diagnostic {
        let severity = match d.severity {
            animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                diagnostics::DiagnosticSeverity::Error
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                diagnostics::DiagnosticSeverity::Warning
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Info => {
                diagnostics::DiagnosticSeverity::Info
            },
            animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                diagnostics::DiagnosticSeverity::Hint
            },
        };
        let (line, col, end_line, end_col) = match &d.location.span {
            Some(span) => {
                let s = Span::from_range(source, span.clone());
                (
                    s.start_line.saturating_sub(1),
                    s.start_col.saturating_sub(1),
                    s.end_line.saturating_sub(1),
                    s.end_col.saturating_sub(1),
                )
            },
            None => {
                let line = d.location.line.unwrap_or(1).saturating_sub(1);
                let col = d.location.column.unwrap_or(1).saturating_sub(1);
                (line, col, line, col + 1)
            },
        };
        diagnostics::Diagnostic {
            severity,
            line,
            col,
            end_line,
            end_col,
            message: d.message,
            code: Some(d.code.to_string()),
        }
    }

    /// Source range of the declaration of `name`, as 0-based
    /// `(line, col, end_line, end_col)`.
    ///
    /// Matches inline children too, so a diagnostic naming an actor inside a
    /// container still resolves. Build-layer diagnostics carry only a subject
    /// label; this is how a consumer gives them a position.
    pub fn declaration_range(&self, name: &str) -> Option<(usize, usize, usize, usize)> {
        let byte = fixes::first_declaration_byte(&self.occurrences, name)?;
        let end_byte = self
            .occurrences
            .iter()
            .find(|o| o.declaration && o.name == name && o.span.start == byte)
            .map(|o| o.span.end)
            .unwrap_or(byte);
        let start = animatix_syntax::token::byte_to_line_col(&self.source, byte);
        let end = animatix_syntax::token::byte_to_line_col(&self.source, end_byte);
        Some((start.0, start.1, end.0, end.1))
    }
}
