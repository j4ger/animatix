use crate::app::commands::Effect;
use crate::app::stores::{DocumentStore, PreviewStore};

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
