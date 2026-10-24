//! Capability projection accessors.
//!
//! Built-in actors derive caps from their type on read; extension primitives
//! that override caps install an explicit component.

use super::{EntityId, World};

impl World {
    /// Capability projection for an entity.
    pub fn caps(&self, id: EntityId) -> animatix_core::caps::ActorCaps {
        self.caps_overrides.get(id).copied().unwrap_or_else(|| {
            self.tracks
                .get_by_id(id)
                .and_then(|track| animatix_std::caps_for_type(&track.actor_type))
                .unwrap_or_default()
        })
    }

    /// Install an explicit capability override (extension primitives).
    pub fn set_caps(&mut self, id: EntityId, caps: animatix_core::caps::ActorCaps) {
        self.caps_overrides.insert(id, caps);
    }

    /// Set the actor type and clear any capability override so caps re-derive.
    pub fn set_identity(&mut self, id: EntityId, actor_type: &str) {
        if let Some(track) = self.tracks.get_by_id_mut(id) {
            track.actor_type = actor_type.to_string();
        }
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
