//! Borrowed bundles of the per-entity property tiers.
//!
//! The property engine still reads and writes the tier structs through
//! `AnimationTrack` today. [`TrackFields`] / [`TrackFieldsMut`] are the seam
//! that lets a caller assemble the same view from `World` components once a
//! tier moves out of the track: `World::fields(id)` is the only constructor,
//! so moving a tier changes this file and nothing else.
//!
//! See `docs/ecs.md` §6 ("Tier-migration prerequisite").

use std::collections::HashMap;

use crate::primitives::PrimitivePayload;
use crate::timeline::animation_track::{
    GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_track::PropertyTrack;

use super::{EntityId, World};

/// Immutable view of one entity's property tiers.
pub struct TrackFields<'a> {
    /// Geometry tier (at/position/size/rotation/…).
    pub geometry: &'a GeometryTracks,
    /// Style tier (color/opacity/stroke/…).
    pub style: &'a StyleTracks,
    /// Shape tier (shape_type/line_from/…).
    pub shape: &'a ShapeTracks,
    /// Text tier.
    pub text: &'a TextTracks,
    /// Equation-fragment highlight tier.
    pub highlight: &'a HighlightTracks,
    /// Tagged union tracks keyed by property name.
    pub tagged_tracks: &'a HashMap<String, Option<PropertyTrack<PropertyValue>>>,
    /// Primitive payload (plot/SVG/image).
    pub payload: &'a PrimitivePayload,
}

/// Mutable view of one entity's property tiers.
pub struct TrackFieldsMut<'a> {
    /// Geometry tier.
    pub geometry: &'a mut GeometryTracks,
    /// Style tier.
    pub style: &'a mut StyleTracks,
    /// Shape tier.
    pub shape: &'a mut ShapeTracks,
    /// Text tier.
    pub text: &'a mut TextTracks,
    /// Highlight tier.
    pub highlight: &'a mut HighlightTracks,
    /// Tagged union tracks.
    pub tagged_tracks: &'a mut HashMap<String, Option<PropertyTrack<PropertyValue>>>,
    /// Primitive payload.
    pub payload: &'a mut PrimitivePayload,
}

impl World {
    /// Assemble the immutable tier bundle for `id`, when the entity exists.
    pub fn fields(&self, id: EntityId) -> Option<TrackFields<'_>> {
        let track = self.tracks.get_by_id(id)?;
        Some(TrackFields {
            geometry: &track.geometry,
            style: &track.style,
            shape: &track.shape,
            text: &track.text,
            highlight: &track.highlight,
            tagged_tracks: &track.tagged_tracks,
            payload: &track.payload,
        })
    }

    /// Assemble the mutable tier bundle for `id`, when the entity exists.
    pub fn fields_mut(&mut self, id: EntityId) -> Option<TrackFieldsMut<'_>> {
        let track = self.tracks.get_by_id_mut(id)?;
        Some(TrackFieldsMut {
            geometry: &mut track.geometry,
            style: &mut track.style,
            shape: &mut track.shape,
            text: &mut track.text,
            highlight: &mut track.highlight,
            tagged_tracks: &mut track.tagged_tracks,
            payload: &mut track.payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Timeline;

    #[test]
    fn fields_expose_the_entity_tiers() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        timeline.world_mut().tracks.get_by_id_mut(id).unwrap().style.opacity =
            Some(PropertyTrack::new(0.5));

        let fields = timeline.world().fields(id).expect("entity fields");
        assert_eq!(fields.style.opacity.as_ref().unwrap().default_value, 0.5);
        assert!(timeline.world().fields(EntityId(999)).is_none());
    }

    #[test]
    fn mutable_fields_write_through_to_the_track() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");

        {
            let fields = timeline.world_mut().fields_mut(id).expect("entity fields");
            fields.style.opacity = Some(PropertyTrack::new(0.25));
            fields.geometry.size = Some(PropertyTrack::new([10.0, 20.0]));
        }

        let track = timeline.world().tracks.get_by_id(id).unwrap();
        assert_eq!(track.style.opacity.as_ref().unwrap().default_value, 0.25);
        assert_eq!(track.geometry.size.as_ref().unwrap().default_value, [10.0, 20.0]);
    }
}
