//! Per-entity resolution cache for the primitive an entity renders with.
//!
//! `PrimitiveRegistry::find` is a linear scan over every registered type
//! name (36 built-ins before any extension), and the render walk asks for
//! the same resolution once per node per frame. The answer only changes
//! when the entity's identity changes (a rebuild — fresh caches) or while
//! a cross-type morph picks a different primitive than the identity type
//! (rare, time-scoped), so the resolved handle is cached per entity,
//! keyed by the actor type it was resolved from. `invalidate_frame_cache`
//! clears it with every other entity-indexed cache.

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

    /// The primitive an entity renders with at `time_ms`, via the per-entity
    /// handle cache.
    ///
    /// While a cross-type morph is active, `render_type_name` returns a name
    /// other than the identity type and the per-frame name picks the
    /// primitive; those lookups deliberately bypass the cache.
    pub(crate) fn cached_primitive_for(
        &self,
        node_id: EntityId,
        time_ms: u64,
    ) -> Option<crate::primitives::PrimitiveHandle> {
        let actor_type = self.world.actor_type(node_id);
        let name = self.world.shape_or_default(node_id).render_type_name(actor_type, time_ms);
        if name != actor_type {
            if let Some(def) = self.behavior_registry.find(name) {
                if let Some(handle) = crate::primitives::PrimitiveHandle::from_behavior(def) {
                    return Some(handle);
                }
            }
            return self.primitive_registry.find_handle(name);
        }
        let idx = node_id.index();
        {
            let cache = self.eval_caches.primitive_handles.borrow();
            if let Some(Some((cached_type, handle))) = cache.get(idx) {
                if cached_type.as_ref() == actor_type {
                    return Some(handle.clone());
                }
            }
        }
        let handle = if let Some(def) = self.behavior_for_node(node_id, time_ms) {
            if let Some(handle) = crate::primitives::PrimitiveHandle::from_behavior(def) {
                handle
            } else {
                self.primitive_registry.find_handle(actor_type)?
            }
        } else {
            self.primitive_registry.find_handle(actor_type)?
        };
        let mut cache = self.eval_caches.primitive_handles.borrow_mut();
        if idx >= cache.len() {
            cache.resize(idx + 1, None);
        }
        cache[idx] = Some((actor_type.into(), handle.clone()));
        Some(handle)
    }
}
