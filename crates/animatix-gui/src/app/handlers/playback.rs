use std::time::{Duration, Instant};

use crate::app::commands::Effect;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};
use crate::document::timeline_keyframe_times_s;

pub fn handle_scrub_to(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    next_time: f64,
) -> Vec<Effect> {
    preview_store.preview.playback.current_time_s = next_time;
    preview_store.preview.playback.clamp_time();
    preview_store.preview.playback.is_playing = false;
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects: Vec<Effect> = vec![];
    if ui_store.editor_sync_enabled {
        if let Some(line) = document_store
            .document
            .find_keyframe_line_at(preview_store.preview.playback.current_time_s)
        {
            effects.push(Effect::EditorScroll(line));
            effects.push(Effect::EditorHighlight(line));
        }
    }
    effects
}

pub fn handle_toggle_playback(preview_store: &mut PreviewStore) -> Vec<Effect> {
    preview_store.preview.playback.toggle_playback();
    preview_store.preview_dirty = true;
    vec![Effect::Repaint]
}

pub fn handle_toggle_editor_sync(ui_store: &mut UiStore) -> Vec<Effect> {
    ui_store.editor_sync_enabled = !ui_store.editor_sync_enabled;
    vec![if ui_store.editor_sync_enabled {
        Effect::Status("Editor sync ON".to_string())
    } else {
        Effect::Status("Editor sync OFF".to_string())
    }]
}

pub fn handle_editor_changed(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    document_store
        .document
        .set_source_text(document_store.editor.text().to_string());
    preview_store.pending_rebuild_at =
        Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
    preview_store.preview.error = None;
    document_store.document.diagnostics.clear();
    vec![
        Effect::Status("Editing source • rebuild scheduled".to_string()),
        Effect::RebuildScheduled,
    ]
}

pub fn handle_prev_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = timeline_keyframe_times_s(
        if document_store.document.composition.is_some() {
            None
        } else {
            document_store.document.active_timeline()
        },
        document_store.document.composition.as_ref(),
        document_store.document.active_scene.as_deref(),
    );
    preview_store.preview.playback.go_to_previous_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    let status = format!(
        "Previous keyframe • t = {:.2}s / {:.2}s",
        preview_store.preview.playback.current_time_s,
        preview_store.preview.playback.duration_s
    );
    let mut effects = vec![Effect::Status(status)];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

pub fn handle_next_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = timeline_keyframe_times_s(
        if document_store.document.composition.is_some() {
            None
        } else {
            document_store.document.active_timeline()
        },
        document_store.document.composition.as_ref(),
        document_store.document.active_scene.as_deref(),
    );
    preview_store.preview.playback.go_to_next_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    let status = format!(
        "Next keyframe • t = {:.2}s / {:.2}s",
        preview_store.preview.playback.current_time_s,
        preview_store.preview.playback.duration_s
    );
    let mut effects = vec![Effect::Status(status)];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

fn sync_active_scene_from_time(
    document_store: &mut DocumentStore,
    preview_store: &PreviewStore,
) {
    if let Some(composition) = document_store.document.composition.as_ref() {
        let (scene, _, _) =
            composition.evaluate(preview_store.preview.playback.current_time_s);
        document_store.document.active_scene = (!scene.is_empty()).then_some(scene);
    }
}

fn editor_sync_effects(
    document_store: &DocumentStore,
    ui_store: &UiStore,
    time_s: f64,
) -> Vec<Effect> {
    let mut effects = vec![];
    if ui_store.editor_sync_enabled {
        if let Some(line) = document_store.document.find_keyframe_line_at(time_s) {
            effects.push(Effect::EditorScroll(line));
            effects.push(Effect::EditorHighlight(line));
        }
    }
    effects
}
