//! Find-references provider.

use animatix_syntax::token::{Token, TokenKind, byte_to_line_col};

/// Find all references to a symbol name in a file.
/// Returns a list of (start_line, start_col, end_line, end_col) ranges.
pub fn find_references(
    tokens: &[Token],
    source: &str,
    symbol_name: &str,
) -> Vec<(usize, usize, usize, usize)> {
    tokens
        .iter()
        .filter(|t| matches!(&t.kind, TokenKind::Ident(name) if name == symbol_name))
        .map(|t| {
            let (start_line, start_col) = byte_to_line_col(source, t.span.start);
            let (end_line, end_col) = byte_to_line_col(source, t.span.end);
            (start_line, start_col, end_line, end_col)
        })
        .collect()
}
