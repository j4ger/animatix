//! `World` construction and scene-graph wiring: spawn, hierarchy, and store reset.

use crate::timeline::components::AutoColorComponent;
use crate::timeline::entity::EntityId;
use crate::timeline::world::World;

impl World {
    /// Create an entity, or return the existing one; re-declaring a label keeps
    /// the same entity and its current data.
    pub fn spawn(&mut self, label: &str, actor_type: impl Into<String>) -> EntityId {
        let actor_type = actor_type.into();
        if let Some(id) = self.tracks.get_id(label) {
            // The entity may predate the stores (tests); keep them aligned first.
            self.ensure_len();
            self.attach_pending_auto_color(id, label);
            return id;
        }
        let id = self.tracks.insert(label.to_string());
        self.apply_identity(id, &actor_type);
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// Replace an entity's data, resetting its optional/required components.
    /// The label keeps its entity id but every component starts fresh; identity
    /// is re-derived from `actor_type` and optional components stay cleared
    /// (`docs/ecs.md` §2).
    pub fn spawn_replacing(&mut self, label: &str, actor_type: &str) -> EntityId {
        let replaced = self.tracks.contains_key(label);
        let id = self.spawn(label, actor_type);
        if replaced {
            self.unlink(id);
        }
        self.ensure_len();
        for_each_store!(reset_stores_at, self, id);
        self.apply_identity(id, actor_type);
        // `apply_identity` seeds the optional tiers a fresh `spawn` would;
        // a replacement clears them, so undo those three seeds.
        self.highlight.reset_at(id);
        self.shape.reset_at(id);
        self.text.reset_at(id);
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// Record the scene-graph relation for a declaration, called by `add_node`
    /// before the child's track exists; an existing child links immediately,
    /// otherwise the link is buffered and adopted by [`Self::spawn`].
    pub fn set_parent_label(&mut self, child_label: &str, parent_label: Option<&str>) {
        // Entities may not exist yet; grow the stores so the link can write
        // both endpoints.
        self.ensure_len();
        let child_id = self.tracks.get_id(child_label);
        match (child_id, parent_label) {
            (Some(child), Some(parent)) => {
                let Some(parent_id) = self.tracks.get_id(parent) else {
                    tracing::warn!(
                        "child '{child_label}' registered before parent '{parent}' has a track; \
                         skipping hierarchy link"
                    );
                    return;
                };
                self.link(parent_id, child);
                self.pending_parents.remove(child_label);
                self.pending_roots.retain(|label| label != child_label);
            },
            (Some(child), None) => {
                // A root-level re-declaration of a nested actor keeps it nested:
                // `add_node(None)` only records a *new* root, it does not
                // re-parent. Use `link`/`unlink` to move an entity.
                if self.hierarchy(child).parent.is_none() && !self.roots.contains(&child) {
                    self.roots.push(child);
                }
                self.pending_parents.remove(child_label);
                self.pending_roots.retain(|label| label != child_label);
            },
            (None, Some(parent)) => {
                let Some(parent_id) = self.tracks.get_id(parent) else {
                    tracing::warn!(
                        "child '{child_label}' registered before parent '{parent}' has a track; \
                         skipping hierarchy link"
                    );
                    return;
                };
                self.pending_parents.insert(child_label.to_string(), parent_id);
                self.pending_roots.retain(|label| label != child_label);
            },
            (None, None) => {
                self.pending_parents.remove(child_label);
                if !self.pending_roots.iter().any(|label| label == child_label) {
                    self.pending_roots.push(child_label.to_string());
                }
            },
        }
    }

    /// Link `child` under `parent`, detaching it from any previous parent/root.
    pub fn link(&mut self, parent: EntityId, child: EntityId) {
        self.ensure_len();
        if parent == child {
            tracing::warn!("ignoring hierarchy cycle: '{:?}' cannot parent itself", child);
            return;
        }
        self.unlink(child);
        self.hierarchy_mut(parent).push_child(child);
        self.hierarchy_mut(child).parent = Some(parent);
    }

    /// Detach `child` from its parent and from the root set.
    pub fn unlink(&mut self, child: EntityId) {
        self.ensure_len();
        if let Some(parent) = self.hierarchy(child).parent {
            self.hierarchy_mut(parent).remove_child(child);
        }
        self.hierarchy_mut(child).parent = None;
        self.roots.retain(|id| *id != child);
    }

    /// Adopt a freshly created entity into the hierarchy.
    fn adopt(&mut self, id: EntityId, label: &str) {
        if let Some(parent) = self.pending_parents.remove(label) {
            self.link(parent, id);
            return;
        }
        if let Some(index) = self.pending_roots.iter().position(|pending| pending == label) {
            self.pending_roots.remove(index);
        }
        if self.hierarchy(id).parent.is_none() && !self.roots.contains(&id) {
            self.roots.push(id);
        }
    }

    /// Attach an auto-color slot resolved before the entity existed: `color:
    /// auto` is evaluated before the track is created, so the slot waits in
    /// [`Self::pending_auto_color`].
    fn attach_pending_auto_color(&mut self, id: EntityId, label: &str) {
        let matches = self
            .pending_auto_color
            .as_ref()
            .is_some_and(|(pending_label, _)| pending_label == label);
        if !matches {
            return;
        }
        let (_, slot) = self.pending_auto_color.take().expect("checked above");
        self.auto_color.insert(id, AutoColorComponent { slot });
    }

    /// Resolve buffered hierarchy labels after a build pass; a pending label
    /// with no entity is dropped with a warning.
    pub fn finish_hierarchy(&mut self) {
        self.ensure_len();
        let pending_parents = std::mem::take(&mut self.pending_parents);
        for (child_label, parent) in pending_parents {
            if let Some(child) = self.tracks.get_id(&child_label) {
                self.link(parent, child);
            } else {
                tracing::warn!("hierarchy link for '{child_label}' dropped: no track was created");
            }
        }
        let pending_roots = std::mem::take(&mut self.pending_roots);
        for label in pending_roots {
            if let Some(id) = self.tracks.get_id(&label) {
                if self.hierarchy(id).parent.is_none() && !self.roots.contains(&id) {
                    self.roots.push(id);
                }
            } else {
                tracing::warn!("root '{label}' dropped: no track was created");
            }
        }
    }

    /// Grow every component store to the entity count.
    pub(crate) fn ensure_len(&mut self) {
        let len = self.tracks.len();
        for_each_store!(ensure_store_len, self, len);
    }

    /// Drop every entity and component.
    pub fn clear(&mut self) {
        self.tracks.clear();
        for_each_store!(clear_stores, self);
        self.roots.clear();
        self.pending_parents.clear();
        self.pending_auto_color = None;
        self.pending_roots.clear();
    }
}
