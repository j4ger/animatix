//! Prepare-rename and rename requests.

use std::collections::HashMap;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::paths::uri_to_path;
use crate::server::Backend;

pub(crate) async fn prepare_rename(
    backend: &Backend,
    params: TextDocumentPositionParams,
) -> Result<Option<PrepareRenameResponse>> {
    let uri = params.text_document.uri.to_string();
    let position = params.position;

    let range = {
        let analyzers = backend.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        match analyzer.rename_at(position.line as usize, position.character as usize) {
            Ok(target) => target.range,
            // Refusing here is what makes the editor show "cannot rename"
            // instead of letting the user type a name that would be
            // rejected at apply time.
            Err(_) => return Ok(None),
        }
    };

    Ok(Some(PrepareRenameResponse::Range(Range::new(
        Position::new(range.0 as u32, range.1 as u32),
        Position::new(range.2 as u32, range.3 as u32),
    ))))
}

pub(crate) async fn rename(
    backend: &Backend,
    params: RenameParams,
) -> Result<Option<WorkspaceEdit>> {
    let uri = params.text_document_position.text_document.uri.to_string();
    let position = params.text_document_position.position;
    let new_name = params.new_name;

    if let Err(reason) = animatix_analyzer::validate_new_name(&new_name) {
        // Rejecting without an edit surfaces the reason through the client
        // rather than writing a name the analyzer would then flag.
        backend
            .client
            .show_message(MessageType::ERROR, format!("Rename failed: {reason}"))
            .await;
        return Ok(None);
    }

    let analyzers = backend.analyzers.lock().await;

    // Resolve the target in the originating file: its scope-aware
    // references are authoritative.
    let Some(analyzer) = analyzers.get(&uri) else {
        return Ok(None);
    };
    let target = match analyzer.rename_at(position.line as usize, position.character as usize) {
        Ok(target) => target,
        Err(reason) => {
            drop(analyzers);
            backend
                .client
                .show_message(MessageType::ERROR, format!("Rename failed: {}", reason.message()))
                .await;
            return Ok(None);
        },
    };

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

    // The originating file uses cursor-based scope resolution. Other files
    // fall back to name lookup, with one safety guard: a file that
    // *declares its own* binding of the same name keeps it — those
    // occurrences are its own symbol, not the renamed one, and rewriting
    // them would be a silent wrong edit. Files that merely reference an
    // imported name (no declaration of their own) are renamed.
    let workspace = backend.cached_workspace.lock().await;
    let target_path = uri_to_path(&uri);
    for (file_uri, file_analyzer) in analyzers.iter() {
        let ranges = if file_uri == &uri {
            target.references.clone()
        } else {
            let file_path = uri_to_path(file_uri);
            // Only rewrite another file's occurrences when that file
            // reaches the renamed module through its imports (directly or
            // via a re-exporting intermediate): a same-named identifier
            // elsewhere is a different symbol, and rewriting it on name
            // coincidence is a silent wrong edit.
            let imports_target = match (workspace.as_deref(), &target_path, &file_path) {
                (Some(workspace), Some(target_path), Some(file_path)) => {
                    workspace.imports_transitively(file_path, target_path)
                },
                // Without a workspace (single-file session) there is no
                // cross-file set to be wrong about.
                _ => false,
            };
            if !imports_target
                || file_analyzer
                    .occurrences()
                    .iter()
                    .any(|o| o.declaration && o.name == target.name)
            {
                continue;
            }
            file_analyzer.find_references(&target.name)
        };
        if ranges.is_empty() {
            continue;
        }
        let Ok(url) = Url::parse(file_uri) else {
            continue;
        };
        let edits = ranges
            .into_iter()
            .map(|(start_line, start_col, end_line, end_col)| TextEdit {
                range: Range::new(
                    Position::new(start_line as u32, start_col as u32),
                    Position::new(end_line as u32, end_col as u32),
                ),
                new_text: new_name.clone(),
            })
            .collect();
        changes.insert(url, edits);
    }

    if changes.is_empty() {
        return Ok(None);
    }

    Ok(Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }))
}
