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

    // Remember which positions were scenes vs non-scenes
    let original: Vec<animatix_syntax::ast::Stmt> = std::mem::take(stmts);
    let mut scene_entries: Vec<(String, animatix_syntax::ast::Stmt)> = Vec::new();
    let mut other_entries: Vec<animatix_syntax::ast::Stmt> = Vec::new();

    for stmt in original.clone() {
        match &stmt {
            animatix_syntax::ast::Stmt::Scene { name, .. } => {
                scene_entries.push((name.clone(), stmt));
            }
            _ => {
                other_entries.push(stmt);
            }
        }
    }

    // Validate that new_order contains exactly the same scenes
    let mut old_names: Vec<String> = scene_entries.iter().map(|(n, _)| n.clone()).collect();
    old_names.sort();
    let mut new_names = new_order.clone();
    new_names.sort();
    if old_names != new_names {
        // Restore original order and bail
        *stmts = original;
        return vec![];
    }

    // Build name -> stmt map
    let mut scene_map: std::collections::HashMap<String, animatix_syntax::ast::Stmt> =
        scene_entries.into_iter().collect();

    // Rebuild: for each original position, if it was a scene take from new_order,
    // else take the next non-scene statement
    let mut scene_iter = new_order.into_iter();
    let mut other_iter = other_entries.into_iter();

    for original_stmt in &original {
        match original_stmt {
            animatix_syntax::ast::Stmt::Scene { .. } => {
                if let Some(name) = scene_iter.next() {
                    if let Some(stmt) = scene_map.remove(&name) {
                        stmts.push(stmt);
                    }
                }
            }
            _ => {
                if let Some(stmt) = other_iter.next() {
                    stmts.push(stmt);
                }
            }
        }
    }

    // Re-serialize source
    document_store.source.document.source_text =
        animatix_syntax::to_source::stmts_to_source(stmts);
    document_store.source.document.is_dirty = true;
    document_store.source.editor.replace_text(document_store.source.document.source_text.clone());
    preview_store.preview_dirty = true;

    vec![Effect::Status("Scenes reordered".to_string())]
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
        document_store.source.document.source_text =
            animatix_syntax::to_source::stmts_to_source(stmts);
        document_store.source.document.is_dirty = true;
        document_store.source.editor.replace_text(document_store.source.document.source_text.clone());
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
        document_store.source.document.source_text =
            animatix_syntax::to_source::stmts_to_source(stmts);
        document_store.source.document.is_dirty = true;
        document_store.source.editor.replace_text(document_store.source.document.source_text.clone());
        preview_store.preview_dirty = true;
        ui_store.selection.selected_actors.clear();
        preview_store.preview.status = format!("Deleted scene '{}'", scene);
        vec![Effect::Status(format!("Deleted scene '{}'", scene))]
    } else {
        preview_store.preview.status = format!("Failed to delete scene '{}'", scene);
        vec![]
    }
}
