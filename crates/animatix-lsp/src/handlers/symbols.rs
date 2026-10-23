//! Document/workspace symbols and semantic tokens.

use animatix_syntax::token::LineIndex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::paths::path_to_uri;
use crate::server::{Backend, SEMANTIC_TOKEN_TYPES};

pub(crate) async fn semantic_tokens_full(
    backend: &Backend,
    params: SemanticTokensParams,
) -> Result<Option<SemanticTokensResult>> {
    let uri = params.text_document.uri.to_string();
    let data = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        let roles = analyzer.token_roles();
        build_semantic_tokens(analyzer.source(), &roles)
    };
    Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    })))
}

pub(crate) async fn document_symbol(
    backend: &Backend,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri.to_string();
    let symbols = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        analyzer.document_symbols()
    };

    let lsp_symbols: Vec<SymbolInformation> = symbols
        .into_iter()
        .map(|sym| {
            let kind = match sym.kind {
                animatix_analyzer::SymbolKind::Actor => SymbolKind::VARIABLE,
                animatix_analyzer::SymbolKind::Variable => SymbolKind::VARIABLE,
                animatix_analyzer::SymbolKind::Component => SymbolKind::CLASS,
                animatix_analyzer::SymbolKind::Block => SymbolKind::NAMESPACE,
            };

            #[allow(deprecated)]
            SymbolInformation {
                name: sym.name,
                kind,
                location: Location {
                    uri: params.text_document.uri.clone(),
                    range: Range::new(
                        Position::new(sym.line as u32, sym.col as u32),
                        Position::new(sym.line as u32, sym.col as u32),
                    ),
                },
                tags: None,
                deprecated: None,
                container_name: None,
            }
        })
        .collect();

    Ok(Some(DocumentSymbolResponse::Flat(lsp_symbols)))
}

pub(crate) async fn symbol(
    backend: &Backend,
    params: WorkspaceSymbolParams,
) -> Result<Option<Vec<SymbolInformation>>> {
    let query = params.query.to_lowercase();
    let analyzers = backend.analyzers.lock().await;
    let mut all_symbols = Vec::new();

    for (uri, analyzer) in analyzers.iter() {
        let symbols = analyzer.document_symbols();
        for sym in symbols {
            if !query.is_empty() && !sym.name.to_lowercase().contains(&query) {
                continue;
            }
            let kind = match sym.kind {
                animatix_analyzer::SymbolKind::Actor => SymbolKind::VARIABLE,
                animatix_analyzer::SymbolKind::Variable => SymbolKind::VARIABLE,
                animatix_analyzer::SymbolKind::Component => SymbolKind::CLASS,
                animatix_analyzer::SymbolKind::Block => SymbolKind::NAMESPACE,
            };

            #[allow(deprecated)]
            all_symbols.push(SymbolInformation {
                name: sym.name,
                kind,
                location: Location {
                    uri: Url::parse(uri).unwrap_or_else(|_| {
                        path_to_uri(uri).unwrap_or_else(|| Url::parse("file:///unknown").unwrap())
                    }),
                    range: Range::new(
                        Position::new(sym.line as u32, sym.col as u32),
                        Position::new(sym.line as u32, sym.col as u32),
                    ),
                },
                tags: None,
                deprecated: None,
                container_name: None,
            });
        }
    }

    if all_symbols.is_empty() {
        Ok(None)
    } else {
        Ok(Some(all_symbols))
    }
}

/// Build LSP semantic token deltas from analyzer token roles.
fn build_semantic_tokens(
    source: &str,
    roles: &[(usize, usize, &'static str)],
) -> Vec<SemanticToken> {
    let line_index = LineIndex::new(source);
    let mut data = Vec::with_capacity(roles.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    for &(start, end, role) in roles {
        let (line, col) = line_index.byte_to_line_col_utf16(start);
        let (_, end_col) = line_index.byte_to_line_col_utf16(end);
        let delta_line = line as u32 - prev_line;
        let delta_start = if delta_line == 0 {
            col as u32 - prev_col
        } else {
            col as u32
        };
        let length = (end_col - col) as u32;
        let token_type = role_index(role);

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line as u32;
        prev_col = col as u32;
    }

    data
}

fn role_index(role: &str) -> u32 {
    SEMANTIC_TOKEN_TYPES
        .iter()
        .position(|name| *name == role)
        .map(|idx| idx as u32)
        .unwrap_or(6) // fall back to variable for unknown roles
}
