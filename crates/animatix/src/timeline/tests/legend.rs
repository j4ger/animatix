//! Tests for the Legend primitive

use super::*;
use crate::ast::Property;
use crate::primitives::{EvaluateCtx, LEGEND, Primitive, RenderCommand, TextCompileCtx};

fn make_config() -> Stmt {
    Stmt::Config {
        settings: vec![Property {
            name: "colorscheme".to_string(),
            value: Expr::Str("editorial-dark".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        span: None,
    }
}

/// Build a minimal timeline with a single Legend actor.
fn build_legend_timeline() -> Timeline {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "legend".to_string(),
                array_index: None,
                ty: "Legend".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    report.output
}

#[test]
fn test_legend_wrapped_label_respects_max_width() {
    use kurbo::{Affine, Shape};

    let mut track = AnimationTrack::new("legend".to_string());
    track.kind = ActorKindId::Legend;
    track.legend.entries =
        vec![("A very long series label that should wrap".to_string(), [1.0, 0.0, 0.0, 1.0])];
    track.legend.text_max_width = 80.0;

    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        target_resolver: None,
    };
    let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let mut text_ctx = TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_context,
    };

    let commands = LEGEND
        .evaluate(&ctx, Some(&mut text_ctx))
        .expect("evaluate should succeed")
        .unwrap();
    let text_command = commands
        .iter()
        .find_map(|cmd| match cmd {
            RenderCommand::Text { paths } => Some(paths.as_ref()),
            _ => None,
        })
        .expect("legend should emit a text command");
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for text_path in text_command {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
    }
    assert!((max_x - min_x) <= 82.0, "wrapped label exceeded max width");
}

#[test]
fn test_legend_union_property_sets_actor_mode() {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "hidden_series".to_string(),
                    array_index: None,
                    ty: "Line".to_string(),
                    props: vec![Property {
                        name: "legend".to_string(),
                        value: Expr::Bool(false),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "named_series".to_string(),
                    array_index: None,
                    ty: "Line".to_string(),
                    props: vec![Property {
                        name: "legend".to_string(),
                        value: Expr::Str("Revenue".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let hidden = timeline.get_track("hidden_series").expect("hidden series track");
    assert_eq!(
        crate::timeline::legend::legend_mode_for_track(hidden),
        super::super::LegendMode::Hidden
    );
    let named = timeline.get_track("named_series").expect("named series track");
    assert_eq!(
        crate::timeline::legend::legend_mode_for_track(named),
        super::super::LegendMode::Label("Revenue".to_string())
    );
}

#[test]
fn test_legend_basic_rendering() {
    // Test that a Legend renders color swatches and labels
    let timeline = build_legend_timeline();

    let track = timeline.get_track("legend").expect("legend track should exist");
    assert_eq!(track.kind, ActorKindId::Legend, "track kind should be Legend");

    // A legend-only scene has no color-bearing candidates.
    assert!(track.legend.entries.is_empty(), "legend should have no entries");

    // Evaluate at time 0 — should not panic and should produce RenderCommands
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_legend_entries() {
    // Verify that legend entries are stored correctly
    let timeline = build_legend_timeline();
    let track = timeline.get_track("legend").expect("legend track should exist");

    // A legend-only scene has no color-bearing candidates.
    assert!(track.legend.entries.is_empty(), "legend should have no entries");
}

#[test]
fn test_legend_empty() {
    // Verify that a Legend with no entries renders nothing.
    // Build a track manually and call evaluate directly to bypass
    // the build-time placeholder insertion.
    use kurbo::Affine;

    let mut track = AnimationTrack::new("empty_legend".to_string());
    track.kind = ActorKindId::Legend;

    // Ensure entries are empty (default)
    assert!(track.legend.entries.is_empty(), "fresh track should have empty legend entries");

    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        target_resolver: None,
    };

    let result = LEGEND.evaluate(&ctx, None).expect("evaluate should succeed");
    assert!(result.is_none(), "empty legend should return None from evaluate");
}

#[test]
fn test_legend_auto_extraction_from_colored_actors() {
    let actor = |label: &str, color: &str, legend: Expr| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Line".to_string(),
        props: vec![
            Property {
                name: "color".to_string(),
                value: Expr::Ident(color.to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "legend".to_string(),
                value: legend,
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    };

    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                actor("line_a", "red", Expr::Str("Revenue".to_string())),
                actor("line_b", "green", Expr::Bool(false)),
                actor("line_c", "blue", Expr::Bool(true)),
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "legend".to_string(),
                    array_index: None,
                    ty: "Legend".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_track("legend").expect("legend track should exist");

    assert_eq!(
        track.legend.entries,
        vec![
            ("Revenue".to_string(), [1.0, 0.0, 0.0, 1.0]),
            ("Line C".to_string(), [0.0, 0.0, 1.0, 1.0]),
        ]
    );
}

#[test]
fn test_legend_style_properties_are_parsed() {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "legend".to_string(),
                array_index: None,
                ty: "Legend".to_string(),
                props: vec![
                    Property {
                        name: "title".to_string(),
                        value: Expr::Str("Metrics".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "font_size".to_string(),
                        value: Expr::Num(18.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "label_color".to_string(),
                        value: Expr::Ident("white".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "swatch_size".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "gap".to_string(),
                        value: Expr::Num(12.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "text_max_width".to_string(),
                        value: Expr::Num(180.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let track = report.output.get_track("legend").expect("legend track");
    assert_eq!(track.legend.title, "Metrics");
    assert_eq!(track.legend.font_size, 18.0);
    assert_eq!(track.legend.label_color, Some([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(track.legend.swatch_size, 20.0);
    assert_eq!(track.legend.gap, 12.0);
    assert_eq!(track.legend.text_max_width, 180.0);

    use crate::timeline::property_engine::PropertyValue;
    use crate::timeline::property_registry::ActorField;
    let read = |key: &'static str| {
        crate::timeline::dispatch::read_property_value(track, ActorField::Tagged(key), 0)
    };
    assert_eq!(read("legend_title"), Some(PropertyValue::String("Metrics".to_string())));
    assert_eq!(read("legend_font_size"), Some(PropertyValue::F32(18.0)));
    assert_eq!(read("legend_label_color"), Some(PropertyValue::Color([1.0, 1.0, 1.0, 1.0])));
    assert_eq!(read("legend_swatch_size"), Some(PropertyValue::F32(20.0)));
    assert_eq!(read("legend_gap"), Some(PropertyValue::F32(12.0)));
    assert_eq!(read("legend_text_max_width"), Some(PropertyValue::F32(180.0)));
}

#[test]
fn test_legend_excludes_full_viewport_background() {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "backdrop".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Str("100%".to_string()),
                                Expr::Str("100%".to_string()),
                            ]),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("white".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "legend".to_string(),
                    array_index: None,
                    ty: "Legend".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_track("legend").expect("legend track");
    assert!(track.legend.entries.is_empty(), "full-viewport background should be excluded");
}

#[test]
fn test_legend_true_force_includes_background() {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "backdrop".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Str("100%".to_string()),
                                Expr::Str("100%".to_string()),
                            ]),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("white".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "legend".to_string(),
                            value: Expr::Bool(true),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "legend".to_string(),
                    array_index: None,
                    ty: "Legend".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let track = report.output.get_track("legend").expect("legend track");
    assert_eq!(track.legend.entries.len(), 1, "legend:true should force-include a background");
}

#[test]
fn test_legend_source_order_is_preserved() {
    let actor = |label: &str, color: &str| Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Line".to_string(),
        props: vec![Property {
            name: "color".to_string(),
            value: Expr::Ident(color.to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    };
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                actor("line_z", "red"),
                actor("line_a", "green"),
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "legend".to_string(),
                    array_index: None,
                    ty: "Legend".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let track = report.output.get_track("legend").expect("legend track");
    assert_eq!(
        track.legend.entries.iter().map(|(label, _)| label.clone()).collect::<Vec<_>>(),
        vec!["Line Z".to_string(), "Line A".to_string()]
    );
}

#[test]
fn example_legend_entries_are_scanned() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/projects/legend_example.amx"
    ))
    .expect("legend example should exist");
    let ast = animatix_syntax::parser::parse_source(&source).0.expect("example should parse");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_track("legend").expect("legend track should exist");
    assert_eq!(
        track.legend.entries,
        vec![
            ("Series A".to_string(), [1.0, 0.0, 0.0, 1.0]),
            ("Series B".to_string(), [0.0, 1.0, 0.0, 1.0]),
            ("Series C".to_string(), [0.0, 0.0, 1.0, 1.0]),
        ]
    );
    assert_eq!(
        track.geometry.position.get(0, [0.0, 0.0]),
        [320.0, 100.0],
        "Legend 'at' should resolve to a stored position"
    );
}

#[test]
fn test_legend_render_commands_produced() {
    // Create a track with entries and verify evaluate returns Some(commands)
    use kurbo::Affine;

    let mut track = AnimationTrack::new("legend".to_string());
    track.kind = ActorKindId::Legend;

    // Set manual entries to exercise label rendering without a scene build.
    track.legend.entries = vec![
        ("Series A".to_string(), [1.0, 0.0, 0.0, 1.0]),
        ("Series B".to_string(), [0.0, 1.0, 0.0, 1.0]),
        ("Series C".to_string(), [0.0, 0.0, 1.0, 1.0]),
    ];

    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        target_resolver: None,
    };

    let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let mut text_ctx = TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_context,
    };

    let result = LEGEND.evaluate(&ctx, Some(&mut text_ctx)).expect("evaluate should succeed");
    assert!(result.is_some(), "legend with entries should return Some(commands)");

    let commands = result.unwrap();
    assert_eq!(commands.len(), 6, "3 entries should produce 6 RenderCommands (2 per entry)");

    for (i, cmd) in commands.iter().enumerate() {
        match cmd {
            RenderCommand::Paths { paths } => {
                assert_eq!(paths.len(), 1, "each swatch command should contain 1 path");
            },
            RenderCommand::Text { paths } => {
                assert!(!paths.is_empty(), "legend label should compile glyph paths");
            },
            _ => panic!("Command {} should be Paths or Text variant", i),
        }
    }
}
