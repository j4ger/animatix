//! Registry-driven `PropertyPlan` component accessors.

use super::{EntityId, World};

impl World {
    /// Property plan for an entity.
    #[inline(always)]
    pub fn property_plan(&self, id: EntityId) -> &crate::timeline::plan::PropertyPlan {
        self.property_plan.get(id)
    }

    /// Mutable property plan for an entity.
    #[inline(always)]
    pub fn property_plan_mut(&mut self, id: EntityId) -> &mut crate::timeline::plan::PropertyPlan {
        self.property_plan.get_mut(id)
    }

    /// Rebuild an entity's plan from its identity, preserving extension slots.
    pub fn rebuild_property_plan(&mut self, id: EntityId) {
        let (caps, actor_type) = {
            let track = self.tracks.get_by_id(id).expect("rebuild requires an existing entity");
            (track.caps, track.actor_type.clone())
        };
        let mut plan = crate::timeline::plan::PropertyPlan::for_actor(&caps, &actor_type);
        plan.preserve_extension_slots(self.property_plan.get(id));
        *self.property_plan.get_mut(id) = plan;
    }
}

#[cfg(test)]
mod tests {
    use crate::timeline::Timeline;

    #[test]
    fn world_property_plan_rebuilds() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        timeline.world_mut().rebuild_property_plan(id);
        assert!(
            timeline
                .world()
                .property_plan(id)
                .get(crate::timeline::property_registry::property_id("opacity").expect("opacity"))
                .is_some()
        );
    }
}
