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
