use crate::app::commands::Effect;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

/// Select a scene in the read-only composition review.
pub fn handle_select_scene(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    _ui_store: &mut UiStore,
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
            return vec![Effect::Status(crate::app::i18n::StatusText::message(
                "gui-playback-selected-scene",
                [
                    ("scene", scene),
                    ("time", format!("{:.2}", preview_store.preview.playback.current_time_s)),
                    ("duration", format!("{:.2}", preview_store.preview.playback.duration_s)),
                ],
            ))];
        }
    }
    vec![]
}
