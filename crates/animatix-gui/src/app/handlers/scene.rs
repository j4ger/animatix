use crate::app::commands::{Command, Effect};
use crate::app::stores::{DocumentStore, PreviewStore};

pub fn handle_reorder_scenes(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    new_order: Vec<String>,
) -> Vec<Effect> {
    let Some(ref mut stmts) = document_store.source.document.raw_statements else {
        return vec![];
    };

    let edit = crate::source_edit::SourceEdit::ReorderScenes { new_order };
    match crate::source_edit::apply_edit(stmts, edit) {
        Ok(()) => {
            let (new_source, source_index) = (
                animatix_syntax::to_source::stmts_to_source(stmts),
                animatix_syntax::source_index::SourceIndex::build(stmts),
            );
            document_store.source.commit_source(new_source, source_index);
            preview_store.preview_dirty = true;
            vec![Effect::Status("Scenes reordered".to_string())]
        }
        Err(e) => {
            tracing::warn!("ReorderScenes failed: {e}");
            vec![]
        }
    }
}

pub fn handle_select_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    scene: String,
) -> Vec<Effect> {
    if let Some(composition) = document_store.source.document.composition.as_ref() {
        if composition.scenes.contains_key(&scene) {
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
    }
    vec![]
}

pub fn handle_duplicate_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    scene: String,
) -> Vec<Effect> {
    document_store.snapshot(Command::DuplicateScene(scene.clone()));
    let Some(ref mut stmts) = document_store.source.document.raw_statements else {
        return vec![];
    };

    if crate::source_edit::apply_edit(
        stmts,
        crate::source_edit::SourceEdit::DuplicateScene { name: scene.clone() },
    ).is_ok() {
        let (new_source, source_index) = (animatix_syntax::to_source::stmts_to_source(stmts), animatix_syntax::source_index::SourceIndex::build(stmts));
        document_store.source.commit_source(new_source, source_index);
        preview_store.preview_dirty = true;
        preview_store.preview.status = format!("Duplicated scene '{}'", scene);
        vec![Effect::Status(format!("Duplicated scene '{}'", scene))]
    } else {
        preview_store.preview.status = format!("Failed to duplicate scene '{}'", scene);
        vec![]
    }
}

pub fn handle_delete_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut crate::app::stores::UiStore,
    scene: String,
) -> Vec<Effect> {
    document_store.snapshot(Command::DeleteScene(scene.clone()));
    let Some(ref mut stmts) = document_store.source.document.raw_statements else {
        return vec![];
    };

    if crate::source_edit::apply_edit(
        stmts,
        crate::source_edit::SourceEdit::DeleteScene { name: scene.clone() },
    ).is_ok() {
        let (new_source, source_index) = (animatix_syntax::to_source::stmts_to_source(stmts), animatix_syntax::source_index::SourceIndex::build(stmts));
        document_store.source.commit_source(new_source, source_index);
        preview_store.preview_dirty = true;
        ui_store.selection.selected_actors.clear();
        preview_store.preview.status = format!("Deleted scene '{}'", scene);
        vec![Effect::Status(format!("Deleted scene '{}'", scene))]
    } else {
        preview_store.preview.status = format!("Failed to delete scene '{}'", scene);
        vec![]
    }
}
