//! Capability projection accessors.
//!
//! Caps are stored as an ECS component (`RequiredStore<ActorCaps>`) initialized
//! at `spawn`/`set_identity` from the catalog; extension primitives that
//! override caps install an explicit override in `caps_overrides`.

use super::{EntityId, World};

impl World {
    /// Capability projection for an entity.
    #[inline(always)]
    pub fn caps(&self, id: EntityId) -> animatix_core::caps::ActorCaps {
        self.caps_overrides
            .get(id)
            .copied()
            .unwrap_or_else(|| self.caps.try_get(id).copied().unwrap_or_default())
    }

    /// Install an explicit capability override (extension primitives).
    pub fn set_caps(&mut self, id: EntityId, caps: animatix_core::caps::ActorCaps) {
        self.caps_overrides.insert(id, caps);
    }

    /// Set the actor type and clear any capability override so caps re-derive.
    pub fn set_identity(&mut self, id: EntityId, actor_type: &str) {
        self.ensure_len();
        *self.actor_types.get_mut(id) = actor_type.to_string();
        *self.caps.get_mut(id) = animatix_std::caps_for_type(actor_type).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_initializes_caps_for_all_catalog_primitives() {
        for info in animatix_std::CATALOG.iter() {
            let mut world = World::new();
            let id = world.spawn("test_actor", info.type_name.as_ref());
            let expected = animatix_std::caps_from_info(info);
            assert_eq!(
                world.caps(id),
                expected,
                "caps mismatch for primitive '{}'",
                info.type_name.as_ref()
            );
        }
    }

    #[test]
    fn set_identity_updates_caps() {
        let mut world = World::new();
        let id = world.spawn("node", "Rect");
        assert_eq!(world.caps(id), animatix_std::caps_for_type("Rect").unwrap());
        assert_eq!(world.actor_type(id), "Rect");

        world.set_identity(id, "Text");
        assert_eq!(world.caps(id), animatix_std::caps_for_type("Text").unwrap());
        assert_eq!(world.actor_type(id), "Text");
    }

    #[test]
    fn set_caps_overrides_and_set_identity_clears_override() {
        let mut world = World::new();
        let id = world.spawn("node", "Rect");
        let rect_caps = animatix_std::caps_for_type("Rect").unwrap();
        assert_eq!(world.caps(id), rect_caps);

        let custom = animatix_core::caps::ActorCaps {
            image_payload: true,
            ..Default::default()
        };
        world.set_caps(id, custom);
        assert_eq!(world.caps(id), custom);

        world.set_identity(id, "Ellipse");
        assert_eq!(world.caps(id), animatix_std::caps_for_type("Ellipse").unwrap());
        assert_eq!(world.actor_type(id), "Ellipse");
    }
}
