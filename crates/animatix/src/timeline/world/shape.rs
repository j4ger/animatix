//! Shape geometry tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::ShapeTracks;

impl World {
    /// Shape tier for an entity.
    #[inline(always)]
    pub fn shape(&self, id: EntityId) -> &ShapeTracks {
        self.shape.get(id)
    }

    /// Mutable shape tier for an entity.
    #[inline(always)]
    pub fn shape_mut(&mut self, id: EntityId) -> &mut ShapeTracks {
        self.shape.get_mut(id)
    }
}
