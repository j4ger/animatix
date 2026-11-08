use super::containers::equation_markup_escaped;
use super::*;
use crate::easing::Easing;
use crate::timeline::{PropertyTrack, ShapeType};

/// Helper to create a minimal Timeline with one root track.
fn make_minimal_timeline() -> Timeline {
    let mut timeline = Timeline::new();

    let id = spawn_seen(&mut timeline, "test_box", "Rect");
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
    timeline.world.ensure_shape(id).shape_type = Some({
        let mut t = PropertyTrack::new(ShapeType::Rect);
        t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
        t
    });
    timeline
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
    let group_id = spawn_seen(&mut timeline, "grp", "Group");
    timeline.world.geometry_mut(group_id).size = {
        let mut t = PropertyTrack::new([40.0, 30.0]);
        t.add_keyframe(0, [40.0, 30.0], Easing::Linear);
        Some(t)
    };

    // An empty Text actor: evaluate() -> Ok(None), no content glyphs.
    let empty_id = spawn_seen(&mut timeline, "empty", "Text");
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
    let child_id = spawn_seen(&mut timeline, "child", "Rect");
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
    timeline.world.ensure_shape(child_id).shape_type = Some({
        let mut t = PropertyTrack::new(ShapeType::Rect);
        t.add_keyframe(0, ShapeType::Rect, Easing::Linear);
        t
    });

    // Create Mask actor
    let mask_id = spawn_seen(&mut timeline, "mask", "Mask");
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

/// A root-level filter may only take the zero-readback post-render blit
/// when it is the LAST root node; otherwise later siblings would render
/// underneath the blit. (Regression: the last-among-siblings check only
/// ran for i > 0, so root filters always deferred and landed on top.)
#[test]
fn root_filter_post_composite_requires_last_root_node() {
    let mut timeline = Timeline::new();
    spawn_seen(&mut timeline, "fx", "Rect");
    spawn_seen(&mut timeline, "late", "Rect");

    let fx_id = timeline.get_entity_id("fx").expect("fx");
    assert!(
        !timeline.can_post_composite_filter(fx_id),
        "filter followed by a later sibling must not post-composite"
    );

    let mut timeline = Timeline::new();
    spawn_seen(&mut timeline, "late", "Rect");
    spawn_seen(&mut timeline, "fx", "Rect");

    let fx_id = timeline.get_entity_id("fx").expect("fx");
    assert!(
        timeline.can_post_composite_filter(fx_id),
        "a last root filter may post-composite"
    );
}

fn spawn_seen(timeline: &mut Timeline, label: &str, actor_type: &str) -> EntityId {
    let id = timeline.world.spawn(label, actor_type);
    timeline.world.meta_mut(id).first_seen_ms = 0;
    id
}

// Per-subtree static-cache gating and the clone-free presentation path

fn build_timeline(src: &str) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(src);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new()).output
}

/// A backend supplied on every frame (as the web player does) without any
/// GPU work. It only needs to exist so `backend_can_run` is true; the default
/// trait methods make the filter fall back to inline children harmlessly.
struct StubBackend;

impl crate::timeline::effects::FilterBackend for StubBackend {
    fn render_scene_to_image_gpu_filtered(
        &mut self,
        _scene: &vello::Scene,
        _dimensions: SceneDimensions,
        _region: Option<crate::timeline::effects::EffectRegion>,
        _chain: &crate::timeline::effects::EffectChain,
    ) -> Result<crate::timeline::image::SceneImage, String> {
        Err("stub backend does not render".to_string())
    }
}

/// Compare two encoded vello scenes stream-by-stream (vello does not derive
/// `PartialEq`/`Debug` for `Scene`/`Encoding`, but every stream is public and
/// its element type is `PartialEq`).
fn assert_same_encoding(a: &vello::Scene, b: &vello::Scene) {
    let ea = a.encoding();
    let eb = b.encoding();
    assert!(ea.path_data == eb.path_data, "path_data stream differs");
    assert!(ea.draw_data == eb.draw_data, "draw_data stream differs");
    assert!(ea.path_tags == eb.path_tags, "path_tags stream differs");
    assert!(ea.draw_tags == eb.draw_tags, "draw_tags stream differs");
    assert!(ea.transforms == eb.transforms, "transforms stream differs");
    assert!(ea.styles == eb.styles, "styles stream differs");
    assert_eq!(ea.n_paths, eb.n_paths);
    assert_eq!(ea.n_path_segments, eb.n_path_segments);
    assert_eq!(ea.n_clips, eb.n_clips);
    assert_eq!(ea.n_open_clips, eb.n_open_clips);
    assert_eq!(ea.flags, eb.flags);
}

/// With a backend present, the whole-frame `backend_can_run` flag is true for
/// a filtered scene. The old `allow_static` gate was that whole-frame flag, so
/// it disabled the static-subtree cache for *every* root. The per-root gate
/// must still cache a fully-static root while vetoing the effect-scope root.
#[test]
fn static_root_caches_when_another_root_has_an_effect_scope() {
    let timeline = build_timeline(
        "config { resolution: (200, 200) }\n\
         #0s\n\
         plane: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
         fx: Filter, at: (100, 100), size: (60, 60), blur: 4 {\n\
           inner: Rect, size: (20, 20), color: (0.0, 1.0, 0.0, 1.0), at: (10, 10)\n\
         }\n",
    );
    let dims = SceneDimensions {
        width: 200,
        height: 200,
    };
    let mut stub = StubBackend;
    {
        let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = Some(&mut stub);
        let _ =
            timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut backend);
    }

    assert!(
        timeline.has_static_subtree_cached("plane", dims, false, DebugRenderOptions::default()),
        "a fully-static root must still cache in a scene that also has a Filter scope"
    );
    assert!(
        !timeline.has_static_subtree_cached("fx", dims, false, DebugRenderOptions::default()),
        "the effect-scope root must bypass the cache: its composite lives in the backend side channel"
    );
}

/// A filter whose parameter animates across frames must never have its
/// encoding cached and replayed: with a backend active the composite is held
/// in the pending side channel, so a cached vello Scene would drop it. The
/// static-subtree cache must stay empty for such a root and the conservative
/// whole-frame cache must stay off.
#[test]
fn animated_filter_scope_is_never_cached_or_replayed() {
    let timeline = build_timeline(
        "config { resolution: (200, 200) }\n\
         #0s\n\
         fx: Filter {\n\
           soft: Blur, radius: 2\n\
           inner: Rect, size: (80, 80), color: (0.9, 0.3, 0.25, 1.0), at: (100, 100)\n\
         }\n\
         #1s\n\
         soft.radius = 12 [1s]\n",
    );
    let dims = SceneDimensions {
        width: 200,
        height: 200,
    };
    assert!(timeline.has_effect_scopes, "the scene must contain an effect scope");

    let mut stub = StubBackend;
    {
        let mut backend: Option<&mut dyn crate::timeline::effects::FilterBackend> = Some(&mut stub);
        // Two frames where `soft.radius` differs (2 at t=0, 12 after the tween).
        let _s0 =
            timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut backend);
        let _s1 =
            timeline.evaluate_with_debug(2.0, dims, DebugRenderOptions::default(), &mut backend);
    }

    assert!(
        !timeline.has_static_subtree_cached("fx", dims, false, DebugRenderOptions::default()),
        "a subtree containing an animated filter parameter must not be cached (no stale composite)"
    );
    assert!(
        timeline.eval_caches.frame_cache.borrow().is_none(),
        "the whole-frame cache stays conservatively gated on backend_can_run in a filtered scene"
    );
}

/// Cached subtree encodings are invalidated by `content_epoch`: every content
/// mutation funnels through `invalidate_frame_cache`, which bumps the epoch and
/// clears the static-subtree cache, so no stale encoding can be replayed.
#[test]
fn static_subtree_encoding_is_invalidated_by_content_epoch() {
    let timeline = build_timeline(
        "config { resolution: (200, 200) }\n\
         #0s\n\
         plane: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n",
    );
    let dims = SceneDimensions {
        width: 200,
        height: 200,
    };
    let _ = timeline.evaluate(0.0, dims);
    assert!(
        timeline.has_static_subtree_cached("plane", dims, false, DebugRenderOptions::default()),
        "static root should be cached before invalidation"
    );
    let epoch_before = timeline.content_epoch();
    timeline.invalidate_frame_cache();
    assert!(timeline.content_epoch() > epoch_before, "invalidation bumps content_epoch");
    assert!(
        timeline.eval_caches.static_subtree_cache.borrow().is_empty(),
        "a content_epoch bump must clear cached subtree encodings"
    );
}

/// `evaluate_for_presentation` must produce the same encoded scene as the
/// default path at the same time, but must never populate `frame_cache`.
#[test]
fn presentation_scene_matches_debug_and_skips_frame_cache() {
    let dims = SceneDimensions {
        width: 800,
        height: 600,
    };

    let timeline = make_minimal_timeline();
    let via_debug =
        timeline.evaluate_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);
    let via_presentation = timeline.evaluate_for_presentation(1.0, dims, &mut None);
    assert_same_encoding(&via_debug, &via_presentation);

    let fresh = make_minimal_timeline();
    assert!(
        fresh.eval_caches.frame_cache.borrow().is_none(),
        "a fresh timeline has no frame cache"
    );
    let _ = fresh.evaluate_for_presentation(1.0, dims, &mut None);
    assert!(
        fresh.eval_caches.frame_cache.borrow().is_none(),
        "evaluate_for_presentation must not write eval_caches.frame_cache"
    );
}
