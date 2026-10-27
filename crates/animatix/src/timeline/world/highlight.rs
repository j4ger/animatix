//! Equation-highlight component accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::HighlightTracks;

impl World {
    /// Highlight tier for an entity, when present.
    #[inline(always)]
    pub fn highlight(&self, id: EntityId) -> Option<&HighlightTracks> {
        self.highlight.get(id)
    }

    /// Mutable highlight tier for an entity, when present.
    #[inline(always)]
    pub fn highlight_mut(&mut self, id: EntityId) -> Option<&mut HighlightTracks> {
        self.highlight.get_mut(id)
    }

    /// Ensure an entity has a highlight tier and borrow it.
    #[inline(always)]
    pub fn ensure_highlight(&mut self, id: EntityId) -> &mut HighlightTracks {
        self.highlight.get_or_insert_with(id, HighlightTracks::default)
    }

    /// Highlight tier for an entity, falling back to default static tracks when absent.
    #[inline(always)]
    pub fn highlight_or_default(&self, id: EntityId) -> &HighlightTracks {
        self.highlight.get(id).unwrap_or(&self.default_highlight)
    }
}

impl World {
    /// Maximum keyframe time across the entity's property stores, payload and
    /// highlight.
    ///
    /// Registry-backed slots are introspected through the property plan and
    /// the tier components; the payload contributes its plot parameter tracks
    /// and transition end times.
    #[inline]
    pub fn max_keyframe_time(&self, id: EntityId) -> Option<u64> {
        let mut max: Option<u64> = None;
        if let Some(plot) = self.plot_payload.get(id) {
            for pt in plot.plot_param_tracks.values() {
                if let Some(t) = pt.last_keyframe_time() {
                    max = Some(max.map_or(t, |m| m.max(t)));
                }
            }
            for ft in &plot.func_transitions {
                max = Some(max.map_or(ft.end_ms, |m| m.max(ft.end_ms)));
            }
            for bt in &plot.bar_data_transitions {
                max = Some(max.map_or(bt.end_ms, |m| m.max(bt.end_ms)));
            }
        }
        let highlight = self.highlight.get(id).and_then(|h| h.max_keyframe_time());
        let tagged = self.tagged_tracks.try_get(id).and_then(|tracks| {
            tracks.values().flatten().filter_map(|t| t.last_keyframe_time()).max()
        });
        let plan = self.property_plan.get(id).max_keyframe_time();
        let geometry = self.geometry.try_get(id).and_then(|g| g.max_keyframe_time());
        let style = self.style.try_get(id).and_then(|s| s.max_keyframe_time());
        let shape = self.shape.get(id).and_then(|s| s.max_keyframe_time());
        let text = self.text.get(id).and_then(|t| t.max_keyframe_time());
        [highlight, tagged, plan, geometry, style, shape, text]
            .into_iter()
            .flatten()
            .fold(max, |acc, t| Some(acc.map_or(t, |m| m.max(t))))
    }

    /// Whether any property store (payload, highlight and plan included) has
    /// animated keyframes.
    #[inline]
    pub fn has_any_keyframes(&self, id: EntityId) -> bool {
        let payload_animated = self.plot_payload.get(id).is_some_and(|plot| {
            plot.plot_param_tracks.values().any(|t| !t.is_effectively_static())
                || !plot.func_transitions.is_empty()
                || !plot.bar_data_transitions.is_empty()
        });
        payload_animated
            || self.highlight.get(id).is_some_and(|highlight| highlight.has_any_keyframes())
            || self.tagged_tracks.try_get(id).is_some_and(|tracks| {
                tracks.values().flatten().any(|track| !track.is_effectively_static())
            })
            || self.property_plan.get(id).has_any_keyframes()
            || self.geometry.try_get(id).is_some_and(|geometry| geometry.has_any_keyframes())
            || self.style.try_get(id).is_some_and(|style| style.has_any_keyframes())
            || self.shape.get(id).is_some_and(|shape| shape.has_any_keyframes())
            || self.text.get(id).is_some_and(|text| text.has_any_keyframes())
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
        timeline.world_mut().ensure_highlight(id).highlight_opacity.ensure(0.5);
        assert!(timeline.world().highlight(id).unwrap().highlight_opacity.is_some());
    }
}
