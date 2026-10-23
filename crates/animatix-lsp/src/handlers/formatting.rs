//! Document formatting.

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

    // Replace the entire document
    let lines: Vec<&str> = source.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map(|l| l.len()).unwrap_or(0) as u32;

    Ok(Some(vec![TextEdit {
        range: Range::new(Position::new(0, 0), Position::new(last_line, last_char)),
        new_text: formatted,
    }]))
}
