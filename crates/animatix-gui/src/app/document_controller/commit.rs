//! The single AST-commit path: serialize, index, install, and schedule a
//! debounced rebuild.

use std::time::Duration;

use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

/// Commit a mutated AST back to source text through the single edit-commit
/// path.
///
/// Serializes `stmts`, rebuilds the source index, installs the AST, commits the
/// source text with the post-edit UI snapshot, rescans keyframe lines, records
/// adjustment flashes, and schedules the debounced rebuild.
pub(crate) fn commit_ast(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    stmts: Vec<animatix_syntax::ast::Stmt>,
    flashes: Vec<f64>,
) {
    commit_ast_with_debounce(
        document_store,
        preview_store,
        ui_store,
        stmts,
        flashes,
        ui_store.rebuild_debounce_ms,
    );
}

/// [`commit_ast`] with an explicit rebuild delay.
///
/// Some actor handlers predate the `rebuild_debounce_ms` setting and schedule
/// their rebuild 100ms out; they call this to keep that timing while still
/// sharing the single commit path.
pub(crate) fn commit_ast_with_debounce(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    stmts: Vec<animatix_syntax::ast::Stmt>,
    flashes: Vec<f64>,
    debounce_ms: u64,
) {
    let new_source = animatix_syntax::to_source::stmts_to_source(&stmts);
    let source_index = animatix_syntax::source_index::SourceIndex::build(&stmts);
    document_store.source.document.raw_statements = Some(stmts);

    let ui_after = ui_store.snapshot_with_preview(preview_store);
    document_store.commit_source(new_source, source_index, ui_after);
    document_store.source.document.rescan_keyframe_lines();
    for time in flashes {
        preview_store
            .preview
            .flashed_keyframe_times
            .push((time, std::time::Instant::now()));
    }
    preview_store.pending_rebuild_at =
        Some(std::time::Instant::now() + Duration::from_millis(debounce_ms));
}
