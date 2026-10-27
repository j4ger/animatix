//! Scene-graph hierarchy component.
//!
//! Replaces the duplicated `children`/`children_ids` and `parent`/`parent_id`
//! fields on the old monolithic track. `children` is the single authoritative
//! order; `parent` is a back-reference kept in sync by `World`.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::super::entity::EntityId;

/// Parent/children links for one entity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SceneHierarchyComponent {
    /// Parent entity, or `None` for a root.
    pub parent: Option<EntityId>,
    /// Children in authored/insertion order.
    pub children: Vec<EntityId>,
}

impl SceneHierarchyComponent {
    /// True when this entity has no parent.
    #[inline(always)]
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    /// Append a child if it is not already present.
    pub fn push_child(&mut self, child: EntityId) {
        if !self.children.contains(&child) {
            self.children.push(child);
        }
    }

    /// Remove a child, preserving the order of the rest.
    pub fn remove_child(&mut self, child: EntityId) {
        self.children.retain(|existing| *existing != child);
    }
}

impl super::World {
    /// Borrow an entity's hierarchy component.
    #[inline(always)]
    pub fn hierarchy(&self, id: EntityId) -> &SceneHierarchyComponent {
        self.hierarchy.get(id)
    }

    /// Mutably borrow an entity's hierarchy component.
    #[inline(always)]
    pub fn hierarchy_mut(&mut self, id: EntityId) -> &mut SceneHierarchyComponent {
        self.hierarchy.get_mut(id)
    }
}
