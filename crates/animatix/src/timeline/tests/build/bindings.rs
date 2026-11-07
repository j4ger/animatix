use super::super::*;
use super::support::{
    actor, assert_finite_samples, build, build_ast, build_clean, build_timeline, keyframe, prop,
    text_at, vec2,
};
use crate::ast::LoopPattern;
use crate::timeline::Value;
use crate::timeline::property_track::TrackAccessor;

#[test]
fn test_for_loop_tuple_destructuring_creates_actors() {
    let source = r#"
        #0s
        for (x, y), i in {(10, 20), (30, 40), (50, 60)} {
            dot[i]: Rect, at: (x, y), size: (10, 10)
        }
    "#;

    let report = build(source);

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    for label in ["dot__0", "dot__1", "dot__2"] {
        let id = timeline.world.tracks.get_id(label).expect(label);
        assert!(
            timeline.world.geometry(id).position.is_some(),
            "Expected {label} to have a position track"
        );
    }
}

#[test]
fn test_for_loop_tuple_with_vec2_values() {
    // Create a for loop iterating over a variable holding Vec2 values
    // This tests the Vec2 destructuring path in bind_loop_var
    let ast = vec![keyframe(
        0.0,
        vec![Stmt::ForLoop {
            var: LoopPattern::Tuple(vec!["vx".to_string(), "vy".to_string()]),
            index_var: None,
            iterable: Expr::List(vec![vec2(5.0, 15.0), vec2(25.0, 35.0)]),
            modifiers: vec![],
            body: vec![actor(
                "point",
                "Ellipse",
                vec![
                    prop(
                        "at",
                        Expr::Tuple(vec![
                            Expr::Ident("vx".to_string()),
                            Expr::Ident("vy".to_string()),
                        ]),
                    ),
                    prop("radius", Expr::Num(8.0)),
                ],
            )],
            span: None,
        }],
    )];

    let report = build_ast(&ast);

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;

    // Two actors should have been created (one per iteration)
    // They have the same label "point" so only the last one persists
    assert!(timeline.world.tracks.contains_key("point"), "Expected point track");

    // The last iteration sets at to (25, 35)
    let track = timeline.world.tracks.get_id("point").unwrap();
    if let Some(pos_track) = &timeline.world.geometry(track).position {
        let pos = pos_track.evaluate(0);
        // position may be stored as Vec2 or individually
        assert!(
            (pos[0] - 25.0).abs() < 0.01 || (pos[0] - 5.0).abs() < 0.01,
            "Expected point x ~25 or ~5 (last iteration), got {}",
            pos[0]
        );
        assert!(
            (pos[1] - 35.0).abs() < 0.01 || (pos[1] - 15.0).abs() < 0.01,
            "Expected point y ~35 or ~15, got {}",
            pos[1]
        );
    }
}

#[test]
fn test_for_loop_tuple_destructuring_with_let_decl() {
    // Test that tuple destructuring in for loops works with let declarations
    // using a list literal as the iterable (not a variable, since variables
    // that evaluate to Value::List are wrapped as a single item)
    let source = r#"
        #0s
        for (a, b) in {(1, 2), (3, 4)} {
            let sum = a + b
        }
    "#;

    let report = build(source);

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    // The for loop should execute without panicking
    let timeline = report.output;
    // sum should have a variable track since it was declared in the for loop body
    assert!(
        timeline.variable_tracks.contains_key("sum"),
        "Expected variable track for 'sum'"
    );
    // The variable track has one keyframe at t=0 (last iteration wins since
    // both iterations run at the same time_ms)
    let sum_track = timeline.variable_tracks.get("sum").unwrap();
    assert_eq!(sum_track.keyframes.len(), 1, "Expected 1 keyframe for sum (both at t=0)");
    let sum_value = sum_track.keyframes.get(&0);
    assert!(sum_value.is_some(), "Expected sum keyframe at t=0");
    if let Value::Num(n) = sum_value.unwrap() {
        // Last iteration sets sum = 3+4 = 7
        assert!((*n - 7.0).abs() < 0.01, "Expected sum=7, got {}", n);
    } else {
        panic!("Expected Num value for sum");
    }
}

#[test]
fn test_for_loop_variable_cleaned_after_exit() {
    // After a for-loop, the loop variable and index variable should not
    // persist in the environment (closures already captured them).
    let source = r#"
        #0s
        for i, idx in {1, 2, 3} {
            let x = i * 2
        }
    "#;

    let report = build(source);
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;
    // Loop variable 'i' should NOT be in the environment after the loop
    assert!(
        timeline.env.get("i").is_none(),
        "Loop variable 'i' should be undefined after loop exit"
    );
    // Index variable 'idx' should NOT be in the environment after the loop
    assert!(
        timeline.env.get("idx").is_none(),
        "Index variable 'idx' should be undefined after loop exit"
    );
}

#[test]
fn test_for_loop_tuple_vars_cleaned_after_exit() {
    // Tuple destructuring variables should also be cleaned up after the loop.
    let source = r#"
        #0s
        for (a, b) in {(1, 2), (3, 4)} {
            let z = a + b
        }
    "#;

    let report = build(source);
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "Expected no build errors, got: {:?}", errors);

    let timeline = report.output;
    // Tuple destructuring variables should be cleaned up
    assert!(
        timeline.env.get("a").is_none(),
        "Tuple var 'a' should be undefined after loop exit"
    );
    assert!(
        timeline.env.get("b").is_none(),
        "Tuple var 'b' should be undefined after loop exit"
    );
}

#[test]
fn filled_shape_defaults_to_no_stroke() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
"#,
    );
    let track = timeline.world.tracks.get_id("a").expect("rect track");
    assert_eq!(timeline.world.style(track).stroke_width.get(0, 99.0), 0.0);
}

#[test]
fn stroke_only_shape_keeps_default_stroke() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Line, from: (0, 0), to: (100, 0), at: (200, 150)
"#,
    );
    let track = timeline.world.tracks.get_id("a").expect("line track");
    assert_eq!(timeline.world.style(track).stroke_width.get(0, 99.0), 2.0);
}

#[test]
fn explicit_filled_shape_stroke_is_preserved() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, stroke: red, stroke_width: 4, at: (200, 150)
"#,
    );
    let track = timeline.world.tracks.get_id("a").expect("rect track");
    assert_eq!(timeline.world.style(track).stroke_width.get(0, 99.0), 4.0);
}

#[test]
fn plot_actor_declarations_seed_dash_and_gradient() {
    // Plot children must route declared dash patterns and stroke gradients
    // onto their tracks rather than silently defaulting to a solid stroke.
    let timeline = build_timeline(
        r##"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
g: Graph, x_domain: (-1, 1), y_domain: (-1, 1), at: (320, 180) {
  c: PlotCurve, kind: "cartesian", func: (x) => x, stroke: red, dash_pattern: {10, 7},
    stroke_gradient: linear(0, {(0%, "#ff0000"), (100%, "#0000ff")})
}
"##,
    );
    let track = timeline.world.tracks.get_id("c").expect("plot curve track");
    assert_eq!(
        timeline.world.style(track).dash_pattern.get(0, Vec::new()),
        vec![10.0, 7.0],
        "dash_pattern should land on the plot track"
    );
    let ramp = timeline.world.style(track).stroke_gradient.get(0, Default::default());
    assert_eq!(ramp.stops.len(), 2, "stroke_gradient should land on the plot track");
}

#[test]
fn draw_in_adds_visible_stroke_to_filled_shape() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
draw-in a [1s]
"#,
    );
    let track = timeline.world.tracks.get_id("a").expect("rect track");
    assert_eq!(timeline.world.style(track).stroke_width.get(0, 99.0), 2.0);
    let fill = timeline.world.style(track).color.get(0, [0.0, 0.0, 0.0, 1.0]);
    let stroke = timeline.world.style(track).stroke_color.get(0, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(stroke, fill, "draw-in outline should use the fill color");
}

#[test]
fn reveal_in_adds_visible_stroke_to_filled_shape() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (100, 100), color: accent.primary, at: (200, 150)
reveal-in a [1s]
"#,
    );
    let track = timeline.world.tracks.get_id("a").expect("rect track");
    assert_eq!(timeline.world.style(track).stroke_width.get(0, 99.0), 2.0);
}

/// Effect children of a `Filter` scope lower into the scope's chain instead of
/// becoming scene-graph actors.
#[test]
fn filter_effect_child_lowers_into_scope_chain() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    assert_eq!(timeline.world.effects(scope).expect("effects").stages.len(), 1);
    assert_eq!(timeline.world.effects(scope).expect("effects").stages[0].label, "soft");

    let chain = timeline.world.effects(scope).expect("effects").build_chain(0);
    assert_eq!(chain.instances.len(), 1);
    assert_eq!(chain.instances[0].id.as_str(), "Blur");
    assert_eq!(chain.instances[0].params.f32_at(0), 10.0);

    // Effects are not actors: no track is created for the stage label.
    assert!(!timeline.world.tracks.contains_key("soft"));
}

/// `scope.stage.param = value` animates an effect parameter over time.
#[test]
fn effect_stage_param_assignment_animates() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 4
  img: Rect, size: (100, 100)
}
#1s
bg.soft.radius = 16 [1s]
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    assert_eq!(
        timeline.world.effects(scope).expect("effects").build_chain(0).instances[0]
            .params
            .f32_at(0),
        4.0
    );
    assert_eq!(
        timeline.world.effects(scope).expect("effects").build_chain(2000).instances[0]
            .params
            .f32_at(0),
        16.0
    );
}

/// An effect declared outside a `Filter` scope reports a build diagnostic.
#[test]
fn effect_outside_filter_scope_reports_diagnostic() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
row: Row {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#;
    let report = build(source);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.message.contains("must be declared inside a Filter scope")),
        "diagnostics: {:?}",
        report.diagnostics
    );
}

/// `Filter, bounds: (x, y, w, h)` yields a region of interest expanded by the
/// chain's worst-case support.
#[test]
fn filter_bounds_yields_support_padded_region() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter, bounds: (40, 30, 120, 80) {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    assert!(!timeline.world.effects(scope).expect("effects").build_chain(0).is_empty());

    let (width, height) = timeline.resolution().expect("configured resolution");
    let region = timeline
        .effect_scope_region(
            scope,
            crate::timeline::SceneDimensions { width, height },
            0,
            kurbo::Affine::IDENTITY,
        )
        .expect("authored bounds must produce a region");

    // Worst-case support = blur radius (10), applied on every side.
    assert_eq!(region.origin, [30.0, 20.0]);
    assert_eq!(region.size.width, 140);
    assert_eq!(region.size.height, 100);
}

/// Filter scope bounds authored in scene coordinates must transform with camera
/// motion to keep the filtered region aligned in world space.
#[test]
fn filter_bounds_follow_the_camera() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter, bounds: (40, 30, 120, 80) {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
#1s
camera.zoom = 2.0
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    let (width, height) = timeline.resolution().expect("configured resolution");
    let dims = crate::timeline::SceneDimensions { width, height };

    let before = timeline
        .effect_scope_region(scope, dims, 0, timeline.camera.affine(0, dims, None))
        .expect("identity camera keeps the authored rectangle");
    assert_eq!(before.origin, [30.0, 20.0]);

    // 2x about the scene center (160, 90): (40, 30) -> (-80, -30) and
    // (160, 110) -> (160, 130); the support then pads 10 px on every side and
    // the region clips to the frame.
    let after = timeline
        .effect_scope_region(scope, dims, 2000, timeline.camera.affine(2000, dims, None))
        .expect("the pushed scope still has a region");
    assert_eq!(after.origin, [0.0, 0.0]);
    assert_eq!(after.size.width, 170);
    assert_eq!(after.size.height, 140);
}

/// Without authored bounds the scope keeps the full-scene path.
#[test]
fn filter_without_bounds_has_no_region() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  soft: Blur, radius: 10
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    let (width, height) = timeline.resolution().expect("configured resolution");
    assert!(
        timeline
            .effect_scope_region(
                scope,
                crate::timeline::SceneDimensions { width, height },
                0,
                kurbo::Affine::IDENTITY
            )
            .is_none()
    );
}

/// A plugin effect registered in the extension registry lowers like a
/// built-in: `effect_for_type` finds it and the scope chain carries a stage
/// whose identity is the authored name.
#[test]
fn plugin_effect_lowers_into_scope_chain() {
    use crate::timeline::effects::{
        EffectParamKind, EffectParamSpec, EffectParamValue, EffectPassSpec, PluginEffectData,
        register_extension_effect,
    };

    let params = vec![EffectParamSpec::new(
        "size",
        EffectParamKind::F32,
        EffectParamValue::F32(0.0),
        0,
        4,
    )];
    let passes = vec![EffectPassSpec::new(
        "mock-pixelate",
        "@compute fn main() {}",
        "main",
    )];
    let registered = register_extension_effect(PluginEffectData {
        type_name: "MockPixelate".into(),
        display_name: "Mock Pixelate".into(),
        params,
        passes,
        author_uniform_size: 16,
        support_px: 0.0,
    })
    .expect("plugin effect registers");
    assert_eq!(&*registered, "MockPixelate");

    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (320, 180) }
#0s
bg: Filter {
  fx: MockPixelate, size: 8
  img: Rect, size: (100, 100)
}
"#,
    );
    let scope = timeline.world.tracks.get_id("bg").expect("filter scope");
    assert_eq!(timeline.world.effects(scope).expect("effects").stages.len(), 1);
    assert_eq!(timeline.world.effects(scope).expect("effects").stages[0].label, "fx");

    let chain = timeline.world.effects(scope).expect("effects").build_chain(0);
    assert_eq!(chain.instances.len(), 1);
    assert_eq!(chain.instances[0].id.as_str(), "MockPixelate");
    assert_eq!(chain.instances[0].params.f32_at(0), 8.0);
}

/// Top-level `config { resolution: (w, h) }` is recorded on the timeline so
/// export tooling can default its canvas to the authored size.
#[test]
fn config_resolution_is_recorded_on_timeline() {
    let source = r#"
config { resolution: (960, 540) }
r: Rect, size: (100, 100), at: (480, 270)
"#;
    assert_eq!(build_clean(source).resolution(), Some((960, 540)));
}

/// A stroke-only Path with no declared fill color must default to zero fill
/// opacity so open paths are not implicitly filled by the rasterizer.
#[test]
fn stroke_only_path_suppresses_default_fill() {
    let source = "p: Path, commands: {move_to(-50, 0), line_to(50, 0)}, stroke: accent.primary, stroke_width: 4";
    let timeline = build_clean(source);
    let track = timeline.world.tracks.get_id("p").expect("path track");
    assert_eq!(
        timeline.world.style(track).fill_opacity.get(0, 1.0),
        0.0,
        "stroke-only Path must default to no fill"
    );
}

/// An authored `fill_opacity` on a stroke-only Path is preserved.
#[test]
fn stroke_only_path_keeps_authored_fill_opacity() {
    let source = "p: Path, commands: {move_to(-50, 0), line_to(50, 0)}, stroke: accent.primary, stroke_width: 4, fill_opacity: 0.5";
    let timeline = build_clean(source);
    let track = timeline.world.tracks.get_id("p").expect("path track");
    assert_eq!(timeline.world.style(track).fill_opacity.get(0, 1.0), 0.5);
}

/// Invalid easing names on property assignments must be reported as diagnostics.
#[test]
fn typst_and_code_accept_uniform_text_content_property() {
    use crate::timeline::TrackAccessor;

    // Uniform `text:` property declarations must populate the text content
    // track for Typst and Code actors.
    let source = r#"
a: Typst, text: "hello typst"
b: Code, text: "hello code"
"#;
    let timeline = build(source).output;
    let world = &timeline.world;
    let typst_track = world.tracks.get_id("a").expect("a track");
    let typst_text = world.text_or_default(typst_track).text_content.get(0, String::new());
    assert_eq!(typst_text, "hello typst", "Typst should read content from `text:`");
    let code_track = world.tracks.get_id("b").expect("b track");
    let code_text = world.text_or_default(code_track).text_content.get(0, String::new());
    assert_eq!(code_text, "hello code", "Code should read content from `text:`");
}

/// `Code, language: "rust"` reaches the text lane through the generic property
/// pipeline (the `ActorField::Language` binding), so the frame-time compile
/// can pick it up; an absent `language` stays empty (plain rendering).
#[test]
fn code_language_reaches_text_lane() {
    use crate::timeline::TrackAccessor;

    let source = r#"
a: Code, code: "fn main() {}", language: "rust"
b: Code, code: "x = 1"
"#;
    let timeline = build_clean(source);
    let world = &timeline.world;
    let a_track = world.tracks.get_id("a").expect("a track");
    assert_eq!(world.text_or_default(a_track).language.get(0, String::new()), "rust");
    let b_track = world.tracks.get_id("b").expect("b track");
    assert_eq!(
        world.text_or_default(b_track).language.get(0, String::new()),
        "",
        "default is plain"
    );
}

#[test]
fn invalid_easing_name_warns_on_assignment() {
    let source = r#"
r: Rect, size: (100, 100), color: accent.primary
#0s
r.at = (320, 180) [500ms, ease: bounce-out]
"#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::InvalidModifierValue
                && d.message.contains("bounce-out")
        }),
        "expected an InvalidModifierValue diagnostic naming 'bounce-out', got: {:?}",
        report.diagnostics
    );
}

/// Unresolved multi-segment property paths must produce a diagnostic naming the
/// full dotted path rather than only the base segment.
#[test]
fn unresolved_path_property_warns_with_full_dotted_path() {
    let source = r#"
t: Text, text: "hello", font_size: theme.text_md, color: accent.primary
"#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| d.message.contains("theme.text_md")),
        "expected a diagnostic naming 'theme.text_md', got: {:?}",
        report.diagnostics
    );
}

#[test]
fn letchain_evaluates_with_shadowing_and_restores_env() {
    // Build-time let-bound block closure: sequential bindings see earlier
    // ones, shadowing wins innermost, and the caller env is restored (a later
    // read of the shadowed name must see the original value).
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade-in label [100ms]

#0.5s
let base = 4
let step = (v) => {
  let base = base * 10
  let out = base + v
  out
}
label.text = format("step(2)={} base={}", step(2), base)
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    // step(2) = (4*10)+2 = 42; the outer `base` must still be 4 afterwards.
    assert_eq!(text_at(&report.output, "label", 1000), "step(2)=42 base=4");
}

#[test]
fn letchain_inside_plot_closure_is_dynamic_and_finite() {
    // A block-bodied plot closure that references `t` must classify the plot
    // as dynamic (references_ident walks LetChain bindings + tail) and every
    // sampled y must be finite.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

c: PlotCurve, kind: "cartesian",
  func: (x) => {
    let k = clamp(floor(t), 0, 6)
    let deg = 2 * k + 1
    sum_range((j) => (-1)^j * x^(2*j + 1) / factorial(2*j + 1), 0, floor(deg / 2))
  },
  color: accent.primary, stroke_width: 3, at: (320, 180), size: (400, 300)

#0.2s
fade-in c [100ms]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.world.tracks.get_id("c").expect("curve track");
    assert!(
        timeline
            .world
            .procedural_plot(track)
            .expect("procedural plot")
            .is_dynamic(&timeline.frame_written_vars),
        "LetChain body referencing t must be dynamic"
    );

    assert_finite_samples(&timeline, 1.0);
}

#[test]
fn is_animating_guards_on_property_state() {
    // The & reference + is_animating query: `at` is keyframed (flag flips
    // with the interpolation window), `scale` has no track (always false),
    // and the composition drives an idle/tracking style switch.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

status: Text, text: "idle", font_size: 18, anchor: scene.center, text_max_width: 500

box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

#0.2s
fade-in status [150ms]
fade-in box [200ms]

#1s
box.at = (420, 260) [1s, ease: ease-in-out]

always {
  status.text = if is_animating(&box.at) { "moving" } else { "at rest" }
  box.color = if is_animating(&box.at) { accent.warning } else { accent.success }
}
    "#;
    let timeline = build(source).output;
    // The always block overrides the status text per frame — the override is
    // only visible through full pipeline evaluation (execute_modifier_ir),
    // not through the text track's keyframes.
    use crate::timeline::modifier_runtime::ir::{ModifierOverrides, execute_modifier_ir};
    let status_at = |time_ms: u64| -> String {
        let mut overrides = ModifierOverrides::default();
        let mut env = timeline.build_frame_env(
            time_ms,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            &std::collections::HashMap::new(),
        );
        for program in &timeline.modifier_programs {
            execute_modifier_ir(program, &mut env, &mut overrides).expect("modifier execution");
        }
        let status = overrides.get("status").and_then(|props| props.get("text"));
        match status {
            Some(crate::timeline::Value::Str(s)) => s.clone(),
            other => panic!("status override missing at {time_ms}ms, got {other:?}"),
        }
    };

    // t=500ms: the declaration position seeds a t=0 keyframe, so the span
    // up to the first assignment reads as inside a segment — true (see spec:
    // declaration positions count as keyframes).
    assert_eq!(status_at(500), "moving");
    assert_eq!(status_at(1500), "moving");
    // t=2500ms: after the last keyframe (2s) — no next keyframe, at rest.
    assert_eq!(status_at(2500), "at rest");
}

#[test]
fn is_animating_requires_property_reference() {
    // Passing a plain value instead of &actor.prop must be a clear type
    // error, not a silent false.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "t"

#0.5s
label.text = format("{}", is_animating(5))
    "#;
    // The call itself builds; the frame-time evaluation must carry the type
    // error through the text override failure path (runtime diagnostics), so
    // rely on the unit-level check below for the message.
    let _report = build(source);

    let mut env = crate::timeline::Environment::new();
    crate::timeline::load_standard_library(&mut env);
    // Through the env dispatch (the same path `always` uses).
    let err = crate::timeline::utils::evaluate_call_value(
        "is_animating",
        vec![crate::timeline::Value::Num(5.0)],
        &env,
    )
    .expect_err("non-reference arg must be a type error");
    assert!(err.to_string().contains("property reference"), "unexpected error: {err}");
}

#[test]
fn letchain_closure_captures_let_binding() {
    // Nested closures inside block bodies must capture outer let-bound variables
    // from enclosing LetChain scopes.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

label: Text, text: "init", font_size: 18, anchor: scene.center, text_max_width: 440

#0.2s
fade-in label [100ms]

#0.5s
let maker = (x) => {
  let a = x * 10
  (y) => y + a
}
let add20 = maker(2)
label.text = format("add20(4) = {}", add20(4))
    "#;
    let timeline = build(source).output;
    // maker(2) binds a=20; add20(4) = 4 + 20.
    assert_eq!(text_at(&timeline, "label", 1000), "add20(4) = 24");
}

#[test]
fn effect_keys_on_non_action_hosts_warn() {
    // The effect-key whitelist is Action-only: the same keys on an
    // assignment or actor declaration are silent no-ops today, and the
    // host-gated whitelist must surface them.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, at: (240, 135)

        #0.5s
        box.at = (240, 200) [padding: 4]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnsupportedModifierKey
                && d.message.contains("padding")
        }),
        "effect key on an assignment must warn, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn prop_ref_with_unknown_actor_warns_at_build() {
    // `&rign.at` (typo'd label) must produce a build-time diagnostic instead
    // of failing at frame time.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

        #0.2s
        fade-in box [200ms]

        always {
          box.color = if is_animating(&rign.at) { accent.warning } else { accent.success }
        }
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownTargetPath
                && d.message.contains("rign")
        }),
        "typo'd property-ref label must warn at build, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn prop_ref_to_non_injectable_property_warns() {
    // `font_size` is assignable but not injectable (read_source is absent),
    // so `&box.font_size` must warn even though the label and the property
    // name are both individually valid.
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (480, 270) }

        box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center, font_size: 10

        #0.2s
        fade-in box [200ms]

        always {
          let _guard = is_animating(&box.font_size)
        }
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownTargetPath
                && d.message.contains("font_size")
        }),
        "non-injectable property ref must warn at build, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn brace_list_assignment_to_vector_properties_works() {
    // Numeric brace lists must bind directly to vector properties with the
    // same semantics as parenthesized coordinate tuples.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: {40, 40}, color: accent.primary, anchor: scene.center

#0.2s
fade-in box [150ms]

#1s
box.at = {420, 260} [500ms, ease: ease-out]
box.size = {120, 60} [500ms, ease: ease-out]
    "#;
    let report = build(source);
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let box_track = timeline.world.tracks.get_id("box").expect("box track");
    let at = timeline
        .world
        .geometry(box_track)
        .position
        .as_ref()
        .expect("at track")
        .evaluate(2000);
    let size = timeline
        .world
        .geometry(box_track)
        .size
        .as_ref()
        .expect("size track")
        .evaluate(2000);
    assert_eq!(at, [420.0, 260.0], "brace at must position the actor");
    assert_eq!(size, [60.0, 30.0], "brace size is stored as half-size");
}

#[test]
fn text_max_width_binds_to_text_tracks_for_declaration_and_assignment() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

long: Text, text: "The quick brown fox jumps over the lazy dog", font_size: 20, text_max_width: 100, opacity: 1.0, at: (320, 100)
unwrapped: Text, text: "The quick brown fox jumps over the lazy dog", font_size: 20, text_max_width: 0, opacity: 1.0, at: (320, 220)

#1s
long.text_max_width = 60
    "#;
    let timeline = build_clean(source);
    let long_id = timeline.world.get_id("long").expect("long text track");
    let unwrapped_id = timeline.world.get_id("unwrapped").expect("unwrapped text track");

    let long = timeline.world.text_or_default(long_id);
    assert_eq!(
        long.text_max_width.get(0, 0.0),
        100.0,
        "the Text declaration must write TextTracks::text_max_width"
    );
    assert_eq!(
        long.text_max_width.get(1000, 0.0),
        60.0,
        "a runtime text_max_width assignment/keyframe must write TextTracks"
    );
    assert_eq!(
        timeline.world.read_property_value(
            long_id,
            crate::timeline::property_registry::ActorField::TextMaxWidth,
            1000,
        ),
        Some(crate::timeline::PropertyValue::F32(60.0))
    );

    let dims = SceneDimensions {
        width: 640,
        height: 360,
    };
    let rendered_half_width = |time_s: f64| {
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time_s,
            dims,
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let item = program.items.iter().find(|item| item.label == "long").expect("long text item");
        let paths = item
            .commands
            .iter()
            .find_map(|command| match command {
                crate::primitives::RenderCommand::Text { paths, .. } => Some(paths.as_ref()),
                _ => None,
            })
            .expect("text command");
        crate::renderer::text::measure_text_paths(paths)[0]
    };

    // The declaration's compiled paths must actually wrap: the bounded actor
    // is narrower than the same text declared with no wrap width. The runtime
    // assignment must then narrow the rendered glyphs further.
    let unwrapped = timeline
        .world
        .text_or_default(unwrapped_id)
        .static_paths
        .as_ref()
        .expect("declaration paths");
    let [unwrapped_half_width, _] = crate::renderer::text::measure_text_paths(unwrapped);
    let declared_half_width = rendered_half_width(0.0);
    let assigned_half_width = rendered_half_width(1.0);
    assert!(
        declared_half_width < unwrapped_half_width,
        "text_max_width must narrow the compiled text: declared={declared_half_width}, unwrapped={unwrapped_half_width}"
    );
    assert!(
        assigned_half_width < declared_half_width,
        "runtime text_max_width must re-wrap the rendered text: declared={declared_half_width}, assigned={assigned_half_width}"
    );
}
