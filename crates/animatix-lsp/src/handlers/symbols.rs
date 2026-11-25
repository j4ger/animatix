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
        let (first_line, first_col) = line_index.byte_to_line_col_utf16(start);
        for (offset, part) in source[start..end].split_inclusive('\n').enumerate() {
            let line = first_line + offset;
            let col = if offset == 0 { first_col } else { 0 };
            // Line comments end immediately before LF and can own the CR of a CRLF.
            let text = if let Some(text) = part.strip_suffix('\n') {
                text.strip_suffix('\r').unwrap_or(text)
            } else if source[end..].starts_with('\n') {
                part.strip_suffix('\r').unwrap_or(part)
            } else {
                part
            };
            let length = text.encode_utf16().count();
            if length > 0 {
                let delta_line = line as u32 - prev_line;
                data.push(SemanticToken {
                    delta_line,
                    delta_start: if delta_line == 0 {
                        col as u32 - prev_col
                    } else {
                        col as u32
                    },
                    length: length as u32,
                    token_type: role_index(role),
                    token_modifiers_bitset: 0,
                });
                prev_line = line as u32;
                prev_col = col as u32;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiline_tokens_use_line_local_utf16_ranges() {
        let s = "let face = r#\"中🎬\r\n\r\nX\"#\n#2s";
        let start = s.find("r#").unwrap();
        let end = s.find("\n#2s").unwrap();
        let tokens =
            build_semantic_tokens(s, &[(start, end, "string"), (end + 1, s.len(), "number")]);
        let positions: Vec<_> =
            tokens.iter().map(|t| (t.delta_line, t.delta_start, t.length)).collect();
        assert_eq!(positions, [(0, 11, 6), (2, 0, 3), (1, 0, 3)]);
    }
    #[test]
    fn line_comment_before_crlf_excludes_carriage_return() {
        let source = "// 🎬\r\n#2s";
        let lf = source.find('\n').unwrap();
        let tokens = build_semantic_tokens(
            source,
            &[(0, lf, "comment"), (lf + 1, source.len(), "keyframe")],
        );
        assert_eq!((tokens[0].length, tokens[1].delta_line, tokens[1].delta_start), (5, 1, 0));
    }
}
