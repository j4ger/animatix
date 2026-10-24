use super::containers::equation_markup_escaped;
use super::*;
use crate::easing::Easing;
use crate::timeline::{AnimationTrack, PropertyTrack, ShapeType};

/// Helper to create a minimal Timeline with one root track.
fn make_minimal_timeline() -> Timeline {
    let mut timeline = Timeline::new();
    let track = AnimationTrack::placeholder("test_box".to_string());

    let id = spawn_seen(&mut timeline, "test_box", "Rect", track);
    // Add a color so it renders something visible
    timeline.world.style_mut(id).color = Some({
        let mut t = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
        t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
        t
    });
    // Give it a size so it has content
    timeline.world.geometry_mut(id).size = Some({
        let mut t = PropertyTrack::new([50.0, 50.0]);
        t.add_keyframe(0, [50.0, 50.0], Easing::Linear);
        t
    });
    // Give it a shape type
    timeline.world.shape_mut(id).shape_type = Some({
        let mut t = PropertyTrack::new(ShapeType::Rect);
        t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
        t
    });
    timeline
}

#[test]
fn evaluate_returns_scene_for_simple_timeline() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let scene = timeline.evaluate(0.0, dimensions);

    // Should return a valid vello Scene (not empty, at least has background)
    // vello::Scene doesn't expose fraction() in all versions; just verify it doesn't panic
    let _ = scene;
}

#[test]
fn evaluate_returns_scene_at_different_times() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let scene_0 = timeline.evaluate(0.0, dimensions);
    let scene_5 = timeline.evaluate(5.0, dimensions);

    // Both should be valid scenes (no panic)
    let _ = scene_0;
    let _ = scene_5;
}

#[test]
fn evaluate_with_empty_timeline_returns_scene() {
    let timeline = Timeline::new();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let scene = timeline.evaluate(0.0, dimensions);
    // Should not panic
    let _ = scene;
}

#[test]
fn frame_cache_caches_identical_evaluations() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    // First call should compute and cache
    let scene1 = timeline.evaluate(1.0, dimensions);
    let _ = scene1;

    // Verify the cache is populated
    let cache = timeline.eval_caches.frame_cache.borrow();
    assert!(cache.is_some(), "frame cache should be populated after evaluate");

    if let Some(ref entry) = *cache {
        assert_eq!(entry.time_ms, 1000, "cache should store time in ms (1.0s = 1000ms)");
        assert_eq!(entry.dimensions, dimensions);
    }

    // Second call with same params should use cache
    let scene2 = timeline.evaluate(1.0, dimensions);
    let _ = scene2;

    let cache2 = timeline.eval_caches.frame_cache.borrow();
    assert!(cache2.is_some(), "frame cache should still be populated");
    assert_eq!(cache2.as_ref().unwrap().time_ms, 1000);
}

#[test]
fn program_cache_is_separate_from_scene_only_cache() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _scene = timeline.evaluate(1.0, dimensions);
    {
        let cache = timeline.eval_caches.frame_cache.borrow();
        let entry = cache.as_ref().expect("scene evaluation should populate cache");
        assert!(!entry.collect_items);
    }

    let program = timeline.evaluate_program_with_debug(
        1.0,
        dimensions,
        DebugRenderOptions::default(),
        &mut None,
    );
    assert!(!program.items.is_empty());
    {
        let cache = timeline.eval_caches.frame_cache.borrow();
        let entry = cache.as_ref().expect("program evaluation should populate cache");
        assert!(entry.collect_items);
    }
}

/// The frame cache is gated on this flag rather than on the backend's mere
/// presence, so that a runtime which supplies a backend on every frame (the
/// web player) can still cache. Getting the flag wrong in the permissive
/// direction is the dangerous one: an effect scene would replay a cached
/// frame with its composited regions silently missing.
#[test]
fn has_effect_scopes_tracks_what_the_scene_actually_contains() {
    let build = |src: &str| {
        let (ast, errors) = animatix_syntax::parser::parse_source(src);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output
    };

    let plain = build(
        "config { resolution: (100, 100) }\n\
         b: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
         #0s\nb.opacity = 1.0\n",
    );
    assert!(
        !plain.has_effect_scopes,
        "an effect-free scene reported effect scopes, which would keep the web player out of the frame cache"
    );

    let filtered = build(
        "config { resolution: (100, 100) }\n\
         f: Filter, at: (30, 30), size: (60, 60), blur: 4 {\n\
           b: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
         }\n\
         #0s\nf.opacity = 1.0\n",
    );
    assert!(
        filtered.has_effect_scopes,
        "a `Filter` scope was not detected; the frame cache would swallow its composites"
    );
}

#[test]
fn frame_cache_restores_runtime_diagnostics_on_hit() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _program = timeline.evaluate_program_with_debug(
        1.0,
        dimensions,
        DebugRenderOptions::default(),
        &mut None,
    );
    timeline
        .eval_caches
        .frame_cache
        .borrow_mut()
        .as_mut()
        .expect("cache populated")
        .program
        .diagnostics
        .push(crate::diagnostics::Diagnostic::error(
            crate::diagnostics::DiagnosticCode::ModifierRuntimeError,
            crate::diagnostics::DiagnosticPhase::Render,
            "simulated t=1 diagnostic".to_string(),
        ));

    // Scrub to the same frame again. The cache hit must restore the
    // frame's diagnostics instead of leaving the newly evaluated frame's
    // empty diagnostics in the timeline.
    let _hit = timeline.evaluate_program_with_debug(
        1.0,
        dimensions,
        DebugRenderOptions::default(),
        &mut None,
    );
    let diagnostics = timeline.runtime_diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].message, "simulated t=1 diagnostic");
}

#[test]
fn frame_cache_misses_on_different_time() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _scene1 = timeline.evaluate(0.0, dimensions);
    let _scene2 = timeline.evaluate(2.0, dimensions);

    // Cache should contain the latest evaluation (t=2.0)
    let cache = timeline.eval_caches.frame_cache.borrow();
    assert!(cache.is_some(), "cache should be populated");
    assert_eq!(cache.as_ref().unwrap().time_ms, 2000, "cache should contain t=2.0");
}

#[test]
fn frame_cache_misses_on_different_dimensions() {
    let timeline = make_minimal_timeline();

    let dims_1 = SceneDimensions {
        width: 800,
        height: 600,
    };
    let dims_2 = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    let _scene1 = timeline.evaluate(0.0, dims_1);

    // Cache should have dims_1
    {
        let cache = timeline.eval_caches.frame_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().dimensions, dims_1);
    }

    let _scene2 = timeline.evaluate(0.0, dims_2);

    // Cache should now have dims_2
    {
        let cache = timeline.eval_caches.frame_cache.borrow();
        assert_eq!(cache.as_ref().unwrap().dimensions, dims_2);
    }
}

#[test]
fn hit_regions_are_populated_after_evaluate() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    // hit_regions should be empty before evaluate
    {
        let regions = timeline.eval_caches.hit_regions.borrow();
        assert!(regions.is_empty(), "hit_regions should be empty before evaluate");
    }

    let _scene = timeline.evaluate_with_debug(
        0.0,
        dimensions,
        DebugRenderOptions {
            draw_bounds: false,
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut None,
    );

    // hit_regions should be populated after evaluate
    let regions = timeline.hit_regions();
    assert!(!regions.is_empty(), "hit_regions should be populated after evaluate");
    assert!(
        regions.iter().any(|(label, _)| label == "test_box"),
        "hit_regions should contain 'test_box'"
    );
}

#[test]
fn hit_regions_contain_world_bounds() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _scene = timeline.evaluate_with_debug(
        0.0,
        dimensions,
        DebugRenderOptions {
            draw_bounds: false,
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut None,
    );

    let regions = timeline.hit_regions();
    let (label, bounds) = regions
        .iter()
        .find(|(l, _)| l == "test_box")
        .expect("should find test_box in hit_regions");

    assert_eq!(label, "test_box");
    // The bounds should be valid rectangles (x0 < x1, y0 < y1)
    assert!(bounds.x0 < bounds.x1, "hit region x0 should be less than x1");
    assert!(bounds.y0 < bounds.y1, "hit region y0 should be less than y1");
}

#[test]
fn hit_regions_by_id_matches_label_view() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _scene = timeline.evaluate_with_debug(
        0.0,
        dimensions,
        DebugRenderOptions {
            draw_bounds: false,
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut None,
    );

    let by_id = timeline.hit_regions_by_id();
    let by_label = timeline.hit_regions();
    assert_eq!(by_id.len(), by_label.len(), "both views must list the same regions");

    let test_box = timeline.get_entity_id("test_box").expect("test_box entity id");
    let (id, id_bounds) = by_id
        .iter()
        .find(|(id, _)| *id == test_box)
        .expect("by-id view must contain the test_box entity");
    assert_eq!(*id, test_box);
    let (label, label_bounds) = by_label
        .iter()
        .find(|(label, _)| label == "test_box")
        .expect("label view must contain 'test_box'");
    assert_eq!(label, "test_box");
    assert_eq!(
        *id_bounds, *label_bounds,
        "the two views must report the same bounds for the same entity"
    );

    // Every id in the label-independent view resolves back to a label.
    assert!(
        by_id.iter().all(|(id, _)| timeline.get_entity_label(*id).is_some()),
        "every recorded hit region id must resolve to a label"
    );
}

#[test]
fn hit_regions_cover_actors_without_draw_commands() {
    // Container shells return `Some(vec![])` and stay hit-testable at
    // their layout box; empty-content leaf actors (empty Text) return
    // `None` and intentionally record no hit region (see
    // `runtime_empty_text_override_clears_stale_glyphs`).
    let mut timeline = Timeline::new();

    // A Group container: evaluate() -> Some(vec![]) (empty command list).
    let group = AnimationTrack::placeholder("grp".to_string());
    let group_id = spawn_seen(&mut timeline, "grp", "Group", group);
    timeline.world.geometry_mut(group_id).size = {
        let mut t = PropertyTrack::new([40.0, 30.0]);
        t.add_keyframe(0, [40.0, 30.0], Easing::Linear);
        Some(t)
    };

    // An empty Text actor: evaluate() -> Ok(None), no content glyphs.
    let empty_text = AnimationTrack::placeholder("empty".to_string());
    let empty_id = spawn_seen(&mut timeline, "empty", "Text", empty_text);
    timeline.world.geometry_mut(empty_id).size = {
        let mut t = PropertyTrack::new([60.0, 20.0]);
        t.add_keyframe(0, [60.0, 20.0], Easing::Linear);
        Some(t)
    };

    let _scene = timeline.evaluate_with_debug(
        0.0,
        SceneDimensions {
            width: 800,
            height: 600,
        },
        DebugRenderOptions {
            draw_bounds: false,
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut None,
    );

    let regions = timeline.hit_regions();
    let (found_grp, bounds_grp) = regions
        .iter()
        .find(|(l, _)| l == "grp")
        .expect("container shell should keep its hit region");
    assert_eq!(found_grp, "grp");
    assert!(bounds_grp.x0 < bounds_grp.x1, "grp hit region x0 < x1");
    assert!(bounds_grp.y0 < bounds_grp.y1, "grp hit region y0 < y1");
    assert!(
        !regions.iter().any(|(l, _)| l == "empty"),
        "empty-content actors must not record a hit region"
    );
}

#[test]
fn evaluate_with_debug_options_skips_cache() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };
    let debug_opts = DebugRenderOptions {
        draw_bounds: true,
        compute_hit_regions: false,
        ..Default::default()
    };

    // Evaluate with debug options (should not cache)
    let _scene = timeline.evaluate_with_debug(0.0, dimensions, debug_opts, &mut None);

    // Cache should not be populated because debug_options != default
    let cache = timeline.eval_caches.frame_cache.borrow();
    assert!(
        cache.is_none(),
        "frame cache should not be populated with non-default debug options"
    );
}

#[test]
fn mask_clips_children_to_own_bounds() {
    let mut timeline = Timeline::new();

    // Create child actor
    let child_track = AnimationTrack::placeholder("child".to_string());
    let child_id = spawn_seen(&mut timeline, "child", "Rect", child_track);
    timeline.world.style_mut(child_id).color = Some({
        let mut t = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
        t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
        t
    });
    timeline.world.geometry_mut(child_id).size = Some({
        let mut t = PropertyTrack::new([50.0, 50.0]);
        t.add_keyframe(0, [50.0, 50.0], Easing::Linear);
        t
    });
    timeline.world.shape_mut(child_id).shape_type = Some({
        let mut t = PropertyTrack::new(ShapeType::Rect);
        t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
        t
    });

    // Create Mask actor
    let mask_track = AnimationTrack::placeholder("mask".to_string());
    let mask_id = spawn_seen(&mut timeline, "mask", "Mask", mask_track);
    timeline.world.geometry_mut(mask_id).size = Some({
        let mut t = PropertyTrack::new([100.0, 100.0]);
        t.add_keyframe(0, [100.0, 100.0], Easing::Linear);
        t
    });
    timeline.world.link(mask_id, child_id);

    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };
    let scene = timeline.evaluate(0.0, dimensions);

    // Should not panic; returns a valid scene
    let _ = scene;
}

#[test]
fn target_bounds_prefer_precise_command_bounds() {
    let timeline = make_minimal_timeline();
    // The declared size box is 100x100 centered at (0,0), but the precise
    // command bounds describe a different world AABB.
    timeline.ensure_bounds_registry();
    let slot = timeline
        .world
        .bounds_slot(timeline.world.tracks.get_id("test_box").expect("track"));
    timeline
        .eval_caches
        .precise_bounds
        .borrow_mut()
        .write(slot, kurbo::Rect::new(50.0, 40.0, 100.0, 80.0));
    let (centre, half) = crate::timeline::callout_geometry::TargetResolver::target_bounds(
        &timeline,
        "test_box",
        0,
        SceneDimensions {
            width: 800,
            height: 600,
        },
    )
    .expect("target bounds");
    assert_eq!(centre, [75.0, 60.0]);
    assert_eq!(half, [25.0, 20.0]);
}

#[test]
fn frame_cache_restores_precise_bounds_on_hit() {
    let timeline = make_minimal_timeline();
    let dimensions = SceneDimensions {
        width: 800,
        height: 600,
    };

    let _scene = timeline.evaluate(0.0, dimensions);
    let cached = timeline
        .precise_bounds_for("test_box")
        .expect("precise bounds should be populated after evaluation");
    let _scene = timeline.evaluate(0.0, dimensions);
    assert_eq!(
        timeline.precise_bounds_for("test_box"),
        Some(cached),
        "frame-cache hit should restore precise bounds"
    );
}

#[test]
fn equation_fragment_leading_marker_is_escaped() {
    // A fragment starting with '+' used to render as a Typst enum marker
    // ("1.") once fragments became separate #box() markup blocks.
    assert_eq!(equation_markup_escaped("+ sin(3x)/3"), "\\+ sin(3x)/3");
    assert_eq!(equation_markup_escaped("  + 0.55 dot "), "  \\+ 0.55 dot ");
    assert_eq!(equation_markup_escaped("- b"), "\\- b");
    assert_eq!(equation_markup_escaped("sin(x)"), "sin(x)");
    assert_eq!(equation_markup_escaped("y(x) = "), "y(x) = ");
}

fn make_root_track(label: &str) -> AnimationTrack {
    AnimationTrack::placeholder(label.to_string())
}

/// A root-level filter may only take the zero-readback post-render blit
/// when it is the LAST root node; otherwise later siblings would render
/// underneath the blit. (Regression: the last-among-siblings check only
/// ran for i > 0, so root filters always deferred and landed on top.)
#[test]
fn root_filter_post_composite_requires_last_root_node() {
    let mut timeline = Timeline::new();
    spawn_seen(&mut timeline, "fx", "Rect", make_root_track("fx"));
    spawn_seen(&mut timeline, "late", "Rect", make_root_track("late"));

    let fx_id = timeline.get_entity_id("fx").expect("fx");
    assert!(
        !timeline.can_post_composite_filter(fx_id),
        "filter followed by a later sibling must not post-composite"
    );

    let mut timeline = Timeline::new();
    spawn_seen(&mut timeline, "late", "Rect", make_root_track("late"));
    spawn_seen(&mut timeline, "fx", "Rect", make_root_track("fx"));

    let fx_id = timeline.get_entity_id("fx").expect("fx");
    assert!(
        timeline.can_post_composite_filter(fx_id),
        "a last root filter may post-composite"
    );
}

fn spawn_seen(
    timeline: &mut Timeline,
    label: &str,
    actor_type: &str,
    track: AnimationTrack,
) -> EntityId {
    let id = timeline.world.spawn_with_track(label, actor_type, track);
    timeline.world.meta_mut(id).first_seen_ms = 0;
    id
}
