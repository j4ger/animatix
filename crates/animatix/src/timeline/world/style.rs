//! Style tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::StyleTracks;

impl World {
    /// Style tier for an entity.
    #[inline(always)]
    pub fn style(&self, id: EntityId) -> &StyleTracks {
        self.style.get(id)
    }

    /// Mutable style tier for an entity.
    #[inline(always)]
    pub fn style_mut(&mut self, id: EntityId) -> &mut StyleTracks {
        self.style.get_mut(id)
    }
}
