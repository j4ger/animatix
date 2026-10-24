use super::super::tests::without_content_lints;
use super::part2::build_timeline;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{Timeline, Value};

#[test]
fn brace_list_with_non_numeric_element_warns() {
    // Heterogeneous lists stay rejected (no silent 0.0 coercion): a
    // non-numeric element produces an InvalidPropertyValue diagnostic and
    // the previous position is kept.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (480, 270) }

box: Rect, size: (80, 40), color: accent.primary, anchor: scene.center

#0.2s
fade-in box [200ms]

#1s
box.at = {280, "x"} [300ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue),
        "non-numeric brace element must warn, got: {:?}",
        report.diagnostics
    );
}

/// The layout readers take a number or a `(x, y)` tuple and used to leave
/// anything else alone with no output at all. `gap`'s type row now admits a
/// string (BarChart reads `"auto"` as the same name), so the drop has to be
/// reported by the reader instead of relying on the type check.
#[test]
fn container_layout_value_it_cannot_read_is_reported() {
    for (label, source) in [
        ("Row string gap", r#"row: Row, gap: "auto" { a: Rect, size: (10, 10) }"#),
        ("Col string gap", r#"col: Col, gap: "auto" { a: Rect, size: (10, 10) }"#),
        (
            "Grid string gap",
            r#"grid: Grid, cols: 2, gap: "auto" { a: Rect, size: (10, 10) }"#,
        ),
    ] {
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "{label} parse errors: {parse_errors:?}");
        let ast = ast.expect("parsed AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                    && d.message.contains("gap")
            }),
            "{label} must report the gap it dropped, got: {:?}",
            report.diagnostics
        );
    }
}

/// A timed declaration seeds a start snapshot *and* an end keyframe for every
/// shape value it carries. `corner_radius` was missing from the start list
/// while being present in the end list, which made a timed radius declaration
/// jump straight to its final value instead of animating from the current one.
#[test]
fn timed_declaration_snapshots_the_corner_radius() {
    let source = r#"
#0s
r: Rect, at: (100, 50), size: (120, 70), color: (1, 1, 1, 1), corner_radius: 30 [600ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let track = report.output.get_track("r").expect("track r");
    let id = track.id;
    // corner_radius lives in the shape component store, so the world-aware
    // accessors are the ones that can see it.
    assert_eq!(
        report
            .output
            .world()
            .property_keyframe_times(id, crate::timeline::ActorField::CornerRadius),
        vec![0, 600],
        "a timed declaration needs a start snapshot and an end keyframe"
    );
    assert_eq!(
        report
            .output
            .world()
            .read_property_value(id, crate::timeline::ActorField::CornerRadius, 0),
        Some(crate::timeline::PropertyValue::F32(0.0)),
        "the animation starts from the pre-declaration value"
    );
    assert_eq!(
        report.output.world().read_property_value(
            id,
            crate::timeline::ActorField::CornerRadius,
            600
        ),
        Some(crate::timeline::PropertyValue::F32(30.0)),
        "and ends at the declared value"
    );
}

// `config { duration: N }` is documented as overriding the keyframe-inferred
// duration (docs/spec.md, "Config key scopes"), and the per-scene composition
// path has always honoured it. These pin the single-scene equivalent.

fn build_source(source: &str) -> crate::timeline::BuildReport<Timeline> {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    Timeline::build_with_diagnostics(&ast.expect("parsed AST"), &std::collections::HashMap::new())
}

const SCENE_WITH_DURATION: &str = r#"
config { colorscheme: "editorial-dark", resolution: (1280, 720), duration: 2.0 }
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#5s
fade-in r [500ms]
"#;

#[test]
fn declared_duration_overrides_the_inferred_length() {
    let report = build_source(SCENE_WITH_DURATION);

    assert_eq!(
        report.output.duration_seconds(),
        5.5,
        "the inferred extent still reaches the last keyframe"
    );
    assert_eq!(
        report.output.declared_duration_seconds(),
        Some(2.0),
        "the declared value is what config asked for"
    );
    assert_eq!(
        report.output.playback_duration_seconds(),
        2.0,
        "playback ends at the declared duration even though keyframes run past it"
    );
}

#[test]
fn truncating_at_the_declared_duration_warns() {
    let report = build_source(SCENE_WITH_DURATION);

    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::DurationShorterThanContent
                && d.location.subject.as_deref() == Some("duration")
        }),
        "content past the declared duration must not be dropped silently, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn declared_duration_longer_than_content_extends_playback_without_warning() {
    let report = build_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (1280, 720), duration: 8.0 }
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert_eq!(report.output.playback_duration_seconds(), 8.0);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::DurationShorterThanContent),
        "a duration longer than the content truncates nothing, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn duration_config_must_be_a_positive_number() {
    for value in ["0.0", "-3.0", "\"long\""] {
        let report = build_source(&format!(
            r#"
config {{ colorscheme: "editorial-dark", duration: {value} }}
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#
        ));

        assert_eq!(
            report.output.declared_duration_seconds(),
            None,
            "`duration: {value}` is not a usable length"
        );
        assert!(
            report.diagnostics.iter().any(|d| {
                d.code == crate::diagnostics::DiagnosticCode::InvalidConfigValue
                    && d.location.subject.as_deref() == Some("duration")
            }),
            "`duration: {value}` must be reported, got: {:?}",
            report.diagnostics
        );
        assert_eq!(
            report.output.playback_duration_seconds(),
            report.output.duration_seconds(),
            "an unusable duration falls back to the inferred extent"
        );
    }
}

#[test]
fn unknown_config_key_is_reported_instead_of_dropped() {
    let report = build_source(
        r#"
config { colorscheme: "editorial-dark", colur_scheme: "warm" }
r: Rect, size: (100, 100), color: accent.primary, opacity: 0.25, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey
                && d.location.subject.as_deref() == Some("colur_scheme")
        }),
        "a config key no phase reads must be reported, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn every_documented_config_key_is_recognised() {
    let report = build_source(
        r#"
config {
    colorscheme: "editorial-dark",
    resolution: (1280, 720),
    duration: 8.0,
    dynamic_layout: true,
    strict_types: false,
    text_fast_path: true,
    export_preset: "1080p30",
}
r: Rect, size: (100, 100), color: accent.primary, at: (640, 360)
#1s
fade-in r [500ms]
"#,
    );

    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey),
        "every documented config key must be recognised, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn rect_to_ellipse_redeclaration_keeps_rect_geometry_before_the_morph() {
    // A same-label re-declaration that changes the actor type (the morph
    // syntax) must not reach back before its own beat: the actor renders as
    // its declared type until the morph starts. Regression: `box: Ellipse`
    // at #1.5s made the Rect draw as a circle from frame 0 (the morph target
    // leaked into the pre-morph span).
    let source = r#"
config { resolution: (640, 360), duration: 3 }
box: Rect, size: (140, 140), color: accent.primary, at: (320, 180)

#0.3s
fade-in box [250ms, ease: ease-out]

#1.5s
box: Ellipse, size: (140, 140), color: accent.success [900ms, strategy: match]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let track = timeline.world.tracks.get("box").expect("box track exists");

    let shape = timeline.world.shape(track.id);
    let radius_pre = shape.corner_radius.get(500, 0.0);
    assert_eq!(
        radius_pre, 0.0,
        "corner_radius at t=500ms (before the #1.5s morph) must be the Rect's 0, got {radius_pre}"
    );
    let shape_pre = crate::timeline::TrackAccessor::get(
        &shape.shape_type,
        500,
        crate::timeline::ShapeType::Rect,
    );
    assert_eq!(
        shape_pre,
        crate::timeline::ShapeType::Rect,
        "shape_type at t=500ms must still be Rect"
    );

    // The frame-time primitive must follow the shape_type track, not the
    // (last-write-wins) track identity: the actor renders as the declared
    // Rect until the morph beat and as the re-declared Ellipse after it.
    assert_eq!(track.render_type_name(shape, 500), "Rect");
    assert_eq!(track.render_type_name(shape, 1_400), "Rect");
    assert_eq!(track.render_type_name(shape, 2_400), "Ellipse");
}

#[test]
fn bounce_action_settles_with_decaying_hops() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 100]
"#,
    );
    let track = timeline.world.tracks.get("a").expect("rect track");
    let y = |ms: u64| track.geometry.motion_offset.get(ms, [0.0, 0.0])[1];

    // restitution 0.6 ⇒ six visible hops filling the 1s window, so the first
    // airtime is 1000·(1-0.6)/(1-0.6⁶) ≈ 420ms: apex at 210, contact at 420.
    assert!(y(210) < -95.0, "first apex should reach ~100px up, got {}", y(210));
    assert!(y(420).abs() < 1.5, "first contact must return to rest, got {}", y(420));
    // Hop 2 keeps 0.6² of the height and 0.6 of the airtime.
    assert!(y(545) > -40.0 && y(545) < -30.0, "second apex should be ~36px, got {}", y(545));
    assert!(y(1000).abs() < 1.5, "the actor must finish where it started, got {}", y(1000));
    // A rise decelerates and a fall accelerates: that pair is the parabola, so
    // the arc must not be linear in either half. A quarter into the rise the
    // actor is already most of the way up; a quarter into the fall it has
    // barely left the apex.
    assert!(y(52) < -35.0, "rising should be front-loaded, got {}", y(52));
    assert!(y(262) < -85.0, "falling should be back-loaded, got {}", y(262));
}

#[test]
fn bounce_restitution_stretches_the_series_to_the_duration() {
    let timeline = build_timeline(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 100, restitution: 0.9]
"#,
    );
    let track = timeline.world.tracks.get("a").expect("rect track");
    let y = |ms: u64| track.geometry.motion_offset.get(ms, [0.0, 0.0])[1];
    // A springier ball is still airborne at 900ms — the hop cap must not leave
    // it parked early — and it is home at the end of the duration.
    assert!(y(900) < -1.0, "0.9 restitution should still be bouncing, got {}", y(900));
    assert!(
        y(1000).abs() < 1.5,
        "the series should end on the rest position, got {}",
        y(1000)
    );
}

/// The timing parser used to keep its own list of effect modifier keys, so an
/// action gaining a parameter (`bounce [restitution: …]`) produced a warning
/// about a key its own signature declares.
#[test]
fn action_modifiers_are_tolerated_from_the_signature_not_a_literal_list() {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensity: 40, restitution: 0.7]
"#,
    );
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("ast"), &std::collections::HashMap::new());
    let noisy: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("Unsupported modifier key"))
        .map(|d| d.message.clone())
        .collect();
    assert!(noisy.is_empty(), "declared modifiers warned: {noisy:?}");
}

#[test]
fn a_misspelled_action_modifier_still_warns() {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0s
a: Rect, size: (40, 40), color: accent.primary, at: (200, 150)
bounce a [1s, intensy: 40]
"#,
    );
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("ast"), &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().any(|d| d.message.contains("intensy")),
        "a typo in an action modifier produced no diagnostic"
    );
}

/// `Arrow` paints both its shaft and its head from `stroke_color`, so an
/// authored `color:` had nowhere to go: the actor built, rendered grey, and
/// warned about nothing.
#[test]
fn stroke_only_shapes_inherit_an_authored_color_into_their_stroke() {
    for (ty, label) in [("Arrow", "arrow"), ("Line", "line")] {
        let source = format!(
            "config {{ colorscheme: \"editorial-dark\", resolution: (640, 360) }}\n\
             #0s\na: {ty}, from: (100, 100), to: (300, 200), color: accent.danger, at: (0, 0)\n"
        );
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "{label} parse errors: {parse_errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.unwrap(), &std::collections::HashMap::new());
        let track = report.output.world.tracks.get("a").expect("track");
        let stroke = track.style.stroke_color.get(0, [0.0; 4]);
        let fill = track.style.color.get(0, [0.0; 4]);
        // Assert the tracks exist, not just that they agree: two absent tracks
        // both read back as the [0,0,0,0] default and would match vacuously.
        assert!(fill != [0.0, 0.0, 0.0, 0.0], "{label}: no color track was built at all");
        assert_eq!(
            stroke, fill,
            "{label}: authored `color:` never reached stroke_color ({stroke:?} vs {fill:?})"
        );
    }
}

/// The plot dispatch detected an authored `opacity:` and then overwrote it
/// with 1.0, so dimmed backdrops were impossible on Graph/BarChart/ContourSet/
/// VectorField without a warning to say so.
#[test]
fn plot_actors_honour_an_authored_opacity() {
    for ty in ["Graph", "BarChart"] {
        let source = format!(
            "config {{ colorscheme: \"editorial-dark\", resolution: (640, 360) }}\n\
             #0s\ng: {ty}, size: (400, 300), values: {{1, 2, 3}}, x_domain: (-1, 1), \
             y_domain: (-1, 1), opacity: 0.05, at: (320, 180)\n"
        );
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(&source);
        assert!(parse_errors.is_empty(), "{ty} parse errors: {parse_errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.unwrap(), &std::collections::HashMap::new());
        let track = report.output.world.tracks.get("g").expect("plot track");
        let opacity = track.style.opacity.get(0, 99.0);
        assert!(
            (opacity - 0.05).abs() < 1e-6,
            "{ty} dropped the authored opacity (got {opacity})"
        );
    }
}

/// A re-declaration at a shared timestamp must not steal its own target as its
/// starting value.
///
/// `insert_start_keyframes` captures what the track currently holds and writes
/// it at the declaration's timestamp, so a sibling that had already written a
/// keyframe at that same stamp could in principle become the "start" — the
/// animation would then run from its destination and the frame before the stamp
/// would show the new shape instead of the old one. Four orderings are checked
/// here because the interesting one is the combination: a statement sharing the
/// `#1s` stamp with the morph, written before it.
#[test]
fn a_redeclaration_starts_from_what_it_replaces_not_from_its_target() {
    let prelude = "config { resolution: (400, 300), duration: 3 }\n\
                   a: Ellipse, size: (60, 60), color: (1.0, 0.0, 0.0, 1.0), at: (100, 100)\n";
    let cases = [
        (
            "a morph on its own",
            "#1s\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
        (
            "an instant assignment first",
            "#1s\na.color = (0.0, 1.0, 0.0, 1.0)\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
        (
            "a timed assignment first",
            "#1s\na.color = (0.0, 1.0, 0.0, 1.0) [500ms]\na: Rect, size: (60, 60), color: (0.0, 0.0, 1.0, 1.0), at: (100, 100) [500ms]\n",
        ),
    ];

    for (label, tail) in cases {
        let (ast, errors) = animatix_syntax::parser::parse_source(&(prelude.to_string() + tail));
        assert!(errors.is_empty(), "{label}: parse errors {errors:?}");
        let timeline =
            Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
                .output;
        let track = timeline
            .world
            .tracks
            .get("a")
            .unwrap_or_else(|| panic!("{label}: no track 'a'"));

        // The frame before the stamp is still the declared ellipse, and still
        // carries no blue at all.
        assert_eq!(
            timeline
                .world
                .shape(track.id)
                .shape_type
                .get(999, crate::timeline::ShapeType::Rect),
            crate::timeline::ShapeType::Ellipse,
            "{label}: the frame before the morph already shows the target shape"
        );
        assert!(
            track.style.color.get(999, [0.0; 4])[2] < 0.01,
            "{label}: the pre-morph frame started from the target colour: {:?}",
            track.style.color.get(999, [0.0; 4])
        );
        // And the morph itself still lands on the target.
        assert_eq!(
            timeline
                .world
                .shape(track.id)
                .shape_type
                .get(1500, crate::timeline::ShapeType::Ellipse),
            crate::timeline::ShapeType::Rect,
            "{label}: the morph never reached its target shape"
        );
        assert!(
            track.style.color.get(1500, [0.0; 4])[2] > 0.99,
            "{label}: the colour morph did not finish: {:?}",
            track.style.color.get(1500, [0.0; 4])
        );
    }
}

/// An undated `.text =` swap must not begin before its own stamp.
///
/// The track model reads any two consecutive keyframes as an interpolation
/// segment, so unless the change is fenced the previously declared string
/// interpolates all the way into the new one — and the cross-fade branch in
/// `primitives` then compiles and draws *both* strings across the whole
/// preceding gap. That is the roadmap's "static keyframe `.text =` assignments
/// overprint", which forced `web/demos/hash/scene.amx` onto one actor per string.
#[test]
fn an_undated_text_swap_holds_the_old_string_until_its_own_stamp() {
    let source = "\
config { resolution: (400, 300), duration: 3 }
t: Text, text: \"first\", font_size: 30, at: (100, 100)
#2s
t.text = \"second\"
";
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output;
    let text_track = timeline.world.tracks.get("t").expect("text track");
    let text = timeline.world.text(text_track.id);

    for ms in [0u64, 500, 1000, 1999] {
        assert_eq!(
            text.text_content.get(ms, String::new()),
            "first",
            "the text swap leaked into the frame before its stamp at {ms}ms"
        );
        // The renderer cross-fades whenever a segment is open *between two
        // different strings*; a segment whose endpoints are the same string is
        // harmless, so that is the property asserted here.
        if let Some((_, prev, found, _, _)) =
            text.text_content.as_ref().and_then(|track| track.interpolation_segment(ms))
        {
            assert_eq!(
                prev, found,
                "two different strings were interpolating at {ms}ms, before the swap is due"
            );
        }
    }

    assert_eq!(
        text.text_content.get(2000, String::new()),
        "second",
        "the swap did not take effect at its own stamp"
    );
}

/// The numeric half of the same rule: a bare assignment is a step at its stamp.
///
/// `docs/spec.md` calls this an "Instant Change". The track model has no step
/// keyframe, so the value has to be fenced one millisecond before the stamp or
/// the previous keyframe eases into it across the whole gap — which is what
/// happened until `write_property_plan_slot` learned to fence undated writes.
#[test]
fn an_undated_numeric_assignment_is_a_step_not_a_backwards_ramp() {
    let source = "\
config { resolution: (400, 300), duration: 3 }
r: Rect, size: (40, 40), color: (1.0, 0.0, 0.0, 1.0), opacity: 1.0, at: (200, 200)
#2s
r.opacity = 0.0
";
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new())
            .output;
    let rect = timeline.world.tracks.get("r").expect("rect track");

    for ms in [0u64, 500, 1000, 1999] {
        assert_eq!(
            rect.style.opacity.get(ms, 0.0),
            1.0,
            "the rect was already fading at {ms}ms, two seconds before the assignment"
        );
    }
    assert_eq!(
        rect.style.opacity.get(2000, 1.0),
        0.0,
        "the step did not take effect at its own stamp"
    );
}

#[test]
fn gradient_paints_are_parsed_stamped_and_interpolated() {
    // `fill_gradient:` / `stroke_gradient:` are registry-backed paints: the
    // declaration seeds them, dated assignments interpolate, and the shared
    // `gradient_extend:` / `gradient_space:` settings ride the stamp.
    let source = r##"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

r: Rect, size: (200, 120), at: (320, 180), color: accent.primary,
  fill_gradient: linear(135, {"#ff0000", "#00ff00"}),
  gradient_extend: "reflect"
s: Path, commands: {move_to(100, 300), line_to(500, 300)},
  stroke: accent.warning, stroke_width: 6, fill_opacity: 0.0,
  stroke_gradient: sweep(45, {"#ffffff", "#888888", "#000000"})

#0s
r.fill_gradient = radial((0.5, 0.5), 0.6, {(0%, "#ff0000"), (100%, "#0000ff")})
#1s
r.fill_gradient = radial((0.5, 0.5), 0.2, {(0%, "#ff0000"), (100%, "#0000ff")}) [1s]
    "##;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed),
        "expected only never-revealed hints, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let paints_at = |time: f64| -> (
        Option<crate::timeline::GradientSpec>,
        Option<crate::timeline::GradientSpec>,
    ) {
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time,
            crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        let mut found = (None, None);
        for item in &program.items {
            for command in &item.commands {
                if let crate::primitives::RenderCommand::Paths { paths } = command {
                    for vp in paths {
                        if vp.fill_gradient.is_some() {
                            found.0 = vp.fill_gradient.as_deref().cloned();
                        }
                        if vp.stroke_gradient.is_some() {
                            found.1 = vp.stroke_gradient.as_deref().cloned();
                        }
                    }
                }
            }
        }
        found
    };

    // The `[1s]` ramp runs 1s..2s: it holds 0.6 before it starts and lands on
    // 0.4 halfway through, which is what makes a painted ramp animatable.
    let early = paints_at(0.5).0.expect("declared ramp must be stamped early");
    match early.shape {
        crate::timeline::GradientShape::Radial { radius, .. } => {
            assert!((radius - 0.6).abs() < 1e-3, "ramp holds before its duration, got {radius}");
        },
        other => panic!("expected the radial ramp, got {other:?}"),
    }
    let (fill, stroke) = paints_at(1.5);
    let fill = fill.expect("the dated ramp must be stamped on the fill");
    match fill.shape {
        crate::timeline::GradientShape::Radial { center, radius } => {
            assert!((radius - 0.4).abs() < 1e-3, "ramp radius must lerp, got {radius}");
            assert!(
                (center[0] - 0.5).abs() < 1e-3 && (center[1] - 0.5).abs() < 1e-3,
                "center must hold at (0.5, 0.5), got {center:?}"
            );
        },
        other => panic!("expected the interpolated ramp to stay radial, got {other:?}"),
    }
    assert_eq!(fill.stops.len(), 2);
    assert_eq!(
        fill.stops[0].color,
        [1.0, 0.0, 0.0, 1.0],
        "a #rrggbb stop must resolve as a color, not fall back to gray"
    );
    assert_eq!(
        fill.extend,
        crate::timeline::GradientExtend::Reflect,
        "gradient_extend: must ride the stamp"
    );
    assert_eq!(fill.space, crate::timeline::GradientSpace::Oklab);

    // A three-color sweep spreads its stops evenly at 0, 0.5, 1.
    let stroke = stroke.expect("stroke_gradient: must be stamped on the stroke");
    let offsets: Vec<f32> = stroke.stops.iter().map(|s| s.offset).collect();
    assert_eq!(stroke.stops[1].color, [0.533_333_36, 0.533_333_36, 0.533_333_36, 1.0]);
    assert_eq!(offsets, vec![0.0, 0.5, 1.0], "unspaced stops spread evenly");
    assert!(
        matches!(
            stroke.shape,
            crate::timeline::GradientShape::Sweep { angle, .. } if (angle - 45.0).abs() < 1e-3
        ),
        "sweep angle must come through authored, got {:?}",
        stroke.shape
    );

    // The brush actually materializes: positioned in the shape's bbox it keeps
    // every stop, which is what the renderer hands to Vello.
    let brush = stroke.to_peniko(kurbo::Rect::new(100.0, 297.0, 500.0, 303.0), 1.0);
    assert_eq!(brush.stops.len(), 3);
    assert!(matches!(brush.kind, vello::peniko::GradientKind::Sweep(_)));
}

#[test]
fn seamless_loop_lints_values_that_do_not_wrap() {
    // `config { seamless_loop: true }` asks the build to check the seam: a
    // keyframed value that differs between the first and last frame jumps on
    // every replay. A scene whose values do wrap must stay quiet.
    let broken = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary
#0s
fade-in box [200ms]
box.at = (200, 180)
#2s
box.at = (440, 180)
    "#;
    // The fixed version of the same scene: no entrance (an actor that fades in
    // is transparent at frame 0 and opaque at the end, which is itself a seam)
    // and a position that returns to where it started.
    let seamless = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary, opacity: 1.0
#0s
box.at = (200, 180)
#1s
box.at = (440, 180)
#2s
box.at = (200, 180)
    "#;

    let codes = |source: &str| -> Vec<String> {
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast.unwrap(),
            &std::collections::HashMap::new(),
        );
        report.diagnostics.iter().map(|d| d.code.to_string()).collect()
    };

    let broken_codes = codes(broken);
    assert!(
        broken_codes.iter().any(|c| c == "loop-not-seamless"),
        "a scene whose position does not wrap must warn, got {broken_codes:?}"
    );
    // The warning must name the offending property so the author can fix it.
    let warning = report_message(broken);
    assert!(
        warning.contains("`box.position`"),
        "warning should name `box.position`, got {warning}"
    );

    let seamless_codes = codes(seamless);
    assert!(
        !seamless_codes.iter().any(|c| c == "loop-not-seamless"),
        "a scene that returns to its start must not warn, got {seamless_codes:?}"
    );
}

/// The camera is not a track, so the seam walk over `tracks` cannot see it. A
/// scene that ends pushed in and restarts unzoomed jumps at every replay exactly
/// like an un-wrapped `at`, so `Camera::seam_pairs` feeds the same check.
#[test]
fn seamless_loop_lints_camera_axes() {
    let pushed = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }
box: Rect, size: (100, 100), at: (320, 180), color: accent.primary, opacity: 1.0
#0s
camera.zoom = 1.0
#1s
camera.zoom = 1.8
"#;
    let wrapped = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), seamless_loop: true }
box: Rect, size: (100, 100), at: (320, 180), color: accent.primary, opacity: 1.0
#0s
camera.zoom = 1.0
camera.at = (0, 0)
#1s
camera.zoom = 1.8
camera.at = (60, 0)
#2s
camera.zoom = 1.0
camera.at = (0, 0)
"#;

    let warning = report_message(pushed);
    assert!(
        warning.contains("`camera.zoom`"),
        "a loop that ends pushed in must name the axis, got {warning}"
    );
    let quiet = report_message(wrapped);
    assert!(
        !quiet.contains("camera."),
        "a camera that returns to its start must not warn, got {quiet}"
    );
}

/// The text of the first `loop-not-seamless` warning for `source`.
fn report_message(source: &str) -> String {
    let (ast, _) = animatix_syntax::parser::parse_source(source);
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::LoopNotSeamless)
        .map(|d| d.message.clone())
        .unwrap_or_default()
}

#[test]
fn settle_in_and_pop_in_ramp_scale_onto_the_authored_scale() {
    // The entrance presets are a fade plus a scale ramp *onto* what the actor
    // already authored, so `scale: 2` still ends at 2 instead of being
    // flattened to 1 by the entrance.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

card: Rect, size: (120, 80), at: (200, 180), color: accent.primary
chip: Ellipse, size: (60, 60), at: (420, 180), color: accent.warning

#0.5s
settle-in card [500ms]
#1s
pop-in chip [500ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected a clean build, got {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    use crate::timeline::property_registry::lookup_property;
    use crate::timeline::read_property_value;

    let read = |label: &str, prop: &str, ms: u64| {
        let schema = lookup_property(prop).expect("property");
        let track = timeline.world.tracks.get(label).unwrap();
        read_property_value(track, schema.field, ms)
            .unwrap_or_else(|| (schema.default_value)(&timeline.world.caps(track.id)))
    };

    // settle-in: 0.5s..1.0s, scale 0.92 -> 1.0 relative to the sampled scale,
    // opacity 0 -> 1.
    let crate::timeline::PropertyValue::F32(scale_start) = read("card", "scale", 500) else {
        panic!("expected a numeric scale");
    };
    let crate::timeline::PropertyValue::F32(scale_end) = read("card", "scale", 1000) else {
        panic!("expected a numeric scale");
    };
    assert!(
        (scale_start - 0.92).abs() < 1e-3,
        "settle-in must start 8% under its resting scale, got {scale_start}"
    );
    assert!(
        (scale_end - 1.0).abs() < 1e-3,
        "the ramp must land back on the resting scale, got {scale_end}"
    );
    let crate::timeline::PropertyValue::F32(opacity_mid) = read("card", "opacity", 750) else {
        panic!("expected a numeric opacity");
    };
    assert!(
        opacity_mid > 0.0 && opacity_mid < 1.0,
        "settle-in must be mid-fade at the halfway point, got {opacity_mid}"
    );

    // pop-in: starts at 60% and overshoots past its target, which is what the
    // `back` arrival buys.
    let crate::timeline::PropertyValue::F32(pop_start) = read("chip", "scale", 1000) else {
        panic!("expected a numeric scale");
    };
    assert!(
        (pop_start - 0.6).abs() < 1e-3,
        "pop-in must start at 60% scale, got {pop_start}"
    );
    let mut max_scale = 0.0_f32;
    for ms in (1000..=1500).step_by(25) {
        let crate::timeline::PropertyValue::F32(v) = read("chip", "scale", ms) else {
            continue;
        };
        max_scale = max_scale.max(v);
    }
    assert!(max_scale > 1.0, "pop-in must overshoot past its target, peaked at {max_scale}");
}

#[test]
fn anticipate_inserts_a_counter_move_before_the_travel() {
    // `[anticipate: 100ms]` leans the actor back by 12% of its travel before
    // the move starts, so the translation reads as intentional.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

dot: Ellipse, size: (40, 40), at: (200, 180), color: accent.primary

#0s
fade-in dot [200ms]
#1s
move dot [to: (200, 0), 500ms, anticipate: 100ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected a clean build, got {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    use crate::timeline::property_registry::lookup_property;
    use crate::timeline::read_property_value;

    let offset_at = |ms: u64| -> [f32; 2] {
        let schema = lookup_property("shift").expect("property");
        match read_property_value(timeline.world.tracks.get("dot").unwrap(), schema.field, ms) {
            Some(crate::timeline::PropertyValue::Vec2(v)) => v,
            other => panic!("expected a Vec2 offset at {ms}ms, got {other:?}"),
        }
    };

    let held = offset_at(900);
    let leaned = offset_at(950);
    let [hx, hy] = held;
    let [lx, ly] = leaned;
    assert!(
        lx < hx - 1.0,
        "the actor must lean back (negative x) before travelling +200: held {held:?}, leaned {leaned:?}"
    );
    assert!(
        (ly - hy).abs() < 1e-3,
        "the lean follows the travel axis only, got {held:?} -> {leaned:?}"
    );
    let at_start = offset_at(1000);
    assert!(
        at_start[0] < 0.0,
        "the move must begin from the leaned position, got {at_start:?}"
    );
    let landed = offset_at(1500);
    assert!(
        (landed[0] - 200.0).abs() < 1.0 && landed[1].abs() < 1e-3,
        "the travel must still land on the authored offset, got {landed:?}"
    );
}

#[test]
fn cli_defines_shadow_the_authored_let_defaults() {
    // `--set NAME=VALUE` is the template × data seam: the file declares its
    // defaults with a top-level `let`, and the command line replaces them
    // before anything resolves against the build environment.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

let label = "Default"
let tint = (1.0, 0.0, 0.0, 1.0)

t: Text, text: label, at: (200, 180), color: tint
u: Text, text: label, at: (200, 240)

#0s
fade-in t [200ms]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let namespaces = std::collections::HashMap::new();

    let defines = vec![
        ("label".to_string(), "\"Overridden\"".to_string()),
        ("tint".to_string(), "(0.0, 1.0, 0.0, 1.0)".to_string()),
    ];
    let report =
        crate::timeline::Timeline::build_with_diagnostics_and_defines(&ast, &namespaces, &defines);
    let owned: Vec<String> = report.diagnostics.iter().map(|d| format!("{:?}", d.code)).collect();
    assert!(
        owned.iter().all(|c| c.contains("NeverRevealed")),
        "the override must not introduce diagnostics, got {owned:?}"
    );
    let timeline = report.output;

    // The override must reach the *resolved* properties, not just the
    // environment — re-applying the values after the walk would leave every
    // property that already resolved against them at its authored default.
    let sampled = |label: &str, prop: &str| {
        let schema = crate::timeline::property_registry::lookup_property(prop).unwrap();
        let track = timeline.world.tracks.get(label).unwrap();
        timeline
            .world
            .read_property_value(track.id, schema.field, 0)
            .unwrap_or_else(|| (schema.default_value)(&timeline.world.caps(track.id)))
    };
    assert_eq!(
        sampled("t", "text"),
        crate::timeline::PropertyValue::String("Overridden".to_string()),
        "`--set label` must reach text resolved during the walk"
    );
    assert_eq!(
        sampled("u", "text"),
        crate::timeline::PropertyValue::String("Overridden".to_string())
    );
    assert_eq!(
        sampled("t", "color"),
        crate::timeline::PropertyValue::Color([0.0, 1.0, 0.0, 1.0]),
        "`--set tint` must reach the fill"
    );
}

#[test]
fn beat_stamps_resolve_against_the_declared_tempo() {
    // `#2b` is a musical stamp: its millisecond position comes from the scene's
    // `config { bpm: … }`, so a retempo moves the whole arrangement without
    // editing any stamp.
    let source = |config: &str| {
        format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (640, 360), {config} }}

box: Rect, size: (100, 100), at: (200, 180), color: accent.primary, opacity: 1.0
#0s
box.shift = (0, 0)
#2b
box.shift = (240, 0)
"#
        )
    };
    let shift_at = |config: &str, ms: u64| -> [f32; 2] {
        let (ast, errors) = animatix_syntax::parser::parse_source(&source(config));
        assert!(errors.is_empty(), "Parse errors: {errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            ast.as_ref().unwrap(),
            &std::collections::HashMap::new(),
        );
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed
                    || d.code == crate::diagnostics::DiagnosticCode::UnknownConfigKey
                    || d.code == crate::diagnostics::DiagnosticCode::InvalidConfigValue),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        use crate::timeline::read_property_value;
        let schema = crate::timeline::property_registry::lookup_property("shift").unwrap();
        let track = report.output.world.tracks.get("box").unwrap();
        match read_property_value(track, schema.field, ms) {
            Some(crate::timeline::PropertyValue::Vec2(v)) => v,
            other => panic!("expected a Vec2 shift, got {other:?}"),
        }
    };

    // 60 bpm: two beats is exactly two seconds, so the move has not started at
    // 1999 ms and has landed by 2000 ms.
    assert_eq!(shift_at("bpm: 60", 1999), [0.0, 0.0]);
    assert_eq!(shift_at("bpm: 60", 2000), [240.0, 0.0]);
    // 120 bpm is the documented default, so the same stamp lands at one second.
    assert_eq!(shift_at("bpm: 120", 1000), [240.0, 0.0]);
    assert_eq!(shift_at("", 1000), [240.0, 0.0], "no bpm means the default tempo");
    assert_eq!(shift_at("bpm: 120", 999), [0.0, 0.0]);
}

/// `icon: "check"` must expand to exactly the `Path` geometry an equivalent
/// hand-written `commands:` list produces — the whole contract of the property.
#[test]
fn icon_property_expands_to_commands_geometry() {
    // `check`'s bundled path data is `M20 6 9 17l-5-5`, which the shared
    // SVG parser lowers to move_to(20,6) + implicit line_to(9,17) +
    // relative line_to(4,12).
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        p_icon: Path, icon: "check", at: (100, 100)
        p_manual: Path,
          commands: {move_to(20, 6), line_to(9, 17), line_to(4, 12)},
          at: (300, 100)
    "#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );

    let geometry = |label: &str| -> Vec<kurbo::PathEl> {
        let track = report
            .output
            .world
            .tracks
            .get(label)
            .unwrap_or_else(|| panic!("no track {label}"));
        let paths = track.evaluate_vector_paths_value(report.output.world.shape(track.id), 0);
        let mut elements = Vec::new();
        for vp in paths {
            elements.extend(vp.path.elements().iter().copied());
        }
        elements
    };

    let from_icon = geometry("p_icon");
    let from_manual = geometry("p_manual");
    assert!(!from_icon.is_empty(), "`icon: \"check\"` produced no path geometry at all");
    assert_eq!(
        from_icon, from_manual,
        "`icon: \"check\"` geometry must match the hand-written commands list"
    );
}

/// An unknown icon name must warn (naming close candidates) rather than draw
/// nothing in silence.
#[test]
fn unknown_icon_warns() {
    let source = r#"
        config { colorscheme: "editorial-dark", resolution: (640, 360) }

        p: Path, icon: "nope", at: (100, 100)
    "#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );

    let warned = report.diagnostics.iter().find(|d| {
        d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
            && d.message.contains("nope")
    });
    assert!(
        warned.is_some(),
        "unknown `icon` value must emit an InvalidPropertyValue warning; got {:?}",
        report.diagnostics
    );
}

#[test]
fn an_authored_scale_reaches_the_transform() {
    // `scale:` on a declaration was dropped on the floor for every actor kind:
    // it is not in the legacy per-primitive loop and was not routed through the
    // generic engine, so `scale: 4.0` rendered at 1.0 while the keyframed
    // `x.scale = 4.0` worked. Bundled icons made it visible — a 24 px mark that
    // could not be enlarged was otherwise unusable.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (20, 20), at: (200, 180), color: accent.primary, scale: 4.0
half: Rect, size: (20, 20), at: (400, 180), color: accent.primary, scale: 0.5
plain: Rect, size: (20, 20), at: (500, 180), color: accent.primary

#0s
fade-in box [200ms]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;
    use crate::timeline::read_property_value;
    let schema = crate::timeline::property_registry::lookup_property("scale").unwrap();
    let scale_of = |label: &str| -> Option<f32> {
        let track = timeline.world.tracks.get(label).unwrap();
        match read_property_value(track, schema.field, 0) {
            Some(crate::timeline::PropertyValue::F32(v)) => Some(v),
            None => None,
            other => panic!("expected a numeric scale for {label}, got {other:?}"),
        }
    };
    assert_eq!(scale_of("box"), Some(4.0), "authored scale must land");
    assert_eq!(scale_of("half"), Some(0.5), "a shrink must land too");
    assert_eq!(
        scale_of("plain"),
        None,
        "an actor that never authors scale gets no track, and renders at the 1.0 default"
    );
}

#[test]
fn move_along_bakes_arc_length_keyframes_onto_the_route() {
    // `move … [along: {…}]` must land on the route at constant speed: sampling by
    // arc length is the whole point, since the bezier parameter would crawl
    // through the curve and sprint along the straight.
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

dot: Ellipse, size: (20, 20), at: (100, 100), color: accent.primary, opacity: 1.0

#0s
dot.at = (100, 100)
#0.5s
move dot [along: {move_to(100, 100), line_to(500, 100)}, 2s]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "Parse errors: {errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        ast.as_ref().unwrap(),
        &std::collections::HashMap::new(),
    );
    let offenders: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code != crate::diagnostics::DiagnosticCode::NeverRevealed)
        .collect();
    assert!(offenders.is_empty(), "unexpected diagnostics: {offenders:?}");
    let timeline = report.output;
    use crate::timeline::read_property_value;
    let schema = crate::timeline::property_registry::lookup_property("position").unwrap();
    let track = timeline.world.tracks.get("dot").unwrap();
    let at = |ms: u64| match read_property_value(track, schema.field, ms) {
        Some(crate::timeline::PropertyValue::Vec2(v)) => v,
        other => panic!("expected a position at {ms}ms, got {other:?}"),
    };

    // 2.5s is the far end of the 2s travel; 1.5s is the halfway mark, which on a
    // 400-unit straight route must sit at x = 300 (arc length, not the parameter).
    let mid = at(1500);
    assert!(
        (mid[0] - 300.0).abs() < 6.0,
        "the halfway sample must sit at the half-length point, got {mid:?}"
    );
    let end = at(2500);
    assert!(
        (end[0] - 500.0).abs() < 2.0 && (end[1] - 100.0).abs() < 2.0,
        "the route's end must be reached, got {end:?}"
    );
    // Keyframed densely enough that a curve route reads as smooth.
    assert!(
        track.geometry.position.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) > 8,
        "the route must be baked into many samples"
    );
}

#[test]
fn draw_in_by_word_steps_through_the_words() {
    // `draw-in` on text is a typewriter by default; `by: word` brings each word
    // in as a step, which is the difference between a caption typing out
    // letter-by-letter and a line of kinetic typography landing in phrases.
    let build = |extra: &str| {
        let source = format!(
            r#"
config {{ colorscheme: "editorial-dark", resolution: (640, 360) }}

line: Text, text: "aa bb cc dd", at: (320, 180), font_size: 40
#0s
draw-in line [4s{extra}]
"#
        );
        let (ast, errors) = animatix_syntax::parser::parse_source(&source);
        assert!(errors.is_empty(), "Parse errors: {errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            ast.as_ref().unwrap(),
            &std::collections::HashMap::new(),
        );
        let offenders: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code != crate::diagnostics::DiagnosticCode::NeverRevealed)
            .collect();
        assert!(offenders.is_empty(), "unexpected diagnostics: {offenders:?}");
        report.output
    };
    let progress_at = |timeline: &crate::timeline::Timeline, ms: u64| -> f32 {
        let track = timeline.world.tracks.get("line").unwrap();
        timeline.world.text(track.id).char_progress.get(ms, 1.0)
    };

    let words = build(", by: word");
    // Four words over 4s: at 1.5s the second word has landed (6 of 11 chars) and
    // the third has not, so progress sits at the word boundary, not mid-word.
    let at_1500 = progress_at(&words, 1500);
    assert!(
        (at_1500 - 6.0 / 11.0).abs() < 0.06,
        "expected the second word boundary (~{:.3}), got {at_1500:.3}",
        6.0 / 11.0
    );
    // Each word lands at the start of its slot and holds: 2.5s is inside the
    // third slot, so the third boundary (9 of 11 chars) must still be holding.
    assert!(
        (progress_at(&words, 2500) - 9.0 / 11.0).abs() < 0.02,
        "the third boundary must hold until the fourth word, got {}",
        progress_at(&words, 2500)
    );
    assert!(
        progress_at(&words, 1600) > 0.5 && progress_at(&words, 1600) < 0.6,
        "a word must hold its boundary rather than creep, got {}",
        progress_at(&words, 1600)
    );
    assert!((progress_at(&words, 4000) - 1.0).abs() < 1e-3, "must finish full");

    // The default stays a smooth typewriter, and it inherits the role default
    // for entrances: `expo-out` puts a quarter of the *time* at ~82% of the
    // characters, which is why the reveal reads as arriving rather than ticking.
    let chars = build("");
    let mid = progress_at(&chars, 1000);
    assert!(
        mid > 0.7 && mid < 0.95,
        "the default reveal is a smooth ramp under expo-out, got {mid}"
    );
    assert!(
        progress_at(&chars, 200) < 0.35,
        "and it must still start sparse, got {}",
        progress_at(&chars, 200)
    );
}

#[test]
fn pre_keyframe_actor_declaration_has_decl_span_and_explicit_warning() {
    let source = "box: Rect, size: (100, 100)\n\n#1s\nbox.at = (200, 200)\n";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let mut ast = ast.expect("parsed AST");
    if let crate::ast::Stmt::ActorDecl { span, .. } = &mut ast[0] {
        *span = Some(animatix_syntax::ast::Span::new(1, 1, 1, 27));
    }
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let track = report.output.world.tracks.get("box").expect("box track must exist");
    let span = report
        .output
        .world
        .decl_span(track.id)
        .copied()
        .expect("track must preserve its declaration span");
    assert_eq!(span.start_line, 1, "declaration starts on line 1");

    let diag = report
        .diagnostics
        .iter()
        .find(|d| {
            d.code == crate::diagnostics::DiagnosticCode::NeverRevealed
                && d.location.subject.as_deref() == Some("box")
        })
        .expect("must emit NeverRevealed for pre-keyframe actor without entrance action");

    assert_eq!(diag.location.line, Some(1), "diagnostic must carry the declaration line");
    assert_eq!(diag.location.column, Some(1), "diagnostic must carry the declaration column");
    assert!(
        diag.message.contains("implicit `opacity: 0.0`"),
        "diagnostic message must explain the implicit opacity default: {}",
        diag.message
    );
    assert!(
        diag.message.contains("opacity: 1.0"),
        "diagnostic message must advise how to make it visible from the start: {}",
        diag.message
    );
}

#[test]
fn pre_keyframe_actor_with_explicit_opacity_assignment_does_not_warn_never_revealed() {
    let source = "box: Rect, size: (100, 100)\n\n#1s\nbox.opacity = 1.0\n";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::NeverRevealed
                && d.location.subject.as_deref() == Some("box")),
        "subsequent opacity assignment must count as a reveal: {:?}",
        report.diagnostics
    );
}

#[test]
fn unary_minus_on_dotted_path_resolves_in_graph_domain() {
    let source = "let geo = Geo { y_hi: 1.75 }\ng: Graph, size: (400, 300), x_domain: (-5, 5), y_domain: (-geo.y_hi, geo.y_hi)";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    match report.output.env().get("g_y_domain") {
        Some(Value::Vec2([min, max])) => {
            assert_eq!([min, max], [-1.75, 1.75]);
        },
        other => panic!("expected Vec2([-1.75, 1.75]) for g_y_domain, got {other:?}"),
    }
}

#[test]
fn unary_minus_on_imported_module_path_resolves_in_graph_domain() {
    let dir = std::env::temp_dir().join(format!("animatix_geo_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create dir");
    std::fs::write(dir.join("geo.amx"), "pub let y_hi = 1.75\n").expect("write geo");
    let scene_path = dir.join("scene.amx");
    std::fs::write(
        &scene_path,
        "import \"./geo.amx\" as geo\ng: Graph, opacity: 1.0, size: (400, 300), x_domain: (-5, 5), y_domain: (-geo.y_hi, geo.y_hi)\n",
    )
    .expect("write scene");
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let program = graph.load_program(&scene_path).expect("program loads");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &program.namespaces);
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "diagnostics: {:?}",
        report.diagnostics
    );
    match report.output.env().get("g_y_domain") {
        Some(Value::Vec2([min, max])) => {
            assert_eq!([min, max], [-1.75, 1.75]);
        },
        other => panic!("expected Vec2([-1.75, 1.75]) for g_y_domain, got {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
