//! Strongly typed entity identifier and contiguous track storage.
//!
//! Replaces string-keyed map lookups with zero-cost integer handles (`EntityId`)
//! and contiguous vector storage (`TrackStore`).

use std::collections::HashMap;
use std::ops::{Index, IndexMut};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::dispatch::AnimationTrack;

/// Strongly typed, zero-overhead entity handle representing a slot in `TrackStore`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EntityId(pub u32);

impl EntityId {
    /// Sentinel invalid entity id.
    pub const INVALID: EntityId = EntityId(u32::MAX);

    /// Convert entity ID to a `usize` index into dense arrays.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Check if this entity ID is valid (i.e. not `INVALID`).
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

impl From<u32> for EntityId {
    #[inline(always)]
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<usize> for EntityId {
    #[inline(always)]
    fn from(val: usize) -> Self {
        Self(val as u32)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "e{}", self.0)
    }
}

/// Contiguous, cache-friendly storage for actor animation tracks.
///
/// Internally stores tracks in a flat `Vec<AnimationTrack>` indexed by `EntityId`,
/// while maintaining bidirectional `String <-> EntityId` lookup tables for backwards
/// compatibility with label-based API access.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TrackStore {
    pub(crate) dense: Vec<AnimationTrack>,
    pub(crate) label_to_id: HashMap<String, EntityId>,
    pub(crate) id_to_label: Vec<String>,
}

impl TrackStore {
    /// Create a new, empty track store.
    pub fn new() -> Self {
        Self {
            dense: Vec::new(),
            label_to_id: HashMap::new(),
            id_to_label: Vec::new(),
        }
    }

    /// Create an empty track store with preallocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            dense: Vec::with_capacity(capacity),
            label_to_id: HashMap::with_capacity(capacity),
            id_to_label: Vec::with_capacity(capacity),
        }
    }

    /// Number of tracks in the store.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.dense.len()
    }

    /// True if the store contains no tracks.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.dense.is_empty()
    }

    /// Check if a track with the given label exists.
    #[inline(always)]
    pub fn contains_key(&self, label: &str) -> bool {
        self.label_to_id.contains_key(label)
    }

    /// Resolve an actor label to its `EntityId`.
    #[inline(always)]
    pub fn get_id(&self, label: &str) -> Option<EntityId> {
        self.label_to_id.get(label).copied()
    }

    /// Alias for [`Self::get_id`].
    #[inline(always)]
    pub fn get_entity_id(&self, label: &str) -> Option<EntityId> {
        self.get_id(label)
    }

    /// Resolve an `EntityId` to its actor label.
    #[inline(always)]
    pub fn get_label(&self, id: EntityId) -> Option<&str> {
        self.id_to_label.get(id.index()).map(|s| s.as_str())
    }

    /// Look up a track by label.
    #[inline(always)]
    pub fn get(&self, label: &str) -> Option<&AnimationTrack> {
        self.label_to_id.get(label).and_then(|&id| self.dense.get(id.index()))
    }

    /// Look up a mutable track by label.
    #[inline(always)]
    pub fn get_mut(&mut self, label: &str) -> Option<&mut AnimationTrack> {
        let id = *self.label_to_id.get(label)?;
        self.dense.get_mut(id.index())
    }

    /// Look up a track directly by `EntityId` with zero hashing overhead.
    #[inline(always)]
    pub fn get_by_id(&self, id: EntityId) -> Option<&AnimationTrack> {
        self.dense.get(id.index())
    }

    /// Look up a mutable track directly by `EntityId` with zero hashing overhead.
    #[inline(always)]
    pub fn get_by_id_mut(&mut self, id: EntityId) -> Option<&mut AnimationTrack> {
        self.dense.get_mut(id.index())
    }

    /// Insert or update a track by label. Returns the previous track if one existed.
    pub fn insert(&mut self, label: String, mut track: AnimationTrack) -> Option<AnimationTrack> {
        if let Some(&id) = self.label_to_id.get(&label) {
            track.id = id;
            track.label = label;
            let old = std::mem::replace(&mut self.dense[id.index()], track);
            Some(old)
        } else {
            let id = EntityId(self.dense.len() as u32);
            track.id = id;
            track.label = label.clone();
            self.dense.push(track);
            self.id_to_label.push(label.clone());
            self.label_to_id.insert(label, id);
            None
        }
    }

    /// Internal helper to insert and return the assigned `EntityId`.
    pub(crate) fn insert_internal(&mut self, label: String, mut track: AnimationTrack) -> EntityId {
        if let Some(&id) = self.label_to_id.get(&label) {
            track.id = id;
            track.label = label;
            self.dense[id.index()] = track;
            id
        } else {
            let id = EntityId(self.dense.len() as u32);
            track.id = id;
            track.label = label.clone();
            self.dense.push(track);
            self.id_to_label.push(label.clone());
            self.label_to_id.insert(label, id);
            id
        }
    }

    /// Entry API for in-place insertion or modification.
    pub fn entry(&mut self, label: String) -> TrackEntry<'_> {
        if let Some(&id) = self.label_to_id.get(&label) {
            TrackEntry::Occupied(OccupiedTrackEntry {
                track: &mut self.dense[id.index()],
            })
        } else {
            TrackEntry::Vacant(VacantTrackEntry { store: self, label })
        }
    }

    /// Contiguous slice of all tracks.
    #[inline(always)]
    pub fn as_slice(&self) -> &[AnimationTrack] {
        &self.dense
    }

    /// Contiguous mutable slice of all tracks.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [AnimationTrack] {
        &mut self.dense
    }

    /// Iterator over references to all tracks in contiguous storage order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &AnimationTrack> + ExactSizeIterator {
        self.dense.iter()
    }

    /// Iterator over mutable references to all tracks.
    pub fn values_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = &mut AnimationTrack> + ExactSizeIterator {
        self.dense.iter_mut()
    }

    /// Iterator over all track labels.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator {
        self.id_to_label.iter()
    }

    /// Iterator over `(&label, &track)` pairs.
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&String, &AnimationTrack)> + ExactSizeIterator {
        self.id_to_label.iter().zip(self.dense.iter())
    }

    /// Iterator over `(&label, &mut track)` pairs.
    pub fn iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&String, &mut AnimationTrack)> + ExactSizeIterator {
        self.id_to_label.iter().zip(self.dense.iter_mut())
    }

    /// Clear all tracks and lookup indices.
    pub fn clear(&mut self) {
        self.dense.clear();
        self.label_to_id.clear();
        self.id_to_label.clear();
    }
}

/// In-place entry representation for `TrackStore::entry`.
pub enum TrackEntry<'a> {
    /// An entry that already exists.
    Occupied(OccupiedTrackEntry<'a>),
    /// An entry that is not yet present.
    Vacant(VacantTrackEntry<'a>),
}

/// Occupied entry in `TrackStore`.
pub struct OccupiedTrackEntry<'a> {
    track: &'a mut AnimationTrack,
}

impl<'a> OccupiedTrackEntry<'a> {
    /// Return a mutable reference to the occupied track.
    pub fn get_mut(&mut self) -> &mut AnimationTrack {
        self.track
    }

    /// Consume into the mutable track reference.
    pub fn into_mut(self) -> &'a mut AnimationTrack {
        self.track
    }
}

/// Vacant entry in `TrackStore`.
pub struct VacantTrackEntry<'a> {
    store: &'a mut TrackStore,
    label: String,
}

impl<'a> TrackEntry<'a> {
    /// Ensure a track exists by inserting the result of `default` if vacant.
    pub fn or_insert_with<F: FnOnce() -> AnimationTrack>(
        self,
        default: F,
    ) -> &'a mut AnimationTrack {
        match self {
            Self::Occupied(entry) => entry.track,
            Self::Vacant(entry) => {
                let track = default();
                let id = entry.store.insert_internal(entry.label, track);
                &mut entry.store.dense[id.index()]
            },
        }
    }
}

impl Index<EntityId> for TrackStore {
    type Output = AnimationTrack;

    #[inline(always)]
    fn index(&self, id: EntityId) -> &Self::Output {
        &self.dense[id.index()]
    }
}

impl IndexMut<EntityId> for TrackStore {
    #[inline(always)]
    fn index_mut(&mut self, id: EntityId) -> &mut Self::Output {
        &mut self.dense[id.index()]
    }
}

impl Index<&EntityId> for TrackStore {
    type Output = AnimationTrack;

    #[inline(always)]
    fn index(&self, id: &EntityId) -> &Self::Output {
        &self.dense[id.index()]
    }
}

impl IndexMut<&EntityId> for TrackStore {
    #[inline(always)]
    fn index_mut(&mut self, id: &EntityId) -> &mut Self::Output {
        &mut self.dense[id.index()]
    }
}

impl Index<&str> for TrackStore {
    type Output = AnimationTrack;

    #[inline]
    fn index(&self, label: &str) -> &Self::Output {
        self.get(label).unwrap_or_else(|| panic!("no track found with label '{label}'"))
    }
}

impl IndexMut<&str> for TrackStore {
    #[inline]
    fn index_mut(&mut self, label: &str) -> &mut Self::Output {
        let id = *self
            .label_to_id
            .get(label)
            .unwrap_or_else(|| panic!("no track found with label '{label}'"));
        &mut self.dense[id.index()]
    }
}

impl Index<&String> for TrackStore {
    type Output = AnimationTrack;

    #[inline]
    fn index(&self, label: &String) -> &Self::Output {
        &self[label.as_str()]
    }
}

impl IndexMut<&String> for TrackStore {
    #[inline]
    fn index_mut(&mut self, label: &String) -> &mut Self::Output {
        &mut self[label.as_str()]
    }
}

impl Index<usize> for TrackStore {
    type Output = AnimationTrack;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.dense[index]
    }
}

impl IndexMut<usize> for TrackStore {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.dense[index]
    }
}

impl<'a> IntoIterator for &'a TrackStore {
    type Item = (&'a String, &'a AnimationTrack);
    type IntoIter =
        std::iter::Zip<std::slice::Iter<'a, String>, std::slice::Iter<'a, AnimationTrack>>;

    fn into_iter(self) -> Self::IntoIter {
        self.id_to_label.iter().zip(self.dense.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_id_basics() {
        let id = EntityId(42);
        assert_eq!(id.index(), 42);
        assert!(id.is_valid());
        assert_eq!(format!("{id}"), "e42");

        let inv = EntityId::INVALID;
        assert!(!inv.is_valid());
    }

    #[test]
    fn test_track_store_crud() {
        let mut store = TrackStore::new();
        assert!(store.is_empty());

        let t1 = AnimationTrack::new("box".to_string(), "Rect");
        let t2 = AnimationTrack::new("text".to_string(), "Text");

        let prev = store.insert("box".to_string(), t1);
        assert!(prev.is_none());
        assert_eq!(store.len(), 1);
        assert!(store.contains_key("box"));

        let id_box = store.get_id("box").unwrap();
        assert_eq!(id_box.index(), 0);
        assert_eq!(store.get_label(id_box), Some("box"));
        assert_eq!(store[id_box].label, "box");
        assert_eq!(store["box"].label, "box");

        store.insert("text".to_string(), t2);
        assert_eq!(store.len(), 2);
        let id_text = store.get_id("text").unwrap();
        assert_eq!(id_text.index(), 1);

        // Iteration
        let labels: Vec<&String> = store.keys().collect();
        assert_eq!(labels, vec!["box", "text"]);

        // Mutation via index
        store[id_box].first_seen_ms = 0;
        assert_eq!(store["box"].first_seen_ms, 0);

        // Entry API
        store
            .entry("circle".to_string())
            .or_insert_with(|| AnimationTrack::new("circle".to_string(), "Circle"));
        assert_eq!(store.len(), 3);
        assert!(store.contains_key("circle"));
    }
}
