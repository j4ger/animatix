//! The timeline ECS world: entities plus typed component stores.
mod caps;
mod component;
mod fields;
mod geometry;
mod hierarchy;
mod highlight;
mod keyframes;
mod meta;
#[cfg(test)]
mod meta_default_tests;
mod payload;
mod plan;
mod shape;
mod style;
mod tagged;
mod text;
mod view;
pub use component::{ComponentStore, RequiredStore};
pub use fields::{TrackFields, TrackFieldsMut};
pub use hierarchy::SceneHierarchyComponent;
pub use meta::EntityMeta;
pub use tagged::TaggedTracksComponent;
pub use view::{EntityMut, EntityRef};

use std::collections::HashMap;

use super::components::{
    AutoColorComponent, ChildOrderComponent, CoordinateFrameComponent, PersistenceComponent,
    TranscriptComponent,
};
use super::effects::EffectChainTrack;
use super::entity::{EntityId, EntityStore};
use super::legend::LegendTracks;
use super::types::ContainerMetadata;
use crate::primitives::{ImagePayload, PlotPayload, SvgPayload};

/// Entities and component stores for one scene timeline.
#[derive(Clone, Debug, Default)]
pub struct World {
    pub(crate) tracks: EntityStore,
    pub(crate) hierarchy: RequiredStore<SceneHierarchyComponent>,
    pub(crate) highlight: ComponentStore<crate::timeline::animation_track::HighlightTracks>,
    pub(crate) tagged_tracks: RequiredStore<TaggedTracksComponent>,
    /// Primitive identity (registry key) for every entity.
    pub(crate) actor_types: RequiredStore<String>,
    pub(crate) caps: RequiredStore<animatix_core::caps::ActorCaps>,
    pub(crate) behavior: RequiredStore<crate::timeline::behavior::BehaviorId>,
    /// Shape-command memo cells per entity.
    pub(crate) shape_memos:
        RequiredStore<std::cell::RefCell<Box<crate::primitives::ShapeCommandMemo>>>,
    /// Memo bounds handoff slot per entity.
    pub(crate) pending_shape_bounds: RequiredStore<std::cell::Cell<Option<Option<kurbo::Rect>>>>,
    /// Dense precise-bounds table slot per entity (`0` until `EvalCaches::ensure_bounds_registry` stamps it).
    pub(crate) bounds_slots: RequiredStore<std::cell::Cell<u32>>,
    /// Style tier for every entity.
    pub(crate) style: RequiredStore<crate::timeline::animation_track::StyleTracks>,
    pub(crate) geometry: RequiredStore<crate::timeline::animation_track::GeometryTracks>,
    /// Text tier for entities with text behavior.
    pub(crate) text: ComponentStore<crate::timeline::animation_track::TextTracks>,
    /// Shape geometry tier for entities with shape behavior.
    pub(crate) shape: ComponentStore<crate::timeline::animation_track::ShapeTracks>,
    /// Fallback static defaults for optional tiers.
    pub(crate) default_shape: crate::timeline::animation_track::ShapeTracks,
    pub(crate) default_text: crate::timeline::animation_track::TextTracks,
    pub(crate) default_highlight: crate::timeline::animation_track::HighlightTracks,
    /// Explicit capability overrides for extension primitives.
    pub(crate) caps_overrides: ComponentStore<animatix_core::caps::ActorCaps>,
    /// Registry-driven property slots for every entity.
    pub(crate) property_plan: RequiredStore<crate::timeline::plan::PropertyPlan>,
    /// Per-entity visibility/lock/camera metadata.
    pub(crate) meta: ComponentStore<EntityMeta>,
    /// Layout configuration for `Row`/`Col`/`Grid`/`Stack` actors.
    pub(crate) container_layout: ComponentStore<ContainerMetadata>,
    /// Animated child order for containers targeted by `swap` / `reorder`.
    pub(crate) child_order: ComponentStore<ChildOrderComponent>,
    /// Mathematical coordinate frame for plot hosts (`Graph` and any primitive with `plot_host`).
    pub(crate) coordinate_frame: ComponentStore<CoordinateFrameComponent>,
    /// `persist` / `remove` state for actors that were targeted by an action.
    pub(crate) persistence: ComponentStore<PersistenceComponent>,
    /// `color: auto` slot for actors declared with automatic coloring.
    pub(crate) auto_color: ComponentStore<AutoColorComponent>,
    /// Parsed speech transcript for `Audio` / `Captions` actors.
    pub(crate) transcript: ComponentStore<TranscriptComponent>,
    /// Precomputed or cached local-space bounding box for an entity.
    pub(crate) local_bounds: ComponentStore<crate::timeline::LocalBoundsComponent>,
    /// Legend data for actors that participate in a legend.
    pub(crate) legend: ComponentStore<LegendTracks>,
    /// Source span of the actor's declaration, for editor navigation.
    pub(crate) decl_span: ComponentStore<animatix_syntax::ast::Span>,
    /// Post-processing effect chain for a compositing scope.
    pub(crate) effects: ComponentStore<EffectChainTrack>,
    /// Dedicated component stores for primitive payloads.
    pub(crate) plot_payload: ComponentStore<PlotPayload>,
    pub(crate) svg_payload: ComponentStore<SvgPayload>,
    pub(crate) image_payload: ComponentStore<ImagePayload>,
    /// Root entities in authored/insertion order.
    pub(crate) roots: Vec<EntityId>,
    /// Build-time command buffer: pending parent mappings.
    pub(crate) pending_parents: HashMap<String, EntityId>,
    /// Build-time command buffer: pending auto-color slot.
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
    /// Whether `id` refers to a live entity.
    #[inline(always)]
    pub fn exists(&self, id: EntityId) -> bool {
        self.tracks.exists(id)
    }
    /// A read-only entity view.
    #[inline(always)]
    pub fn view(&self, id: EntityId) -> Option<EntityRef<'_>> {
        self.tracks.exists(id).then(|| EntityRef::new(self, id))
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
        self.tracks.exists(id).then(|| EntityMut::new(self, id))
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

    /// Create an entity, or return the existing one.
    ///
    /// Mirrors the build path's `tracks.entry(label).or_insert_with(...)`:
    /// re-declaring a label keeps the same entity and its current data.
    pub fn spawn(&mut self, label: &str, actor_type: impl Into<String>) -> EntityId {
        let actor_type = actor_type.into();
        if let Some(id) = self.tracks.get_id(label) {
            // The entity may have been created by a direct `EntityStore`
            // insert (tests); keep the component stores aligned before anyone
            // borrows them.
            self.ensure_len();
            self.attach_pending_auto_color(id, label);
            return id;
        }
        let id = self.tracks.insert(label.to_string());
        self.ensure_len();
        let caps = animatix_std::caps_for_type(&actor_type).unwrap_or_default();
        *self.caps.get_mut(id) = caps;
        *self.behavior.get_mut(id) =
            crate::timeline::behavior::behavior_id_for_builtin(&actor_type)
                .unwrap_or(crate::timeline::behavior::BehaviorId::UNKNOWN);
        if caps.is_shape || caps.shape.is_some() || caps.connector || caps.callout {
            self.ensure_shape(id);
        }
        if caps.text.is_some() || caps.text_paths || caps.callout {
            self.ensure_text(id);
        }
        if actor_type == "Fragment"
            || actor_type == "Equation"
            || caps.child_processing == animatix_core::caps::ChildProcessingKind::Equation
        {
            self.ensure_highlight(id);
        }
        *self.actor_types.get_mut(id) = actor_type;
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// Replace an entity's data, resetting its optional/required components.
    ///
    /// This is the carry-injection path's old insert-replacement semantics:
    /// the label keeps its entity id, but every component starts fresh rather
    /// than inheriting the previous scene's state.
    pub fn spawn_replacing(&mut self, label: &str, actor_type: &str) -> EntityId {
        let replaced = self.tracks.contains_key(label);
        let id = self.spawn(label, actor_type);
        if replaced {
            self.unlink(id);
        }
        self.ensure_len();
        *self.actor_types.get_mut(id) = actor_type.to_string();
        self.caps.reset_at(id);
        *self.caps.get_mut(id) = animatix_std::caps_for_type(actor_type).unwrap_or_default();
        self.behavior.reset_at(id);
        *self.behavior.get_mut(id) = crate::timeline::behavior::behavior_id_for_builtin(actor_type)
            .unwrap_or(crate::timeline::behavior::BehaviorId::UNKNOWN);
        self.hierarchy.reset_at(id);
        self.highlight.reset_at(id);
        self.tagged_tracks.reset_at(id);
        self.shape_memos.reset_at(id);
        self.pending_shape_bounds.reset_at(id);
        self.bounds_slots.reset_at(id);
        self.style.reset_at(id);
        self.geometry.reset_at(id);
        self.text.reset_at(id);
        self.shape.reset_at(id);
        self.property_plan.reset_at(id);
        self.meta.reset_at(id);
        self.caps_overrides.reset_at(id);
        self.container_layout.reset_at(id);
        self.child_order.reset_at(id);
        self.coordinate_frame.reset_at(id);
        self.persistence.reset_at(id);
        self.auto_color.reset_at(id);
        self.transcript.reset_at(id);
        self.local_bounds.reset_at(id);
        self.effects.reset_at(id);
        self.legend.reset_at(id);
        self.plot_payload.reset_at(id);
        self.svg_payload.reset_at(id);
        self.image_payload.reset_at(id);
        self.decl_span.reset_at(id);
        self.attach_pending_auto_color(id, label);
        self.adopt(id, label);
        id
    }

    /// The behavior ID for an entity, or [`BehaviorId::UNKNOWN`](crate::timeline::behavior::BehaviorId::UNKNOWN) when unregistered.
    #[inline(always)]
    pub fn behavior(&self, id: EntityId) -> crate::timeline::behavior::BehaviorId {
        self.behavior
            .try_get(id)
            .copied()
            .unwrap_or(crate::timeline::behavior::BehaviorId::UNKNOWN)
    }

    /// Set an entity's behavior ID directly (extension registration / dynamic rebinding).
    pub fn set_behavior(&mut self, id: EntityId, behavior: crate::timeline::behavior::BehaviorId) {
        self.ensure_len();
        *self.behavior.get_mut(id) = behavior;
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
    /// [`Self::spawn`]. A missing parent is a build bug and keeps
    /// the old path's `warn!` instead of silently orphaning the child.
    pub fn set_parent_label(&mut self, child_label: &str, parent_label: Option<&str>) {
        // Entities may not exist yet when a relation is registered; grow the
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
    /// Handles entities created outside [`Self::spawn`] (carry injection,
    /// tests) that never adopted their buffered links. A pending
    /// label with no entity is dropped with a warning: the label never became a
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
        self.hierarchy.ensure_len(len);
        self.highlight.ensure_len(len);
        self.tagged_tracks.ensure_len(len);
        self.actor_types.ensure_len(len);
        self.caps.ensure_len(len);
        self.behavior.ensure_len(len);
        self.shape_memos.ensure_len(len);
        self.pending_shape_bounds.ensure_len(len);
        self.bounds_slots.ensure_len(len);
        self.style.ensure_len(len);
        self.geometry.ensure_len(len);
        self.text.ensure_len(len);
        self.shape.ensure_len(len);
        self.property_plan.ensure_len(len);
        self.meta.ensure_len(len);
        self.caps_overrides.ensure_len(len);
        self.container_layout.ensure_len(len);
        self.child_order.ensure_len(len);
        self.coordinate_frame.ensure_len(len);
        self.persistence.ensure_len(len);
        self.auto_color.ensure_len(len);
        self.transcript.ensure_len(len);
        self.local_bounds.ensure_len(len);
        self.legend.ensure_len(len);
        self.effects.ensure_len(len);
        self.plot_payload.ensure_len(len);
        self.svg_payload.ensure_len(len);
        self.image_payload.ensure_len(len);
        self.decl_span.ensure_len(len);
    }

    /// Drop every entity and component.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.hierarchy.clear();
        self.highlight.clear();
        self.tagged_tracks.clear();
        self.actor_types.clear();
        self.caps.clear();
        self.behavior.clear();
        self.shape_memos.clear();
        self.pending_shape_bounds.clear();
        self.bounds_slots.clear();
        self.style.clear();
        self.geometry.clear();
        self.text.clear();
        self.shape.clear();
        self.property_plan.clear();
        self.meta.clear();
        self.caps_overrides.clear();
        self.container_layout.clear();
        self.child_order.clear();
        self.coordinate_frame.clear();
        self.persistence.clear();
        self.auto_color.clear();
        self.transcript.clear();
        self.local_bounds.clear();
        self.legend.clear();
        self.effects.clear();
        self.plot_payload.clear();
        self.svg_payload.clear();
        self.image_payload.clear();
        self.decl_span.clear();
        self.roots.clear();
        self.pending_parents.clear();
        self.pending_auto_color = None;
        self.pending_roots.clear();
    }
}

#[cfg(test)]
mod tests;
