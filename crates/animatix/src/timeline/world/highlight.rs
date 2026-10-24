//! Equation-highlight component accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::HighlightTracks;

impl World {
    /// Highlight tier for an entity.
    #[inline(always)]
    pub fn highlight(&self, id: EntityId) -> &HighlightTracks {
        self.highlight.get(id)
    }

    /// Mutable highlight tier for an entity.
    #[inline(always)]
    pub fn highlight_mut(&mut self, id: EntityId) -> &mut HighlightTracks {
        self.highlight.get_mut(id)
    }
}

impl World {
    /// Maximum keyframe time across the track tiers, payload and highlight.
    #[inline]
    pub fn max_keyframe_time(&self, id: EntityId) -> Option<u64> {
        let track = self
            .tracks
            .get_by_id(id)
            .and_then(|track| track.max_keyframe_time_with(self.payload.get(id)));
        let highlight = self.highlight.try_get(id).and_then(|h| h.max_keyframe_time());
        let tagged = self.tagged_tracks.try_get(id).and_then(|tracks| {
            tracks.values().flatten().filter_map(|t| t.last_keyframe_time()).max()
        });
        let plan = self.property_plan.get(id).max_keyframe_time();
        let geometry = self.geometry.try_get(id).and_then(|g| g.max_keyframe_time());
        let style = self.style.try_get(id).and_then(|s| s.max_keyframe_time());
        let shape = self.shape.try_get(id).and_then(|s| s.max_keyframe_time());
        let text = self.text.try_get(id).and_then(|t| t.max_keyframe_time());
        [track, highlight, tagged, plan, geometry, style, shape, text]
            .into_iter()
            .flatten()
            .max()
    }

    /// Whether any tier (payload, highlight and plan included) has keyframes.
    #[inline]
    pub fn has_any_keyframes(&self, id: EntityId) -> bool {
        self.tracks
            .get_by_id(id)
            .is_some_and(|track| track.has_any_keyframes_with(self.payload.get(id)))
            || self
                .highlight
                .try_get(id)
                .is_some_and(|highlight| highlight.has_any_keyframes())
            || self.tagged_tracks.try_get(id).is_some_and(|tracks| {
                tracks.values().flatten().any(|track| !track.is_effectively_static())
            })
            || self.property_plan.get(id).has_any_keyframes()
            || self.geometry.try_get(id).is_some_and(|geometry| geometry.has_any_keyframes())
            || self.style.try_get(id).is_some_and(|style| style.has_any_keyframes())
            || self.shape.try_get(id).is_some_and(|shape| shape.has_any_keyframes())
            || self.text.try_get(id).is_some_and(|text| text.has_any_keyframes())
    }
}

#[cfg(test)]
mod tests {
    use crate::timeline::Timeline;
    use crate::timeline::property_track::TrackAccessor;

    #[test]
    fn world_highlight_accessor_reads_the_component() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("eq", "Equation");
        timeline.world_mut().highlight_mut(id).highlight_opacity.ensure(0.5);
        assert!(timeline.world().highlight(id).highlight_opacity.is_some());
    }
}
