use super::begin_snapshot;
use crate::app::commands::{Command, Effect};
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

/// Shared commit path for scene-level source edits.
///
/// Takes the undo snapshot, applies `edit`, commits through
/// [`crate::app::document_controller::commit_ast`], and reports the outcome.
/// On failure the pending snapshot is aborted and the untouched AST is put
/// back. `success` is reported both as the preview status and as the returned
/// [`Effect::Status`]; `failure` (when given) becomes the preview status on a
/// failed edit. Reorder passes `None` because it only logs, exactly as before.
fn apply_scene_edit(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    command: Command,
    edit: crate::source_edit::SourceEdit,
    success: String,
    failure: Option<String>,
) -> Vec<Effect> {
    begin_snapshot(document_store, preview_store, ui_store, command);
    let Some(mut stmts) = document_store.source.document.raw_statements.take() else {
        document_store.abort_snapshot();
        return vec![];
    };
    if let Err(err) = crate::source_edit::apply_edit(&mut stmts, edit) {
        document_store.source.document.raw_statements = Some(stmts);
        document_store.abort_snapshot();
        tracing::warn!("Scene edit failed: {err}");
        if let Some(failure) = failure {
            preview_store.preview.status = failure;
        }
        return vec![];
    }
    crate::app::document_controller::commit_ast(
        document_store,
        preview_store,
        ui_store,
        stmts,
        Vec::new(),
    );
    preview_store.preview_dirty = true;
    preview_store.preview.status = success.clone();
    vec![Effect::Status(success)]
}

pub fn handle_reorder_scenes(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    new_order: Vec<String>,
) -> Vec<Effect> {
    apply_scene_edit(
        document_store,
        preview_store,
        ui_store,
        Command::ReorderScenes(new_order.clone()),
        crate::source_edit::SourceEdit::ReorderScenes { new_order },
        "Scenes reordered".to_string(),
        None,
    )
}

pub fn handle_select_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut crate::app::stores::UiStore,
    scene: String,
) -> Vec<Effect> {
    if let Some(composition) = document_store.source.document.composition.as_ref()
        && composition.scenes.contains_key(&scene)
    {
        document_store.source.document.active_scene = Some(scene.clone());
        if let Some(start) = composition.scene_start_times.get(&scene) {
            let mut target_time = *start;
            for edge in composition.edges.values() {
                if edge.to_scene == scene {
                    target_time += edge.transition.duration_ms as f64 / 1000.0;
                    break;
                }
            }
            preview_store.preview.playback.scrub_to(target_time);
            preview_store.preview.playback.clamp_time();
            preview_store.preview.playback.is_playing = false;
            preview_store.preview_dirty = true;
            return vec![Effect::Status(format!(
                "Scene {} • t = {:.2}s / {:.2}s",
                scene,
                preview_store.preview.playback.current_time_s,
                preview_store.preview.playback.duration_s
            ))];
        }
    }
    vec![]
}

pub fn handle_duplicate_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: String,
) -> Vec<Effect> {
    let success = format!("Duplicated scene '{}'", scene);
    let failure = format!("Failed to duplicate scene '{}'", scene);
    apply_scene_edit(
        document_store,
        preview_store,
        ui_store,
        Command::DuplicateScene(scene.clone()),
        crate::source_edit::SourceEdit::DuplicateScene { name: scene },
        success,
        Some(failure),
    )
}

pub fn handle_delete_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut crate::app::stores::UiStore,
    scene: String,
) -> Vec<Effect> {
    let success = format!("Deleted scene '{}'", scene);
    let failure = format!("Failed to delete scene '{}'", scene);
    let effects = apply_scene_edit(
        document_store,
        preview_store,
        ui_store,
        Command::DeleteScene(scene.clone()),
        crate::source_edit::SourceEdit::DeleteScene { name: scene },
        success,
        Some(failure),
    );
    if !effects.is_empty() {
        ui_store.selection.selected_actors.clear();
    }
    effects
}

pub fn handle_add_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: String,
) -> Vec<Effect> {
    let success = format!("Added scene '{}'", scene);
    let failure = format!("Failed to add scene '{}'", scene);
    apply_scene_edit(
        document_store,
        preview_store,
        ui_store,
        Command::AddScene(scene.clone()),
        crate::source_edit::SourceEdit::AddScene { name: scene },
        success,
        Some(failure),
    )
}
