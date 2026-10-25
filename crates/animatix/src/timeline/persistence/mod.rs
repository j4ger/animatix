//! # Scene Persistence (CarryBag)
//!
//! CarryBag is the mechanism for carrying actors from one scene to the next
//! across a `play` edge in a multi-scene composition.  Actors marked with
//! `persist` are snapshotted at exit time and injected into the next scene's
//! timeline.
//!
//! ## Phase 1 scope
//!
//! This module provides:
//!
//! - `CarryBag` — bag holding snapshot entries keyed by actor label.
//! - `CarryEntry` — a single snapshotted actor plus its recursive subtree.
//! - [`Timeline::compute_carry_bag`] — snapshots all `persistent == true` actors and their children
//!   at a given time (each property tier collapses to a single keyframe at `t=0`
//!   holding the value sampled at `time_ms`).

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
    ContainerMetadata, CoordinateFrameComponent, PlacementMode, PositionBinding, SceneDimensions,
    Timeline,
};

// ---------------------------------------------------------------------------
// CarryBag
// ---------------------------------------------------------------------------

/// Bag of actors to carry from one scene to the next.
///
/// Created by [`Timeline::compute_carry_bag`] and consumed by the composition
/// engine when injecting into the next scene.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CarryBag {
    /// Persistent actor entries, keyed by actor label.
    pub entries: BTreeMap<String, CarryEntry>,
    /// The scene camera's axes, when the scene wrote `persist camera`.
    ///
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
    ///
    /// Entities are identified by label on the wire; the bag records the
    /// authored type explicitly so injection can re-install it. Empty only
    /// across a legacy deserialization boundary.
    #[cfg_attr(feature = "serde", serde(default))]
    pub actor_type: String,
    /// Recursive subtree of carried children (keyed by child label).
    pub children: BTreeMap<String, CarryEntry>,
    /// Whether the actor carries the `persistent` flag forward.
    pub persistent: bool,
    /// Auto-color slot index (for `color: auto`), if the actor uses auto-color.
    pub auto_color_slot: Option<usize>,
    /// Container layout metadata, if the actor is a `Row`/`Col`/`Grid`/`Stack`.
    ///
    /// Carried so a re-rooted container keeps its layout inside the successor
    /// scene instead of falling back to a plain, un-laid-out actor.
    #[cfg_attr(feature = "serde", serde(default))]
    pub container_layout: Option<ContainerMetadata>,
    /// Speech transcript attached to the actor, if it declares one.
    ///
    /// Carried as data rather than re-read from the destination scene's assets:
    /// the transcript belongs to the actor, so a carried `Captions`/`Audio`
    /// actor keeps word timings that its destination scene never declares.
    #[cfg_attr(feature = "serde", serde(default))]
    pub transcript: Option<SpeechTranscript>,
    /// Post-processing effect chain snapshot for a compositing scope.
    #[cfg_attr(feature = "serde", serde(default))]
    pub effects: Option<EffectChainTrack>,
    /// Coordinate frame of a plot host (a `Graph` and any `plot_host`
    /// primitive), if the actor has one.
    ///
    /// Carried so a persistent `Graph` keeps its domains/scales/padding in the
    /// successor scene, where its child plots resolve coordinates from it.
    /// Domain values are scene-relative, so no entity-id remap is needed.
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
    ///
    /// Carried like every other authored tier: without it, a persistent actor
    /// that uses tagged properties would lose them on scene change.
    #[cfg_attr(feature = "serde", serde(default))]
    pub tagged: crate::timeline::world::TaggedTracksComponent,
}

// ---------------------------------------------------------------------------
// carry_bag build helpers
// ---------------------------------------------------------------------------

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

    // Build child entries recursively before creating this entry so that
    // children references can be moved in.
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
    let highlight = snapshot_highlight_at(timeline.world.highlight(id), time_ms);
    let property_plan = snapshot_property_plan_at(timeline.world.property_plan(id), time_ms);
    let tagged = snapshot_tagged_at(timeline.world.tagged_tracks(id), time_ms);
    let shape = snapshot_shape_at(timeline.world.shape(id), time_ms);
    let text = snapshot_text_at(timeline.world.text(id), time_ms);
    let geometry = snapshot_geometry_at(timeline.world.geometry(id), time_ms);
    let style = snapshot_style_at(timeline.world.style(id), time_ms);
    let payload = timeline
        .world
        .payload(id)
        .map_or_else(crate::primitives::PrimitivePayload::default, |payload| {
            snapshot_payload_at(payload, time_ms)
        });
    let actor_type = timeline.world.actor_type(id).to_string();

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
        },
    );
}

impl Timeline {
    /// Snapshot the timeline at a given time, returning a `CarryBag`.
    ///
    /// Iterates the entities' [`crate::timeline::PersistenceComponent`]s, finds
    /// entries where `persistent == true`, collapses each property tier to a
    /// single keyframe, and recursively snapshots children.
    ///
    /// `has_successor` — when `false`, any persisted actors will emit a
    /// `PersistTargetNotCarried` warning since there is no next scene.
    pub fn compute_carry_bag(&self, time_ms: u64, _has_successor: bool) -> CarryBag {
        let mut entries = BTreeMap::new();

        // `persist camera` is the camera's own entry: no track to snapshot, so
        // the axes are sampled where the scene ends.
        let camera = self.camera_persistent.then(|| {
            let (pan, zoom, spin) = self.camera.values_at(time_ms);
            CarriedCamera { pan, zoom, spin }
        });

        // Collect labels whose persistence flag is set.
        let persistent_labels: Vec<&str> = self
            .persistence_flags_iter()
            .filter(|(_, persistent)| *persistent)
            .map(|(label, _)| label)
            .collect();

        for label in persistent_labels {
            if label == crate::timeline::camera::CAMERA_TARGET {
                // Not an actor: handled above, and `collect_persistent_entries`
                // would only warn that the track is missing.
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
    snapshot_geometry_at, snapshot_highlight_at, snapshot_payload_at, snapshot_property_plan_at,
    snapshot_shape_at, snapshot_style_at, snapshot_tagged_at, snapshot_text_at,
};

#[cfg(test)]
mod tests;
