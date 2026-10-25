//! Tests for the two-tier transform cache (`TransformCacheEntry::Timed` vs
//! `Static`): keyframe-free entities cache their node transform keyed on
//! parent state + layout position instead of time, so playback skips their
//! per-frame property sampling — while animated parents still move them.

use super::*;

fn build_timeline(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).output
}

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
