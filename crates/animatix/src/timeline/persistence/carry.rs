//! Carry-bag construction: `Timeline::compute_carry_bag` and the recursive
//! per-actor snapshot walk.

use std::collections::BTreeMap;

use super::snapshot::snapshot_effects;
use super::{
    CarriedCamera, CarriedChildOrder, CarriedMeta, CarryBag, CarryEntry, snapshot_geometry_at,
    snapshot_highlight_at, snapshot_local_bounds, snapshot_payload_at, snapshot_property_plan_at,
    snapshot_shape_at, snapshot_style_at, snapshot_tagged_at, snapshot_text_at,
};
use crate::timeline::{EntityId, Timeline};

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
