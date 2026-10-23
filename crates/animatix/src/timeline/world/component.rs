//! Typed component stores for the timeline ECS world.
//!
//! Two layouts, chosen by whether every entity owns the component:
//!
//! - [`RequiredStore<C>`] is a dense `Vec<C>`. `World::spawn` fills it with
//!   `C::default()`, so an existing entity is guaranteed to have a value.
//! - [`ComponentStore<C>`] is a `Vec<Option<C>>`. Only some entities carry the
//!   component (a `Filter` chain, a `Graph` coordinate frame, …).
//!
//! Both are aligned with `EntityStore` slots: `values[EntityId::index()]` is
//! that entity's component. Entities are append-only (see `docs/ecs.md` §3), so
//! an index never moves and the stores never need a free list or generation.

use super::super::entity::EntityId;

/// A component every entity owns, stored densely.
#[derive(Clone, Debug)]
pub struct RequiredStore<C> {
    values: Vec<C>,
}

impl<C: Default + Clone> Default for RequiredStore<C> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<C: Default + Clone> RequiredStore<C> {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entity slots in the store.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True when no entity slot has been allocated yet.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Borrow an entity's component.
    ///
    /// Panics when `id` is outside the store: that means the entity was
    /// created without going through `World::spawn`/`ensure_len`, which is a
    /// build bug rather than a caller error. Use [`Self::try_get`] where the
    /// entity may legitimately be absent (tests constructing a store by hand).
    #[inline(always)]
    pub fn get(&self, id: EntityId) -> &C {
        self.try_get(id).unwrap_or_else(|| {
            panic!(
                "required component missing for {id}: store has {} slots (entity was not spawned through World)",
                self.values.len()
            )
        })
    }

    /// Borrow an entity's component if the slot exists.
    #[inline(always)]
    pub fn try_get(&self, id: EntityId) -> Option<&C> {
        self.values.get(id.index())
    }

    /// Mutably borrow an entity's component.
    #[inline(always)]
    pub fn get_mut(&mut self, id: EntityId) -> &mut C {
        let len = self.values.len();
        self.try_get_mut(id).unwrap_or_else(|| {
            panic!(
                "required component missing for {id}: store has {len} slots (entity was not spawned through World)"
            )
        })
    }

    /// Mutably borrow an entity's component if the slot exists.
    #[inline(always)]
    pub fn try_get_mut(&mut self, id: EntityId) -> Option<&mut C> {
        self.values.get_mut(id.index())
    }

    /// Iterate over `(EntityId, &C)` in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &C)> + '_ {
        self.values
            .iter()
            .enumerate()
            .map(|(index, value)| (EntityId(index as u32), value))
    }

    /// Iterate over `(EntityId, &mut C)` in slot order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut C)> + '_ {
        self.values
            .iter_mut()
            .enumerate()
            .map(|(index, value)| (EntityId(index as u32), value))
    }

    /// Grow the store to `len` slots, filling new slots with `C::default()`.
    pub fn ensure_len(&mut self, len: usize) {
        if self.values.len() < len {
            self.values.resize_with(len, C::default);
        }
    }

    /// Reset one entity's component to its default.
    pub fn reset_at(&mut self, id: EntityId) {
        self.ensure_len(id.index() + 1);
        self.values[id.index()] = C::default();
    }

    /// Drop every slot.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

/// A component only some entities own.
#[derive(Clone, Debug)]
pub struct ComponentStore<C> {
    values: Vec<Option<C>>,
}

impl<C> Default for ComponentStore<C> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

impl<C> ComponentStore<C> {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entity slots in the store (present components included).
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// True when no entity slot has been allocated yet.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Number of entities that actually carry the component.
    #[inline(always)]
    pub fn present(&self) -> usize {
        self.values.iter().filter(|value| value.is_some()).count()
    }

    /// Borrow an entity's component.
    #[inline(always)]
    pub fn get(&self, id: EntityId) -> Option<&C> {
        self.values.get(id.index())?.as_ref()
    }

    /// Mutably borrow an entity's component.
    #[inline(always)]
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut C> {
        self.values.get_mut(id.index())?.as_mut()
    }

    /// Mutably borrow an entity's component, inserting `default()` if absent.
    pub fn get_or_insert_with<F: FnOnce() -> C>(&mut self, id: EntityId, default: F) -> &mut C {
        let index = id.index();
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        self.values[index].get_or_insert_with(default)
    }

    /// Insert (or replace) an entity's component, returning the previous value.
    pub fn insert(&mut self, id: EntityId, value: C) -> Option<C> {
        let index = id.index();
        if self.values.len() <= index {
            self.values.resize_with(index + 1, || None);
        }
        self.values[index].replace(value)
    }

    /// Remove an entity's component, returning it.
    pub fn remove(&mut self, id: EntityId) -> Option<C> {
        self.values.get_mut(id.index())?.take()
    }

    /// Whether the entity carries the component.
    #[inline(always)]
    pub fn contains(&self, id: EntityId) -> bool {
        self.get(id).is_some()
    }

    /// Iterate over `(EntityId, &C)` for present components, in slot order.
    pub fn iter(&self) -> impl Iterator<Item = (EntityId, &C)> + '_ {
        self.values.iter().enumerate().filter_map(|(index, value)| {
            value.as_ref().map(|value| (EntityId(index as u32), value))
        })
    }

    /// Iterate over `(EntityId, &mut C)` for present components.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (EntityId, &mut C)> + '_ {
        self.values.iter_mut().enumerate().filter_map(|(index, value)| {
            value.as_mut().map(|value| (EntityId(index as u32), value))
        })
    }

    /// Grow the store to `len` empty slots.
    pub fn ensure_len(&mut self, len: usize) {
        if self.values.len() < len {
            self.values.resize_with(len, || None);
        }
    }

    /// Clear one entity's component, keeping the slots aligned.
    pub fn reset_at(&mut self, id: EntityId) {
        self.ensure_len(id.index() + 1);
        self.values[id.index()] = None;
    }

    /// Drop every slot.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct Health(u32);

    #[test]
    fn required_store_fills_on_grow_and_keeps_slots_aligned() {
        let mut store = RequiredStore::<Health>::new();
        store.ensure_len(3);
        assert_eq!(store.len(), 3);
        assert_eq!(store.get(EntityId(1)), &Health(0));

        store.get_mut(EntityId(1)).0 = 7;
        assert_eq!(store.try_get(EntityId(1)), Some(&Health(7)));
        assert!(store.try_get(EntityId(3)).is_none());

        store.reset_at(EntityId(1));
        assert_eq!(store.get(EntityId(1)), &Health(0));
    }

    #[test]
    fn component_store_insert_remove_and_iterate() {
        let mut store = ComponentStore::<String>::new();
        assert!(store.is_empty());
        assert_eq!(store.insert(EntityId(2), "two".to_string()), None);
        assert_eq!(store.len(), 3);
        assert_eq!(store.present(), 1);
        assert_eq!(store.get(EntityId(2)).map(String::as_str), Some("two"));
        assert!(!store.contains(EntityId(0)));

        let values: Vec<_> = store.iter().map(|(id, value)| (id.0, value.clone())).collect();
        assert_eq!(values, vec![(2, "two".to_string())]);

        assert_eq!(store.insert(EntityId(2), "again".to_string()), Some("two".to_string()));
        assert_eq!(store.remove(EntityId(2)), Some("again".to_string()));
        assert_eq!(store.present(), 0);
    }

    #[test]
    fn component_store_reset_keeps_alignment() {
        let mut store = ComponentStore::<u8>::new();
        store.insert(EntityId(4), 9);
        store.reset_at(EntityId(4));
        assert_eq!(store.len(), 5);
        assert!(!store.contains(EntityId(4)));
        assert_eq!(store.present(), 0);
    }

    #[test]
    fn required_store_clone_is_independent() {
        let mut store = RequiredStore::<Health>::new();
        store.ensure_len(1);
        store.get_mut(EntityId(0)).0 = 3;
        let mut copy = store.clone();
        copy.get_mut(EntityId(0)).0 = 9;
        assert_eq!(store.get(EntityId(0)), &Health(3));
    }
}
