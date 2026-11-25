use std::time::{Duration, Instant};

use crate::app::commands::Effect;
use crate::app::document::timeline_keyframe_times_s;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

pub fn handle_scrub_to(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
    next_time: f64,
) -> Vec<Effect> {
    preview_store.preview.playback.scrub_to(next_time);
    preview_store.preview.playback.clamp_time();
    preview_store.preview.playback.is_playing = false;
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects: Vec<Effect> = vec![];
    if ui_store.editor_sync_enabled
        && let Some(line) = document_store
            .source
            .document
            .find_keyframe_line_at(preview_store.preview.playback.current_time_s)
    {
        effects.push(Effect::EditorScroll(line));
        effects.push(Effect::EditorHighlight(line));
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
        Effect::Status(crate::app::i18n::StatusText::message("gui-playback-editor-sync-on", []))
    } else {
        Effect::Status(crate::app::i18n::StatusText::message("gui-playback-editor-sync-off", []))
    }]
}

pub fn handle_editor_changed(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    document_store.sync_from_editor();
    preview_store.pending_rebuild_at =
        Some(Instant::now() + Duration::from_millis(ui_store.rebuild_debounce_ms));
    preview_store.preview.error = None;
    vec![Effect::Status(crate::app::i18n::StatusText::message(
        "gui-playback-rebuild-scheduled",
        [],
    ))]
}

fn collect_keyframes(document_store: &DocumentStore) -> Vec<f64> {
    timeline_keyframe_times_s(
        if document_store.source.document.composition.is_some() {
            None
        } else {
            document_store.source.document.active_timeline()
        },
        document_store.source.document.composition.as_ref(),
        document_store.source.document.active_scene.as_deref(),
    )
}

fn keyframe_effects(
    document_store: &DocumentStore,
    preview_store: &PreviewStore,
    ui_store: &UiStore,
    message: &'static str,
) -> Vec<Effect> {
    let status = crate::app::i18n::StatusText::message(
        message,
        [
            ("time", format!("{:.2}", preview_store.preview.playback.current_time_s)),
            ("duration", format!("{:.2}", preview_store.preview.playback.duration_s)),
        ],
    );
    let mut effects = vec![Effect::Status(status)];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

pub fn handle_prev_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = collect_keyframes(document_store);
    preview_store.preview.playback.go_to_previous_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    keyframe_effects(document_store, preview_store, ui_store, "gui-playback-keyframe-previous")
}

pub fn handle_next_keyframe(
    document_store: &DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let keyframes = collect_keyframes(document_store);
    preview_store.preview.playback.go_to_next_keyframe(&keyframes);
    preview_store.preview_dirty = true;
    keyframe_effects(document_store, preview_store, ui_store, "gui-playback-keyframe-next")
}

pub fn handle_frame_step_forward(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let fps = ui_store.snap_fps.max(1.0);
    preview_store.preview.playback.frame_step_forward(fps);
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects = vec![Effect::Status(crate::app::i18n::StatusText::message(
        "gui-playback-frame-forward",
        [("time", format!("{:.2}", preview_store.preview.playback.current_time_s))],
    ))];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

pub fn handle_frame_step_backward(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    let fps = ui_store.snap_fps.max(1.0);
    preview_store.preview.playback.frame_step_backward(fps);
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects = vec![Effect::Status(crate::app::i18n::StatusText::message(
        "gui-playback-frame-backward",
        [("time", format!("{:.2}", preview_store.preview.playback.current_time_s))],
    ))];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

fn sync_active_scene_from_time(document_store: &mut DocumentStore, preview_store: &PreviewStore) {
    if let Some(composition) = document_store.source.document.composition.as_ref() {
        document_store.source.document.active_scene = crate::app::document::review_scene_at_time(
            composition,
            preview_store.preview.playback.current_time_s,
        );
    }
}

fn editor_sync_effects(
    document_store: &DocumentStore,
    ui_store: &UiStore,
    time_s: f64,
) -> Vec<Effect> {
    let mut effects = vec![];
    if ui_store.editor_sync_enabled
        && let Some(line) = document_store.source.document.find_keyframe_line_at(time_s)
    {
        effects.push(Effect::EditorScroll(line));
        effects.push(Effect::EditorHighlight(line));
    }
    effects
}

pub fn handle_step_next(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    preview_store.preview.playback.presentation_step_next();
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects = vec![
        Effect::Repaint,
        Effect::Status(crate::app::i18n::StatusText::message(
            "gui-playback-step-next",
            [
                ("time", format!("{:.2}", preview_store.preview.playback.current_time_s)),
                ("duration", format!("{:.2}", preview_store.preview.playback.duration_s)),
            ],
        )),
    ];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

pub fn handle_step_previous(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &UiStore,
) -> Vec<Effect> {
    preview_store.preview.playback.presentation_step_prev();
    preview_store.preview_dirty = true;
    sync_active_scene_from_time(document_store, preview_store);
    let mut effects = vec![
        Effect::Repaint,
        Effect::Status(crate::app::i18n::StatusText::message(
            "gui-playback-step-prev",
            [
                ("time", format!("{:.2}", preview_store.preview.playback.current_time_s)),
                ("duration", format!("{:.2}", preview_store.preview.playback.duration_s)),
            ],
        )),
    ];
    effects.extend(editor_sync_effects(
        document_store,
        ui_store,
        preview_store.preview.playback.current_time_s,
    ));
    effects
}

pub fn handle_toggle_presentation_mode(preview_store: &mut PreviewStore) -> Vec<Effect> {
    preview_store.preview.playback.toggle_presentation_mode();
    if preview_store.preview.playback.presentation_mode {
        preview_store.preview.fit_zoom_requested = true;
    }
    let status = if preview_store.preview.playback.presentation_mode {
        crate::app::i18n::StatusText::message("gui-playback-presentation-on", [])
    } else {
        crate::app::i18n::StatusText::message("gui-playback-presentation-off", [])
    };
    vec![Effect::Repaint, Effect::Status(status)]
}
