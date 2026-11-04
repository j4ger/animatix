use std::collections::HashMap;

use super::*;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const HD: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

fn property(name: &str, value: Expr) -> Property {
    Property {
        name: name.to_string(),
        value,
        value_span: None,
        trailing_comment: None,
    }
}

fn pair(x: f64, y: f64) -> Expr {
    Expr::Tuple(vec![Expr::Num(x), Expr::Num(y)])
}

fn color4(r: f64, g: f64, b: f64, a: f64) -> Expr {
    Expr::Tuple(vec![Expr::Num(r), Expr::Num(g), Expr::Num(b), Expr::Num(a)])
}

fn actor(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}

fn config_stmt(settings: Vec<Property>) -> Stmt {
    Stmt::Config {
        settings,
        span: None,
    }
}

fn keyframe(body: Vec<Stmt>) -> Stmt {
    Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body,
        span: None,
    }
}

fn assignment(target: &str, property_name: &str, value: Expr) -> Stmt {
    Stmt::Assignment {
        target: vec![crate::ast::TargetSegment::Static(target.to_string())],
        property: property_name.to_string(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    }
}

/// A single static `box1: Rect` with `size: (50, 50)` and `color`.
fn static_box_ast(color: &str, with_config: bool, at: Option<(f64, f64)>) -> Vec<Stmt> {
    let mut props = vec![
        property("size", pair(50.0, 50.0)),
        property("color", Expr::Ident(color.to_string())),
    ];
    if let Some((x, y)) = at {
        props.push(property("at", pair(x, y)));
    }
    let mut ast = Vec::new();
    if with_config {
        ast.push(config_stmt(vec![property("colorscheme", Expr::Str("editorial-dark".into()))]));
    }
    ast.push(keyframe(vec![actor("box1", "Rect", props)]));
    ast
}

fn text_actor(label: &str, text: &str) -> Stmt {
    actor(
        label,
        "Text",
        vec![
            property("text", Expr::Str(text.into())),
            property("font_size", Expr::Num(48.0)),
            property("font_family", Expr::Str("Open Sans".into())),
        ],
    )
}

fn build_ast(ast: &[Stmt]) -> BuildReport<Timeline> {
    Timeline::build_with_diagnostics(ast, &HashMap::new())
}

fn build_source(source: &str) -> BuildReport<Timeline> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    build_ast(&ast.expect("parsed AST"))
}

fn timeline_from(source: &str) -> Timeline {
    build_source(source).output
}

// ---------------------------------------------------------------------------
// Static subtree cache
// ---------------------------------------------------------------------------

#[test]
fn static_scene_cache_populated_after_first_evaluate() {
    let timeline = build_ast(&static_box_ast("accent.primary", true, Some((100.0, 100.0)))).output;
    let dims = HD;

    let _scene1 = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    assert!(
        !timeline.eval_caches.static_subtree_cache.borrow().is_empty(),
        "static subtree cache should be populated after first evaluate"
    );
    assert!(
        timeline.has_static_subtree_cached("box1", dims, false, DebugRenderOptions::default()),
        "cache should contain box1"
    );

    // Second evaluation at a different time should reuse the cached encoding.
    let _scene2 = timeline.evaluate_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);
    assert!(
        !timeline.eval_caches.static_subtree_cache.borrow().is_empty(),
        "static subtree cache should still have entries"
    );
}

#[test]
fn static_scene_cache_is_keyed_by_debug_options() {
    let timeline = build_ast(&static_box_ast("accent.primary", true, None)).output;
    let dims = HD;

    let _default =
        timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let debug_opts = DebugRenderOptions {
        draw_bounds: true,
        ..DebugRenderOptions::default()
    };
    let _debug = timeline.evaluate_with_debug(0.0, dims, debug_opts, &mut None);

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let box1_id = timeline.world.get_id("box1").expect("box1 id");
    assert!(cache.contains_key(&(box1_id, dims, false, DebugRenderOptions::default())));
    assert!(cache.contains_key(&(box1_id, dims, false, debug_opts)));
    assert_eq!(cache.len(), 2, "debug options must not reuse the default scene");
}

#[test]
fn static_scene_cache_is_bypassed_for_hit_regions() {
    let timeline = build_ast(&static_box_ast("accent.primary", true, None)).output;
    let dims = HD;

    let _default =
        timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let hit_opts = DebugRenderOptions {
        compute_hit_regions: true,
        ..DebugRenderOptions::default()
    };
    let _hit = timeline.evaluate_with_debug(0.0, dims, hit_opts, &mut None);

    assert!(
        !timeline.hit_regions().is_empty(),
        "hit-region evaluation must recompute regions instead of reusing a cached scene"
    );
}

#[test]
fn static_scene_only_cache_does_not_collect_items() {
    let timeline = build_ast(&static_box_ast("red", false, None)).output;
    let dims = HD;

    let _scene = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let box1_id = timeline.world.get_id("box1").expect("box1 id");
    let scene_only_key = (box1_id, dims, false, DebugRenderOptions::default());
    let (_, _, items, _) =
        cache.get(&scene_only_key).expect("scene-only static subtree should be cached");
    assert!(items.is_empty(), "scene-only evaluation must not collect SceneItems");
    assert!(
        !cache.contains_key(&(box1_id, dims, true, DebugRenderOptions::default())),
        "scene-only evaluation must not populate the program cache entry"
    );
}

#[test]
fn static_program_after_scene_only_still_collects_items() {
    let timeline = build_ast(&static_box_ast("red", false, None)).output;
    let dims = HD;

    let _scene = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let program =
        timeline.evaluate_program_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    assert!(
        !program.items.is_empty(),
        "program path must collect items after scene-only cache"
    );

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let box1_id = timeline.world.get_id("box1").expect("box1 id");
    let program_key = (box1_id, dims, true, DebugRenderOptions::default());
    let (_, _, items, _) = cache
        .get(&program_key)
        .expect("program static subtree should be cached separately");
    assert!(!items.is_empty());
}

#[test]
fn static_program_entries_are_stable_on_repeat_hits() {
    let timeline = build_ast(&static_box_ast("red", false, None)).output;
    let dims = HD;

    let first =
        timeline.evaluate_program_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let second =
        timeline.evaluate_program_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);
    assert_eq!(first.items.len(), second.items.len());
    assert_eq!(first.items[0].transform, second.items[0].transform);
    assert_eq!(first.items[0].opacity, second.items[0].opacity);
}

#[test]
fn static_scene_cache_is_keyed_by_dimensions() {
    let timeline = build_ast(&static_box_ast("red", false, None)).output;
    let dims_1 = HD;
    let dims_2 = SceneDimensions {
        width: 1280,
        height: 720,
    };

    let _first =
        timeline.evaluate_with_debug(0.0, dims_1, DebugRenderOptions::default(), &mut None);
    let _second =
        timeline.evaluate_with_debug(0.0, dims_2, DebugRenderOptions::default(), &mut None);

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let box1_id = timeline.world.get_id("box1").expect("box1 id");
    assert!(cache.contains_key(&(box1_id, dims_1, false, DebugRenderOptions::default())));
    assert!(cache.contains_key(&(box1_id, dims_2, false, DebugRenderOptions::default())));
    assert_eq!(cache.len(), 2, "dimensions must not reuse a static subtree scene");
}

#[test]
fn static_scene_skips_frame_env() {
    let timeline = build_ast(&static_box_ast("accent.primary", true, Some((100.0, 100.0)))).output;

    // A static scene with no modifiers or procedural plots needs no frame env.
    let msg = format!(
        "static scene should not need frame env. modifiers={}, programs={}, proc_plots={}",
        timeline.modifiers.len(),
        timeline.modifier_programs.len(),
        timeline.has_procedural_plots()
    );
    assert!(!timeline.needs_frame_env(), "{}", msg);
}

#[test]
fn static_subtree_cache_works_alongside_modifiers() {
    let timeline = timeline_from(
        r##"
        config { colorscheme: "editorial-dark" }

        static_box: Rect, size: (100, 100), color: accent.primary, at: (50, 50)
        dynamic_box: Rect, size: (80, 80), color: accent.secondary, at: (200, 200)

        always {
            dynamic_box.at = (200 + t * 10, 200)
        }
    "##,
    );

    let static_id = timeline.get_entity_id("static_box").expect("static_box id");
    let dynamic_id = timeline.get_entity_id("dynamic_box").expect("dynamic_box id");
    assert!(timeline.is_static_subtree_id(static_id));
    assert!(!timeline.is_static_subtree_id(dynamic_id));

    let _scene1 = timeline.evaluate_with_debug(0.0, HD, DebugRenderOptions::default(), &mut None);
    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    assert!(
        cache.contains_key(&(static_id, HD, false, DebugRenderOptions::default())),
        "static_box should be cached despite modifiers existing in scene"
    );
    assert!(
        !cache.contains_key(&(dynamic_id, HD, false, DebugRenderOptions::default())),
        "dynamic_box must not be in static subtree cache"
    );
}

#[test]
fn time_dependent_caps_prevent_static_subtree_caching() {
    let timeline = timeline_from(
        r##"
        subs: Captions, style: "karaoke", at: (640, 600) {
            c1: Cue, start: 0.5, end: 1.5, text: "Hello"
        }
    "##,
    );

    let subs = timeline.world.tracks.get_id("subs").expect("subs track must exist");
    assert!(
        timeline.world.caps(subs).time_dependent,
        "Captions must have time_dependent capability"
    );
    let subs_id = timeline.get_entity_id("subs").expect("subs id");
    assert!(!timeline.is_static_subtree_id(subs_id), "Captions must not be a static subtree");

    let cue = timeline.world.tracks.get_id("c1").expect("c1 cue track must exist");
    assert!(
        timeline.world.caps(cue).time_dependent,
        "Cue must have time_dependent capability"
    );
    let cue_id = timeline.get_entity_id("c1").expect("c1 id");
    assert!(!timeline.is_static_subtree_id(cue_id), "Cue must not be a static subtree");
}

// ---------------------------------------------------------------------------
// Text evaluation
// ---------------------------------------------------------------------------

#[test]
fn test_runtime_text_recompilation() {
    let ast = vec![
        config_stmt(vec![property("colorscheme", Expr::Str("editorial-dark".into()))]),
        keyframe(vec![
            actor(
                "counter",
                "Text",
                vec![
                    property("text", Expr::Str("0.00".into())),
                    property("font_size", Expr::Num(48.0)),
                    property("font_family", Expr::Str("Open Sans".into())),
                    property("color", color4(1.0, 1.0, 1.0, 1.0)),
                ],
            ),
            Stmt::Always {
                body: vec![assignment(
                    "counter",
                    "text",
                    Expr::Call(
                        "format".to_string(),
                        vec![Expr::Str("t={}".to_string()), Expr::Ident("t".to_string())],
                    ),
                )],
                span: None,
            },
        ]),
    ];
    let timeline = build_ast(&ast).output;

    // The compile cache is process-wide; start from a clean slate so the entry
    // count below only reflects this test's compilations (lib tests run with
    // --test-threads=1).
    crate::renderer::text::clear_text_compile_cache();

    let dims = SceneDimensions {
        width: 400,
        height: 200,
    };
    let _scene_0s = timeline.evaluate(0.0, dims);
    let _scene_1_5s = timeline.evaluate(1.5, dims);

    // Both evaluated times should have memoized their distinct text contents.
    let cache_len = crate::renderer::text::text_compile_cache_len();
    assert!(
        cache_len >= 2,
        "Text compile cache should have at least 2 entries for different times, got {cache_len}"
    );
}

#[test]
fn equation_frames_reuse_one_typst_compilation() {
    // The Equation path aggregates its Fragment children into one Typst
    // document on *every* frame. Nothing about that body changes while
    // scrubbing, so all frames must collapse onto one compilation instead of
    // re-running Typst per frame (see benches/equation_frame.rs).
    let source = r##"
config { colorscheme: "editorial-dark" }

#0s
eq: Equation, font_size: 48, color: text.primary, at: (960, 540) {
  lhs: Fragment, text: "E"
  eq_sign: Fragment, text: " = "
  mass: Fragment, text: "m"
  c2: Fragment, text: "c^2"
}

// Without this the subtree is static and the static-subtree cache serves
// every frame, so the assertion below would pass without memoizing anything.
always {
  eq.at = (960 + 40 * sin(t), 540)
}
"##;
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    crate::renderer::text::clear_text_compile_cache();

    for i in 0..8 {
        let _ = timeline.evaluate(i as f64 * 0.05, HD);
    }

    assert_eq!(
        crate::renderer::text::grouped_text_compile_cache_len(),
        1,
        "eight frames with identical fragment content must memoize to one compilation"
    );
}

#[test]
fn runtime_empty_text_override_clears_stale_glyphs() {
    let ast = vec![keyframe(vec![
        text_actor("counter", "visible"),
        Stmt::Always {
            body: vec![assignment("counter", "text", Expr::Str("".to_string()))],
            span: None,
        },
    ])];
    let timeline = build_ast(&ast).output;
    let dimensions = SceneDimensions {
        width: 400,
        height: 200,
    };

    // The actor has build-time glyphs, but the runtime override is explicitly
    // empty. Evaluation must produce no text commands instead of silently
    // reusing the cached build-time content, so no precise bounds are recorded.
    let _scene = timeline.evaluate(0.0, dimensions);
    assert!(
        timeline.precise_bounds_for("counter").is_none(),
        "empty runtime text override should clear visible content"
    );
}

#[test]
fn text_content_assignment_crossfades_paths_at_midpoint() {
    let source = r##"
        config { colorscheme: "editorial-dark" }

        label: Text, text: "Hello", font_size: 48

        #1s
        label.text = "World" [1s]
    "##;
    let report = build_source(source);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_entity_id("label").expect("label track should exist");
    let paths_mid = report
        .output
        .world()
        .text_or_default(track)
        .evaluate_paths(report.output.world().style(track), 1500);

    // The default path-track morph would return one set of glyphs at the
    // midpoint. The content-assignment path stores a Fade morph so both
    // endpoint glyph sets are returned at partial opacity.
    assert!(
        paths_mid.len() > 5,
        "Expected source+target glyph sets at midpoint, got {} paths",
        paths_mid.len()
    );
    assert!(
        paths_mid.iter().all(|p| p.opacity > 0.0 && p.opacity < 1.0),
        "Expected cross-fade opacities strictly inside (0, 1)"
    );
}

// ---------------------------------------------------------------------------
// World structure and transient overrides
// ---------------------------------------------------------------------------

/// The root set and every hierarchy `children` list must keep authored
/// declaration order — rendering order is the authored order, and the frame
/// loop walks `World::root_entities()` and `World::hierarchy(id).children`
/// directly.
#[test]
fn hierarchy_preserves_authored_order() {
    let report = build_source(
        r##"
        root1: Group {
            child1: Rect, size: (10, 10)
            child2: Group {
                grandchild1: Rect, size: (5, 5)
            }
        }
        root2: Rect, size: (20, 20)
    "##,
    );
    assert!(
        !report.diagnostics.iter().any(|d| d.is_error()),
        "unexpected build errors: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let root_labels: Vec<&str> = timeline
        .world
        .root_entities()
        .iter()
        .map(|&id| timeline.get_entity_label(id).expect("root id resolves"))
        .collect();
    assert_eq!(root_labels, vec!["root1", "root2"]);

    let root1 = timeline.get_entity_id("root1").expect("root1 id");
    let children: Vec<&str> = timeline.world.child_labels(root1).collect();
    assert_eq!(children, vec!["child1", "child2"]);

    let child2 = timeline.get_entity_id("child2").expect("child2 id");
    let grandchildren: Vec<&str> = timeline.world.child_labels(child2).collect();
    assert_eq!(grandchildren, vec!["grandchild1"]);
}

#[test]
fn transient_property_overrides_bypass_cache_and_override_evaluation() {
    let timeline = timeline_from(
        r##"
        r: Rect, size: (100, 100), color: "#ff0000", opacity: 0.5
    "##,
    );

    let dims = SceneDimensions {
        width: 800,
        height: 600,
    };
    let debug_opts = DebugRenderOptions::default();

    // 1. Initial evaluation with no overrides.
    let mut fb = None;
    let p1 = timeline.evaluate_program_with_debug(0.0, dims, debug_opts, &mut fb);
    assert_eq!(p1.items.len(), 1);
    assert_eq!(p1.items[0].label, "r");
    assert!((p1.items[0].opacity - 0.5).abs() < 1e-4);

    // 2. Evaluation with a transient override: opacity -> 0.95.
    let mut transient = HashMap::new();
    let mut r_overrides = HashMap::new();
    r_overrides.insert("opacity".to_string(), Value::Num(0.95));
    transient.insert("r".to_string(), r_overrides);

    let mut fb = None;
    let p2 = timeline
        .evaluate_program_with_transient_overrides(0.0, dims, debug_opts, &mut fb, &transient);
    assert_eq!(p2.items.len(), 1);
    assert_eq!(p2.items[0].label, "r");
    assert!(
        (p2.items[0].opacity - 0.95).abs() < 1e-4,
        "Transient override must take effect: expected 0.95, got {}",
        p2.items[0].opacity
    );

    // 3. After the transient override is cleared: must return the original 0.5,
    // not stale cache from the transient evaluation.
    let mut fb = None;
    let p3 = timeline.evaluate_program_with_debug(0.0, dims, debug_opts, &mut fb);
    assert_eq!(p3.items.len(), 1);
    assert_eq!(p3.items[0].label, "r");
    assert!(
        (p3.items[0].opacity - 0.5).abs() < 1e-4,
        "Restoring to no overrides must yield base track value 0.5, got {}",
        p3.items[0].opacity
    );
}

#[test]
fn captions_cue_start_end_and_active_color_reach_the_primitive() {
    let report = build_source(
        r##"
subs: Captions, at: (960, 900), font_size: 32, text_max_width: 640 {
    cue: Cue, text: "Hello captions", start: 0.5, end: 1.5, active_color: "#ff8800"
}
"##,
    );
    assert!(
        super::without_content_lints(&report.diagnostics).next().is_none(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let cue_id = timeline.world.get_id("cue").expect("cue track");

    // The primitive samples start/end/active_color from the cue's property
    // plan; the custom Cue build must have written all three there.
    let assets = crate::timeline::assets::AssetCache::new();
    let ctx = crate::primitives::EvaluateCtx {
        id: cue_id,
        label: "cue",
        caps: timeline.world.caps(cue_id),
        time_ms: 1000,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: HD,
        background_color: [0.0, 0.0, 0.0, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &assets,
        highlight: timeline.world.highlight(cue_id),
        property_plan: timeline.world.property_plan(cue_id),
        world: Some(timeline.world()),
        shape: timeline.world.shape(cue_id),
        text: timeline.world.text(cue_id),
        geometry: timeline.world.geometry(cue_id),
        style: timeline.world.style(cue_id),
        payload: None,
        target_resolver: None,
        legend: None,
    };
    let handle = timeline.cached_primitive_for(cue_id, 1000).expect("cue primitive");
    let cue = handle.as_dyn().caption_cue(&ctx).expect("caption cue");
    assert_eq!(cue.text, "Hello captions");
    assert_eq!(cue.start_ms, 500);
    assert_eq!(cue.end_ms, 1500);
    assert_eq!(cue.active_color, Some([1.0, 0.53333336, 0.0, 1.0]));

    // The Captions container only paints an active cue: the inside-window
    // frame must encode more drawing than the frames on either side.
    let before = timeline.evaluate(0.25, HD);
    let inside = timeline.evaluate(1.0, HD);
    let after = timeline.evaluate(2.0, HD);
    assert!(
        inside.encoding().draw_tags.len() > before.encoding().draw_tags.len(),
        "expected the caption to draw inside its [start, end) window"
    );
    assert!(
        inside.encoding().draw_tags.len() > after.encoding().draw_tags.len(),
        "expected the caption to stop drawing after its window"
    );
}

// ---------------------------------------------------------------------------
// Blend layer bounds (scene_eval/render.rs)
// ---------------------------------------------------------------------------

/// The historical unbounded blend layer is a ±1e9 `Rect`. Its clip-path
/// coordinates are stored as f32 bit patterns in `Scene::encoding().path_data`
/// (vello encodes clip shapes in local coordinates and carries the affine
/// separately), so a full-surface layer is detectable without a rendered-frame
/// readback. A tight rect never produces a coordinate near this magnitude in a
/// normally-sized scene. Inspection stays on the encoded `vello::Scene` per
/// AGENTS.md — engine unit tests must not depend on `animatix-render` readback.
fn encoded_blend_clip_is_unbounded(scene: &vello::Scene) -> bool {
    scene.encoding().path_data.iter().any(|&bits| {
        let value = f32::from_bits(bits);
        value.is_finite() && value.abs() >= 1.0e8
    })
}

/// A `blend: "screen"` child that reaches outside its parent's own bounds is
/// exactly the case a tight, parent-sized clip would wrongly cut. The
/// conservative fallback must keep the full-surface layer so the child still
/// composites. Structural assertions on the encoded scene, not a pixel A/B.
#[test]
fn blend_screen_child_outside_parent_keeps_unbounded_layer() {
    let timeline = timeline_from(
        r##"
        outer: Rect, size: (100, 100), color: "#ff0000", blend: "screen", at: (400, 300), opacity: 1 {
            inner: Rect, size: (400, 400), color: "#0000ff", at: (300, 300), opacity: 1
        }
        "##,
    );
    let scene = timeline.evaluate(0.0, HD);

    // A blend node that has children is a container: its own world-space bounds
    // are not recorded, so `blend_layer_clip_rect` cannot prove a tight rect and
    // must keep the full-surface layer. That is exactly the safety net for the
    // "child reaches outside the parent" case — a child that overflows a
    // parent-sized clip is never cut. (Leaf blend actors — the StageFloor glows —
    // do record bounds and take the tight path; see
    // `static_blend_scene_uses_bounded_clip_with_stable_structure`.)
    assert!(
        timeline.precise_bounds_for("outer").is_none(),
        "a blend container records no own bounds, so the tight clip is not provable"
    );
    assert!(
        encoded_blend_clip_is_unbounded(&scene),
        "child outside parent bounds must take the unbounded-layer fallback"
    );
    assert_eq!(
        scene.encoding().n_clips,
        2,
        "the blend layer (push/pop pair) must still be present"
    );
}

/// A camera-active scene must not get a tight world-space clip: the camera
/// moves the picture under the layer, so keep the historical unbounded layer.
#[test]
fn blend_layer_falls_back_when_camera_is_active() {
    let timeline = timeline_from(
        r##"
        glow: Rect, size: (200, 200), color: "#ff8800", blend: "screen", at: (400, 300), opacity: 1
        always { camera.at = (4.0, 4.0) }
        "##,
    );
    assert!(timeline.camera_used.get(), "test setup: camera must be active");
    let scene = timeline.evaluate(0.0, HD);
    assert!(
        encoded_blend_clip_is_unbounded(&scene),
        "camera-active blend layer must take the unbounded fallback"
    );
}

/// A mask/clip ancestor wraps the blend node in another clip layer; the
/// conservative gate must keep the unbounded layer (no wrong nested clip).
#[test]
fn blend_layer_falls_back_when_mask_or_clip_is_present() {
    let timeline = timeline_from(
        r##"
        plane: Mask, size: (300, 300), at: (400, 300), opacity: 1 {
            clip_shape: Rect, size: (300, 300), opacity: 1
            glow: Rect, size: (200, 200), color: "#ff8800", blend: "screen", opacity: 1
        }
        "##,
    );
    let scene = timeline.evaluate(0.0, HD);
    assert!(
        encoded_blend_clip_is_unbounded(&scene),
        "mask/clip-bearing blend subtree must take the unbounded fallback"
    );
}

/// Where bounds are well-defined (static, no camera/mask/effects, contained
/// children) the tight rect is used, and the encoded output stays structurally
/// stable across frames. A same-build control (identical scene without blend)
/// proves the only structural change is the single blend layer — draw geometry
/// is untouched (docs/roadmap.md:111 — prefer structural over pixel A/B).
#[test]
fn static_blend_scene_uses_bounded_clip_with_stable_structure() {
    let timeline = timeline_from(
        r##"
        glow: Rect, size: (200, 200), color: "#ff8800", blend: "screen", at: (400, 300), opacity: 1
        "##,
    );
    let first = timeline.evaluate(0.0, HD);
    let second = timeline.evaluate(1.0, HD);
    assert_eq!(
        first.encoding().draw_tags.len(),
        second.encoding().draw_tags.len(),
        "static blend encoding must be structurally stable between frames"
    );
    assert_eq!(first.encoding().n_clips, second.encoding().n_clips);
    assert_eq!(first.encoding().n_paths, second.encoding().n_paths);

    let control = timeline_from(
        r##"
        glow: Rect, size: (200, 200), color: "#ff8800", at: (400, 300), opacity: 1
        "##,
    )
    .evaluate(0.0, HD);
    assert_eq!(control.encoding().n_clips, 0, "control has no blend layer");
    assert_eq!(
        first.encoding().n_clips,
        2,
        "blend scene still encodes exactly one push/pop layer pair"
    );
    assert!(
        !encoded_blend_clip_is_unbounded(&first),
        "well-defined static bounds must use the tight rect, not the unbounded fallback"
    );
}
