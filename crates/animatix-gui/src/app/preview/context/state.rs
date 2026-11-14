use super::PreviewContext;
use crate::app::preview::ActorProps;

impl PreviewContext<'_> {
    pub(crate) fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    pub(crate) fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline?;
        let id = timeline.get_entity_id(actor)?;
        let half = timeline.world().geometry(id).size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps {
            position,
            size,
            rotation,
            pivot_offset,
        })
    }
}
