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

impl Default for CarriedMeta {
    fn default() -> Self {
        Self {
            visible: true,
            camera_follow: true,
            parallax: 1.0,
            hidden_by_default: false,
        }
    }
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

/// Recursively collect persistent actors and their children into a CarryBag.
fn collect_persistent_entries(
    timeline: &Timeline,
    label: &str,
    time_ms: u64,
    ancestors_are_persistent: bool,
    entries: &mut BTreeMap<String, CarryEntry>,
) {
    let is_persistent = timeline.persistence_flag(label) || ancestors_are_persistent;

    if !is_persistent {
        return;
    }

    // Build child entries first so their references can be moved in.
    let mut child_entries = BTreeMap::new();
    for child_label in timeline.child_labels(label) {
        collect_persistent_entries(
            timeline,
            child_label,
            time_ms,
            true, // parent is persistent, so children carry too
            &mut child_entries,
        );
    }

    let Some(id) = timeline.world.get_id(label) else {
        tracing::warn!("Skipping persistent entry for missing actor '{}'", label);
        return;
    };
    let effects = timeline.world.effects(id).map(|chain| snapshot_effects(chain, time_ms));
    let highlight = snapshot_highlight_at(timeline.world.highlight_or_default(id), time_ms);
    let property_plan = snapshot_property_plan_at(timeline.world.property_plan(id), time_ms);
    let tagged = snapshot_tagged_at(timeline.world.tagged_tracks(id), time_ms);
    let shape = snapshot_shape_at(timeline.world.shape_or_default(id), time_ms);
    let text = snapshot_text_at(timeline.world.text_or_default(id), time_ms);
    let geometry = snapshot_geometry_at(timeline.world.geometry(id), time_ms);
    let style = snapshot_style_at(timeline.world.style(id), time_ms);
    let payload = timeline
        .world
        .payload(id)
        .map_or_else(crate::primitives::PrimitivePayload::default, |payload| {
            snapshot_payload_at(&payload, time_ms)
        });
    let actor_type = timeline.world.actor_type(id).to_string();
    let caps = timeline.world.caps_overrides.get(id).copied();
    let meta = timeline.world.meta.get(id).map(|meta| CarriedMeta {
        visible: meta.visible,
        camera_follow: meta.camera_follow,
        parallax: meta.parallax,
        hidden_by_default: meta.hidden_by_default,
    });
    let child_order = timeline.world.child_order.get(id).map(|component| {
        let order_labels = |ids: &[EntityId]| -> Vec<String> {
            ids.iter()
                .filter_map(|child_id| timeline.world.label(*child_id).map(str::to_string))
                .collect()
        };
        CarriedChildOrder {
            default_order: order_labels(&component.track.default_value),
            keyframes: component
                .track
                .keyframes
                .iter()
                .map(|(time, (ids, easing))| (*time, order_labels(ids), *easing))
                .collect(),
        }
    });

    let local_bounds = snapshot_local_bounds(&timeline.world, id);

    entries.insert(
        label.to_string(),
        CarryEntry {
            actor_type,
            children: child_entries,
            persistent: timeline.persistence_flag(label),
            auto_color_slot: timeline.auto_color_slot(label),
            container_layout: timeline.container_layout(label).cloned(),
            transcript: timeline.transcript(label).cloned(),
            coordinate_frame: timeline.coordinate_frame_for(label).cloned(),
            effects,
            payload,
            highlight,
            shape,
            text,
            geometry,
            style,
            property_plan,
            tagged,
            caps,
            meta,
            child_order,
            local_bounds,
        },
    );
}

impl Timeline {
    /// Snapshot the timeline at a given time, returning a `CarryBag`.
    ///
    /// Iterates the entities' [`crate::timeline::PersistenceComponent`]s,
    /// collapses each property tier to a single keyframe, and recursively
    /// snapshots children. `has_successor == false` warns
    /// `PersistTargetNotCarried` since there is no next scene.
    pub fn compute_carry_bag(&self, time_ms: u64, _has_successor: bool) -> CarryBag {
        let mut entries = BTreeMap::new();

        // `persist camera` has no track; sample its axes where the scene ends.
        let camera = self.camera_persistent.then(|| {
            let (pan, zoom, spin) = self.camera.values_at(time_ms);
            CarriedCamera { pan, zoom, spin }
        });

        let persistent_labels: Vec<&str> = self
            .persistence_flags_iter()
            .filter(|(_, persistent)| *persistent)
            .map(|(label, _)| label)
            .collect();

        for label in persistent_labels {
            if label == crate::timeline::camera::CAMERA_TARGET {
                // Not an actor: handled above, and collecting it would only warn.
                continue;
            }
            collect_persistent_entries(self, label, time_ms, false, &mut entries);
        }

        CarryBag { entries, camera }
    }
}

mod inject;
mod snapshot;

use snapshot::snapshot_effects;
pub use snapshot::{
    snapshot_geometry_at, snapshot_highlight_at, snapshot_local_bounds, snapshot_payload_at,
    snapshot_property_plan_at, snapshot_shape_at, snapshot_style_at, snapshot_tagged_at,
    snapshot_text_at,
};

#[cfg(test)]
mod tests;
