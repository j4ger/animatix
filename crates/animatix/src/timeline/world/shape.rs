//! Shape geometry tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::ShapeTracks;

impl World {
    /// Shape tier for an entity, when present.
    #[inline(always)]
    pub fn shape(&self, id: EntityId) -> Option<&ShapeTracks> {
        self.shape.get(id)
    }

    /// Mutable shape tier for an entity, when present.
    #[inline(always)]
    pub fn shape_mut(&mut self, id: EntityId) -> Option<&mut ShapeTracks> {
        self.shape.get_mut(id)
    }

    /// Ensure an entity has a shape tier and borrow it.
    #[inline(always)]
    pub fn ensure_shape(&mut self, id: EntityId) -> &mut ShapeTracks {
        self.shape.get_or_insert_with(id, ShapeTracks::default)
    }

    /// Shape tier for an entity, falling back to default static tracks when absent.
    #[inline(always)]
    pub fn shape_or_default(&self, id: EntityId) -> &ShapeTracks {
        self.shape.get(id).unwrap_or(&self.default_shape)
    }
}
