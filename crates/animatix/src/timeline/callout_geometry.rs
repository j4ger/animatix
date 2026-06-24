//! Shared geometry derivation for `Callout` actors.
//!
//! Both the core renderer (`CalloutPrimitive::evaluate`) and the GUI
//! (handle drawing / hit-testing) use [`derive_callout_geometry`] so that the
//! tip position is computed by exactly one formula.

use crate::timeline::{AnimationTrack, Timeline, TrackAccessor};

// -- Resolver trait --

/// Narrow read-only interface that primitives need for target-mode layout.
///
/// This deliberately does **not** expose the full `Timeline`; primitives
/// must only query geometric data about other actors, not mutate state.
pub trait TargetResolver {
    /// Return the scene-space centre and half-size of the named actor at
    /// `time_ms`, or `None` if the actor does not exist.
    fn target_bounds(&self, name: &str, time_ms: u64) -> Option<([f32; 2], [f32; 2])>;
}

impl TargetResolver for Timeline {
    fn target_bounds(&self, name: &str, time_ms: u64) -> Option<([f32; 2], [f32; 2])> {
        let track = self.get_track(name)?;
        let centre = track.geometry.position.get(time_ms, [0.0, 0.0]);
        let half = track.geometry.size.get(time_ms, [50.0, 50.0]);
        Some((centre, half))
    }
}

// -- Geometry output --

/// Minimal geometry produced by targeted-callout derivation.
pub struct CalloutGeometry {
    /// Scene-space tip point (`to`).
    pub to: [f32; 2],
    /// Scene-space tail origin point (`from`).
    pub from: [f32; 2],
    /// Scene-space label origin (absolute, i.e. `to + label_at`).
    pub label_point: [f32; 2],
}

// -- Public helpers --

/// Derive full callout geometry from the actor's animation track.
///
/// `resolver` is used only when `target` is non-empty.  Pass `None` to
/// always fall back to the track's manual `from`/`to` values.
///
/// Attach-point formula (unrotated AABB):
/// - `"above"` / `"top"` -> centre top edge
/// - `"below"` / `"bottom"` -> centre bottom edge
/// - `"left"` -> centre left edge
/// - anything else (incl. `"right"`) -> centre right edge
pub fn derive_callout_geometry(
    track: &AnimationTrack,
    time_ms: u64,
    resolver: Option<&dyn TargetResolver>,
) -> CalloutGeometry {
    let target_name = track.geometry.callout_target.get(time_ms, String::new());
    let manual_to = track.shape.line_to.get(time_ms, [100.0, 0.0]);
    let manual_from = track.shape.line_from.get(time_ms, [-100.0, 0.0]);
    let label_at = track.geometry.label_at.get(time_ms, [0.0, 50.0]);
    let place = track.geometry.callout_place.get(time_ms, "right".to_string());
    let standoff = track.geometry.callout_standoff.get(time_ms, 40.0);
    let to_offset = track.geometry.callout_to_offset.get(time_ms, [0.0, 0.0]);

    let (to, from) = if !target_name.is_empty() {
        if let Some(res) = resolver {
            if let Some((centre, half)) = res.target_bounds(&target_name, time_ms) {
                let attach = attach_point(&place, centre, half);
                let to = [attach[0] + to_offset[0], attach[1] + to_offset[1]];
                let dir = place_direction(&place);
                let from = [to[0] + dir[0] * standoff, to[1] + dir[1] * standoff];
                (to, from)
            } else {
                (manual_to, manual_from)
            }
        } else {
            (manual_to, manual_from)
        }
    } else {
        (manual_to, manual_from)
    };

    CalloutGeometry { to, from, label_point: [to[0] + label_at[0], to[1] + label_at[1]] }
}

// -- Private helpers --

fn attach_point(place: &str, centre: [f32; 2], half: [f32; 2]) -> [f32; 2] {
    match place {
        "above" | "top" => [centre[0], centre[1] - half[1]],
        "below" | "bottom" => [centre[0], centre[1] + half[1]],
        "left" => [centre[0] - half[0], centre[1]],
        _ => [centre[0] + half[0], centre[1]], // "right" and fallback
    }
}

fn place_direction(place: &str) -> [f32; 2] {
    match place {
        "above" | "top" => [0.0, -1.0],
        "below" | "bottom" => [0.0, 1.0],
        "left" => [-1.0, 0.0],
        _ => [1.0, 0.0],
    }
}
