//! Document formatting.

use animatix_syntax::token::LineIndex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::server::Backend;

pub(crate) async fn formatting(
    backend: &Backend,
    params: DocumentFormattingParams,
) -> Result<Option<Vec<TextEdit>>> {
    let uri = params.text_document.uri.to_string();

    let (source, formatted) = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        let source = analyzer.source();
        let stmts = match analyzer.ast() {
            Some(stmts) => stmts,
            None => return Ok(None),
        };
        let fmt = animatix_syntax::formatter::Formatter::default();
        let formatted = fmt.format(stmts);
        if source == formatted {
            return Ok(None);
        }
        (source.to_string(), formatted)
    };

    Ok(Some(vec![full_document_edit(&source, formatted)]))
}

/// Replace the entire document range.
///
/// `source.len()` is a byte offset; the end position must be its
/// UTF-16-aware line/column. `str::lines()` is not usable here because it
/// drops the trailing newline, which would leave the original newline behind
/// when the edit is applied.
fn full_document_edit(source: &str, formatted: String) -> TextEdit {
    let index = LineIndex::new(source);
    let (end_line, end_col) = index.byte_to_line_col_utf16(source.len());
    TextEdit {
        range: Range::new(Position::new(0, 0), Position::new(end_line as u32, end_col as u32)),
        new_text: formatted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_edit(source: &str, edit: &TextEdit) -> String {
        let index = LineIndex::new(source);
        let start = index.line_col_to_byte_utf16(
            edit.range.start.line as usize,
            edit.range.start.character as usize,
        );
        let end = index.line_col_to_byte_utf16(
            edit.range.end.line as usize,
            edit.range.end.character as usize,
        );
        format!("{}{}{}", &source[..start], edit.new_text, &source[end..])
    }

    #[test]
    fn full_document_edit_includes_trailing_newline() {
        let source = "title: Text\n";
        let formatted = "title: Text\n";
        let edit = full_document_edit(source, formatted.to_string());

        assert_eq!(edit.range.end, Position::new(1, 0));
        let applied = apply_edit(source, &edit);
        assert_eq!(applied, formatted);
        assert_eq!(applied.matches('\n').count(), 1, "exactly one trailing newline");
    }

    #[test]
    fn full_document_edit_uses_utf16_columns_on_non_ascii_last_line() {
        // The emoji is four bytes and two UTF-16 code units; the edit end
        // must count code units, and the replacement includes the newline the
        // formatter adds.
        let source = "title: Text\ncontent: \"😀\"";
        let formatted = "title: Text\ncontent: \"😀\"\n";
        let edit = full_document_edit(source, formatted.to_string());

        let last_line = source.lines().last().expect("last line");
        assert_eq!(edit.range.end, Position::new(1, last_line.encode_utf16().count() as u32));
        assert_ne!(
            edit.range.end.character as usize,
            last_line.len(),
            "UTF-16 columns must differ from UTF-8 byte length"
        );
        let applied = apply_edit(source, &edit);
        assert_eq!(applied, formatted);
        assert_eq!(applied.matches('\n').count(), 2, "one separator + one trailing newline");
    }
}
