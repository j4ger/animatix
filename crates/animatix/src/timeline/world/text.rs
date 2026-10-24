//! Text tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::TextTracks;

impl World {
    /// Text tier for an entity.
    #[inline(always)]
    pub fn text(&self, id: EntityId) -> &TextTracks {
        self.text.get(id)
    }

    /// Mutable text tier for an entity.
    #[inline(always)]
    pub fn text_mut(&mut self, id: EntityId) -> &mut TextTracks {
        self.text.get_mut(id)
    }
}
