//! Shared geometry derivation for `Callout` actors.
//!
//! Both the core renderer (`CalloutPrimitive::evaluate`) and the GUI
//! (handle drawing / hit-testing) use [`derive_callout_geometry`] so that the
//! tip position is computed by exactly one formula.

use crate::timeline::animation_track::{CalloutPlace, SceneAnchor};
use crate::timeline::{
    AnimationTrack, Environment, SceneDimensions, Timeline, TrackAccessor, Value,
};

// -- Resolver trait --

/// Narrow read-only interface that primitives need for target-mode layout.
///
/// This deliberately does **not** expose the full `Timeline`; primitives
/// must only query geometric data about other actors, not mutate state.
pub trait TargetResolver {
    /// Return the world-space AABB centre and half-size of the named actor at
    /// `time_ms`, or `None` if the actor does not exist.
    ///
    /// The returned half-size is the half-extent of the world-space AABB (i.e.
    /// the local `size` corners transformed through the world affine), so
    /// nested translations, scales, and rotations are reflected in the result.
    fn target_bounds(
        &self,
        name: &str,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
    ) -> Option<([f32; 2], [f32; 2])>;
}

impl TargetResolver for Timeline {
    fn target_bounds(
        &self,
        name: &str,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
    ) -> Option<([f32; 2], [f32; 2])> {
        // Prefer the exact world AABB emitted from render commands on the
        // current frame. This reflects text/path/image geometry in addition to
        // the declared size box and works for already-evaluated targets.
        if let Some(rect) = self.precise_bounds_for(name) {
            let centre = rect.center();
            let half = rect.size() * 0.5;
            return Some((
                [centre.x as f32, centre.y as f32],
                [half.width as f32, half.height as f32],
            ));
        }

        let track = self.get_track(name)?;
        let half_local = track.geometry.size.get(time_ms, [50.0, 50.0]);
        // Use the world affine to get a transform-aware AABB.
        if let Some(world) = self.actor_world_affine(name, time_ms, scene_dimensions) {
            // Transform the four corners of the local rect through the world affine.
            let hw = half_local[0] as f64;
            let hh = half_local[1] as f64;
            let corners = [
                world * kurbo::Point::new(-hw, -hh),
                world * kurbo::Point::new(hw, -hh),
                world * kurbo::Point::new(-hw, hh),
                world * kurbo::Point::new(hw, hh),
            ];
            let x0 = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let x1 = corners.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
            let y0 = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let y1 = corners.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
            let centre = [((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32];
            let half = [((x1 - x0) / 2.0) as f32, ((y1 - y0) / 2.0) as f32];
            Some((centre, half))
        } else {
            // Fallback for actors not yet in the scene graph: use raw position + size.
            let centre = track.geometry.position.get(time_ms, [0.0, 0.0]);
            Some((centre, half_local))
        }
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
    /// Whether the callout has a non-empty target (targeted mode).
    pub is_targeted: bool,
    /// Target AABB centre in scene space (only valid when `is_targeted`).
    pub target_centre: [f32; 2],
    /// Target AABB half-extents in scene space (only valid when `is_targeted`).
    pub target_half: [f32; 2],
    /// Current standoff value (distance from attach point to `from`).
    pub standoff: f32,
    /// Current place value.
    pub place: CalloutPlace,
}

// -- Public helpers --

/// Derive full callout geometry from the actor's animation track.
///
/// `resolver` is used only when `target` is non-empty.  Pass `None` to
/// always fall back to the track's manual `from`/`to` values.
///
/// `scene_dimensions` is forwarded to the resolver so world-transform-aware
/// bounds (translation, scale, rotation) can be computed correctly.
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
    scene_dimensions: SceneDimensions,
) -> CalloutGeometry {
    let target_name = track.geometry.callout_target.get(time_ms, String::new());
    let manual_to = track.shape.line_to.get(time_ms, [100.0, 0.0]);
    let manual_from = track.shape.line_from.get(time_ms, [-100.0, 0.0]);
    let label_at = track.geometry.label_at.get(time_ms, [0.0, 50.0]);
    let place = track.geometry.callout_place.get(time_ms, CalloutPlace::Right);
    let standoff = track.geometry.callout_standoff.get(time_ms, 40.0);
    let to_offset = track.geometry.callout_to_offset.get(time_ms, [0.0, 0.0]);

    let (to, from, is_targeted, target_centre, target_half) = if !target_name.is_empty() {
        if let Some(res) = resolver {
            if let Some((centre, half)) = res.target_bounds(&target_name, time_ms, scene_dimensions)
            {
                let attach = attach_point(place, centre, half);
                let to = [attach[0] + to_offset[0], attach[1] + to_offset[1]];
                let dir = place_direction(place);
                let from = [to[0] + dir[0] * standoff, to[1] + dir[1] * standoff];
                (to, from, true, centre, half)
            } else {
                (manual_to, manual_from, false, [0.0f32; 2], [0.0f32; 2])
            }
        } else {
            (manual_to, manual_from, false, [0.0f32; 2], [0.0f32; 2])
        }
    } else {
        (manual_to, manual_from, false, [0.0f32; 2], [0.0f32; 2])
    };

    CalloutGeometry {
        to,
        from,
        label_point: [to[0] + label_at[0], to[1] + label_at[1]],
        is_targeted,
        target_centre,
        target_half,
        standoff,
        place,
    }
}

// -- Public helpers (shared with G5/G6 actor-anchor-point resolution) --

/// Compute the world-space point for an anchor on an actor's world-space AABB.
///
/// `centre` and `half` are the world-space AABB centre and half-extents
/// (as returned by [`TargetResolver::target_bounds`]).
///
/// The 9-variant vocabulary mirrors [`SceneAnchor`] exactly.
pub fn bounds_anchor_point(anchor: SceneAnchor, centre: [f32; 2], half: [f32; 2]) -> [f32; 2] {
    match anchor {
        SceneAnchor::TopLeft => [centre[0] - half[0], centre[1] - half[1]],
        SceneAnchor::Top => [centre[0], centre[1] - half[1]],
        SceneAnchor::TopRight => [centre[0] + half[0], centre[1] - half[1]],
        SceneAnchor::Left => [centre[0] - half[0], centre[1]],
        SceneAnchor::Center => centre,
        SceneAnchor::Right => [centre[0] + half[0], centre[1]],
        SceneAnchor::BottomLeft => [centre[0] - half[0], centre[1] + half[1]],
        SceneAnchor::Bottom => [centre[0], centre[1] + half[1]],
        SceneAnchor::BottomRight => [centre[0] + half[0], centre[1] + half[1]],
    }
}

/// Compute an actor anchor point from the frame environment (lazy resolution).
///
/// Reads `{actor}.at` (centre position) and `{actor}.size` from `env`,
/// then computes the world-space anchor point using [`bounds_anchor_point`].
/// Returns `None` if `{actor}.at` is not in the environment.
///
/// This is the lazy alternative to the now-removed `resolve_anchor_point`: it does not
/// require the `Timeline` or `TargetResolver` — just the per-frame env.
/// Because it reads the env (which reflects `always`-block overrides),
/// it produces up-to-date positions even when modifiers move the actor.
pub fn env_anchor_point(env: &Environment, actor: &str, anchor: SceneAnchor) -> Option<[f64; 2]> {
    let at = env.get(&format!("{actor}.at"))?;
    let centre = match at {
        Value::Vec2(c) => c,
        _ => return None,
    };
    let half = match env.get(&format!("{actor}.size")) {
        Some(Value::Vec2(s)) => [s[0] / 2.0, s[1] / 2.0],
        _ => [0.0, 0.0], // No size → point anchor (treat as zero area)
    };
    let point = bounds_anchor_point(
        anchor,
        [centre[0] as f32, centre[1] as f32],
        [half[0] as f32, half[1] as f32],
    );
    Some([point[0] as f64, point[1] as f64])
}

// -- Private helpers --

fn attach_point(place: CalloutPlace, centre: [f32; 2], half: [f32; 2]) -> [f32; 2] {
    match place {
        CalloutPlace::Top => [centre[0], centre[1] - half[1]],
        CalloutPlace::Bottom => [centre[0], centre[1] + half[1]],
        CalloutPlace::Left => [centre[0] - half[0], centre[1]],
        CalloutPlace::Right | CalloutPlace::Auto => [centre[0] + half[0], centre[1]],
    }
}

fn place_direction(place: CalloutPlace) -> [f32; 2] {
    match place {
        CalloutPlace::Top => [0.0, -1.0],
        CalloutPlace::Bottom => [0.0, 1.0],
        CalloutPlace::Left => [-1.0, 0.0],
        CalloutPlace::Right | CalloutPlace::Auto => [1.0, 0.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_anchor_point_returns_correct_world_point() {
        // A Rect at (200, 300) with size (40, 40): centre=(220, 320), half=(20, 20)
        let centre = [220.0, 320.0];
        let half = [20.0, 20.0];

        assert_eq!(bounds_anchor_point(SceneAnchor::Right, centre, half), [240.0, 320.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::Left, centre, half), [200.0, 320.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::Top, centre, half), [220.0, 300.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::Bottom, centre, half), [220.0, 340.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::Center, centre, half), [220.0, 320.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::TopLeft, centre, half), [200.0, 300.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::TopRight, centre, half), [240.0, 300.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::BottomLeft, centre, half), [200.0, 340.0]);
        assert_eq!(bounds_anchor_point(SceneAnchor::BottomRight, centre, half), [240.0, 340.0]);
    }

    #[test]
    fn scene_anchor_roundtrip() {
        let names = [
            (SceneAnchor::TopLeft, "top_left"),
            (SceneAnchor::Top, "top"),
            (SceneAnchor::TopRight, "top_right"),
            (SceneAnchor::Left, "left"),
            (SceneAnchor::Center, "center"),
            (SceneAnchor::Right, "right"),
            (SceneAnchor::BottomLeft, "bottom_left"),
            (SceneAnchor::Bottom, "bottom"),
            (SceneAnchor::BottomRight, "bottom_right"),
        ];
        for (anchor, name) in &names {
            assert_eq!(anchor.as_str(), *name);
            assert_eq!(SceneAnchor::from_str(name), Some(*anchor));
        }
        assert_eq!(SceneAnchor::from_str("invalid"), None);
    }

    #[test]
    fn env_anchor_point_uses_at_and_size_from_env() {
        // Rect at (100,100) size (40,40): centre=(100,100), half=(20,20)
        let mut env = Environment::new();
        env.set("r.at", Value::Vec2([100.0, 100.0]));
        env.set("r.size", Value::Vec2([40.0, 40.0]));

        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Right), Some([120.0, 100.0]));
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Top), Some([100.0, 80.0]));
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Center), Some([100.0, 100.0]));
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Left), Some([80.0, 100.0]));
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Bottom), Some([100.0, 120.0]));
    }

    #[test]
    fn env_anchor_point_reflects_overridden_at() {
        let mut env = Environment::new();
        env.set("r.at", Value::Vec2([200.0, 100.0])); // overridden from original (100,100)
        env.set("r.size", Value::Vec2([40.0, 40.0]));

        // right should be (200+20, 100) = (220, 100)
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Right), Some([220.0, 100.0]));
        // top should be (200, 100-20) = (200, 80)
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Top), Some([200.0, 80.0]));
    }

    #[test]
    fn env_anchor_point_returns_none_when_at_missing() {
        let env = Environment::new();
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Right), None);
    }

    #[test]
    fn env_anchor_point_uses_zero_half_when_size_missing() {
        let mut env = Environment::new();
        env.set("r.at", Value::Vec2([100.0, 200.0]));
        // No r.size — defaults to point anchor
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Center), Some([100.0, 200.0]));
        // With no half, all anchors collapse to centre
        assert_eq!(env_anchor_point(&env, "r", SceneAnchor::Right), Some([100.0, 200.0]));
    }
}
