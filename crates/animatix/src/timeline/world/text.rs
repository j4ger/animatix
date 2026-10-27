//! Text tier accessors.

use super::{EntityId, World};
use crate::timeline::animation_track::TextTracks;

impl World {
    /// Text tier for an entity, when present.
    #[inline(always)]
    pub fn text(&self, id: EntityId) -> Option<&TextTracks> {
        self.text.get(id)
    }

    /// Mutable text tier for an entity, when present.
    #[inline(always)]
    pub fn text_mut(&mut self, id: EntityId) -> Option<&mut TextTracks> {
        self.text.get_mut(id)
    }

    /// Ensure an entity has a text tier and borrow it.
    #[inline(always)]
    pub fn ensure_text(&mut self, id: EntityId) -> &mut TextTracks {
        self.text.get_or_insert_with(id, TextTracks::default)
    }

    /// Text tier for an entity, falling back to default static tracks when absent.
    #[inline(always)]
    pub fn text_or_default(&self, id: EntityId) -> &TextTracks {
        self.text.get(id).unwrap_or(&self.default_text)
    }
}
