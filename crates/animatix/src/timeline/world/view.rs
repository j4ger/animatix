//! Read/write views over one entity.
//!
//! `EntityRef` and `EntityMut` are the only entity access surface outside
//! `world/`. Required-component accessors are infallible; optional ones
//! return `Option`. Every accessor is `#[inline]`. For two mutable components
//! at once, use the narrow `World::split_*_mut` helpers instead of nesting
//! these borrows.

use super::super::components::{
    AutoColorComponent, ChildOrderComponent, CoordinateFrameComponent, PersistenceComponent,
    TranscriptComponent,
};
use super::super::effects::EffectChainTrack;
use super::super::entity::EntityId;
use super::super::legend::LegendTracks;
use super::super::types::ContainerMetadata;
use super::World;

/// Borrowed read-only view of one entity.
#[derive(Clone, Copy, Debug)]
pub struct EntityRef<'w> {
    world: &'w World,
    id: EntityId,
}

impl<'w> EntityRef<'w> {
    #[inline(always)]
    pub(crate) fn new(world: &'w World, id: EntityId) -> Self {
        Self { world, id }
    }

    /// The entity handle.
    #[inline(always)]
    pub fn id(self) -> EntityId {
        self.id
    }

    /// The entity's label (empty only if the handle is stale, which cannot
    /// happen for a view produced by `World`).
    #[inline(always)]
    pub fn label(self) -> &'w str {
        self.world.tracks.get_label(self.id).unwrap_or("")
    }

    /// The entity's behavior ID.
    #[inline(always)]
    pub fn behavior(self) -> crate::timeline::behavior::BehaviorId {
        self.world.behavior(self.id)
    }

    /// Source span of the entity's declaration.
    #[inline(always)]
    pub fn decl_span(self) -> Option<&'w animatix_syntax::ast::Span> {
        self.world.decl_span.get(self.id)
    }

    /// The entity's visibility/lock/camera metadata.
    #[inline(always)]
    pub fn meta(self) -> &'w super::EntityMeta {
        self.world.meta.get(self.id).unwrap_or(&super::EntityMeta::DEFAULT)
    }

    /// The entity's parent/children links.
    #[inline(always)]
    pub fn hierarchy(self) -> &'w super::SceneHierarchyComponent {
        self.world.hierarchy.get(self.id)
    }

    /// The entity's container layout configuration, when it is a
    /// `Row`/`Col`/`Grid`/`Stack` container.
    #[inline(always)]
    pub fn container_layout(self) -> Option<&'w ContainerMetadata> {
        self.world.container_layout.get(self.id)
    }

    /// The entity's animated child order, when a `swap`/`reorder` targeted it.
    #[inline(always)]
    pub fn child_order(self) -> Option<&'w ChildOrderComponent> {
        self.world.child_order.get(self.id)
    }

    /// The entity's mathematical coordinate frame, when it is a plot host.
    #[inline(always)]
    pub fn coordinate_frame(self) -> Option<&'w CoordinateFrameComponent> {
        self.world.coordinate_frame.get(self.id)
    }

    /// The entity's `persist`/`remove` state, when an action targeted it.
    #[inline(always)]
    pub fn persistence(self) -> Option<&'w PersistenceComponent> {
        self.world.persistence.get(self.id)
    }

    /// The entity's auto-color slot, when it was declared with `color: auto`.
    #[inline(always)]
    pub fn auto_color(self) -> Option<&'w AutoColorComponent> {
        self.world.auto_color.get(self.id)
    }

    /// The entity's speech transcript, when it declares one.
    #[inline(always)]
    pub fn transcript(self) -> Option<&'w TranscriptComponent> {
        self.world.transcript.get(self.id)
    }

    /// The entity's legend data, when it participates in a legend.
    #[inline(always)]
    pub fn legend(self) -> Option<&'w LegendTracks> {
        self.world.legend.get(self.id)
    }

    /// The entity's post-processing effect chain, when it is a scope.
    #[inline(always)]
    pub fn effects(self) -> Option<&'w EffectChainTrack> {
        self.world.effects.get(self.id)
    }

    /// The entity's primitive payload (plot/SVG/image), when present.
    #[inline(always)]
    pub fn payload(self) -> Option<crate::primitives::PrimitivePayload> {
        self.world.payload(self.id)
    }

    /// The entity's plot payload, when present.
    #[inline(always)]
    pub fn plot_payload(self) -> Option<&'w crate::primitives::PlotPayload> {
        self.world.plot_payload(self.id)
    }

    /// The entity's SVG payload, when present.
    #[inline(always)]
    pub fn svg_payload(self) -> Option<&'w crate::primitives::SvgPayload> {
        self.world.svg_payload(self.id)
    }

    /// The entity's image payload, when present.
    #[inline(always)]
    pub fn image_payload(self) -> Option<&'w crate::primitives::ImagePayload> {
        self.world.image_payload(self.id)
    }
}

/// Borrowed mutable view of one entity.
///
/// Methods take `&mut self` and hand out one component borrow at a time; code
/// that needs two components mutably at once uses the narrow split helpers on
/// `World` instead of nesting these borrows.
#[derive(Debug)]
pub struct EntityMut<'w> {
    world: &'w mut World,
    id: EntityId,
}

impl<'w> EntityMut<'w> {
    #[inline(always)]
    pub(crate) fn new(world: &'w mut World, id: EntityId) -> Self {
        Self { world, id }
    }

    /// The entity handle.
    #[inline(always)]
    pub fn id(&self) -> EntityId {
        self.id
    }

    /// The entity's label.
    #[inline(always)]
    pub fn label(&self) -> &str {
        self.world.tracks.get_label(self.id).unwrap_or("")
    }

    /// Source span of the entity's declaration.
    #[inline(always)]
    pub fn decl_span(&self) -> Option<&animatix_syntax::ast::Span> {
        self.world.decl_span.get(self.id)
    }

    /// Set the entity's declaration span.
    #[inline(always)]
    pub fn set_decl_span(&mut self, span: animatix_syntax::ast::Span) {
        self.world.decl_span.get_or_insert_with(self.id, || span);
    }

    /// The entity's visibility/lock/camera metadata.
    #[inline(always)]
    pub fn meta(&self) -> &super::EntityMeta {
        self.world.meta.get(self.id).unwrap_or(&super::EntityMeta::DEFAULT)
    }

    /// Mutably borrow the entity's metadata.
    #[inline(always)]
    pub fn meta_mut(&mut self) -> &mut super::EntityMeta {
        self.world.meta_mut(self.id)
    }

    /// The entity's parent/children links.
    #[inline(always)]
    pub fn hierarchy(&self) -> &super::SceneHierarchyComponent {
        self.world.hierarchy.get(self.id)
    }

    /// Mutably borrow the entity's parent/children links.
    #[inline(always)]
    pub fn hierarchy_mut(&mut self) -> &mut super::SceneHierarchyComponent {
        self.world.hierarchy.get_mut(self.id)
    }

    /// Mutably borrow the entity's container layout configuration, if it is a
    /// layout container.
    #[inline(always)]
    pub fn container_layout_mut(&mut self) -> Option<&mut ContainerMetadata> {
        self.world.container_layout.get_mut(self.id)
    }

    /// Mutably borrow the entity's animated child order, if present.
    #[inline(always)]
    pub fn child_order_mut(&mut self) -> Option<&mut ChildOrderComponent> {
        self.world.child_order.get_mut(self.id)
    }

    /// Mutably borrow the entity's mathematical coordinate frame, when it is a
    /// plot host.
    #[inline(always)]
    pub fn coordinate_frame_mut(&mut self) -> Option<&mut CoordinateFrameComponent> {
        self.world.coordinate_frame.get_mut(self.id)
    }

    /// Mutably borrow the entity's `persist`/`remove` state, if present.
    #[inline(always)]
    pub fn persistence_mut(&mut self) -> Option<&mut PersistenceComponent> {
        self.world.persistence.get_mut(self.id)
    }

    /// Mutably borrow the entity's auto-color slot, if present.
    #[inline(always)]
    pub fn auto_color_mut(&mut self) -> Option<&mut AutoColorComponent> {
        self.world.auto_color.get_mut(self.id)
    }

    /// Mutably borrow the entity's speech transcript, if present.
    #[inline(always)]
    pub fn transcript_mut(&mut self) -> Option<&mut TranscriptComponent> {
        self.world.transcript.get_mut(self.id)
    }

    /// Mutably borrow the entity's legend data, if present.
    #[inline(always)]
    pub fn legend_mut(&mut self) -> Option<&mut LegendTracks> {
        self.world.legend.get_mut(self.id)
    }

    /// Mutably borrow the entity's post-processing effect chain, if present.
    #[inline(always)]
    pub fn effects_mut(&mut self) -> Option<&mut EffectChainTrack> {
        self.world.effects.get_mut(self.id)
    }

    /// Mutably borrow the entity's plot payload, if present.
    #[inline(always)]
    pub fn plot_payload_mut(&mut self) -> Option<&mut crate::primitives::PlotPayload> {
        self.world.plot_payload_mut(self.id)
    }

    /// Mutably borrow the entity's SVG payload, if present.
    #[inline(always)]
    pub fn svg_payload_mut(&mut self) -> Option<&mut crate::primitives::SvgPayload> {
        self.world.svg_payload_mut(self.id)
    }

    /// Mutably borrow the entity's image payload, if present.
    #[inline(always)]
    pub fn image_payload_mut(&mut self) -> Option<&mut crate::primitives::ImagePayload> {
        self.world.image_payload_mut(self.id)
    }
}
