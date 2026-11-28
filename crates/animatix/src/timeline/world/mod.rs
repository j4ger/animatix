//! The timeline ECS world: entities plus typed component stores.
mod caps;
mod component;
mod fields;
mod geometry;
mod hierarchy;
mod highlight;
mod keyframes;
mod meta;
mod method_table;
mod payload;
mod plan;
mod plan_ownership;
mod shape;
mod style;
mod tagged;
mod text;
mod view;
pub use component::{ComponentStore, RequiredStore};
pub use fields::{TrackFields, TrackFieldsMut};
pub use hierarchy::SceneHierarchyComponent;
pub use meta::EntityMeta;
pub use method_table::MethodTableId;
pub use plan_ownership::PlanOwnership;
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
    pub(crate) shape_memos:
        RequiredStore<std::cell::RefCell<Box<crate::primitives::ShapeCommandMemo>>>,
    /// Memo bounds handoff slot per entity.
    pub(crate) pending_shape_bounds: RequiredStore<std::cell::Cell<Option<Option<kurbo::Rect>>>>,
    /// Dense precise-bounds table slot per entity (`0` until `EvalCaches::ensure_bounds_registry` stamps it).
    pub(crate) bounds_slots: RequiredStore<std::cell::Cell<u32>>,
    pub(crate) style: RequiredStore<crate::timeline::animation_track::StyleTracks>,
    pub(crate) geometry: RequiredStore<crate::timeline::animation_track::GeometryTracks>,
    pub(crate) text: ComponentStore<crate::timeline::animation_track::TextTracks>,
    pub(crate) shape: ComponentStore<crate::timeline::animation_track::ShapeTracks>,
    /// Fallback static defaults for optional tiers.
    pub(crate) default_shape: crate::timeline::animation_track::ShapeTracks,
    pub(crate) default_text: crate::timeline::animation_track::TextTracks,
    pub(crate) default_highlight: crate::timeline::animation_track::HighlightTracks,
    /// Explicit capability overrides for extension primitives.
    pub(crate) caps_overrides: ComponentStore<animatix_core::caps::ActorCaps>,
    pub(crate) property_plan: RequiredStore<crate::timeline::plan::PropertyPlan>,
    /// Plan-mount owner for actors declared inside reusable timeline plans.
    pub(crate) plan_ownership: ComponentStore<PlanOwnership>,
    /// Handle to immutable method metadata for this expanded actor instance.
    pub(crate) method_table_id: ComponentStore<MethodTableId>,
    /// Per-entity visibility/lock/camera metadata.
    pub(crate) meta: ComponentStore<EntityMeta>,
    pub(crate) container_layout: ComponentStore<ContainerMetadata>,
    pub(crate) child_order: ComponentStore<ChildOrderComponent>,
    /// Mathematical coordinate frame for plot hosts (`Graph` and any primitive with `plot_host`).
    pub(crate) coordinate_frame: ComponentStore<CoordinateFrameComponent>,
    /// `persist` / `remove` state for actors that were targeted by an action.
    pub(crate) persistence: ComponentStore<PersistenceComponent>,
    /// `color: auto` slot for actors declared with automatic coloring.
    pub(crate) auto_color: ComponentStore<AutoColorComponent>,
    /// Parsed speech transcript for `Audio` / `Captions` actors.
    pub(crate) transcript: ComponentStore<TranscriptComponent>,
    pub(crate) local_bounds: ComponentStore<crate::timeline::LocalBoundsComponent>,
    pub(crate) legend: ComponentStore<LegendTracks>,
    /// Source span of the actor's declaration, for editor navigation.
    pub(crate) decl_span: ComponentStore<animatix_syntax::ast::Span>,
    pub(crate) effects: ComponentStore<EffectChainTrack>,
    /// Dedicated component stores for primitive payloads.
    pub(crate) plot_payload: ComponentStore<PlotPayload>,
    pub(crate) svg_payload: ComponentStore<SvgPayload>,
    pub(crate) image_payload: ComponentStore<ImagePayload>,
    pub(crate) emitters: ComponentStore<crate::timeline::particles::Emitter>,
    pub(crate) video_clips: ComponentStore<crate::timeline::video::VideoClip>,
    /// Root entities in authored/insertion order.
    pub(crate) roots: Vec<EntityId>,
    /// Build-time command buffer: pending parent mappings.
    pub(crate) pending_parents: HashMap<String, EntityId>,
    /// Build-time command buffer: pending auto-color slot.
    pub(crate) pending_auto_color: Option<(String, usize)>,
    /// Build-time root order for labels whose entity does not exist yet.
    pub(crate) pending_roots: Vec<String>,
}

/// Every slot-aligned component store on [`World`], in one list: the single
/// source for [`World::ensure_len`], [`World::clear`] and [`World::spawn_replacing`]'s reset.
macro_rules! for_each_store {
    ($mac:ident $(, $arg:tt)*) => {
        $mac! { $( $arg, )* hierarchy, highlight, tagged_tracks, actor_types, caps, behavior,
        shape_memos, pending_shape_bounds, bounds_slots, style, geometry, text, shape,
        property_plan, plan_ownership, method_table_id, meta, caps_overrides, container_layout,
        child_order, coordinate_frame,
        persistence, auto_color, transcript, local_bounds, legend, effects, plot_payload,
        svg_payload, image_payload, emitters, video_clips, decl_span, }
    };
}

macro_rules! ensure_store_len {
    ($world:tt, $len:expr, $($store:ident),* $(,)?) => {
        $( $world.$store.ensure_len($len); )*
    };
}

macro_rules! clear_stores {
    ($world:tt, $($store:ident),* $(,)?) => {
        $( $world.$store.clear(); )*
    };
}

macro_rules! reset_stores_at {
    ($world:tt, $id:expr, $($store:ident),* $(,)?) => {
        $( $world.$store.reset_at($id); )*
    };
}

mod api;
mod spawn;

#[cfg(test)]
mod tests;
