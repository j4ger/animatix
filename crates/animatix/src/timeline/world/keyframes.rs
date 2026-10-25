//! Property-name keyed keyframe introspection over one entity.
//!
//! The GUI, build diagnostics and action verbs ask questions like "does
//! `card` have an `opacity` keyframe at 1s" by property *name*; inside the
//! engine everything is an [`ActorField`] on an [`EntityId`]. This module is
//! the boundary conversion, layered on the id-keyed queries in
//! `world/fields.rs`.

use super::{EntityId, World};

impl World {
    /// Read a property value, falling back to the schema default if the
    /// entity has no value for this property.
    ///
    /// The actor type is resolved from the entity itself: it is only needed
    /// for the schema default, whose caps projection is identity-derived.
    pub fn read_property_value_or_default(
        &self,
        id: EntityId,
        schema: &crate::timeline::property_registry::PropertySchema,
        time_ms: u64,
    ) -> crate::timeline::PropertyValue {
        self.read_property_value(id, schema.field, time_ms).unwrap_or_else(|| {
            (schema.default_value)(
                &animatix_std::caps_for_type(self.actor_type(id)).unwrap_or_default(),
            )
        })
    }

    /// Check if a keyframe exists for the given property at exactly `time_ms`.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframe_at(&self, id: EntityId, property: &str, time_ms: u64) -> bool {
        let Some(field) = crate::timeline::property_registry::field_for_property(property) else {
            return false;
        };
        self.property_has_keyframe_at(id, field, time_ms)
    }

    /// Check if the property has any keyframes at all (regardless of time).
    pub fn has_keyframes_for(&self, id: EntityId, property: &str) -> bool {
        let Some(field) = crate::timeline::property_registry::field_for_property(property) else {
            return false;
        };
        self.property_keyframe_count(id, field) > 0
    }

    /// List all keyframe times (in ms) for the given property, sorted and
    /// deduplicated.
    pub fn list_keyframes(&self, id: EntityId, property: &str) -> Vec<u64> {
        let Some(field) = crate::timeline::property_registry::field_for_property(property) else {
            return Vec::new();
        };
        let mut times: Vec<u64> = self.property_keyframe_times(id, field);
        times.sort_unstable();
        times.dedup();
        times
    }
}

#[cfg(test)]
mod tests {
    use crate::timeline::Timeline;
    use crate::timeline::property_track::PropertyTrack;

    #[test]
    fn property_name_queries_resolve_through_the_world() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let mut opacity = PropertyTrack::new(0.0_f32);
        opacity.add_keyframe(100, 1.0, crate::easing::Easing::Linear);
        timeline.world_mut().style_mut(id).opacity = Some(opacity);

        assert!(timeline.world().has_keyframes_for(id, "opacity"));
        assert!(!timeline.world().has_keyframes_for(id, "rotation"));
        assert_eq!(timeline.world().list_keyframes(id, "opacity"), vec![100]);
        assert!(timeline.world().has_keyframe_at(id, "opacity", 100));
        assert!(!timeline.world().has_keyframe_at(id, "opacity", 50));
        assert!(!timeline.world().has_keyframe_at(id, "not_a_property", 0));
    }
}
