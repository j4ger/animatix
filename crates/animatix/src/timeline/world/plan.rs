//! Registry-driven `PropertyPlan` accessors.
//!
//! Transitional delegates: the plan still lives on `AnimationTrack`; these
//! methods let call sites move to entity ids before the storage moves.

use super::{EntityId, World};

impl World {
    /// Property plan for an entity.
    #[inline(always)]
    pub fn property_plan(&self, id: EntityId) -> &crate::timeline::plan::PropertyPlan {
        &self
            .tracks
            .get_by_id(id)
            .expect("property plan access requires an existing entity")
            .property_plan
    }

    /// Mutable property plan for an entity.
    #[inline(always)]
    pub fn property_plan_mut(&mut self, id: EntityId) -> &mut crate::timeline::plan::PropertyPlan {
        &mut self
            .tracks
            .get_by_id_mut(id)
            .expect("property plan access requires an existing entity")
            .property_plan
    }

    /// Rebuild an entity's plan from its identity, preserving extension slots.
    pub fn rebuild_property_plan(&mut self, id: EntityId) {
        self.tracks
            .get_by_id_mut(id)
            .expect("rebuild requires an existing entity")
            .rebuild_property_plan();
    }
}

#[cfg(test)]
mod tests {
    use crate::timeline::Timeline;

    #[test]
    fn world_property_plan_accessor_delegates() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        timeline.world_mut().rebuild_property_plan(id);
        assert!(
            timeline
                .world()
                .property_plan(id)
                .get(crate::timeline::property_registry::property_id("opacity").expect("opacity"),)
                .is_some()
        );
    }
}
