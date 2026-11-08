use super::*;

#[test]
fn always_overrides_keyframes_warnings() {
    let cases = [
        (
            "box1: Rect, size: (100, 100), opacity: 0.8\n#2s\nbox1.opacity = 0.3\nalways { box1.opacity = 0.5 }",
            true,
            "Expected warning when conflict exists"
        ),
        (
            "always { box1.opacity = 0.5 }",
            false,
            "Should NOT emit warning when actor doesn't exist"
        ),
        (
            "box1: Rect, size: (100, 100)\nalways { box1.rotation = 0.5 }",
            false,
            "Should NOT emit warning when no keyframe conflict"
        ),
    ];

    for (source, expect_warning, msg) in cases {
        let report = support::build(&format!("#0s\n{}", source));
        let has_warning = report.diagnostics.iter().any(|d| {
            d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes
        });
        assert_eq!(has_warning, expect_warning, "{}", msg);
    }
}

#[test]
fn absolute_position_on_layout_managed_child_warning() {
    // A child of a Row with explicit `at` should emit a warning.
    let source = r#"
        #0s
        row1: Row, size: (400, 100) {
            child1: Rect, size: (50, 50), at: (100, 200)
        }
    "#;
    let report = support::build(source);

    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
    });
    assert!(
        has_warning,
        "Expected AbsolutePositionOnLayoutManagedChild warning when a Row child has 'at'"
    );
    // The message must not claim the value is ignored: the renderer applies
    // `at` as a manual local offset on top of the layout slot.
    let warning = report
        .diagnostics
        .iter()
        .find(|d| {
            d.code
                == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
        })
        .expect("warning present");
    assert!(
        warning.message.contains("manual local offset"),
        "`at` warning should describe the honoured offset behaviour, got: {}",
        warning.message
    );
}

#[test]
fn stack_gap_warns_it_is_ignored() {
    let source = r#"
#0s
stack0: Stack, size: (200, 200), gap: 12 {
  a: Rect, size: (50, 50), color: accent.primary
}
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::IgnoredStackGap)
        .expect("Stack gap must emit IgnoredStackGap");
    assert!(
        warning.message.contains("Stack container 'stack0'"),
        "warning should name the Stack, got: {}",
        warning.message
    );
}

#[test]
fn stack_without_gap_does_not_warn() {
    let source = r#"
#0s
stack0: Stack, size: (200, 200) {
  a: Rect, size: (50, 50), color: accent.primary
}
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::IgnoredStackGap),
        "no gap should mean no warning"
    );
}

#[test]
fn absolute_position_on_layout_managed_child_no_warning_without_at() {
    // A child of a Row WITHOUT `at` should NOT emit the warning.
    let source = r#"
        #0s
        row1: Row, size: (400, 100) {
            child1: Rect, size: (50, 50)
        }
    "#;
    let report = support::build(source);

    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
    });
    assert!(
        !has_warning,
        "Should NOT emit AbsolutePositionOnLayoutManagedChild warning when child has no 'at'"
    );
}

#[test]
fn conflicting_at_and_anchor_warning() {
    let source = r#"
        box0: Rect, size: (50, 50), at: (100, 100), anchor: scene.center
    "#;
    let report = support::build(source);
    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::ConflictingPositionBinding
    });
    assert!(
        has_warning,
        "Expected ConflictingPositionBinding warning, got: {:?}",
        report.diagnostics
    );
}

/// A Text whose content has a character the shaping path cannot cover warns
/// `missing-glyph` at build time instead of drawing tofu in silence. U+2065
/// is permanently unassigned and sits inside the fast path's Latin gate
/// (0x2000–0x206F), so no font can ever map it and the warning is
/// machine-independent.
#[test]
fn missing_glyph_warning_fires_for_unassignable_codepoints() {
    let source = format!(
        r#"
config {{ resolution: (320, 180) }}
t: Text, text: "a{}b", at: (160, 90)
"#,
        '\u{2065}'
    );
    let report = support::build(&source);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::MissingGlyph),
        "expected a missing-glyph warning, got: {:?}",
        report.diagnostics
    );
}

/// A declaration property no build path consumes (the classic `colour:` typo)
/// warns at build time instead of dropping in silence. Registry-named
/// properties stay silent — the corpus has none of these warnings.
#[test]
fn unknown_declaration_property_warns() {
    let source = r#"
config { resolution: (320, 180) }
r: Rect, size: (100, 80), colour: (1, 0, 0, 1), at: (160, 90)
"#;
    let report = support::build(source);
    let warned = report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownProperty)
        .expect("expected an unknown-property warning for 'colour'");
    assert!(warned.message.contains("colour"), "warning names the property: {warned:?}");
}

fn inapplicable_warnings(source: &str) -> Vec<String> {
    let report = support::build(source);
    report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::InapplicableProperty)
        .map(|d| d.message.clone())
        .collect()
}

/// The case `unknown-property` cannot see: a property name that really exists,
/// on a type that never reads it. `colour:` at least looks wrong in the source;
/// this looks entirely correct and the value disappears anyway.
#[test]
fn a_property_the_primitive_never_reads_warns() {
    let warnings = inapplicable_warnings(
        r#"
config { resolution: (320, 180) }
t: Text, text: "caption", font_size: 20, stroke_width: 6, at: (160, 90)
"#,
    );
    assert_eq!(
        warnings.len(),
        1,
        "`stroke_width` is not a Text property and should warn once: {warnings:?}"
    );
    assert!(
        warnings[0].contains("stroke_width") && warnings[0].contains("(Text)"),
        "the warning should name the actor type and the dropped property: {}",
        warnings[0]
    );
}

/// `text_max_width` applies to Text and Legend actors and must not emit
/// an InapplicableProperty warning on either host.
#[test]
fn a_wrap_width_on_a_text_actor_is_silent() {
    let warnings = inapplicable_warnings(
        r#"
config { resolution: (320, 180) }
t: Text, text: "a long caption that wraps", font_size: 20, text_max_width: 240, at: (160, 90)
l: Legend, at: (160, 150), title: "Series", text_max_width: 200
"#,
    );
    assert!(warnings.is_empty(), "text wrap width warned on both hosts: {warnings:?}");
}

/// Build `source` and report whether the always/keyframe conflict fired.
fn trips_always_override(source: &str) -> bool {
    let report = support::build(source);
    report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes)
}

/// Properties seeded only by declaration defaults must not trigger
/// AlwaysOverridesKeyframes when an `always` block writes to them.
#[test]
fn a_seeded_dash_offset_declaration_does_not_trip_the_lint() {
    assert!(!trips_always_override(
        r#"
config { resolution: (320, 180), duration: 4 }
rail: Path, commands: {move_to(20, 90), line_to(300, 90)}, stroke: accent.primary,
  stroke_width: 2, fill_opacity: 0.0, dash_pattern: {10, 7}, opacity: 1.0
always {
  rail.dash_offset = (rail.dash_offset + t * 34.0) % 17.0
}
"#
    ));
}

/// Unanimated declared properties that remain constant across the timeline
/// must not trigger AlwaysOverridesKeyframes when rewritten in `always`.
#[test]
fn a_constant_declared_size_does_not_trip_the_lint() {
    assert!(!trips_always_override(
        r#"
config { resolution: (320, 180), duration: 4 }
b: Rect, size: (100, 60), at: (160, 90), color: accent.primary, opacity: 1.0
always {
  b.size = (100.0, 60.0)
}
"#
    ));
}

/// …and the lint still fires when the author really did keyframe the property
/// the `always` block wins every frame.
#[test]
fn a_keyframed_value_still_trips_the_lint() {
    assert!(trips_always_override(
        r#"
config { resolution: (320, 180), duration: 4 }
b: Rect, size: (100, 60), at: (160, 90), color: accent.primary, opacity: 1.0
#1s
b.size = (140, 80) [1s]
always {
  b.size = (100.0, 60.0)
}
"#,
    ));
}

/// `Equation` aggregates child `Fragment`s and renders math glyphs with `color`,
/// so declaring `color:` must be accepted cleanly without `inapplicable-property`.
#[test]
fn equation_accepts_color_without_warning() {
    let warnings = inapplicable_warnings(
        r#"
config { resolution: (320, 180) }
eq: Equation, font_size: 48, color: (1.0, 0.0, 0.0, 1.0), at: (160, 90) {
    e: Fragment, text: "E"
}
"#,
    );
    assert!(
        warnings.is_empty(),
        "Equation consumes color at evaluate time; it must not warn as inapplicable: {warnings:?}"
    );
}

/// `Line` and `Arrow` support frame-time dynamic anchor references (`from: pen.center`),
/// which must not be rejected at build time as unknown lookup paths.
#[test]
fn line_and_arrow_accept_actor_anchors_without_unknown_lookup_path() {
    let source = r#"
config { resolution: (320, 180) }
pen: Ellipse, size: (18, 18), at: (100, 100)
dot: Ellipse, size: (14, 14), at: (200, 100)
link: Line, from: pen.center, to: dot.center
arrow: Arrow, from: pen.right, to: dot.left
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let lookup_errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownLookupPath)
        .collect();
    assert!(
        lookup_errors.is_empty(),
        "Line and Arrow dynamic actor anchor refs must not emit unknown-lookup-path: {lookup_errors:?}"
    );
}

/// `perspective` is accepted and stored but has no consumer (the tilt is
/// orthographic), so authoring it must warn rather than silently do nothing.
#[test]
fn perspective_warns_as_unimplemented() {
    let source = r#"
config { resolution: (320, 180) }
card: Rect, size: (100, 60), at: (160, 90), perspective: 800.0
"#;
    let report = support::build(source);

    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::UnimplementedProperty);
    let warning = warning.unwrap_or_else(|| {
        panic!("expected an unimplemented-property warning, got {:?}", report.diagnostics)
    });
    assert!(
        warning.message.contains("card.perspective"),
        "the warning should name the actor and property, got: {}",
        warning.message
    );

    // A scene that never authors it stays quiet.
    let quiet_source = r#"
config { resolution: (320, 180) }
card: Rect, size: (100, 60), at: (160, 90)
"#;
    let quiet = support::build(quiet_source);
    assert!(
        quiet
            .diagnostics
            .iter()
            .all(|d| d.code != crate::diagnostics::DiagnosticCode::UnimplementedProperty),
        "an unauthored `perspective` must not warn: {:?}",
        quiet.diagnostics
    );
}

/// Audio `source` cannot be animated, so an assignment to it must be reported.
#[test]
fn audio_source_assignment_warns_as_unimplemented() {
    let source = r#"
config { resolution: (320, 180) }
voice: Audio, source: "voice.wav"
#1s
voice.source = "voice2.wav" [1s]
"#;
    let report = support::build(source);

    let warning = report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::UnimplementedProperty);
    let warning = warning.unwrap_or_else(|| {
        panic!("expected an unimplemented-property warning, got {:?}", report.diagnostics)
    });
    assert!(
        warning.message.contains("voice.source"),
        "the warning should name the actor and property, got: {}",
        warning.message
    );
}

#[test]
fn audio_volume_animation_creates_envelope() {
    let source = r#"
config { resolution: (320, 180) }
voice: Audio, source: "voice.wav", volume: 1.0
#1s
voice.volume = 0.2 [1s]
"#;
    let timeline = support::build_timeline(source);
    let segs = timeline.audio_segments();
    assert_eq!(segs.len(), 1);
    let seg = &segs[0];
    assert!(seg.volume_envelope.is_some(), "volume envelope should be generated");
    let env = seg.volume_envelope.as_ref().unwrap();
    assert!(env.len() >= 2);
    // At t=0s volume is 1.0, at t=2s volume is 0.2
    assert!((seg.volume_at(0.0) - 1.0).abs() < 1e-3);
    assert!((seg.volume_at(2.0) - 0.2).abs() < 1e-3);
    assert!((seg.volume_at(1.5) - 0.6).abs() < 0.1);
}

/// Actor labels are scene-global in the runtime world. A child label reused
/// under a different Filter scope silently re-parents the shared entity and
/// leaves the first scope empty; the build must warn instead of rendering one
/// cell as an empty hole.
#[test]
fn duplicate_label_across_filter_parents_warns() {
    let source = r#"
config { resolution: (320, 180) }
#0s
a: Filter, at: (80, 90) {
  fx_a: Blur, radius: 4
  art: Rect, size: (40, 40)
}
b: Filter, at: (240, 90) {
  fx_b: Blur, radius: 4
  art: Rect, size: (40, 40)
}
"#;
    let report = support::build(source);

    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::DuplicateLabel)
        .collect();
    assert_eq!(warnings.len(), 1, "expected one duplicate-label warning: {warnings:?}");
    assert!(
        warnings[0].message.contains("art") && warnings[0].message.contains("re-parents"),
        "warning should name the label and explain the re-parenting: {}",
        warnings[0].message
    );
}

/// Layout-managed children specifying an `offset` must emit an IgnoredOffset
/// warning identifying the child, container, and property.
#[test]
fn layout_managed_child_offset_warns() {
    let source = r#"
config { resolution: (320, 180) }
#0s
row: Row, size: (300, 60), gap: 8 {
  a: Rect, size: (40, 40), offset: (0, 10)
}
"#;
    let report = support::build(source);

    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::IgnoredOffset)
        .collect();
    assert_eq!(warnings.len(), 1, "expected one ignored-offset warning: {warnings:?}");
    assert!(
        warnings[0].message.contains("'a'")
            && warnings[0].message.contains("offset")
            && warnings[0].message.contains("row"),
        "warning should name the child, the container and the property: {}",
        warnings[0].message
    );
}

/// `offset` combined with `at` is a different case: the child opts out of
/// layout placement, so the offset is reported by the position-binding path
/// (with `at`) rather than duplicated by the layout-managed-child check.
#[test]
fn layout_managed_child_at_plus_offset_reports_once() {
    let source = r#"
config { resolution: (320, 180) }
#0s
row: Row, size: (300, 60) {
  a: Rect, size: (40, 40), at: (10, 10), offset: (0, 10)
}
"#;
    let report = support::build(source);

    let offset_warnings = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::IgnoredOffset)
        .count();
    assert_eq!(offset_warnings, 1, "offset with `at` must be reported exactly once");
}
