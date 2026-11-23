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
fn needs_filter_backend_tracks_what_the_scene_actually_contains() {
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
        !plain.needs_filter_backend(),
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
        filtered.needs_filter_backend(),
        "a `Filter` scope was not detected; the frame cache would swallow its composites"
    );

    let masked = build(
        "config { resolution: (100, 100) }\n\
         #0s\nm: Mask, size: (60, 60) {\n\
           b: Rect, size: (20, 20), color: (1.0, 0.0, 0.0, 1.0), at: (10, 10)\n\
         }\n",
    );
    assert!(
        !masked.needs_filter_backend(),
        "Mask uses Vello clipping, not the GPU filter backend"
    );
}

#[test]
fn needs_filter_backend_memo_tracks_post_build_scope_changes() {
    let (ast, errors) =
        animatix_syntax::parser::parse_source("#0s\nbox: Rect, size: (20, 20), at: (10, 10)\n");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let mut timeline =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output;

    assert!(!timeline.needs_filter_backend());
    assert_eq!(timeline.eval_caches.needs_filter_backend.get(), Some(false));

    timeline.spawn_entity("scope", "Glass");
    assert_eq!(timeline.eval_caches.needs_filter_backend.get(), None);
    assert!(timeline.needs_filter_backend());

    timeline.world_mut().spawn_replacing("scope", "Mask");
    assert_eq!(timeline.eval_caches.needs_filter_backend.get(), None);
    assert!(!timeline.needs_filter_backend());

    // Capability overrides are part of the cached query too. Toggle a plain
    // Rect to an effect scope and back through the Timeline mutation funnel.
    let box_id = timeline.get_entity_id("box").expect("box entity");
    let plain_caps = timeline.world().caps(box_id);
    let filter_caps = animatix_core::caps::ActorCaps {
        child_processing: animatix_core::caps::ChildProcessingKind::Filter,
        ..plain_caps
    };
    timeline.world_mut().set_caps(box_id, filter_caps);
    assert_eq!(timeline.eval_caches.needs_filter_backend.get(), None);
    assert!(
        timeline.needs_filter_backend(),
        "a custom Filter capability must admit the backend"
    );

    timeline.world_mut().set_caps(box_id, plain_caps);
    assert_eq!(timeline.eval_caches.needs_filter_backend.get(), None);
    assert!(
        !timeline.needs_filter_backend(),
        "restoring plain caps must drop backend admission"
    );

    let cloned = timeline.clone();
    assert_eq!(cloned.eval_caches.needs_filter_backend.get(), None);
    assert!(!cloned.needs_filter_backend());
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
fn invalid_equation_is_reported_during_build() {
    let (statements, errors) =
        animatix_syntax::parser::parse_source("eq: Equation { term: Fragment, text: \"frac(\" }");
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_diagnostics(&statements.unwrap(), &Default::default());
    assert!(report.diagnostics.iter().any(|d| d.message.contains("Equation 'eq'")));
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
        _scene: &crate::scene::Scene,
        _dimensions: SceneDimensions,
        _region: Option<crate::timeline::effects::EffectRegion>,
        _chain: &crate::timeline::effects::EffectChain,
    ) -> Result<crate::timeline::image::SceneImage, String> {
        Err("stub backend does not render".to_string())
    }
}

/// Compare the ordered operation kinds emitted by two scene-evaluation paths.
fn assert_same_draw_kinds(a: &crate::scene::Scene, b: &crate::scene::Scene) {
    assert_eq!(a.draws().len(), b.draws().len(), "draw operation count differs");
    for (left, right) in a.draws().iter().zip(b.draws()) {
        assert_eq!(
            std::mem::discriminant(left),
            std::mem::discriminant(right),
            "draw operation order differs"
        );
    }
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
    assert!(timeline.needs_filter_backend(), "the scene must contain an effect scope");

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
    assert_same_draw_kinds(&via_debug, &via_presentation);

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

#[test]
fn nested_filter_and_glass_scopes_with_future_effects_admit_backend() {
    let timeline = build_timeline(
        "#0s\nouter: Group {\n\
           panel: Filter, size: (40, 40) {\n\
             soft: Blur, radius: 0\n\
             panel_child: Rect, size: (20, 20)\n\
           }\n\
           card: Glass, size: (40, 40) {\n\
             frost: Blur, radius: 0\n\
             glass_child: Rect, size: (20, 20)\n\
           }\n\
         }\n\
         #1s\n\
         panel.soft.radius = 8 [1s]\n\
         card.frost.radius = 8 [1s]\n",
    );

    for (scope, stage) in [("panel", "soft"), ("card", "frost")] {
        let id = timeline.get_entity_id(scope).expect("nested effect scope");
        let effects = timeline.world().effects(id).expect("effect chain");
        assert!(
            effects.build_chain(0).is_empty(),
            "{scope}.{stage} starts at its identity parameter"
        );
        assert!(
            !effects.build_chain(2_000).is_empty(),
            "{scope}.{stage} becomes active on a later frame"
        );
    }
    assert!(
        timeline.needs_filter_backend(),
        "nested Filter/Glass scopes must admit a backend even while their \
         current stages are identity"
    );
}

#[test]
fn precise_bounds_lookup_uses_entity_slots_not_label_sort_order() {
    for labels in [
        ["zebra", "alpha", "middle"],
        ["middle", "zebra", "alpha"],
        ["alpha", "middle", "zebra"],
    ] {
        let mut timeline = Timeline::new();
        for label in labels {
            spawn_seen(&mut timeline, label, "Rect");
        }
        timeline.ensure_bounds_registry();
        for (index, label) in labels.into_iter().enumerate() {
            let id = timeline.get_entity_id(label).expect("fixture entity");
            let slot = timeline.world.bounds_slot(id);
            let x = 10.0 * index as f64;
            let bounds = kurbo::Rect::new(x, 20.0, x + 4.0, 26.0);
            timeline.record_precise_bounds(slot, label, bounds);
        }
        for (index, label) in labels.into_iter().enumerate() {
            let x = 10.0 * index as f64;
            assert_eq!(
                timeline.precise_bounds_for(label),
                Some(kurbo::Rect::new(x, 20.0, x + 4.0, 26.0)),
                "label {label} must retain its own entity's slot in order {labels:?}"
            );
        }
        assert_eq!(timeline.precise_bounds_for("absent"), None);
    }
}

fn deeply_nested_bounds_fixture(depth: usize, reverse_roots: bool) -> Timeline {
    let marker = "m_marker: Rect, at: (500, 100), size: (30, 20), stroke_width: 0, opacity: 1\n";
    let mut source = String::from("#0s\n");
    if reverse_roots {
        source.push_str(marker);
    }
    source.push_str("z_root: Group, at: (100, 100), scale: 1, opacity: 1 {\n");
    for level in 0..depth {
        // Reverse-looking labels keep slot and lexical order deliberately different.
        source.push_str(&format!("g{}: Group, at: (2, 2), opacity: 1 {{\n", depth - level));
    }
    source.push_str("a_leaf: Rect, at: (20, 10), size: (40, 20), stroke_width: 0, opacity: 1\n");
    for _ in 0..depth {
        source.push_str("}\n");
    }
    source.push_str("}\n");
    if !reverse_roots {
        source.push_str(marker);
    }
    source.push_str("#1s\nz_root.scale = 0.5 [1s]\n");
    crate::timeline::tests::build_timeline(&source)
}

fn assert_program_bounds_match_lookup(
    timeline: &Timeline,
    bounds: &std::collections::HashMap<String, kurbo::Rect>,
) {
    for label in ["a_leaf", "m_marker"] {
        let expected = bounds.get(label).expect("rendered leaf/marker bounds");
        assert!(expected.width() > 0.0 && expected.height() > 0.0);
        assert_eq!(timeline.precise_bounds_for(label), Some(*expected), "lookup for {label}");
    }
}

#[test]
fn deep_group_bounds_survive_observable_frame_cache_restore() {
    let dims = SceneDimensions {
        width: 1280,
        height: 720,
    };
    for depth in [1, 4, 16, 32] {
        for reverse_roots in [false, true] {
            let timeline = deeply_nested_bounds_fixture(depth, reverse_roots);
            for time in [0.0, 1.5, 2.0, 0.0] {
                let program = timeline.evaluate_program_with_debug(
                    time,
                    dims,
                    DebugRenderOptions::default(),
                    &mut None,
                );
                assert_program_bounds_match_lookup(&timeline, &program.precise_bounds);
                assert_deep_bounds_geometry(&timeline, depth, time);
                // Leave the cached frame intact but remove the observable table:
                // the next same-time request must restore it, not just retain it.
                timeline.eval_caches.precise_bounds.borrow_mut().clear_frame();
                let cached = timeline.evaluate_program_with_debug(
                    time,
                    dims,
                    DebugRenderOptions::default(),
                    &mut None,
                );
                assert_eq!(cached.precise_bounds, program.precise_bounds);
                assert_program_bounds_match_lookup(&timeline, &program.precise_bounds);
                assert_deep_bounds_geometry(&timeline, depth, time);
            }
        }
    }
}

#[test]
fn deep_group_bounds_survive_scene_only_frame_cache_restore() {
    let dims = SceneDimensions {
        width: 1280,
        height: 720,
    };
    for depth in [1, 4, 16, 32] {
        for reverse_roots in [false, true] {
            let timeline = deeply_nested_bounds_fixture(depth, reverse_roots);
            for time in [0.0, 1.5, 2.0, 0.0] {
                let _ = timeline.evaluate(time, dims);
                let labels = ["a_leaf", "m_marker"];
                assert_deep_bounds_geometry(&timeline, depth, time);
                let expected = labels.map(|label| {
                    timeline.precise_bounds_for(label).expect("rendered descendant bounds")
                });
                timeline.eval_caches.precise_bounds.borrow_mut().clear_frame();
                let _ = timeline.evaluate(time, dims);
                for (label, expected) in labels.into_iter().zip(expected) {
                    assert_eq!(timeline.precise_bounds_for(label), Some(expected));
                }
            }
        }
    }
}

fn assert_deep_bounds_geometry(timeline: &Timeline, depth: usize, time: f64) {
    let scale = if time == 0.0 {
        1.0
    } else if time == 2.0 {
        0.5
    } else {
        // The midpoint is checked for lookup/cache consistency separately;
        // absolute geometry assertions need not assume a default easing curve.
        return;
    };
    let center_x = 100.0 + (20.0 + 2.0 * depth as f64) * scale;
    let center_y = 100.0 + (10.0 + 2.0 * depth as f64) * scale;
    let expected_leaf = kurbo::Rect::new(
        center_x - 20.0 * scale,
        center_y - 10.0 * scale,
        center_x + 20.0 * scale,
        center_y + 10.0 * scale,
    );
    let expected_marker = kurbo::Rect::new(485.0, 90.0, 515.0, 110.0);
    for (label, expected) in [("a_leaf", expected_leaf), ("m_marker", expected_marker)] {
        let actual = timeline.precise_bounds_for(label).expect("rendered bounds");
        for (actual, expected) in [
            (actual.x0, expected.x0),
            (actual.y0, expected.y0),
            (actual.x1, expected.x1),
            (actual.y1, expected.y1),
        ] {
            assert!(
                (actual - expected).abs() < 1e-3,
                "{label} at depth {depth}, time {time}: expected {expected}, got {actual}"
            );
        }
    }
}

#[test]
fn precise_bounds_registry_rebuilds_after_invalidation_and_spawn() {
    let mut timeline = Timeline::new();
    timeline.spawn_entity("z_original", "Rect");
    timeline.spawn_entity("m_original", "Rect");
    for phase in 0..3 {
        if phase == 1 {
            timeline.invalidate_frame_cache();
        } else if phase == 2 {
            // New label sorts before existing labels, but must keep its newly
            // allocated entity slot rather than shifting old slot meanings.
            timeline.spawn_entity("a_new", "Rect");
        }
        assert!(timeline.eval_caches.bounds_registry.borrow().is_none());
        timeline.ensure_bounds_registry();
        let labels: Vec<String> = timeline.world.tracks.keys().cloned().collect();
        for (index, label) in labels.iter().enumerate() {
            assert_eq!(timeline.precise_bounds_for(label), None, "old frame must be cleared");
            let id = timeline.get_entity_id(label).expect("registered entity");
            let x = 10.0 * index as f64;
            let expected = kurbo::Rect::new(x, 0.0, x + 5.0, 7.0);
            timeline.record_precise_bounds(timeline.world.bounds_slot(id), label, expected);
        }
        for (index, label) in labels.iter().enumerate() {
            let x = 10.0 * index as f64;
            assert_eq!(
                timeline.precise_bounds_for(label),
                Some(kurbo::Rect::new(x, 0.0, x + 5.0, 7.0))
            );
        }
    }
}
