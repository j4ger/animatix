//! Position enrichment: declaration positions recorded by the parser, plus
//! the source range of a declaration for consumers that hold only a subject name.

use animatix_syntax::ast::{ByteSpan, Span};
use animatix_syntax::token::{LineIndex, Token, TokenKind};

use crate::Analyzer;
use crate::fixes;
use crate::symbol_table::SymbolTable;

impl Analyzer {
    /// Populate symbol table entries with declaration positions recorded by
    /// the parser, so declarations are distinguished from references.
    pub(super) fn enrich_positions(
        occurrences: &[animatix_syntax::occurrence::Occurrence],
        tokens: &[Token],
        index: &LineIndex<'_>,
        table: &mut SymbolTable,
    ) {
        use animatix_syntax::occurrence::OccurrenceKind;

        for occ in occurrences {
            if !occ.declaration {
                continue;
            }
            // Span columns are character columns (the GUI's editor
            // coordinates), while the symbol-table `line`/`col` fields feed
            // LSP-facing queries and must count UTF-16 code units.
            let span = span_from_index(index, occ.span);
            let (line, col) = index.byte_to_line_col_utf16(occ.span.start);
            let line = line + 1;
            let col = col + 1;
            match occ.kind {
                OccurrenceKind::Label | OccurrenceKind::Variable => {
                    if let Some(info) = table.labels.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = line;
                            info.col = col;
                            info.span = Some(span);
                        }
                    }
                },
                OccurrenceKind::Component => {
                    if let Some(info) = table.components.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = line;
                            info.col = col;
                            info.span = Some(span);
                        }
                    }
                },
                OccurrenceKind::Scene => {
                    if let Some(info) = table.scenes.get_mut(&occ.name) {
                        if info.line == 0 && info.col == 0 {
                            info.line = line;
                            info.col = col;
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
                let span = span_from_index(index, token.span);
                for info in &mut table.imports {
                    if &info.path == path && info.span.is_none() {
                        info.span = Some(span);
                        break;
                    }
                }
            }
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
        let index = LineIndex::new(&self.source);
        let start = index.byte_to_line_col(byte);
        let end = index.byte_to_line_col(end_byte);
        Some((start.0, start.1, end.0, end.1))
    }
}

/// Convert a byte span to the AST's 1-based character-column [`Span`] using a
/// precomputed [`LineIndex`], matching [`Span::from_byte_span`] without
/// rescanning the source for every declaration.
fn span_from_index(index: &LineIndex<'_>, span: ByteSpan) -> Span {
    let (start_line, start_col) = index.byte_to_line_col(span.start);
    let (end_line, end_col) = index.byte_to_line_col(span.end);
    Span::new(start_line + 1, start_col + 1, end_line + 1, end_col + 1)
}
