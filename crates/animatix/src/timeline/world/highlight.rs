//! Equation-highlight component accessors.
//!
//! Transitional delegates: the tier still lives on `AnimationTrack`; these
//! methods let call sites move to `World` before the storage moves.

use super::{EntityId, World};
use crate::timeline::animation_track::HighlightTracks;

impl World {
    /// Highlight tier for an entity.
    pub fn highlight(&self, id: EntityId) -> &HighlightTracks {
        &self
            .tracks
            .get_by_id(id)
            .expect("highlight access requires an existing entity")
            .highlight
    }

    /// Mutable highlight tier for an entity.
    pub fn highlight_mut(&mut self, id: EntityId) -> &mut HighlightTracks {
        &mut self
            .tracks
            .get_by_id_mut(id)
            .expect("highlight access requires an existing entity")
            .highlight
    }
}

#[cfg(test)]
mod tests {
    use crate::timeline::Timeline;
    use crate::timeline::property_track::TrackAccessor;

    #[test]
    fn world_highlight_accessor_delegates() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("eq", "Equation");
        timeline.world_mut().highlight_mut(id).highlight_opacity.ensure(0.5);
        timeline.world_mut().highlight_mut(id).highlight_opacity.ensure(0.75);
        assert!(timeline.world().highlight(id).highlight_opacity.is_some());
    }
}
