//! Capability projection accessors.
//!
//! Built-in actors derive caps from their type on read; extension primitives
//! that override caps install an explicit component.

use super::{EntityId, World};

impl World {
    /// Capability projection for an entity.
    pub fn caps(&self, id: EntityId) -> animatix_core::caps::ActorCaps {
        self.caps_overrides.get(id).copied().unwrap_or_else(|| {
            self.actor_types
                .try_get(id)
                .and_then(|ty| animatix_std::caps_for_type(ty))
                .unwrap_or_default()
        })
    }

    /// Install an explicit capability override (extension primitives).
    pub fn set_caps(&mut self, id: EntityId, caps: animatix_core::caps::ActorCaps) {
        self.caps_overrides.insert(id, caps);
    }

    /// Set the actor type and clear any capability override so caps re-derive.
    pub fn set_identity(&mut self, id: EntityId, actor_type: &str) {
        self.ensure_len();
        *self.actor_types.get_mut(id) = actor_type.to_string();
        self.caps_overrides.remove(id);
    }

    /// Whether the entity is a group-like structural actor.
    pub fn is_group_like(&self, id: EntityId) -> bool {
        self.caps(id).is_group_like()
    }

    /// Whether the entity renders visual content.
    pub fn renders_visual_content(&self, id: EntityId) -> bool {
        self.caps(id).renders_visual_content()
    }
}

impl World {
    /// Authoring/registration type name for an entity.
    #[inline(always)]
    pub fn actor_type(&self, id: EntityId) -> &str {
        self.actor_types.try_get(id).map_or("", String::as_str)
    }
}
