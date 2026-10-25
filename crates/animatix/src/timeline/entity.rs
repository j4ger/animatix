//! Strongly typed entity identifier and the entity store.
//!
//! Entities are zero-cost integer handles (`EntityId`) over an append-only
//! store (`EntityStore`). An entity's identity *is* its slot index: a label is
//! live exactly while its index is below `EntityStore::len()`, so there is no
//! dense element to keep in sync and no free list.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Strongly typed, zero-overhead entity handle representing a slot in `EntityStore`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EntityId(pub u32);

impl EntityId {
    /// Sentinel invalid entity id.
    pub const INVALID: EntityId = EntityId(u32::MAX);

    /// Convert entity ID to a `usize` index into dense component stores.
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

/// Append-only store of entity identities.
///
/// Owns the bidirectional `String <-> EntityId` lookup tables. All per-entity
/// data lives in `World`'s component stores, indexed by `EntityId`; the store
/// only answers "does this label/id exist" and hands out new ids. Slots are
/// never reused (`World::clear()` is for a whole rebuild only), so
/// `EntityId.index()` stays valid for the life of the timeline.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EntityStore {
    pub(crate) label_to_id: HashMap<String, EntityId>,
    pub(crate) id_to_label: Vec<String>,
}

impl EntityStore {
    /// Create a new, empty entity store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty store with preallocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            label_to_id: HashMap::with_capacity(capacity),
            id_to_label: Vec::with_capacity(capacity),
        }
    }

    /// Number of live entities.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.id_to_label.len()
    }

    /// True if the store contains no entities.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.id_to_label.is_empty()
    }

    /// Check if an entity with the given label exists.
    #[inline(always)]
    pub fn contains_key(&self, label: &str) -> bool {
        self.label_to_id.contains_key(label)
    }

    /// Resolve an actor label to its `EntityId`.
    #[inline(always)]
    pub fn get_id(&self, label: &str) -> Option<EntityId> {
        self.label_to_id.get(label).copied()
    }

    /// Resolve an `EntityId` to its actor label.
    #[inline(always)]
    pub fn get_label(&self, id: EntityId) -> Option<&str> {
        self.id_to_label.get(id.index()).map(|s| s.as_str())
    }

    /// Whether `id` refers to a live entity.
    #[inline(always)]
    pub fn exists(&self, id: EntityId) -> bool {
        id.index() < self.id_to_label.len()
    }

    /// Register a new entity under `label`, assigning the next slot.
    ///
    /// The caller is responsible for checking `contains_key` first: labels
    /// never collide because re-declarations reuse the existing entity.
    pub(crate) fn insert(&mut self, label: String) -> EntityId {
        let id = EntityId(self.id_to_label.len() as u32);
        self.id_to_label.push(label.clone());
        self.label_to_id.insert(label, id);
        id
    }

    /// Iterator over all entity labels, in slot order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &String> + ExactSizeIterator {
        self.id_to_label.iter()
    }

    /// Iterator over `(label, EntityId)` pairs, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (&String, EntityId)> {
        self.id_to_label
            .iter()
            .enumerate()
            .map(|(index, label)| (label, EntityId(index as u32)))
    }

    /// Clear all entities and lookup indices.
    pub fn clear(&mut self) {
        self.label_to_id.clear();
        self.id_to_label.clear();
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
    fn test_entity_store_crud() {
        let mut store = EntityStore::new();
        assert!(store.is_empty());

        let id_box = store.insert("box".to_string());
        assert_eq!(store.len(), 1);
        assert!(store.contains_key("box"));
        assert_eq!(id_box.index(), 0);
        assert_eq!(store.get_label(id_box), Some("box"));

        let id_text = store.insert("text".to_string());
        assert_eq!(store.len(), 2);
        assert_eq!(id_text.index(), 1);

        // Iteration
        let labels: Vec<&String> = store.keys().collect();
        assert_eq!(labels, vec!["box", "text"]);

        assert!(store.exists(id_box));
        assert!(!store.exists(EntityId(9)));
    }
}
