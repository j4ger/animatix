//! Completion, hover, definition and reference requests.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::paths::path_to_uri;
use crate::server::Backend;

pub(crate) async fn completion(
    backend: &Backend,
    params: CompletionParams,
) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri.to_string();
    let position = params.text_document_position.position;

    let items = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(Some(CompletionResponse::Array(vec![])));
        };
        analyzer.completions_at(position.line as usize, position.character as usize)
    };

    let lsp_items: Vec<CompletionItem> = items
        .into_iter()
        .map(|item| {
            let kind = match item.kind {
                animatix_analyzer::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                animatix_analyzer::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
                animatix_analyzer::CompletionKind::Property => CompletionItemKind::PROPERTY,
                animatix_analyzer::CompletionKind::Label => CompletionItemKind::VARIABLE,
                animatix_analyzer::CompletionKind::Action => CompletionItemKind::FUNCTION,
                animatix_analyzer::CompletionKind::Value => CompletionItemKind::VALUE,
                animatix_analyzer::CompletionKind::Snippet => CompletionItemKind::SNIPPET,
            };

            CompletionItem {
                label: item.label,
                kind: Some(kind),
                detail: item.detail,
                documentation: item.documentation.map(Documentation::String),
                insert_text: item.insert_text,
                ..Default::default()
            }
        })
        .collect();

    Ok(Some(CompletionResponse::Array(lsp_items)))
}

pub(crate) async fn hover(backend: &Backend, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri.to_string();
    let position = params.text_document_position_params.position;

    let hover_info = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        analyzer.hover_at(position.line as usize, position.character as usize)
    };

    Ok(hover_info.map(|info| {
        let range = info.range.map(|(sl, sc, el, ec)| {
            Range::new(Position::new(sl as u32, sc as u32), Position::new(el as u32, ec as u32))
        });

        Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: info.contents,
            }),
            range,
        }
    }))
}

pub(crate) async fn goto_definition(
    backend: &Backend,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri.to_string();
    let position = params.text_document_position_params.position;

    let location = {
        let analyzers = backend.analyzers.lock().await;
        let workspace = backend.cached_workspace.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        analyzer.definition_at(
            workspace.as_deref(),
            position.line as usize,
            position.character as usize,
        )
    };

    Ok(location.map(|loc| {
        let target_uri = loc
            .file
            .as_deref()
            .and_then(path_to_uri)
            .unwrap_or_else(|| params.text_document_position_params.text_document.uri.clone());

        GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range::new(
                Position::new(loc.line as u32, loc.col as u32),
                Position::new(loc.line as u32, loc.col as u32),
            ),
        })
    }))
}

pub(crate) async fn references(
    backend: &Backend,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri.to_string();
    let position = params.text_document_position.position;

    let analyzers = backend.analyzers.lock().await;
    let Some(analyzer) = analyzers.get(&uri) else {
        return Ok(None);
    };

    let symbol_name = analyzer.symbol_at(position.line as usize, position.character as usize);

    let Some(symbol_name) = symbol_name else {
        return Ok(None);
    };

    // Search for references across all workspace files. The originating
    // file uses cursor-based scope resolution; other files fall back to
    // name lookup because they do not share the selected binding.
    let mut locations = Vec::new();

    for (file_uri, file_analyzer) in analyzers.iter() {
        let refs = if file_uri == &uri {
            file_analyzer.find_references_at(position.line as usize, position.character as usize)
        } else {
            file_analyzer.find_references(&symbol_name)
        };
        for (start_line, start_col, end_line, end_col) in refs {
            if let Ok(uri) = Url::parse(file_uri) {
                locations.push(Location {
                    uri,
                    range: Range::new(
                        Position::new(start_line as u32, start_col as u32),
                        Position::new(end_line as u32, end_col as u32),
                    ),
                });
            }
        }
    }

    if locations.is_empty() {
        Ok(None)
    } else {
        Ok(Some(locations))
    }
}
