//! Geometry tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::GeometryTracks;

impl World {
    /// Geometry tier for an entity.
    #[inline(always)]
    pub fn geometry(&self, id: EntityId) -> &GeometryTracks {
        self.geometry.get(id)
    }

    /// Mutable geometry tier for an entity.
    #[inline(always)]
    pub fn geometry_mut(&mut self, id: EntityId) -> &mut GeometryTracks {
        self.geometry.get_mut(id)
    }
}

impl World {
    /// Dense precise-bounds slot for an entity (`u32::MAX` until stamped).
    #[inline(always)]
    pub fn bounds_slot(&self, id: EntityId) -> u32 {
        self.bounds_slots.get(id).get()
    }
}

impl World {
    /// Stamp the dense bounds slot for an entity.
    pub(crate) fn set_bounds_slot(&self, id: EntityId, slot: u32) {
        self.bounds_slots.get(id).set(slot);
    }
}
