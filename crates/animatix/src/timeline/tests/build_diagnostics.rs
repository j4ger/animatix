use super::*;

#[test]
fn always_overrides_keyframes_warning() {
    // A keyframe assignment for box1.opacity = 0.8 puts an authored value in
    // the opacity track; the always block then writes 0.5 every frame, which
    // must be reported.
    //
    // The authored values have to differ from each other. A single keyframe is
    // bit-for-bit what declaration seeding leaves behind — the seeder writes the
    // *declared* value, not the registry default, so even a value that is not
    // any default looks exactly like a seed — and `is_property_animated` cannot
    // tell the two apart. One keyframe here would silently stop testing
    // anything.
    let ast = vec![
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box1".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::Assignment {
                    target: vec![crate::ast::TargetSegment::Static("box1".to_string())],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.8),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
            ],
            span: None,
        },
        // A second beat at a different value: one keyframe alone is what the
        // build's declaration seeding also produces, and the two cannot be told
        // apart by looking at the track.
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(2.0),
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static("box1".to_string())],
                property: "opacity".to_string(),
                value: Expr::Num(0.3),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static("box1".to_string())],
                property: "opacity".to_string(),
                value: Expr::Num(0.5),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        has_warning,
        "Expected AlwaysOverridesKeyframes warning when both keyframes and always block target the same property"
    );
}

#[test]
fn always_overrides_keyframes_no_warning_without_track() {
    // No keyframe at all, just an always block.  The target actor doesn't
    // exist in tracks, so no warning should be emitted.
    let ast = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec![crate::ast::TargetSegment::Static("box1".to_string())],
            property: "opacity".to_string(),
            value: Expr::Num(0.5),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        !has_warning,
        "Should NOT emit AlwaysOverridesKeyframes warning when actor doesn't exist in tracks"
    );
}

#[test]
fn always_overrides_keyframes_no_warning_without_conflict() {
    // ActorDecl creates a track but the always block writes to a property
    // that has no keyframes (e.g., rotation is not set by insert_end_keyframes).
    // No warning should be emitted.
    let ast = vec![
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "box1".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec![crate::ast::TargetSegment::Static("box1".to_string())],
                property: "rotation".to_string(),
                value: Expr::Num(0.5),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        !has_warning,
        "Should NOT emit AlwaysOverridesKeyframes warning when the always property has no keyframes"
    );
}

#[test]
fn absolute_position_on_layout_managed_child_warning() {
    // A child of a Row with explicit `at` should emit a warning.
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "row1".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![crate::ast::InlineItem::Labeled {
                label: "child1".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
    });
    assert!(
        has_warning,
        "Expected AbsolutePositionOnLayoutManagedChild warning when a Row child has 'at'"
    );
}

#[test]
fn absolute_position_on_layout_managed_child_no_warning_without_at() {
    // A child of a Row WITHOUT `at` should NOT emit the warning.
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "row1".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![crate::ast::InlineItem::Labeled {
                label: "child1".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

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
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
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
    let (ast, errors) = animatix_syntax::parser::parse_source(&source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
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
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
    let warned = report
        .diagnostics
        .iter()
        .find(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownProperty)
        .expect("expected an unknown-property warning for 'colour'");
    assert!(warned.message.contains("colour"), "warning names the property: {warned:?}");
}

fn inapplicable_warnings(source: &str) -> Vec<String> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
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

/// The exemption that must hold, because 94 shipped scenes depend on it:
/// `text_max_width` is Text's wrap width. Its descriptor row used to list only
/// `Legend`, which made the property look inapplicable on every text actor —
/// the same disagreement between table and consumption that this lint exists to
/// surface, on the false-positive side.
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
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(
        &ast.expect("parsed AST"),
        &std::collections::HashMap::new(),
    );
    report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes)
}

/// The shape that made the lint lie: a *declaration* seeds a constant keyframe,
/// and an `always` writing the same property was reported as overriding keyframe
/// animation the author never wrote. `MarchingRail` in `examples/lib/light.amx`
/// is the real instance — `dash_pattern: {10, gap}` seeds `dash_offset` at its
/// default, and the component's `always` owns it by design.
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

/// The same bug in its other guise, noted in batch 2: writing a property the
/// primitive merely *defaults* from `always`. The declaration seeds the same
/// value at both ends of the scene, so nothing moves and nothing is overridden.
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
