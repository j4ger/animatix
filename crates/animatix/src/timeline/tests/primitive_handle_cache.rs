//! Tests for the per-entity primitive-handle cache
//! (`timeline_impl::primitive_cache`): the cached answer must match a fresh
//! registry lookup, survive repeat frames, and be bypassed while a
//! cross-type morph picks a different primitive than the identity type.

use super::*;

fn build_timeline(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).output
}

#[test]
#[allow(deprecated)]
fn handle_cache_serves_the_registry_answer() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark" }
#0s
r: Rect, size: (50, 50), color: accent.primary
e: Ellipse, size: (50, 50), color: accent.secondary
l: Line, from: (0, 0), to: (100, 100)
t: Text, text: "hi"
"#,
    );

    for label in ["r", "e", "l", "t"] {
        let id = timeline.get_entity_id(label).expect("entity exists");
        for time_ms in [0u64, 250, 500] {
            let handle = timeline
                .cached_primitive_for(id, time_ms)
                .unwrap_or_else(|| panic!("no primitive for '{label}'"));
            let by_name = crate::primitives::find_primitive(timeline.world.actor_type(id))
                .unwrap_or_else(|| panic!("registry miss for '{}'", timeline.world.actor_type(id)));
            assert_eq!(
                handle.as_dyn().type_name(),
                by_name.type_name(),
                "cached handle for '{label}' at {time_ms}ms must match the registry answer"
            );
        }
    }

    // Two evaluations populate the cache; every entity must have an entry
    // keyed by its actor type.
    let dims = SceneDimensions {
        width: 640,
        height: 360,
    };
    let _ = timeline.evaluate(0.0, dims);
    let _ = timeline.evaluate(0.1, dims);
    let cache = timeline.eval_caches.primitive_handles.borrow();
    for label in ["r", "e", "l", "t"] {
        let id = timeline.get_entity_id(label).expect("entity exists");
        let entry = cache
            .get(id.index())
            .and_then(|e| e.as_ref())
            .unwrap_or_else(|| panic!("'{label}' has no cached handle after evaluation"));
        assert_eq!(
            entry.0.as_ref(),
            timeline.world.actor_type(id),
            "cache entry must be keyed by the actor type it resolved"
        );
    }
}

#[test]
fn cross_type_morph_frames_bypass_the_identity_handle() {
    // Same-label re-declaration with a different type is the morph syntax;
    // after the morph beat the per-frame `shape_type` value (Ellipse), not
    // the identity type, must pick the primitive.
    let timeline = build_timeline(
        r#"
config { resolution: (640, 360), duration: 3 }
box: Rect, size: (140, 140), color: accent.primary, at: (320, 180)

#0.3s
fade-in box [250ms, ease: ease-out]

#1.5s
box: Ellipse, size: (140, 140), color: accent.success [900ms, strategy: match]
"#,
    );
    let id = timeline.get_entity_id("box").expect("box entity");

    let before = timeline.cached_primitive_for(id, 500).expect("pre-morph primitive");
    assert_eq!(before.as_dyn().type_name(), "Rect");

    let after = timeline.cached_primitive_for(id, 2_400).expect("post-morph primitive");
    assert_eq!(after.as_dyn().type_name(), "Ellipse");

    // The morph bypass must not poison the identity cache: resolving the
    // pre-morph time again still answers Rect.
    let again = timeline
        .cached_primitive_for(id, 500)
        .expect("pre-morph primitive on second lookup");
    assert_eq!(again.as_dyn().type_name(), "Rect");
}

#[test]
fn invalidation_clears_the_handle_cache() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark" }
#0s
r: Rect, size: (50, 50), color: accent.primary
"#,
    );
    let id = timeline.get_entity_id("r").expect("entity");
    let dims = SceneDimensions {
        width: 640,
        height: 360,
    };
    let _ = timeline.evaluate(0.0, dims);
    assert!(
        timeline
            .eval_caches
            .primitive_handles
            .borrow()
            .get(id.index())
            .is_some_and(|e| e.is_some()),
        "handle cached after evaluation"
    );
    timeline.invalidate_frame_cache();
    assert!(
        timeline
            .eval_caches
            .primitive_handles
            .borrow()
            .get(id.index())
            .is_none_or(|e| e.is_none()),
        "invalidation clears the handle cache"
    );
}
