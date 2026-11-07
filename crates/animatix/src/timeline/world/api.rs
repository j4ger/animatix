//! `World` accessors: identity lookup, entity views, and per-component reads.

use crate::timeline::components::CoordinateFrameComponent;
use crate::timeline::effects::EffectChainTrack;
use crate::timeline::entity::EntityId;
use crate::timeline::legend::LegendTracks;
use crate::timeline::world::{EntityMut, EntityRef, World};

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

    /// The behavior ID for an entity, or [`BehaviorId::UNKNOWN`](crate::timeline::behavior::BehaviorId::UNKNOWN) when unregistered.
    #[inline(always)]
    pub fn behavior(&self, id: EntityId) -> crate::timeline::behavior::BehaviorId {
        self.behavior
            .try_get(id)
            .copied()
            .unwrap_or(crate::timeline::behavior::BehaviorId::UNKNOWN)
    }

    /// Assign the resolved behavior id for an entity.
    ///
    /// Identity initialization only knows the static built-in table; build
    /// paths that resolved an extension `BehaviorDef` from the timeline's
    /// registry call this afterwards so the `BehaviorId` component matches it.
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

    /// Ensure legend data, inserting `LegendTracks::default()` when absent.
    /// Callers holding the frame cache must invalidate it.
    pub fn ensure_legend(&mut self, id: EntityId) -> &mut LegendTracks {
        if self.legend.get(id).is_none() {
            self.legend.insert(id, LegendTracks::default());
        }
        self.legend
            .get_mut(id)
            .expect("World::ensure_legend just inserted the component")
    }

    /// Inputs of the child-order cache flag: true when any container carries an
    /// animated child-order track; the scan stops at the first hit.
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

    /// Labels of `id`'s direct children, in authored order; the label boundary
    /// for label-based APIs (frame paths iterate [`crate::timeline::world::hierarchy::SceneHierarchyComponent::children`]).
    pub fn child_labels(&self, id: EntityId) -> impl Iterator<Item = &str> {
        self.hierarchy(id).children.iter().filter_map(|child| self.label(*child))
    }
}
