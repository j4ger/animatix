//! Tests for the two-tier transform cache (`TransformCacheEntry::Timed` vs
//! `Static`): keyframe-free entities cache their node transform keyed on
//! parent state + layout position instead of time, so playback skips their
//! per-frame property sampling — while animated parents still move them.

use super::*;

const DIMS: SceneDimensions = SceneDimensions {
    width: 640,
    height: 360,
};

#[test]
fn static_entities_cache_transforms_across_times() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark" }
#0s
row: Row, at: (100, 100) {
  child: Rect, size: (40, 40), color: accent.primary, at: (10, 10), opacity: 1
}

#1s
row.at = (400, 100) [1s]
"#,
    );
    // Two distinct times: the child (no keyframes) must settle on a Static
    // entry, the row (keyframed `at`) on a Timed entry.
    let _ = timeline.evaluate(0.0, DIMS);
    let _ = timeline.evaluate(0.5, DIMS);

    let child = timeline.get_entity_id("child").expect("child entity");
    let row = timeline.get_entity_id("row").expect("row entity");
    let cache = timeline.eval_caches.transform_cache.borrow();
    assert!(
        matches!(
            cache[child.index()],
            Some(crate::timeline::scene_eval::TransformCacheEntry::Static(..))
        ),
        "keyframe-free child must use a Static (time-free) entry, got {:?}",
        cache[child.index()].map(|_| "Timed")
    );
    assert!(
        matches!(
            cache[row.index()],
            Some(crate::timeline::scene_eval::TransformCacheEntry::Timed(..))
        ),
        "keyframed row must use a Timed entry"
    );
}

#[test]
fn static_child_follows_animated_parent_across_times() {
    // The Static entry is keyed on the parent transform, so parent motion
    // changes the key and forces a recompute — a stale- transform regression
    // here would freeze the child at its first-frame position.
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark" }
#0s
row: Row, at: (100, 100) {
  child: Rect, size: (40, 40), color: accent.primary, at: (10, 10), opacity: 1
}

#1s
row.at = (400, 100) [1s]
"#,
    );

    let _ = timeline.evaluate(0.0, DIMS);
    let early = timeline.precise_bounds_for("child").expect("child bounds recorded at t=0");
    // Mid-transition (the #1s beat runs over 1s, so t=1.5s is halfway to
    // x=400): the row's transform changed, so the child's Static entry key
    // no longer matches and the transform must be recomputed with the new
    // parent state.
    let _ = timeline.evaluate(1.5, DIMS);
    let late = timeline.precise_bounds_for("child").expect("child bounds recorded at t=1.5");

    assert!(
        late.x0 > early.x0 + 100.0,
        "child must move with its animated parent: early x0={}, late x0={}",
        early.x0,
        late.x0
    );
}

#[test]
fn overrides_still_bypass_the_transform_cache() {
    // A modifier writing the child's position must win over any cached
    // transform, exactly as before the Static tier existed.
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark" }
#0s
row: Row, at: (100, 100) {
  child: Rect, size: (40, 40), color: accent.primary, at: (10, 10), opacity: 1
}

always {
  child.at = (300, 10)
}
"#,
    );
    let _ = timeline.evaluate(0.5, DIMS);
    let bounds = timeline.precise_bounds_for("child").expect("child bounds recorded");
    assert!(bounds.x0 >= 300.0, "override position must apply, got x0={}", bounds.x0);
}

fn group_scale_fixture(timing: &str) -> Timeline {
    build_timeline(&format!(
        r#"
config {{ colorscheme: "editorial-dark" }}
#0s
parent: Group, at: (200, 150), opacity: 1 {{
    direct: Rect, at: (80, 0), size: (40, 20), color: accent.primary, opacity: 1
    nested: Group, at: (0, 60), opacity: 1 {{
        grandchild: Rect, at: (40, 0), size: (20, 40), color: accent.primary, opacity: 1
    }}
}}
#1s
parent.scale = 0.25 [{timing}]
"#
    ))
}

fn assert_bounds_scaled_about_parent(actual: kurbo::Rect, baseline: kurbo::Rect, scale: f64) {
    for (actual, original, pivot) in [
        (actual.x0, baseline.x0, 200.0),
        (actual.x1, baseline.x1, 200.0),
        (actual.y0, baseline.y0, 150.0),
        (actual.y1, baseline.y1, 150.0),
    ] {
        let expected = pivot + (original - pivot) * scale;
        assert!(
            (actual - expected).abs() < 1e-3,
            "child bounds must inherit parent scale {scale}: expected {expected}, got {actual}"
        );
    }
}

#[test]
fn group_scale_duration_reaches_children_and_grandchildren() {
    let timeline = group_scale_fixture("1s");
    let _ = timeline.evaluate(0.0, DIMS);
    let labels = ["direct", "grandchild"];
    let baseline = labels.map(|label| timeline.precise_bounds_for(label).expect("initial bounds"));

    // Evaluate the rendered frame, not just the transform helper: this also
    // guards recursive child traversal and the static-child transform cache.
    let _ = timeline.evaluate(1.5, DIMS);
    let halfway = timeline.precise_bounds_for("direct").expect("mid-animation bounds");
    let scale = halfway.width() / baseline[0].width();
    assert!(scale > 0.25 && scale < 1.0, "duration must have an intermediate scale: {scale}");
    for (label, original) in labels.into_iter().zip(baseline) {
        assert_bounds_scaled_about_parent(
            timeline.precise_bounds_for(label).expect("mid-animation descendant bounds"),
            original,
            scale,
        );
    }

    // Non-monotonic sampling catches stale parent transforms when scrubbing.
    for (time, scale) in [(2.5, 0.25), (0.5, 1.0), (2.0, 0.25)] {
        let _ = timeline.evaluate(time, DIMS);
        for (label, original) in labels.into_iter().zip(baseline) {
            assert_bounds_scaled_about_parent(
                timeline.precise_bounds_for(label).expect("descendant bounds"),
                original,
                scale,
            );
        }
    }
}

#[test]
fn group_scale_instant_changes_children_only_at_the_keyframe() {
    let timeline = group_scale_fixture("0s");
    let _ = timeline.evaluate(0.0, DIMS);
    let labels = ["direct", "grandchild"];
    let baseline = labels.map(|label| timeline.precise_bounds_for(label).expect("initial bounds"));

    for (time, scale) in [
        (0.5, 1.0),
        (0.999, 1.0),
        (1.0, 0.25),
        (2.0, 0.25),
        (0.5, 1.0),
    ] {
        let _ = timeline.evaluate(time, DIMS);
        for (label, original) in labels.into_iter().zip(baseline) {
            assert_bounds_scaled_about_parent(
                timeline.precise_bounds_for(label).expect("descendant bounds"),
                original,
                scale,
            );
        }
    }
}

// Keep these stages separately named: the CI evidence records individual test
// results even when the harness does not retain the assertion output.
#[test]
fn group_scale_diagnostic_fixture_builds() {
    let _ = group_scale_fixture("1s");
    let _ = group_scale_fixture("0s");
}

#[test]
fn group_scale_diagnostic_duration_track() {
    let timeline = group_scale_fixture("1s");
    let parent = timeline.get_entity_id("parent").expect("parent entity");
    let scale = &timeline.world.geometry(parent).scale;
    assert_eq!(scale.get(0, 1.0), 1.0);
    assert_eq!(scale.get(500, 1.0), 1.0);
    assert!(scale.get(1500, 1.0) > 0.25 && scale.get(1500, 1.0) < 1.0);
    assert_eq!(scale.get(2000, 1.0), 0.25);
}

#[test]
fn group_scale_diagnostic_instant_track() {
    let timeline = group_scale_fixture("0s");
    let parent = timeline.get_entity_id("parent").expect("parent entity");
    let scale = &timeline.world.geometry(parent).scale;
    assert_eq!(scale.get(0, 1.0), 1.0);
    assert_eq!(scale.get(999, 1.0), 1.0);
    assert_eq!(scale.get(1000, 1.0), 0.25);
}

#[test]
fn group_scale_diagnostic_initial_descendant_bounds() {
    let timeline = group_scale_fixture("1s");
    let _ = timeline.evaluate(0.0, DIMS);
    for label in ["direct", "grandchild"] {
        let bounds = timeline.precise_bounds_for(label).expect("initial descendant bounds");
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }
}

#[test]
fn group_scale_diagnostic_parent_pivot() {
    let timeline = group_scale_fixture("1s");
    for time_ms in [0, 500, 1500, 2000] {
        let transform = timeline
            .resolve_actor_world_transform("parent", time_ms, [640.0, 360.0])
            .expect("parent transform");
        let pivot = transform * kurbo::Point::ORIGIN;
        assert!((pivot.x - 200.0).abs() < 1e-3);
        assert!((pivot.y - 150.0).abs() < 1e-3);
    }
}

fn group_scale_diagnostic_descendant_transform(label: &str) {
    let timeline = group_scale_fixture("1s");
    let early = timeline
        .resolve_actor_world_transform(label, 0, [640.0, 360.0])
        .expect("initial descendant transform");
    let late = timeline
        .resolve_actor_world_transform(label, 2000, [640.0, 360.0])
        .expect("final descendant transform");
    let expected = kurbo::Affine::translate((200.0, 150.0))
        * kurbo::Affine::scale(0.25)
        * kurbo::Affine::translate((-200.0, -150.0))
        * early;
    for (actual, expected) in late.as_coeffs().into_iter().zip(expected.as_coeffs()) {
        assert!((actual - expected).abs() < 1e-3);
    }
}

#[test]
fn group_scale_diagnostic_direct_transform() {
    group_scale_diagnostic_descendant_transform("direct");
}

#[test]
fn group_scale_diagnostic_grandchild_transform() {
    group_scale_diagnostic_descendant_transform("grandchild");
}

fn group_scale_diagnostic_bounds_ratio(label: &str) {
    let timeline = group_scale_fixture("1s");
    let _ = timeline.evaluate(0.0, DIMS);
    let early = timeline.precise_bounds_for(label).expect("initial bounds");
    let _ = timeline.evaluate(2.0, DIMS);
    let late = timeline.precise_bounds_for(label).expect("final bounds");
    assert!((late.width() / early.width() - 0.25).abs() < 1e-3);
    assert!((late.height() / early.height() - 0.25).abs() < 1e-3);
}

#[test]
fn group_scale_diagnostic_direct_rendered_size() {
    group_scale_diagnostic_bounds_ratio("direct");
}

#[test]
fn group_scale_diagnostic_grandchild_rendered_size() {
    group_scale_diagnostic_bounds_ratio("grandchild");
}

fn instant_parent_scale(time_ms: u64) -> f32 {
    let timeline = group_scale_fixture("0s");
    let parent = timeline.get_entity_id("parent").expect("parent entity");
    timeline.world.geometry(parent).scale.get(time_ms, 1.0)
}

#[test]
fn group_scale_diagnostic_instant_initial_default_is_one() {
    assert_eq!(instant_parent_scale(0), 1.0);
}

#[test]
fn group_scale_diagnostic_instant_before_change_is_one() {
    assert_eq!(instant_parent_scale(999), 1.0);
}

#[test]
fn group_scale_diagnostic_instant_target_is_quarter() {
    assert_eq!(instant_parent_scale(1000), 0.25);
}

#[test]
fn group_scale_diagnostic_explicit_seed_instant_track() {
    let timeline = build_timeline(
        r#"
#0s
parent: Group, at: (200, 150), scale: 1, opacity: 1 {
    direct: Rect, size: (40, 20), opacity: 1
}
#1s
parent.scale = 0.25 [0s]
"#,
    );
    let parent = timeline.get_entity_id("parent").expect("parent entity");
    let scale = &timeline.world.geometry(parent).scale;
    assert_eq!(scale.get(0, 1.0), 1.0);
    assert_eq!(scale.get(999, 1.0), 1.0);
    assert_eq!(scale.get(1000, 1.0), 0.25);
}

fn grandchild_bounds_at(time: f64) -> Option<kurbo::Rect> {
    let timeline = group_scale_fixture("1s");
    let _ = timeline.evaluate(time, DIMS);
    timeline.precise_bounds_for("grandchild")
}

#[test]
fn group_scale_diagnostic_grandchild_initial_bounds_exist() {
    assert!(grandchild_bounds_at(0.0).is_some());
}

#[test]
fn group_scale_diagnostic_grandchild_initial_bounds_nonzero() {
    // Existence has its own test; distinguish missing from degenerate bounds.
    if let Some(bounds) = grandchild_bounds_at(0.0) {
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }
}

#[test]
fn group_scale_diagnostic_grandchild_final_bounds_exist() {
    assert!(grandchild_bounds_at(2.0).is_some());
}

#[test]
fn group_scale_diagnostic_grandchild_final_bounds_nonzero() {
    if let Some(bounds) = grandchild_bounds_at(2.0) {
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }
}

#[test]
fn group_scale_diagnostic_grandchild_registered_in_children() {
    let timeline = group_scale_fixture("1s");
    let parent = timeline.get_entity_id("parent").expect("parent");
    let nested = timeline.get_entity_id("nested").expect("nested");
    let grandchild = timeline.get_entity_id("grandchild").expect("grandchild");
    assert!(timeline.world.hierarchy(parent).children.contains(&nested));
    assert!(timeline.world.hierarchy(nested).children.contains(&grandchild));
    assert_eq!(timeline.world.hierarchy(grandchild).parent, Some(nested));
}

#[test]
fn group_scale_diagnostic_grandchild_visible_from_zero() {
    let timeline = group_scale_fixture("1s");
    for label in ["parent", "nested", "grandchild"] {
        let id = timeline.get_entity_id(label).expect("fixture entity");
        let meta = timeline.world.meta(id);
        assert!(meta.visible);
        assert_eq!(meta.first_seen_ms, 0);
        assert_eq!(timeline.world.style(id).opacity.get(0, 1.0), 1.0);
    }
}

#[test]
fn group_scale_diagnostic_grandchild_shape_registered() {
    let timeline = group_scale_fixture("1s");
    let id = timeline.get_entity_id("grandchild").expect("grandchild");
    assert_eq!(timeline.world.actor_type(id), "Rect");
    assert!(timeline.world.caps(id).is_shape);
    assert!(timeline.behavior_for_node(id, 0).is_some());
    assert_eq!(timeline.world.geometry(id).size.get(0, [0.0; 2]), [10.0, 20.0]);
}
