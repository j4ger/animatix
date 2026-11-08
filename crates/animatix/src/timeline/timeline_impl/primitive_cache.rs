//! Helper for resolving the primitive an entity renders with.
//!
//! Provides the entry point for looking up the behavior definition
//! that drives rendering for a given entity at a specific time.

use crate::timeline::*;

impl Timeline {
    /// Return the active behavior definition for `node_id` at `time_ms`.
    pub(crate) fn behavior_for_node(
        &self,
        node_id: EntityId,
        time_ms: u64,
    ) -> Option<&crate::timeline::behavior::BehaviorDef> {
        let actor_type = self.world.actor_type(node_id);
        let name = self.world.shape_or_default(node_id).render_type_name(actor_type, time_ms);
        if name != actor_type {
            return self.behavior_registry.find(name);
        }
        let behavior_id = self.world.behavior(node_id);
        if behavior_id.is_valid() {
            self.behavior_registry.get(behavior_id)
        } else {
            self.behavior_registry.find(actor_type)
        }
    }
}
