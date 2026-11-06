//! # Scene Persistence (CarryBag)
//!
//! CarryBag carries `persist` actors across a `play` edge: they are snapshotted
//! at exit ([`Timeline::compute_carry_bag`]) and injected into the next scene's
//! timeline, each property tier collapsing to one keyframe at `t=0`.

use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use animatix_text::transcript::SpeechTranscript;

use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::animation_track::{
    GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use crate::timeline::effects::EffectChainTrack;
use crate::timeline::property_track::{PropertyTrack, TrackAccessor};
use crate::timeline::{
    ChildOrderComponent, ContainerMetadata, CoordinateFrameComponent, EntityId, PlacementMode,
    PositionBinding, SceneDimensions, Timeline,
};

/// Bag of actors to carry from one scene to the next; created by
/// [`Timeline::compute_carry_bag`] and consumed by the composition engine.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CarryBag {
    /// Persistent actor entries, keyed by actor label.
    pub entries: BTreeMap<String, CarryEntry>,
    /// The scene camera's axes, when the scene wrote `persist camera`.
    /// The camera is not an actor and owns no track, so it cannot ride in
    /// `entries`; a scene that ends mid-push hands over three numbers instead.
    pub camera: Option<CarriedCamera>,
}

/// The three camera axes as one scene left them.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug, Default)]
pub struct CarriedCamera {
    /// `camera.at` / `camera.position` / `camera.pan`, in screen pixels.
    pub pan: [f32; 2],
    /// `camera.zoom` / `camera.scale`; 1.0 is no magnification.
    pub zoom: f32,
    /// `camera.rotation` / `camera.spin`, in radians.
    pub spin: f32,
}

/// A single actor to carry, with its snapshot and recursive subtree.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CarryEntry {
    /// Primitive identity of the actor (`World.actor_types`).
    /// Entities are identified by label on the wire, so the bag records the
    /// authored type to re-install it. Empty only across a legacy boundary.
    #[cfg_attr(feature = "serde", serde(default))]
    pub actor_type: String,
    /// Recursive subtree of carried children (keyed by child label).
    pub children: BTreeMap<String, CarryEntry>,
    /// Whether the actor carries the `persistent` flag forward.
    pub persistent: bool,
    /// Auto-color slot index (for `color: auto`), if the actor uses auto-color.
    pub auto_color_slot: Option<usize>,
    /// Container layout metadata, if the actor is a `Row`/`Col`/`Grid`/`Stack`.
    /// Carried so a re-rooted container keeps its layout in the successor scene
    /// instead of falling back to a plain, un-laid-out actor.
    #[cfg_attr(feature = "serde", serde(default))]
    pub container_layout: Option<ContainerMetadata>,
    /// Speech transcript attached to the actor, if it declares one.
    /// Carried as data: the transcript belongs to the actor, so a carried
    /// `Captions`/`Audio` actor keeps word timings its destination never declares.
    #[cfg_attr(feature = "serde", serde(default))]
    pub transcript: Option<SpeechTranscript>,
    /// Post-processing effect chain snapshot for a compositing scope.
    #[cfg_attr(feature = "serde", serde(default))]
    pub effects: Option<EffectChainTrack>,
    /// Coordinate frame of a plot host (a `Graph` and any `plot_host`
    /// primitive), if the actor has one.
    /// Carried so a persistent `Graph` keeps its domains/scales/padding in the
    /// successor scene; domain values are scene-relative, so no id remap is needed.
    #[cfg_attr(feature = "serde", serde(default))]
    pub coordinate_frame: Option<CoordinateFrameComponent>,
    /// Primitive payload snapshot (plot parametrisation, SVG/image state).
    #[cfg_attr(feature = "serde", serde(default))]
    pub payload: crate::primitives::PrimitivePayload,
    /// Equation-highlight tier snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub highlight: crate::timeline::animation_track::HighlightTracks,
    /// Shape tier snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub shape: crate::timeline::animation_track::ShapeTracks,
    /// Style tier snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub style: crate::timeline::animation_track::StyleTracks,
    /// Geometry tier snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub geometry: crate::timeline::animation_track::GeometryTracks,
    /// Text tier snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub text: crate::timeline::animation_track::TextTracks,
    /// Registry-driven property slots snapshot.
    #[cfg_attr(feature = "serde", serde(default))]
    pub property_plan: crate::timeline::plan::PropertyPlan,
    /// Tagged-union property tracks snapshot (`legend`, extension tags).
    /// Without it, a persistent actor using tagged properties loses them.
    #[cfg_attr(feature = "serde", serde(default))]
    pub tagged: crate::timeline::world::TaggedTracksComponent,
    /// Capability override for extension primitives, if one was installed.
    /// `World::set_identity` re-derives caps from the actor type, so an
    /// extension actor would otherwise fall back to default caps.
    #[cfg_attr(feature = "serde", serde(default))]
    pub caps: Option<animatix_core::caps::ActorCaps>,
    /// Declaration-authored `EntityMeta` flags (see [`CarriedMeta`]).
    /// They live on no property track, so without them a persisted actor resets
    /// to scene defaults (`camera_follow: true`, `parallax: 1.0`).
    #[cfg_attr(feature = "serde", serde(default))]
    pub meta: Option<CarriedMeta>,
    /// Animated child-order track (`swap` / `reorder`), as child labels.
    /// The end state of a completed reorder lives here, not on
    /// [`ContainerMetadata::child_order`], so skipping it would undo the
    /// reorder. Ids are scene-local; the wire form stores labels and injection
    /// remaps them.
    #[cfg_attr(feature = "serde", serde(default))]
    pub child_order: Option<CarriedChildOrder>,
    /// Precomputed or cached local-space bounding box for an entity.
    #[cfg_attr(feature = "serde", serde(default))]
    pub local_bounds: Option<kurbo::Rect>,
}

/// The `EntityMeta` subset that survives a carry (see [`CarryEntry::meta`]).
/// `locked` is editor state and `first_seen_ms` is per-scene; defaults mirror
/// `EntityMeta`'s live defaults.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarriedMeta {
    /// Whether the actor is rendered at all.
    pub visible: bool,
    /// Whether the camera transform applies to the actor.
    pub camera_follow: bool,
    /// Camera parallax factor (`1.0` is none).
    pub parallax: f32,
    /// Whether the actor was declared before the first keyframe un-revealed.
    pub hidden_by_default: bool,
}

/// An animated child-order track with scene-local ids replaced by labels.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, Default)]
pub struct CarriedChildOrder {
    /// Labels of the track's default order.
    pub default_order: Vec<String>,
    /// `(time_ms, order labels, easing)` in track order.
    pub keyframes: Vec<(u64, Vec<String>, Easing)>,
}

mod carry;
mod inject;
mod snapshot;

pub use snapshot::{
    snapshot_geometry_at, snapshot_highlight_at, snapshot_local_bounds, snapshot_payload_at,
    snapshot_property_plan_at, snapshot_shape_at, snapshot_style_at, snapshot_tagged_at,
    snapshot_text_at,
};

#[cfg(test)]
mod tests;
