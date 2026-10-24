//! Tagged-union property tracks (`legend`, extension tags, …).

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_track::PropertyTrack;

/// Tagged-union property tracks keyed by canonical property name.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TaggedTracksComponent(
    /// Canonical name → optional typed track.
    pub HashMap<String, Option<PropertyTrack<PropertyValue>>>,
);

impl Deref for TaggedTracksComponent {
    type Target = HashMap<String, Option<PropertyTrack<PropertyValue>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TaggedTracksComponent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

use super::{EntityId, World};

impl World {
    /// Tagged-union tracks for an entity.
    #[inline(always)]
    pub fn tagged_tracks(&self, id: EntityId) -> &TaggedTracksComponent {
        self.tagged_tracks.get(id)
    }

    /// Mutable tagged-union tracks for an entity.
    #[inline(always)]
    pub fn tagged_tracks_mut(&mut self, id: EntityId) -> &mut TaggedTracksComponent {
        self.tagged_tracks.get_mut(id)
    }
}
