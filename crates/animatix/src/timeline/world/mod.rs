//! The timeline ECS world: entities plus typed component stores.
//!
//! `World` owns the dense `EntityStore` (still `TrackStore` while the
//! migration is in flight) and every component store. Entity ids are stable
//! and append-only; labels are converted to ids at the boundary. See
//! `docs/ecs.md` for the normative design.
//!
//! During the migration, `World::tracks` still holds the monolithic
//! `AnimationTrack`. A domain moves to a component store only when its field is
//! deleted from `AnimationTrack` in the same change, so there is never a second
//! source of truth across a merge boundary.

mod component;
mod fields;
mod hierarchy;
mod meta;
mod view;

pub use component::{ComponentStore, RequiredStore};
pub use fields::{TrackFields, TrackFieldsMut};
pub use hierarchy::SceneHierarchyComponent;
pub use meta::EntityMeta;
pub use view::{EntityMut, EntityRef};

use std::collections::HashMap;

use super::components::{
    AutoColorComponent, ChildOrderComponent, CoordinateFrameComponent, PersistenceComponent,
    TranscriptComponent,
};
use super::dispatch::AnimationTrack;
use super::effects::EffectChainTrack;
use super::entity::{EntityId, TrackStore};
use super::legend::LegendTracks;
use super::types::ContainerMetadata;
use crate::primitives::PrimitivePayload;

/// Entities and component stores for one scene timeline.
#[derive(Clone, Debug, Default)]
pub struct World {
    /// Dense entity storage (transitional: the header and not-yet-migrated
    /// component tiers still live on `AnimationTrack`).
    pub(crate) tracks: TrackStore,
    /// Parent/children links, aligned with `tracks`.
    pub(crate) hierarchy: RequiredStore<SceneHierarchyComponent>,
    /// Per-entity visibility/lock/camera metadata.
    pub(crate) meta: RequiredStore<EntityMeta>,
    /// Layout configuration for `Row`/`Col`/`Grid`/`Stack` actors.
    ///
    /// Presence means the entity is a layout container. The authored child
    /// order lives on [`ContainerMetadata::child_order`] as entity ids;
    /// animated child-order tracks are the separate
    /// [`Self::child_order`] store.
    pub(crate) container_layout: ComponentStore<ContainerMetadata>,
    /// Animated child order for containers targeted by `swap` / `reorder`.
    pub(crate) child_order: ComponentStore<ChildOrderComponent>,
    /// Mathematical coordinate frame for plot hosts (`Graph` and any primitive
    /// with the `plot_host` capability).
    ///
    /// This is the sole source for a hosted child's coordinate resolution. The
    /// `{label}_x_domain` / `_y_domain` / `_padding` env entries are derived
    /// build-time expression variables, never a fallback.
    pub(crate) coordinate_frame: ComponentStore<CoordinateFrameComponent>,
    /// `persist` / `remove` state for actors that were targeted by an action.
    pub(crate) persistence: ComponentStore<PersistenceComponent>,
    /// `color: auto` slot for actors declared with automatic coloring.
    pub(crate) auto_color: ComponentStore<AutoColorComponent>,
    /// Parsed speech transcript for `Audio` / `Captions` actors.
    pub(crate) transcript: ComponentStore<TranscriptComponent>,
    /// Legend data for actors that participate in a legend.
    ///
    /// A `Legend` host carries its presentation settings and the entries the
    /// post-build scene scan generated; an ordinary actor carries only the
    /// color that makes it an automatic candidate. Presence is optional: an
    /// actor with no legend involvement has no component.
    pub(crate) legend: ComponentStore<LegendTracks>,
    /// Source span of the actor's declaration, for editor navigation.
    pub(crate) decl_span: ComponentStore<animatix_syntax::ast::Span>,
    /// Post-processing effect chain for a compositing scope.
    pub(crate) effects: ComponentStore<EffectChainTrack>,
    /// Primitive-specific payload (plots, SVG assets, raster images).
    pub(crate) payload: ComponentStore<PrimitivePayload>,
    /// Root entities in authored/insertion order.
    pub(crate) roots: Vec<EntityId>,
    /// Build-time command buffer: a child label whose track has not been
    /// created yet, mapped to its already-existing parent entity.
    ///
    /// `add_node` runs before the primitive creates the child's track, so the
    /// link cannot be written as an id yet. The pending entry is adopted by
    /// `spawn_with_track` the moment the entity exists. This is build state,
    /// not runtime storage: it is empty after a completed build.
    pub(crate) pending_parents: HashMap<String, EntityId>,
    /// Build-time command buffer: an auto-color slot assigned while the
    /// declaration's properties were evaluated, before its entity existed.
    ///
    /// `color: auto` is resolved during property evaluation, but the primitive
    /// creates the track only afterwards (and build code between the two
    /// deliberately observes the entity as *not yet spawned*, e.g.
    /// `is_first_decl`). The slot is attached by `spawn_with_track`/`spawn`
    /// when the entity appears. Empty after a completed build; not runtime
    /// storage.
    pub(crate) pending_auto_color: Option<(String, usize)>,
    /// Build-time root order for labels whose entity does not exist yet.
    pub(crate) pending_roots: Vec<String>,
}

impl World {
    /// Create an empty world.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of entities.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// True when the world has no entities.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Resolve a label to its entity.
    #[inline(always)]
    pub fn get_id(&self, label: &str) -> Option<EntityId> {
        self.tracks.get_id(label)
    }

    /// Resolve an entity to its label.
    #[inline(always)]
    pub fn label(&self, id: EntityId) -> Option<&str> {
        self.tracks.get_label(id)
    }

    /// Borrow the entity's (transitional) track.
    #[inline(always)]
    pub fn track(&self, id: EntityId) -> Option<&AnimationTrack> {
        self.tracks.get_by_id(id)
    }

    /// Mutably borrow the entity's (transitional) track.
    #[inline(always)]
    pub fn track_mut(&mut self, id: EntityId) -> Option<&mut AnimationTrack> {
        self.tracks.get_by_id_mut(id)
    }

    /// Split borrow of the transitional track and the legend component.
    ///
    /// GUI property edits write both in one call; a single `EntityMut` cannot
    /// hand out two stores at once, so this keeps the split explicit.
    pub fn track_and_legend_mut(
        &mut self,
        id: EntityId,
    ) -> Option<(&mut AnimationTrack, &mut LegendTracks)> {
        let track = self.tracks.get_by_id_mut(id)?;
        let legend = self.legend.get_or_insert_with(id, LegendTracks::default);
        Some((track, legend))
    }

    /// Split borrow of the track, effect chain, and legend component.
    pub fn track_effects_and_legend_mut(
        &mut self,
        id: EntityId,
    ) -> Option<(&mut AnimationTrack, &mut EffectChainTrack, &mut LegendTracks)> {
        let track = self.tracks.get_by_id_mut(id)?;
        let effects = self.effects.get_or_insert_with(id, EffectChainTrack::default);
        let legend = self.legend.get_or_insert_with(id, LegendTracks::default);
        Some((track, effects, legend))
    }

    /// A read-only entity view.
    #[inline(always)]
    pub fn view(&self, id: EntityId) -> Option<EntityRef<'_>> {
        self.tracks.get_by_id(id)?;
        Some(EntityRef::new(self, id))
    }

    /// A read-only entity view resolved by label.
    #[inline(always)]
    pub fn entity(&self, label: &str) -> Option<EntityRef<'_>> {
        let id = self.tracks.get_id(label)?;
        self.view(id)
    }

    /// A mutable entity view.
    #[inline(always)]
    pub fn view_mut(&mut self, id: EntityId) -> Option<EntityMut<'_>> {
        self.tracks.get_by_id(id)?;
        Some(EntityMut::new(self, id))
    }

    /// A mutable entity view resolved by label.
    #[inline(always)]
    pub fn entity_mut(&mut self, label: &str) -> Option<EntityMut<'_>> {
        let id = self.tracks.get_id(label)?;
        self.view_mut(id)
    }

    /// Iterate every entity as a view, in slot order.
    pub fn views(&self) -> impl Iterator<Item = EntityRef<'_>> {
        (0..self.tracks.len()).filter_map(|index| self.view(EntityId(index as u32)))
    }

    /// Create an entity with a blank track, or return the existing one.
    ///
    /// Mirrors the build path's `tracks.entry(label).or_insert_with(...)`:
    /// re-declaring a label keeps the same entity and its current data.
    pub fn spawn(&mut self, label: &str, actor_type: impl Into<String>) -> EntityId {
        if let Some(id) = self.tracks.get_id(label) {
            // The track may have been created by a legacy direct
            // `TrackStore` insert (extension builds, tests); keep the
            // component stores aligned before anyone borrows them.
            self.ensure_len();
            self.attach_pending_auto_color(id, label);
            return id;
        }
        self.spawn_with_track(label, AnimationTrack::new(label.to_string(), actor_type))
    }

    /// Create an entity from a fully built track, or return the existing one.
    pub fn spawn_with_track(&mut self, label: &str, track: AnimationTrack) -> EntityId {
        if let Some(id) = self.tracks.get_id(label) {
            self.ensure_len();
            self.attach_pending_auto_color(id, label);
            return id;
        }
        let id = self.tracks.insert_internal(label.to_string(), track);
        self.ensure_len();
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// Ensure an entity exists and return its track for build-time writes.
    ///
    /// The build equivalent of `tracks.entry(label).or_insert_with(...)`, but
    /// routed through `World` so the entity is adopted into the ECS hierarchy
    /// (pending parent/root links resolve here). Kept on `World`, not
    /// `Timeline`, so callers can still borrow sibling timeline fields (asset
    /// cache, defaults) while writing the track.
    pub fn ensure_track(&mut self, label: &str, actor_type: &str) -> &mut AnimationTrack {
        let id = self.spawn(label, actor_type);
        self.track_mut(id).expect("World::spawn always creates a track")
    }

    /// Replace an entity's track, resetting its optional/required components.
    ///
    /// This is the carry-injection path's old `TrackStore::insert` semantics:
    /// the label keeps its entity id, but every component starts from the new
    /// snapshot rather than inheriting the previous scene's state.
    pub fn spawn_replacing(&mut self, label: &str, track: AnimationTrack) -> EntityId {
        let replaced = self.tracks.insert(label.to_string(), track);
        let id = self.tracks.get_id(label).expect("TrackStore::insert must register the label");
        if replaced.is_some() {
            self.unlink(id);
        }
        self.ensure_len();
        self.hierarchy.reset_at(id);
        self.meta.reset_at(id);
        self.container_layout.reset_at(id);
        self.child_order.reset_at(id);
        self.coordinate_frame.reset_at(id);
        self.persistence.reset_at(id);
        self.auto_color.reset_at(id);
        self.transcript.reset_at(id);
        self.effects.reset_at(id);
        self.legend.reset_at(id);
        self.decl_span.reset_at(id);
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// Borrow an entity's metadata.
    #[inline(always)]
    pub fn meta(&self, id: EntityId) -> &EntityMeta {
        self.meta.get(id)
    }

    /// Mutably borrow an entity's metadata.
    #[inline(always)]
    pub fn meta_mut(&mut self, id: EntityId) -> &mut EntityMeta {
        self.meta.get_mut(id)
    }

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

    /// Borrow an entity's coordinate frame, when it is a plot host.
    #[inline(always)]
    pub fn coordinate_frame(&self, id: EntityId) -> Option<&CoordinateFrameComponent> {
        self.coordinate_frame.get(id)
    }

    /// Source span of the entity's declaration, when the parser recorded one.
    #[inline(always)]
    pub fn decl_span(&self, id: EntityId) -> Option<&animatix_syntax::ast::Span> {
        self.decl_span.get(id)
    }

    /// Set the declaration span for an entity.
    #[inline(always)]
    pub fn set_decl_span(&mut self, id: EntityId, span: animatix_syntax::ast::Span) {
        self.decl_span.get_or_insert_with(id, || span);
    }

    /// Borrow an entity's legend data, when it participates in a legend.
    #[inline(always)]
    pub fn legend(&self, id: EntityId) -> Option<&LegendTracks> {
        self.legend.get(id)
    }

    /// Mutably borrow an entity's legend data, when present.
    #[inline(always)]
    pub fn legend_mut(&mut self, id: EntityId) -> Option<&mut LegendTracks> {
        self.legend.get_mut(id)
    }

    /// Borrow an entity's effect chain, when present.
    #[inline(always)]
    pub fn effects(&self, id: EntityId) -> Option<&EffectChainTrack> {
        self.effects.get(id)
    }

    /// Mutably borrow an entity's effect chain.
    #[inline(always)]
    pub fn effects_mut(&mut self, id: EntityId) -> Option<&mut EffectChainTrack> {
        self.effects.get_mut(id)
    }

    /// Borrow an entity's primitive payload, when present.
    #[inline(always)]
    pub fn payload(&self, id: EntityId) -> Option<&PrimitivePayload> {
        self.payload.get(id)
    }

    /// Mutably borrow an entity's primitive payload.
    #[inline(always)]
    pub fn payload_mut(&mut self, id: EntityId) -> Option<&mut PrimitivePayload> {
        self.payload.get_mut(id)
    }

    /// Insert a default payload when absent and borrow it.
    pub fn ensure_payload(&mut self, id: EntityId) -> &mut PrimitivePayload {
        self.payload.get_or_insert_with(id, PrimitivePayload::default)
    }

    /// Insert a default effect chain when absent and borrow it.
    pub fn ensure_effects(&mut self, id: EntityId) -> &mut EffectChainTrack {
        self.effects.get_or_insert_with(id, EffectChainTrack::default)
    }

    /// Ensure an entity has legend data and return it for a build/render write.
    ///
    /// Inserts `LegendTracks::default()` when absent. Callers that hold the
    /// frame cache (the `Timeline` entry points) must invalidate it; this
    /// `World` method is the build-path form with no cache to invalidate.
    pub fn ensure_legend(&mut self, id: EntityId) -> &mut LegendTracks {
        if self.legend.get(id).is_none() {
            self.legend.insert(id, LegendTracks::default());
        }
        self.legend
            .get_mut(id)
            .expect("World::ensure_legend just inserted the component")
    }

    /// Inputs of the child-order cache flag: true when any container carries an
    /// animated child-order track.
    ///
    /// Stops at the first present component; the frame cache compares this flag
    /// so a scene that never authors `swap`/`reorder` pays one short scan.
    #[inline]
    pub fn has_child_orders(&self) -> bool {
        self.child_order.iter().next().is_some()
    }

    /// Root entities, in authored/insertion order.
    #[inline(always)]
    pub fn root_entities(&self) -> &[EntityId] {
        &self.roots
    }

    /// True when `id` is in the root set.
    #[inline(always)]
    pub fn is_root(&self, id: EntityId) -> bool {
        self.roots.contains(&id)
    }

    /// Labels of `id`'s direct children, in authored/insertion order.
    ///
    /// This is the label boundary for label-based APIs (build-time diagnostics,
    /// action target resolution, the GUI); frame-path consumers iterate
    /// [`SceneHierarchyComponent::children`] directly and never allocate.
    pub fn child_labels(&self, id: EntityId) -> impl Iterator<Item = &str> {
        self.hierarchy(id).children.iter().filter_map(|child| self.label(*child))
    }

    /// Record the scene-graph relation for a declaration.
    ///
    /// Called by `add_node` before the primitive has created the child's
    /// track. When the child entity already exists the link is written
    /// immediately; otherwise it is buffered and adopted by
    /// [`Self::spawn_with_track`]. A missing parent is a build bug and keeps
    /// the old path's `warn!` instead of silently orphaning the child.
    pub fn set_parent_label(&mut self, child_label: &str, parent_label: Option<&str>) {
        // Legacy direct `TrackStore` inserts bypass `spawn_with_track`; grow the
        // component stores so the link below can write both endpoints.
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
                // A root-level re-declaration of an already-nested actor keeps
                // it nested: `add_node(None)` only records a *new* root, it does
                // not re-parent an existing child (the pre-ECS `root_nodes`
                // behaviour). Use `link`/`unlink` to actually move an entity.
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

    /// Attach an auto-color slot that was resolved before the entity existed.
    ///
    /// `color: auto` is evaluated in the declaration's property loop, which
    /// runs before the primitive creates the track; the build deliberately
    /// still sees the entity as absent there (`is_first_decl`). The slot waits
    /// in [`Self::pending_auto_color`] and lands here the moment the entity is
    /// spawned.
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

    /// Resolve buffered hierarchy labels after a build pass.
    ///
    /// Handles entities created by direct `TrackStore` insertion (carry
    /// injection, tests) that bypassed [`Self::spawn_with_track`]. A pending
    /// label with no entity is dropped with a warning: the label never became a
    /// track, so there is nothing to link.
    pub fn finish_hierarchy(&mut self) {
        // Legacy direct `TrackStore` inserts bypassed `spawn_with_track`, so
        // the component stores may be short even though entities exist.
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
    ///
    /// Required stores are filled with defaults; optional stores get empty
    /// slots. Called after every entity insert.
    pub(crate) fn ensure_len(&mut self) {
        let len = self.tracks.len();
        self.hierarchy.ensure_len(len);
        self.meta.ensure_len(len);
        self.container_layout.ensure_len(len);
        self.child_order.ensure_len(len);
        self.coordinate_frame.ensure_len(len);
        self.persistence.ensure_len(len);
        self.auto_color.ensure_len(len);
        self.transcript.ensure_len(len);
        self.legend.ensure_len(len);
        self.decl_span.ensure_len(len);
    }

    /// Drop every entity and component.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.hierarchy.clear();
        self.meta.clear();
        self.container_layout.clear();
        self.child_order.clear();
        self.coordinate_frame.clear();
        self.persistence.clear();
        self.auto_color.clear();
        self.transcript.clear();
        self.legend.clear();
        self.decl_span.clear();
        self.roots.clear();
        self.pending_parents.clear();
        self.pending_auto_color = None;
        self.pending_roots.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_returns_stable_ids_and_keeps_stores_aligned() {
        let mut world = World::new();
        let a = world.spawn("a", "Rect");
        let b = world.spawn("b", "Rect");
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
        assert_eq!(world.len(), 2);
        assert_eq!(world.hierarchy.len(), 2);
        assert_eq!(world.meta.len(), 2, "metadata stays slot-aligned");
        assert_eq!(world.container_layout.len(), 2);
        assert_eq!(world.coordinate_frame.len(), 2, "every store stays slot-aligned");
        assert_eq!(world.label(a), Some("a"));
        assert_eq!(world.get_id("b"), Some(b));

        // Re-spawning a label keeps the entity (build's `entry` semantics).
        assert_eq!(world.spawn("a", "Ellipse"), a);
        assert_eq!(world.len(), 2);
        assert_eq!(world.track(a).unwrap().actor_type, "Rect");
    }

    #[test]
    fn spawn_replacing_keeps_the_id_and_resets_hierarchy() {
        let mut world = World::new();
        let parent = world.spawn("parent", "Rect");
        let id = world.spawn("box", "Rect");
        world.link(parent, id);
        assert_eq!(world.hierarchy(id).parent, Some(parent));

        world.spawn_replacing("box", AnimationTrack::new("box".to_string(), "Ellipse"));
        assert_eq!(world.get_id("box"), Some(id));
        assert_eq!(world.hierarchy(id), &SceneHierarchyComponent::default());
        assert_eq!(world.track(id).unwrap().actor_type, "Ellipse");
        // The old parent no longer lists the replaced entity, which is a root
        // again — the old `TrackStore::insert` replacement semantics.
        assert!(!world.hierarchy(parent).children.contains(&id));
        assert!(world.is_root(id));
    }

    #[test]
    fn spawn_replacing_clears_the_coordinate_frame() {
        let mut world = World::new();
        let id = world.spawn("g", "Graph");
        world.coordinate_frame.insert(
            id,
            CoordinateFrameComponent {
                x_domain: [0.0, 4.0],
                ..CoordinateFrameComponent::default()
            },
        );
        assert!(world.coordinate_frame(id).is_some());

        let replacement = world.spawn_replacing("g", AnimationTrack::new("g".to_string(), "Graph"));
        assert_eq!(replacement, id, "replacement keeps the entity id");
        assert!(
            world.coordinate_frame(id).is_none(),
            "spawn_replacing must not inherit the previous scene's frame"
        );
    }

    #[test]
    fn views_resolve_by_label_and_id() {
        let world = {
            let mut world = World::new();
            world.spawn("a", "Rect");
            world.spawn("b", "Text");
            world
        };
        assert_eq!(world.entity("b").unwrap().id().index(), 1);
        assert_eq!(world.view(EntityId(0)).unwrap().label(), "a");
        assert!(world.entity("missing").is_none());
        assert!(world.view(EntityId(9)).is_none());
        assert_eq!(world.views().count(), 2);
    }

    #[test]
    fn clear_drops_entities_and_components() {
        let mut world = World::new();
        world.spawn("a", "Rect");
        world.clear();
        assert!(world.is_empty());
        assert!(world.hierarchy.is_empty());
        assert!(world.container_layout.is_empty());
        assert!(world.coordinate_frame.is_empty());
    }
}

#[cfg(test)]
mod decl_span_tests {
    use super::*;

    #[test]
    fn declaration_span_is_a_component_and_resets_on_replacement() {
        let mut world = World::new();
        let id = world.spawn("box", "Rect");
        world.set_decl_span(id, animatix_syntax::ast::Span::new(1, 1, 1, 10));
        assert_eq!(world.decl_span(id).map(|s| s.start_line), Some(1));

        world.spawn_replacing("box", AnimationTrack::new("box".into(), "Rect"));
        assert!(world.decl_span(id).is_none(), "replacement starts from a clean span");
    }
}

#[cfg(test)]
mod meta_tests {
    use super::*;

    #[test]
    fn entity_meta_defaults_and_resets_on_replacement() {
        let mut world = World::new();
        let id = world.spawn("box", "Rect");
        assert!(world.meta(id).visible);
        assert!(!world.meta(id).locked);
        assert!(world.meta(id).camera_follow);
        assert_eq!(world.meta(id).parallax, 1.0);

        world.meta_mut(id).visible = false;
        world.meta_mut(id).locked = true;
        world.meta_mut(id).hidden_by_default = true;
        world.spawn_replacing("box", AnimationTrack::new("box".into(), "Rect"));
        let meta = world.meta(id);
        assert!(meta.visible, "replacement starts visible");
        assert!(!meta.locked, "replacement starts unlocked");
        assert!(!meta.hidden_by_default, "replacement starts revealed");
    }
}
