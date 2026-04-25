use animatix::ast::{BinaryOp, Expr, InlineItem, Modifier, Property, Stmt, Time};
use animatix::diagnostics::DiagnosticCode;
use animatix::easing::Easing;
use animatix::module::ModuleGraph;
use animatix::parser::parser;
use animatix::renderer::text::TextPath;
use animatix::timeline::{
    evaluate_expr, parse_color, time_to_ms, AnimationTrack, Interpolate, MorphStrategy,
    PlacementMode, PositionBinding, PropertyTrack, SceneAnchor, Timeline,
};
use chumsky::Parser;
use kurbo::Shape;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn example_path(name: &str) -> String {
    format!("{}/../../examples/{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn text_paths_width(paths: &[TextPath]) -> f64 {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;

    for text_path in paths {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
    }

    if min_x.is_finite() && max_x.is_finite() {
        max_x - min_x
    } else {
        0.0
    }
}

fn vector_path_bounds(timeline: &Timeline, label: &str, time_ms: u64) -> kurbo::Rect {
    timeline
        .tracks
        .get(label)
        .expect("track should exist")
        .vector_paths
        .evaluate(time_ms)[0]
        .path
        .bounding_box()
}

fn temp_project_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "animatix_timeline_{}_{}_{}",
        name,
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

fn parse_program(src: &str) -> Vec<Stmt> {
    parser().parse(src).into_result().unwrap()
}

#[test]
fn test_time_to_ms() {
    assert_eq!(time_to_ms(&Time::Seconds(2.5)), 2500.0);
    assert_eq!(time_to_ms(&Time::Milliseconds(500)), 500.0);
}

#[test]
fn test_parse_color() {
    assert_eq!(
        parse_color(&Expr::Ident("red".to_string())),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        parse_color(&Expr::Ident("unknown".to_string())),
        [0.8, 0.8, 0.8, 1.0]
    );
    assert_eq!(parse_color(&Expr::Num(1.0)), [0.8, 0.8, 0.8, 1.0]);
}

#[test]
fn test_interpolation() {
    let p1: [f32; 2] = [0.0, 0.0];
    let p2: [f32; 2] = [100.0, 50.0];

    let interpolated = p1.interpolate(&p2, 0.5);
    assert_eq!(interpolated, [50.0, 25.0]);
}

#[test]
fn test_property_track_evaluation() {
    let mut track = PropertyTrack::new([0.0, 0.0]);

    track.add_keyframe(0, [0.0, 0.0], Easing::Linear);
    track.add_keyframe(1000, [100.0, 0.0], Easing::Linear);
    track.add_keyframe(2000, [100.0, 100.0], Easing::Linear);

    // Exactly at first keyframe
    assert_eq!(track.evaluate(0), [0.0, 0.0]);

    // Midway between 1st and 2nd
    assert_eq!(track.evaluate(500), [50.0, 0.0]);

    // Exactly at 2nd keyframe
    assert_eq!(track.evaluate(1000), [100.0, 0.0]);

    // Midway between 2nd and 3rd
    assert_eq!(track.evaluate(1500), [100.0, 50.0]);

    // Beyond last keyframe
    assert_eq!(track.evaluate(2500), [100.0, 100.0]);
}

#[test]
fn test_timeline_build_and_evaluate() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "actor1".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("red".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "actor1".to_string(),
                ty: "Square".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("blue".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    assert_eq!(timeline.tracks.len(), 1);

    // Evaluate at 0.5s (500ms) — access the track directly instead of inspecting the rendered Scene
    let track = timeline
        .tracks
        .get("actor1")
        .expect("actor1 track should exist");
    let position = track.position.evaluate(500);
    let color = track.color.evaluate(500);

    // Position should be interpolated from [0.0, 0.0] to [100.0, 100.0] at 500ms → [50.0, 50.0]
    assert_eq!(position, [50.0, 50.0]);

    // Color should be interpolated between red [1.0, 0.0, 0.0, 1.0] and blue [0.0, 0.0, 1.0, 1.0]
    // i.e., [0.5, 0.0, 0.5, 1.0]
    assert_eq!(color, [0.5, 0.0, 0.5, 1.0]);
}

#[test]
fn config_colorscheme_seeds_scene_background_and_text_alias() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![
                    Property {
                        name: "text".to_string(),
                        value: Expr::Str("Animatix".to_string()),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Path(vec!["text".to_string(), "primary".to_string()]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);

    assert_eq!(
        timeline.background_color.evaluate(0),
        [0.04, 0.06, 0.09, 1.0]
    );
    assert_eq!(
        timeline.tracks["title"].color.evaluate(0),
        [0.97, 0.98, 1.0, 1.0]
    );
}

#[test]
fn explicit_color_beats_colorscheme_alias() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "color".to_string(),
                        value: Expr::Path(vec!["accent".to_string(), "primary".to_string()]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("red".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline.tracks["badge"].color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn explicit_stroke_beats_stroke_alias() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "axis".to_string(),
                ty: "Line".to_string(),
                props: vec![
                    Property {
                        name: "stroke".to_string(),
                        value: Expr::Path(vec!["stroke".to_string(), "default".to_string()]),
                    },
                    Property {
                        name: "stroke".to_string(),
                        value: Expr::Ident("red".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline.tracks["axis"].stroke_color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn auto_color_alias_assigns_distinct_colors_and_keeps_identity() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "alice".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "color".to_string(),
                        value: Expr::Ident("auto".to_string()),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "bob".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "color".to_string(),
                        value: Expr::Ident("auto".to_string()),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "alice".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let alice = timeline.tracks["alice"].color.evaluate(0);
    let bob = timeline.tracks["bob"].color.evaluate(0);

    assert_ne!(alice, bob);
    assert_eq!(timeline.tracks["alice"].color.evaluate(1000), alice);
}

#[test]
fn auto_color_assigns_deterministic_colors_to_text_math_and_code() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "title".to_string(),
                    ty: "Text".to_string(),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("Animatix".to_string()),
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "formula".to_string(),
                    ty: "Math".to_string(),
                    props: vec![
                        Property {
                            name: "math".to_string(),
                            value: Expr::Str("E = mc^2".to_string()),
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "snippet".to_string(),
                    ty: "Code".to_string(),
                    props: vec![
                        Property {
                            name: "code".to_string(),
                            value: Expr::Str("fn main() {}".to_string()),
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);

    assert_eq!(
        timeline.tracks["title"].color.evaluate(0),
        [0.38, 0.78, 1.0, 1.0]
    );
    assert_eq!(
        timeline.tracks["formula"].color.evaluate(0),
        [0.35, 0.86, 0.63, 1.0]
    );
    assert_eq!(
        timeline.tracks["snippet"].color.evaluate(0),
        [1.0, 0.46, 0.54, 1.0]
    );
}

#[test]
fn unknown_colorscheme_and_color_reference_report_diagnostics() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("missing-scheme".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["accent".to_string(), "missing".to_string()]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownColorscheme));
    assert!(report
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownColorReference));
}

#[test]
fn primitive_default_colors_apply_when_no_explicit_color() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                // Text without explicit color should get text.primary
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "title".to_string(),
                    ty: "Text".to_string(),
                    props: vec![Property {
                        name: "text".to_string(),
                        value: Expr::Str("Hello".to_string()),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                // Circle without explicit color should get surface.primary
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "badge".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                // Line without explicit stroke should get stroke.default
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "axis".to_string(),
                    ty: "Line".to_string(),
                    props: vec![
                        Property {
                            name: "from".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(-50.0), Expr::Num(0.0)]),
                        },
                        Property {
                            name: "to".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(0.0)]),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);

    // Text should get text.primary from editorial-dark scheme
    assert_eq!(
        timeline.tracks["title"].color.evaluate(0),
        [0.97, 0.98, 1.0, 1.0]
    );
    // Circle should get surface.primary from editorial-dark scheme
    assert_eq!(
        timeline.tracks["badge"].color.evaluate(0),
        [0.11, 0.16, 0.24, 1.0]
    );
    // Line should get stroke.default from editorial-dark scheme
    assert_eq!(
        timeline.tracks["axis"].stroke_color.evaluate(0),
        [0.97, 0.98, 1.0, 1.0]
    );
}

#[test]
fn explicit_color_beats_primitive_default() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
            }],
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                // Circle with explicit color should keep it
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "explicit".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(1.0),
                                Expr::Num(0.0),
                                Expr::Num(0.0),
                                Expr::Num(1.0),
                            ]),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                // Circle with auto should use auto pool, not surface.primary
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "auto_color".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);

    // Explicit color should win over default
    assert_eq!(
        timeline.tracks["explicit"].color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0]
    );
    // Auto should use first auto pool color, not surface.primary
    assert_eq!(
        timeline.tracks["auto_color"].color.evaluate(0),
        [0.38, 0.78, 1.0, 1.0]
    );
}

#[test]
fn default_scheme_applies_primitive_defaults() {
    // Even without explicit config.colorscheme, the default-dark scheme applies
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            // Text without explicit color should get text.primary from default-dark
            Stmt::ActorDecl {
                is_pub: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![Property {
                    name: "text".to_string(),
                    value: Expr::Str("Hello".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            },
            // Circle without explicit color should get surface.primary from default-dark
            Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);

    // Default-dark scheme applies automatically
    assert_eq!(
        timeline.tracks["title"].color.evaluate(0),
        [1.0, 1.0, 1.0, 1.0] // text.primary in default-dark is white
    );
    assert_eq!(
        timeline.tracks["badge"].color.evaluate(0),
        [0.11, 0.16, 0.24, 1.0] // surface.primary in default-dark
    );
}

#[test]
fn component_instances_get_distinct_auto_colors() {
    let dir = temp_project_dir("colorscheme_component_roles");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: surface.primary
    badge: Circle, radius: 12, color: auto
}
"#,
    );

    write_file(
        &entry,
        r#"
config { colorscheme: "editorial-dark" }
import "./components.amx"

left: MetricCard, title: "Latency"
right: MetricCard, title: "Throughput"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    assert_ne!(
        timeline.tracks["left.badge"].color.evaluate(0),
        timeline.tracks["right.badge"].color.evaluate(0)
    );
}

#[test]
fn test_text_spacing_preserves_space_width() {
    let text_with_space = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "spaced".to_string(),
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("A B".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let text_without_space = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "tight".to_string(),
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("AB".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let spaced_timeline = Timeline::build(&text_with_space);
    let tight_timeline = Timeline::build(&text_without_space);

    let spaced_paths = &spaced_timeline
        .tracks
        .get("spaced")
        .expect("spaced track should exist")
        .text_paths
        .evaluate(0);
    let tight_paths = &tight_timeline
        .tracks
        .get("tight")
        .expect("tight track should exist")
        .text_paths
        .evaluate(0);

    assert!(text_paths_width(spaced_paths) > text_paths_width(tight_paths));
}

#[test]
fn test_code_primitive_builds_text_paths() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "snippet".to_string(),
            ty: "Code".to_string(),
            props: vec![
                Property {
                    name: "code".to_string(),
                    value: Expr::Str("fn main() {}".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(24.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let paths = timeline
        .tracks
        .get("snippet")
        .expect("snippet track should exist")
        .text_paths
        .evaluate(0);

    assert!(!paths.is_empty());
}

#[test]
fn test_code_primitive_respects_position_binding() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "anchored_code".to_string(),
            ty: "Code".to_string(),
            props: vec![
                Property {
                    name: "code".to_string(),
                    value: Expr::Str("let x = 1;".to_string()),
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "center".to_string()]),
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(24.0)]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("anchored_code")
            .expect("anchored_code track should exist")
            .position_binding
            .evaluate(0),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Center,
            offset: [0.0, 24.0],
        }
    );
}

#[test]
fn test_math_scene_percent_position_assignment_interpolates_binding() {
    let ast = parse_program(
        r#"
math_title: Math, math: "E = mc^2", at: (30%, 38%)

#1s
math_title.at = (32%, 36%) [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("math_title")
        .expect("math_title track should exist");

    assert_eq!(
        track.position_binding.evaluate(1000),
        PositionBinding::ScenePercent {
            x: 0.30,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    assert_eq!(
        track.position_binding.evaluate(1500),
        PositionBinding::ScenePercent {
            x: 0.31,
            y: 0.37,
            offset: [0.0, 0.0],
        }
    );
    assert_eq!(
        track.position_binding.evaluate(2000),
        PositionBinding::ScenePercent {
            x: 0.32,
            y: 0.36,
            offset: [0.0, 0.0],
        }
    );
}

#[test]
fn test_code_primitive_redeclaration_updates_text_paths() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "snippet".to_string(),
                ty: "Code".to_string(),
                props: vec![Property {
                    name: "code".to_string(),
                    value: Expr::Str("let x = 1;".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "snippet".to_string(),
                ty: "Code".to_string(),
                props: vec![Property {
                    name: "code".to_string(),
                    value: Expr::Str("let x = 2;".to_string()),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("snippet")
        .expect("snippet track should exist");

    assert!(!track.text_paths.evaluate(0).is_empty());
    assert!(!track.text_paths.evaluate(1000).is_empty());
    assert!(track.text_paths.keyframes.contains_key(&0));
    assert!(track.text_paths.keyframes.contains_key(&1000));
}

#[test]
fn test_missing_properties() {
    let track = AnimationTrack::new("empty_actor".to_string());

    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
    assert_eq!(track.size.evaluate(0), [50.0, 50.0]);
    assert_eq!(track.line_from.evaluate(0), [-50.0, 0.0]);
    assert_eq!(track.line_to.evaluate(0), [50.0, 0.0]);
    assert_eq!(track.arc_angles.evaluate(0), [0.0, std::f32::consts::PI]);
    assert_eq!(track.color.evaluate(0), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(track.shape_type.evaluate(0), 0);
    assert_eq!(track.opacity.evaluate(0), 1.0);
}

#[test]
fn test_square_primitive_builds_rect_shape() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "sq".to_string(),
            ty: "Square".to_string(),
            props: vec![Property {
                name: "side".to_string(),
                value: Expr::Num(80.0),
            }],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("sq")
        .expect("square track should exist");

    assert_eq!(track.size.evaluate(0), [40.0, 40.0]);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_dot_primitive_uses_small_default_radius() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "dot".to_string(),
            ty: "Dot".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("dot").expect("dot track should exist");

    assert_eq!(track.size.evaluate(0), [6.0, 6.0]);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_regular_polygon_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "hex".to_string(),
            ty: "RegularPolygon".to_string(),
            props: vec![
                Property {
                    name: "sides".to_string(),
                    value: Expr::Num(6.0),
                },
                Property {
                    name: "radius".to_string(),
                    value: Expr::Num(40.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("hex").expect("hex track should exist");

    assert_eq!(track.shape_type.evaluate(0), 5);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_arrow_primitive_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "arrow".to_string(),
            ty: "Arrow".to_string(),
            props: vec![
                Property {
                    name: "from".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-60.0), Expr::Num(0.0)]),
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(0.0)]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("arrow")
        .expect("arrow track should exist");

    assert_eq!(track.shape_type.evaluate(0), 7);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_arrow_tip_properties_update_size_track() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "arrow".to_string(),
            ty: "Arrow".to_string(),
            props: vec![
                Property {
                    name: "tip_length".to_string(),
                    value: Expr::Num(30.0),
                },
                Property {
                    name: "tip_width".to_string(),
                    value: Expr::Num(20.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("arrow")
        .expect("arrow track should exist");

    assert_eq!(track.size.evaluate(0), [30.0, 20.0]);
}

#[test]
fn test_parametric_plot_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "graph".to_string(),
            ty: "Graph".to_string(),
            props: vec![
                Property {
                    name: "x_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                },
                Property {
                    name: "y_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(300.0)]),
                },
            ],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "curve".to_string(),
                ty: "ParametricPlot".to_string(),
                props: vec![
                    Property {
                        name: "func".to_string(),
                        value: Expr::Closure(
                            vec!["t".to_string()],
                            Box::new(Expr::Tuple(vec![
                                Expr::Ident("t".to_string()),
                                Expr::Call("sin".to_string(), vec![Expr::Ident("t".to_string())]),
                            ])),
                        ),
                    },
                    Property {
                        name: "t_domain".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("cyan".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("curve")
        .expect("parametric track should exist");

    assert_ne!(track.shape_type.evaluate(0), 0);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_implicit_plot_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "graph".to_string(),
            ty: "Graph".to_string(),
            props: vec![
                Property {
                    name: "x_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                },
                Property {
                    name: "y_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(300.0)]),
                },
            ],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "contour".to_string(),
                ty: "ImplicitPlot".to_string(),
                props: vec![
                    Property {
                        name: "func".to_string(),
                        value: Expr::Closure(
                            vec!["x".to_string(), "y".to_string()],
                            Box::new(Expr::Binary(
                                Box::new(Expr::Binary(
                                    Box::new(Expr::Binary(
                                        Box::new(Expr::Ident("x".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("x".to_string())),
                                    )),
                                    BinaryOp::Add,
                                    Box::new(Expr::Binary(
                                        Box::new(Expr::Ident("y".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("y".to_string())),
                                    )),
                                )),
                                BinaryOp::Sub,
                                Box::new(Expr::Num(1.0)),
                            )),
                        ),
                    },
                    Property {
                        name: "resolution".to_string(),
                        value: Expr::Num(48.0),
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("cyan".to_string()),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("contour")
        .expect("implicit plot track should exist");

    assert_ne!(track.shape_type.evaluate(0), 0);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_timeline_duration_seconds_uses_latest_keyframe() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "box".to_string(),
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(40.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(2.5),
            body: vec![Stmt::Assignment {
                target: vec!["box".to_string()],
                property: "rotation".to_string(),
                value: Expr::Num(1.0),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);

    assert!((timeline.duration_seconds() - 2.5).abs() < f64::EPSILON);
}

#[test]
fn imported_component_instances_expand_with_isolated_labels_and_props() {
    let dir = temp_project_dir("component_instances");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text, text: title, at: (0, -20)
    badge: Circle, radius: 12, color: gold
    badge.color = red
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

first: MetricCard, title: "Latency"
second: MetricCard, title: "Throughput"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    assert!(timeline.tracks.contains_key("first"));
    assert!(timeline.tracks.contains_key("second"));
    assert!(timeline.tracks.contains_key("first.title_text"));
    assert!(timeline.tracks.contains_key("second.title_text"));
    assert!(timeline.tracks.contains_key("first.badge"));
    assert!(timeline.tracks.contains_key("second.badge"));

    let first_paths = timeline
        .tracks
        .get("first.title_text")
        .expect("first title track")
        .text_paths
        .evaluate(0);
    let second_paths = timeline
        .tracks
        .get("second.title_text")
        .expect("second title track")
        .text_paths
        .evaluate(0);

    assert!(text_paths_width(&first_paths) > 0.0);
    assert!(text_paths_width(&second_paths) > text_paths_width(&first_paths));

    let first_badge = timeline
        .tracks
        .get("first.badge")
        .expect("first badge track");
    let second_badge = timeline
        .tracks
        .get("second.badge")
        .expect("second badge track");
    assert_eq!(first_badge.color.evaluate(0), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(second_badge.color.evaluate(0), [1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn imported_component_nested_assignment_targets_update_prefixed_tracks() {
    let dir = temp_project_dir("component_nested_targets");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    title_text: Text, text: title, color: white, at: (0, -20)
    badge: Circle, radius: 12, color: gold
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

left: MetricCard, title: "Latency"

#+1s
left.badge.color = red
left.title_text.color = green
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    assert_eq!(
        timeline.tracks["left.badge"].color.evaluate(1000),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert!(!timeline.tracks["left.title_text"]
        .text_paths
        .evaluate(1000)
        .is_empty());
}

#[test]
fn rhs_path_lookup_reads_existing_actor_properties() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "source".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(18.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "mirror".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Path(vec!["source".to_string(), "radius".to_string()]),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Path(vec!["source".to_string(), "at".to_string()]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(timeline.tracks["mirror"].size.evaluate(0), [18.0, 18.0]);
    assert_eq!(
        timeline.tracks["mirror"].position.evaluate(0),
        [120.0, 80.0]
    );
}

#[test]
fn rhs_path_lookup_supports_vector_components() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "source".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "target".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Path(vec![
                            "source".to_string(),
                            "at".to_string(),
                            "x".to_string(),
                        ]),
                        Expr::Path(vec![
                            "source".to_string(),
                            "at".to_string(),
                            "y".to_string(),
                        ]),
                    ]),
                }],
                modifiers: vec![],
                children: vec![],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline.tracks["target"].position.evaluate(0),
        [320.0, 240.0]
    );
}

#[test]
fn rhs_path_lookup_reads_nested_component_properties() {
    let dir = temp_project_dir("component_rhs_lookup");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 14, color: red, at: (-80, 20)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

left: MetricCard, title: "Latency"
copy: Circle, radius: left.badge.radius, at: left.badge.at, color: left.badge.color
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    assert_eq!(timeline.tracks["copy"].size.evaluate(0), [14.0, 14.0]);
    assert_eq!(timeline.tracks["copy"].position.evaluate(0), [-80.0, 20.0]);
    assert_eq!(
        timeline.tracks["copy"].color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn missing_nested_rhs_lookup_reports_diagnostic_in_declaration() {
    let dir = temp_project_dir("component_rhs_lookup_missing_decl");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 14, color: red, at: (-80, 20)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

left: MetricCard, title: "Latency"
copy: Circle, radius: left.missing.radius, at: left.missing.at, color: left.missing.color
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownLookupPath
            && diagnostic.message.contains("left.missing.radius")
            && diagnostic.message.contains("left.badge.radius")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownLookupPath
            && diagnostic.message.contains("left.missing.at")
            && diagnostic.message.contains("left.badge.at")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownLookupPath
            && diagnostic.message.contains("left.missing.color")
            && diagnostic.message.contains("left.badge.color")
    }));

    assert_eq!(report.output.tracks["copy"].size.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        report.output.tracks["copy"].color.evaluate(0),
        [0.8, 0.8, 0.8, 1.0]
    );
}

#[test]
fn missing_nested_rhs_lookup_reports_diagnostic_in_assignment() {
    let dir = temp_project_dir("component_rhs_lookup_missing_assignment");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    badge: Circle, radius: 14, color: red, at: (-80, 20)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

left: MetricCard, title: "Latency"
echo: Circle, radius: 9, color: blue

#0s
echo.radius = left.missing.radius
echo.color = left.missing.color
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownLookupPath
            && diagnostic.message.contains("left.missing.radius")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownLookupPath
            && diagnostic.message.contains("left.missing.color")
    }));

    assert_eq!(report.output.tracks["echo"].size.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        report.output.tracks["echo"].color.evaluate(0),
        [0.8, 0.8, 0.8, 1.0]
    );
}

/// Verifies that assigning to a non-existent nested label reports a diagnostic
/// instead of creating an orphaned track.
#[test]
fn nonexistent_nested_assignment_reports_diagnostic() {
    let dir = temp_project_dir("orphaned_track");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 14, color: red
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

card: MetricCard, title: "Latency"

#0s
card.nonexistent.color = red
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnknownTargetPath
            && diagnostic.message.contains("card.nonexistent")
            && diagnostic.message.contains("card.badge")
    }));
    assert!(
        !report.output.tracks.contains_key("card.nonexistent"),
        "non-existent nested label should no longer create an orphaned track"
    );
}

#[test]
fn unsupported_nested_component_property_assignment_reports_diagnostic() {
    let dir = temp_project_dir("unsupported_component_assignment_property");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 14, color: red
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

card: MetricCard, title: "Latency"

#0s
card.badge.glow = 10
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedAssignmentProperty
            && diagnostic.location.subject.as_deref() == Some("card.badge.glow")
            && diagnostic.message.contains("card.badge")
            && diagnostic.message.contains("glow")
    }));
    assert_eq!(
        report.output.tracks["card.badge"].size.evaluate(0),
        [14.0, 14.0]
    );
    assert_eq!(
        report.output.tracks["card.badge"].color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0]
    );
}

/// Verifies that two instances of the same component get completely isolated namespaces.
/// Changes to one instance's nested labels do not affect the other instance.
#[test]
fn component_instances_have_completely_isolated_namespaces() {
    let dir = temp_project_dir("isolated_namespaces");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120), color: blue
    badge: Circle, radius: 12, color: gold
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

first: MetricCard, title: "Latency"
second: MetricCard, title: "Throughput"

#0s
first.badge.color = red
second.badge.color = blue

#1s
first.badge.radius = 30
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    // Both instances have their own isolated nested labels
    assert!(
        timeline.tracks.contains_key("first.badge"),
        "first.badge should exist"
    );
    assert!(
        timeline.tracks.contains_key("second.badge"),
        "second.badge should exist"
    );

    // Isolated color assignments
    assert_eq!(
        timeline.tracks["first.badge"].color.evaluate(0),
        [1.0, 0.0, 0.0, 1.0],
        "first.badge.color should be red"
    );
    assert_eq!(
        timeline.tracks["second.badge"].color.evaluate(0),
        [0.0, 0.0, 1.0, 1.0],
        "second.badge.color should be blue"
    );

    // Isolated radius change (only first is affected at #1s)
    assert_eq!(
        timeline.tracks["first.badge"].size.evaluate(1000),
        [30.0, 30.0],
        "first.badge.radius should be 30 at #1s"
    );
    // second.badge.radius should remain unchanged (12)
    assert_eq!(
        timeline.tracks["second.badge"].size.evaluate(1000),
        [12.0, 12.0],
        "second.badge.radius should still be 12 at #1s"
    );
}

#[test]
fn test_image_properties_are_animatable() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::Image {
                label: Some("photo".to_string()),
                url: example_path("checker.ppm"),
                at: Some(Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(120.0)])),
                anchor: None,
                offset: None,
                size: Some((48.0, 48.0)),
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![
                Stmt::Assignment {
                    target: vec!["photo".to_string()],
                    property: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(96.0), Expr::Num(96.0)]),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                },
                Stmt::Assignment {
                    target: vec!["photo".to_string()],
                    property: "url".to_string(),
                    value: Expr::Str(example_path("checker.ppm")),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                },
            ],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("photo")
        .expect("photo track should exist");

    assert_eq!(track.position.evaluate(0), [100.0, 120.0]);
    assert_eq!(track.size.evaluate(0), [24.0, 24.0]);
    assert_eq!(track.size.evaluate(1500), [36.0, 36.0]);
    assert_eq!(track.size.evaluate(2000), [48.0, 48.0]);
    assert!(track.image.evaluate(0).is_some());
    assert!(track.image.evaluate(1500).is_some());
}

#[test]
fn test_missing_image_statement_reports_media_load_failure() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Image {
            label: Some("photo".to_string()),
            url: "/definitely/missing/animatix-image.png".to_string(),
            at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            anchor: None,
            offset: None,
            size: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MediaLoadFailure
            && diagnostic.location.subject.as_deref() == Some("photo.url")
    }));
}

#[test]
fn test_invalid_svg_statement_reports_media_load_failure() {
    let dir = temp_project_dir("invalid_svg_diagnostic");
    let invalid_svg = dir.join("broken.svg");
    write_file(&invalid_svg, "<svg><broken></svg>");

    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Svg {
            label: Some("icon".to_string()),
            url: invalid_svg.display().to_string(),
            at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            anchor: None,
            offset: None,
            scale: 1.0,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MediaLoadFailure
            && diagnostic.location.subject.as_deref() == Some("icon.url")
    }));
}

#[test]
fn test_media_actor_declaration_modifiers_report_unsupported_keys() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "photo".to_string(),
            ty: "Image".to_string(),
            props: vec![Property {
                name: "url".to_string(),
                value: Expr::Str(example_path("checker.ppm")),
            }],
            modifiers: vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string()),
                },
            ],
            children: vec![],
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("photo")
        .expect("photo track should exist");

    assert_eq!(track.image.evaluate(0).is_some(), true);
    assert_eq!(track.image.evaluate(500).is_some(), true);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedModifierKey)
            .count(),
        2
    );
}

#[test]
fn test_missing_image_url_assignment_reports_media_load_failure() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::Image {
                label: Some("photo".to_string()),
                url: example_path("checker.ppm"),
                at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
                anchor: None,
                offset: None,
                size: Some((32.0, 32.0)),
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["photo".to_string()],
                property: "url".to_string(),
                value: Expr::Str("/definitely/missing/animatix-image.png".to_string()),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MediaLoadFailure
            && diagnostic.location.subject.as_deref() == Some("photo.url")
    }));
}

#[test]
fn test_svg_url_assignment_reports_unsupported_media_assignment() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::Svg {
                label: Some("icon".to_string()),
                url: example_path("vector.svg"),
                at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
                anchor: None,
                offset: None,
                scale: 1.0,
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["icon".to_string()],
                property: "url".to_string(),
                value: Expr::Str(example_path("vector.svg")),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedMediaAssignment
            && diagnostic.location.subject.as_deref() == Some("icon.url")
    }));
}

#[test]
fn test_line_actor_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "axis".to_string(),
            ty: "Line".to_string(),
            props: vec![
                Property {
                    name: "from".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-40.0), Expr::Num(0.0)]),
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(20.0)]),
                },
                Property {
                    name: "stroke_width".to_string(),
                    value: Expr::Num(3.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("axis")
        .expect("axis track should exist");
    let bounds = vector_path_bounds(&timeline, "axis", 0);

    assert_eq!(track.shape_type.evaluate(0), 2);
    assert_eq!(track.line_from.evaluate(0), [-40.0, 0.0]);
    assert_eq!(track.line_to.evaluate(0), [60.0, 20.0]);
    assert!(track.vector_paths.evaluate(0)[0].fill.is_none());
    assert_eq!(bounds.x0, -40.0);
    assert_eq!(bounds.y0, 0.0);
    assert_eq!(bounds.x1, 60.0);
    assert_eq!(bounds.y1, 20.0);
}

#[test]
fn test_ellipse_actor_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "halo".to_string(),
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(30.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("halo")
        .expect("halo track should exist");
    let bounds = vector_path_bounds(&timeline, "halo", 0);

    assert_eq!(track.shape_type.evaluate(0), 3);
    assert_eq!(track.size.evaluate(0), [80.0, 30.0]);
    assert!((bounds.x0 + 80.0).abs() < 0.1);
    assert!((bounds.y0 + 30.0).abs() < 0.1);
    assert!((bounds.x1 - 80.0).abs() < 0.1);
    assert!((bounds.y1 - 30.0).abs() < 0.1);
}

#[test]
fn test_arc_actor_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "ring".to_string(),
            ty: "Arc".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(40.0),
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(0.0),
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(std::f64::consts::PI),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("ring")
        .expect("ring track should exist");
    let vector_path = &track.vector_paths.evaluate(0)[0];
    let bounds = vector_path.path.bounding_box();

    assert_eq!(track.shape_type.evaluate(0), 4);
    assert_eq!(track.size.evaluate(0), [80.0, 40.0]);
    assert_eq!(track.arc_angles.evaluate(0), [0.0, std::f32::consts::PI]);
    assert!(vector_path.fill.is_none());
    assert!((bounds.x0 + 80.0).abs() < 0.1);
    assert!((bounds.x1 - 80.0).abs() < 0.1);
}

#[test]
fn test_polygon_actor_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Polygon".to_string(),
            props: vec![Property {
                name: "points".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Tuple(vec![Expr::Num(-80.0), Expr::Num(0.0)]),
                    Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-70.0)]),
                    Expr::Tuple(vec![Expr::Num(90.0), Expr::Num(0.0)]),
                    Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(80.0)]),
                ]),
            }],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("badge")
        .expect("badge track should exist");
    let vector_path = &track.vector_paths.evaluate(0)[0];
    let bounds = vector_path.path.bounding_box();

    assert_eq!(track.shape_type.evaluate(0), 5);
    assert!(vector_path.fill.is_some());
    assert!((bounds.x0 + 80.0).abs() < 0.1);
    assert!((bounds.y0 + 70.0).abs() < 0.1);
    assert!((bounds.x1 - 90.0).abs() < 0.1);
    assert!((bounds.y1 - 80.0).abs() < 0.1);
}

#[test]
fn test_path_actor_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "guide".to_string(),
            ty: "Path".to_string(),
            props: vec![Property {
                name: "commands".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Call(
                        "move_to".to_string(),
                        vec![Expr::Num(-120.0), Expr::Num(0.0)],
                    ),
                    Expr::Call(
                        "line_to".to_string(),
                        vec![Expr::Num(-40.0), Expr::Num(-80.0)],
                    ),
                    Expr::Call(
                        "curve_to".to_string(),
                        vec![
                            Expr::Num(20.0),
                            Expr::Num(-120.0),
                            Expr::Num(80.0),
                            Expr::Num(40.0),
                            Expr::Num(140.0),
                            Expr::Num(-10.0),
                        ],
                    ),
                    Expr::Call("close".to_string(), vec![]),
                ]),
            }],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("guide")
        .expect("guide track should exist");
    let vector_path = &track.vector_paths.evaluate(0)[0];
    let bounds = vector_path.path.bounding_box();

    assert_eq!(track.shape_type.evaluate(0), 6);
    assert!(vector_path.stroke.is_some());
    assert!((bounds.x0 + 120.0).abs() < 0.1);
    assert!(bounds.y0 < -80.0);
    assert!((bounds.x1 - 140.0).abs() < 0.1);
    assert!(bounds.y1 > -20.0);
}

#[test]
fn test_path_quad_to_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "quad".to_string(),
            ty: "Path".to_string(),
            props: vec![Property {
                name: "commands".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Call(
                        "move_to".to_string(),
                        vec![Expr::Num(-80.0), Expr::Num(0.0)],
                    ),
                    Expr::Call(
                        "quad_to".to_string(),
                        vec![
                            Expr::Num(0.0),
                            Expr::Num(-120.0),
                            Expr::Num(100.0),
                            Expr::Num(20.0),
                        ],
                    ),
                ]),
            }],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let bounds = vector_path_bounds(&timeline, "quad", 0);

    assert!(bounds.x0 <= -80.0);
    assert!(bounds.x1 >= 100.0);
    assert!(bounds.y0 < 0.0);
}

#[test]
fn test_line_assignments_rebuild_runtime_path() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "axis".to_string(),
                ty: "Line".to_string(),
                props: vec![
                    Property {
                        name: "from".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(-20.0), Expr::Num(0.0)]),
                    },
                    Property {
                        name: "to".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(0.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["axis".to_string()],
                property: "to".to_string(),
                value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(40.0)]),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let bounds = vector_path_bounds(&timeline, "axis", 1000);

    assert_eq!(timeline.tracks["axis"].line_to.evaluate(1000), [20.0, 40.0]);
    assert_eq!(bounds.x0, -20.0);
    assert_eq!(bounds.y0, 0.0);
    assert_eq!(bounds.x1, 20.0);
    assert_eq!(bounds.y1, 40.0);
}

#[test]
fn test_ellipse_assignments_rebuild_runtime_path() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "halo".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius_x".to_string(),
                        value: Expr::Num(80.0),
                    },
                    Property {
                        name: "radius_y".to_string(),
                        value: Expr::Num(30.0),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["halo".to_string()],
                property: "radius_y".to_string(),
                value: Expr::Num(60.0),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let bounds = vector_path_bounds(&timeline, "halo", 1000);

    assert_eq!(timeline.tracks["halo"].size.evaluate(1000), [80.0, 60.0]);
    assert!((bounds.x0 + 80.0).abs() < 0.1);
    assert!((bounds.y0 + 60.0).abs() < 0.1);
    assert!((bounds.x1 - 80.0).abs() < 0.1);
    assert!((bounds.y1 - 60.0).abs() < 0.1);
}

#[test]
fn test_arc_assignments_rebuild_runtime_path() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "ring".to_string(),
                ty: "Arc".to_string(),
                props: vec![
                    Property {
                        name: "radius_x".to_string(),
                        value: Expr::Num(80.0),
                    },
                    Property {
                        name: "radius_y".to_string(),
                        value: Expr::Num(40.0),
                    },
                    Property {
                        name: "start_angle".to_string(),
                        value: Expr::Num(0.0),
                    },
                    Property {
                        name: "sweep_angle".to_string(),
                        value: Expr::Num(std::f64::consts::PI / 2.0),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["ring".to_string()],
                property: "sweep_angle".to_string(),
                value: Expr::Num(std::f64::consts::PI),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let bounds = vector_path_bounds(&timeline, "ring", 1000);

    assert_eq!(
        timeline.tracks["ring"].arc_angles.evaluate(1000),
        [0.0, std::f32::consts::PI]
    );
    assert!((bounds.x0 + 80.0).abs() < 0.1);
    assert!((bounds.x1 - 80.0).abs() < 0.1);
}

#[test]
fn test_arc_negative_sweep_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "ring".to_string(),
            ty: "Arc".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(70.0),
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(50.0),
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(std::f64::consts::PI),
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(-std::f64::consts::PI / 2.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let path = &timeline.tracks["ring"].vector_paths.evaluate(0)[0].path;

    assert!(!path.elements().is_empty());
}

#[test]
fn test_polygon_style_assignment_preserves_geometry() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Polygon".to_string(),
                props: vec![Property {
                    name: "points".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Tuple(vec![Expr::Num(-80.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-70.0)]),
                        Expr::Tuple(vec![Expr::Num(90.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(80.0)]),
                    ]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["badge".to_string()],
                property: "color".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Num(0.2),
                    Expr::Num(0.9),
                    Expr::Num(0.7),
                    Expr::Num(1.0),
                ]),
                modifiers: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let start_bounds = vector_path_bounds(&timeline, "badge", 0);
    let end_path = &timeline.tracks["badge"].vector_paths.evaluate(1000)[0];
    let end_bounds = end_path.path.bounding_box();

    assert!((start_bounds.x0 - end_bounds.x0).abs() < 0.1);
    assert!((start_bounds.y0 - end_bounds.y0).abs() < 0.1);
    assert!(end_path.fill.is_some());
}

#[test]
fn test_polygon_redeclaration_rebuilds_geometry() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Polygon".to_string(),
                props: vec![Property {
                    name: "points".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Tuple(vec![Expr::Num(-80.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-70.0)]),
                        Expr::Tuple(vec![Expr::Num(90.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(80.0)]),
                    ]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Polygon".to_string(),
                props: vec![Property {
                    name: "points".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Tuple(vec![Expr::Num(-110.0), Expr::Num(-20.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(-120.0)]),
                        Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(-10.0)]),
                        Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(120.0)]),
                        Expr::Tuple(vec![Expr::Num(-80.0), Expr::Num(90.0)]),
                    ]),
                }],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let start_bounds = vector_path_bounds(&timeline, "badge", 1000);
    let mid_bounds = vector_path_bounds(&timeline, "badge", 1500);
    let end_bounds = vector_path_bounds(&timeline, "badge", 2000);

    assert!(report.diagnostics.is_empty());
    assert!((mid_bounds.x0 - start_bounds.x0).abs() > 0.1);
    assert!(mid_bounds.x0 > end_bounds.x0);
    assert!(mid_bounds.y0 > end_bounds.y0);
    assert!(mid_bounds.y1 < end_bounds.y1);
}

#[test]
fn test_path_redeclaration_rebuilds_geometry() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "guide".to_string(),
                ty: "Path".to_string(),
                props: vec![Property {
                    name: "commands".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Call(
                            "move_to".to_string(),
                            vec![Expr::Num(-120.0), Expr::Num(40.0)],
                        ),
                        Expr::Call(
                            "line_to".to_string(),
                            vec![Expr::Num(80.0), Expr::Num(100.0)],
                        ),
                        Expr::Call("close".to_string(), vec![]),
                    ]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "guide".to_string(),
                ty: "Path".to_string(),
                props: vec![Property {
                    name: "commands".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Call(
                            "move_to".to_string(),
                            vec![Expr::Num(-130.0), Expr::Num(20.0)],
                        ),
                        Expr::Call(
                            "curve_to".to_string(),
                            vec![
                                Expr::Num(-40.0),
                                Expr::Num(-140.0),
                                Expr::Num(90.0),
                                Expr::Num(-20.0),
                                Expr::Num(130.0),
                                Expr::Num(50.0),
                            ],
                        ),
                        Expr::Call(
                            "line_to".to_string(),
                            vec![Expr::Num(20.0), Expr::Num(120.0)],
                        ),
                        Expr::Call("close".to_string(), vec![]),
                    ]),
                }],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let start_bounds = vector_path_bounds(&timeline, "guide", 1000);
    let mid_bounds = vector_path_bounds(&timeline, "guide", 1500);
    let end_bounds = vector_path_bounds(&timeline, "guide", 2000);

    assert!(report.diagnostics.is_empty());
    assert!((mid_bounds.x0 - start_bounds.x0).abs() > 0.1);
    assert!(mid_bounds.x0 > end_bounds.x0);
    assert!(mid_bounds.y0 > end_bounds.y0);
    assert!(mid_bounds.x1 < end_bounds.x1);
}

#[test]
fn test_actor_morph_modifiers_require_timed_redeclaration() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(48.0),
                }],
                modifiers: vec![Modifier {
                    name: Some("strategy".to_string()),
                    value: Expr::Ident("match".to_string()),
                }],
                children: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.output.tracks.contains_key("badge"));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::InvalidModifierValue
            && diagnostic
                .message
                .contains("require a path-morphing re-declaration")
    }));
}

#[test]
fn test_action_reports_morph_only_modifier_keys_as_unsupported() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Action(animatix::ast::Action {
                verb: "fade-out".to_string(),
                targets: vec!["badge".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: Some("strategy".to_string()),
                    value: Expr::Ident("match".to_string()),
                }],
            })],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.output.tracks.contains_key("badge"));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedModifierKey
            && diagnostic.message.contains("strategy")
    }));
}

#[test]
fn test_action_delay_starts_later() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Action(animatix::ast::Action {
                verb: "fade-out".to_string(),
                targets: vec!["badge".to_string()],
                args: vec![],
                modifiers: vec![
                    Modifier {
                        name: Some("delay".to_string()),
                        value: Expr::Ident("1s".to_string()),
                    },
                    Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    },
                ],
            })],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("badge")
        .expect("badge track should exist");

    assert!(report.diagnostics.is_empty());
    assert_eq!(track.opacity.evaluate(1500), 1.0);
    assert!(track.opacity.evaluate(2500) < 0.75);
    assert_eq!(track.opacity.evaluate(3000), 0.0);
}

#[test]
fn test_sequence_advances_statement_timing() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::Sequence {
                body: vec![
                    Stmt::Action(animatix::ast::Action {
                        verb: "fade-out".to_string(),
                        targets: vec!["badge".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("500ms".to_string()),
                        }],
                    }),
                    Stmt::Assignment {
                        target: vec!["badge".to_string()],
                        property: "radius".to_string(),
                        value: Expr::Num(50.0),
                        modifiers: vec![
                            Modifier {
                                name: Some("delay".to_string()),
                                value: Expr::Ident("100ms".to_string()),
                            },
                            Modifier {
                                name: None,
                                value: Expr::Ident("200ms".to_string()),
                            },
                        ],
                    },
                ],
            },
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("badge")
        .expect("badge track should exist");

    assert!(report.diagnostics.is_empty());
    assert_eq!(track.opacity.evaluate(250), 0.5);
    assert_eq!(track.size.evaluate(550), [24.0, 24.0]);
    assert!(track.size.evaluate(700)[0] > 24.0);
    assert_eq!(track.size.evaluate(800), [50.0, 50.0]);
}

#[test]
fn test_sequence_reports_unsupported_statements() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Sequence {
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "late_badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedSequenceStatement
            && diagnostic.message.contains("actor declaration")
    }));
    assert!(!report.output.tracks.contains_key("late_badge"));
}

#[test]
fn test_stagger_offsets_statement_start_times() {
    let actor = |label: &str| Stmt::ActorDecl {
        is_pub: false,
        label: label.to_string(),
        ty: "Circle".to_string(),
        props: vec![Property {
            name: "radius".to_string(),
            value: Expr::Num(24.0),
        }],
        modifiers: vec![],
        children: vec![],
    };

    let fade_out = |label: &str| {
        Stmt::Action(animatix::ast::Action {
            verb: "fade-out".to_string(),
            targets: vec![label.to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("200ms".to_string()),
            }],
        })
    };

    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            actor("a"),
            actor("b"),
            actor("c"),
            Stmt::Stagger {
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("150ms".to_string()),
                }],
                body: vec![fade_out("a"), fade_out("b"), fade_out("c")],
            },
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track_a = report.output.tracks.get("a").expect("a track");
    let track_b = report.output.tracks.get("b").expect("b track");
    let track_c = report.output.tracks.get("c").expect("c track");

    assert!(report.diagnostics.is_empty());
    assert!(track_a.opacity.evaluate(100) < 1.0);
    assert_eq!(track_b.opacity.evaluate(100), 1.0);
    assert!(track_b.opacity.evaluate(250) < 1.0);
    assert_eq!(track_c.opacity.evaluate(250), 1.0);
    assert!(track_c.opacity.evaluate(450) < 1.0);
}

#[test]
fn test_stagger_reports_unsupported_statements() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Stagger {
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident("150ms".to_string()),
            }],
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "late_badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::UnsupportedStaggerStatement
            && diagnostic.message.contains("actor declaration")
    }));
    assert!(!report.output.tracks.contains_key("late_badge"));
}

#[test]
fn test_rotation_assignment_animates_track() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "panel".to_string(),
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(60.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(0.0),
            body: vec![Stmt::Assignment {
                target: vec!["panel".to_string()],
                property: "rotation".to_string(),
                value: Expr::Num(std::f64::consts::PI / 2.0),
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("panel")
        .expect("panel track should exist");

    assert!(report.diagnostics.is_empty());
    assert!((track.rotation.evaluate(0) - 0.0).abs() < f32::EPSILON);
    assert!((track.rotation.evaluate(1000) - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
}

#[test]
fn test_scale_assignment_animates_track() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "panel".to_string(),
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(60.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(0.0),
            body: vec![Stmt::Assignment {
                target: vec!["panel".to_string()],
                property: "scale".to_string(),
                value: Expr::Num(1.75),
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("panel")
        .expect("panel track should exist");

    assert!(report.diagnostics.is_empty());
    assert!((track.scale.evaluate(0) - 1.0).abs() < f32::EPSILON);
    assert!((track.scale.evaluate(1000) - 1.75).abs() < 0.0001);
}

#[test]
fn test_delayed_first_declaration_stays_hidden_until_apply_time() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Circle".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(24.0),
            }],
            modifiers: vec![Modifier {
                name: Some("delay".to_string()),
                value: Expr::Ident("1s".to_string()),
            }],
            children: vec![],
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("badge")
        .expect("badge track should exist");

    assert!(report.diagnostics.is_empty());
    assert!(track.vector_paths.evaluate(999).is_empty());
    assert!(!track.vector_paths.evaluate(1000).is_empty());
}

#[test]
fn test_duplicate_timing_modifiers_warn_and_last_value_wins() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "badge".to_string(),
            ty: "Circle".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(24.0),
            }],
            modifiers: vec![
                Modifier {
                    name: Some("delay".to_string()),
                    value: Expr::Ident("250ms".to_string()),
                },
                Modifier {
                    name: Some("delay".to_string()),
                    value: Expr::Ident("1s".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("ease-in".to_string()),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string()),
                },
            ],
            children: vec![],
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("badge")
        .expect("badge track should exist");

    assert!(track.vector_paths.evaluate(999).is_empty());
    assert!(!track.vector_paths.evaluate(1000).is_empty());
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == DiagnosticCode::ConflictingModifierKey)
            .count(),
        2
    );
}

#[test]
fn test_timed_redeclaration_stores_and_uses_morph_options() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "shape".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(50.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(300.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "shape".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(180.0)]),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(640.0), Expr::Num(360.0)]),
                    },
                ],
                modifiers: vec![
                    Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    },
                    Modifier {
                        name: Some("strategy".to_string()),
                        value: Expr::Ident("match".to_string()),
                    },
                    Modifier {
                        name: Some("path_arc".to_string()),
                        value: Expr::Num(1.2),
                    },
                    Modifier {
                        name: Some("stretch".to_string()),
                        value: Expr::Bool(true),
                    },
                ],
                children: vec![],
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report
        .output
        .tracks
        .get("shape")
        .expect("shape track should exist");
    let morph_options = track.morph_options.evaluate(2000);
    let mid_bounds = vector_path_bounds(&report.output, "shape", 1500);

    assert!(report.diagnostics.is_empty());
    assert_eq!(morph_options.strategy, MorphStrategy::Match);
    assert!((morph_options.path_arc - 1.2).abs() < f64::EPSILON);
    assert!(morph_options.stretch);
    assert!(mid_bounds.x0.is_finite());
    assert!(mid_bounds.y0.is_finite());
}

#[test]
fn test_row_child_with_explicit_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "origin_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_text_primitive_reports_measured_size() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "title".to_string(),
            ty: "Text".to_string(),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Animatix layout".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(36.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["title"].size.evaluate(0);

    assert_ne!(size, [50.0, 50.0]);
    assert!(size[0] > 0.0);
    assert!(size[1] > 0.0);
}

#[test]
fn test_math_primitive_reports_measured_size() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "formula".to_string(),
            ty: "Math".to_string(),
            props: vec![
                Property {
                    name: "math".to_string(),
                    value: Expr::Str("E = mc^2".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(42.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["formula"].size.evaluate(0);

    assert_ne!(size, [50.0, 50.0]);
    assert!(size[0] > 0.0);
    assert!(size[1] > 0.0);
}

#[test]
fn test_code_primitive_reports_measured_size() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "snippet".to_string(),
            ty: "Code".to_string(),
            props: vec![
                Property {
                    name: "code".to_string(),
                    value: Expr::Str("fn main() {}".to_string()),
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(22.0),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["snippet"].size.evaluate(0);

    assert_ne!(size, [50.0, 50.0]);
    assert!(size[0] > 0.0);
    assert!(size[1] > 0.0);
}

#[test]
fn test_svg_primitive_reports_measured_size() {
    let svg_path = format!("{}/../../examples/vector.svg", env!("CARGO_MANIFEST_DIR"));
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Svg {
            label: Some("logo".to_string()),
            url: svg_path,
            at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            anchor: None,
            offset: None,
            scale: 1.0,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["logo"].size.evaluate(0);

    assert_ne!(size, [50.0, 50.0]);
    assert_eq!(size, [40.0, 40.0]);
}

#[test]
fn test_scaled_svg_primitive_reports_scaled_size() {
    let svg_path = format!("{}/../../examples/vector.svg", env!("CARGO_MANIFEST_DIR"));
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Svg {
            label: Some("logo".to_string()),
            url: svg_path,
            at: Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)])),
            anchor: None,
            offset: None,
            scale: 2.0,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["logo"].size.evaluate(0);

    assert_eq!(size, [80.0, 80.0]);
}

#[test]
fn test_svg_scene_percent_position_assignment_interpolates_binding() {
    let ast = parse_program(
        r#"
logo: Svg { url: "examples/vector.svg", at: (72%, 38%) }

#1s
logo.at = (70%, 32%) [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("logo")
        .expect("logo track should exist");

    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::ScenePercent {
            x: 0.72,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    match track.position_binding.evaluate(1500) {
        PositionBinding::ScenePercent { x, y, offset } => {
            assert!((x - 0.71).abs() < f32::EPSILON * 4.0);
            assert!((y - 0.35).abs() < f32::EPSILON * 4.0);
            assert_eq!(offset, [0.0, 0.0]);
        }
        other => panic!("expected scene-percent binding at midpoint, got {other:?}"),
    }
    assert_eq!(
        track.position_binding.evaluate(2000),
        PositionBinding::ScenePercent {
            x: 0.70,
            y: 0.32,
            offset: [0.0, 0.0],
        }
    );
}

#[test]
fn test_image_scene_percent_position_assignment_interpolates_binding() {
    let ast = parse_program(
        r#"
photo: Image { url: "examples/checker.ppm", at: (30%, 38%) }

#1s
photo.at = (32%, 36%) [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("photo")
        .expect("photo track should exist");

    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::ScenePercent {
            x: 0.30,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    match track.position_binding.evaluate(1500) {
        PositionBinding::ScenePercent { x, y, offset } => {
            assert!((x - 0.31).abs() < f32::EPSILON * 4.0);
            assert!((y - 0.37).abs() < f32::EPSILON * 4.0);
            assert_eq!(offset, [0.0, 0.0]);
        }
        other => panic!("expected scene-percent binding at midpoint, got {other:?}"),
    }
}

#[test]
fn test_showcase_logo_is_layout_managed_inside_anchored_svg_column() {
    let showcase = std::fs::read_to_string(format!(
        "{}/../../examples/showcase.amx",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("showcase example should be readable");

    let ast = parse_program(&showcase);
    let timeline = Timeline::build(&ast);
    let logo_container = timeline
        .tracks
        .get("logo_container")
        .expect("logo_container track should exist");
    let track = timeline
        .tracks
        .get("logo_svg")
        .expect("logo_svg track should exist");

    assert_eq!(
        logo_container.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
    assert_eq!(
        track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
}

#[test]
fn test_svg_source_anchor_and_offset_builds_scene_anchor_binding() {
    let ast = parse_program(
        r#"
icon: Svg { url: "examples/vector.svg", anchor: scene.top, offset: (0, 48) }
"#,
    );

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("icon")
        .expect("icon track should exist");

    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Top,
            offset: [0.0, 48.0],
        }
    );
}

#[test]
fn test_row_child_without_at_is_layout_managed() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                },
            ],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "auto_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("auto_child")
        .expect("auto_child track should exist");

    assert_eq!(
        track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_row_mixed_manual_and_layout_children() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                },
            ],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "manual_child".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                        },
                        Property {
                            name: "at".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "layout_child".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let manual_track = timeline
        .tracks
        .get("manual_child")
        .expect("manual_child track should exist");
    let layout_track = timeline
        .tracks
        .get("layout_child")
        .expect("layout_child track should exist");

    assert_eq!(
        manual_track.placement_mode.evaluate(0),
        PlacementMode::Manual
    );
    assert_eq!(manual_track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        layout_track.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(layout_track.position.evaluate(0), [30.0, 0.0]);
}

#[test]
fn test_col_child_with_explicit_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "col".to_string(),
            ty: "Col".to_string(),
            props: vec![
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(400.0)]),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                },
            ],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "origin_child".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(60.0)]),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
}

#[test]
fn test_row_child_with_explicit_non_origin_stays_manual() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "manual_child".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(45.0), Expr::Num(55.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("manual_child")
        .expect("manual_child track should exist");

    assert_eq!(track.placement_mode.evaluate(0), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(0), [45.0, 55.0]);
}

#[test]
fn test_assignment_at_marks_manual_from_assignment_start() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "child".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["child".to_string()],
                property: "at".to_string(),
                value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(50.0)]),
                modifiers: vec![animatix::ast::Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("child")
        .expect("child track should exist");

    assert_eq!(
        track.placement_mode.evaluate(999),
        PlacementMode::LayoutManaged
    );
    assert_eq!(track.placement_mode.evaluate(1000), PlacementMode::Manual);
    assert_eq!(track.placement_mode.evaluate(1500), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(1500), [50.0, 25.0]);
}

#[test]
fn test_redeclaration_binding_change_does_not_apply_before_keyframe() {
    let ast = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "child".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "child".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(50.0)]),
                    },
                ],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            }],
            span: None,
        },
    ];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("child")
        .expect("child track should exist");

    assert_eq!(
        track.placement_mode.evaluate(999),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.evaluate(999),
        PositionBinding::Absolute
    );
    assert_eq!(track.placement_mode.evaluate(1000), PlacementMode::Manual);
    assert_eq!(track.position.evaluate(1500), [50.0, 25.0]);
}

#[test]
fn test_root_row_without_at_uses_container_default_center_binding() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("row").expect("row track should exist");

    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::ContainerDefault {
            anchor: SceneAnchor::Center,
        }
    );
}

#[test]
fn test_grid_layout_positions_children_in_cells() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "grid".to_string(),
            ty: "Grid".to_string(),
            props: vec![
                Property {
                    name: "cols".to_string(),
                    value: Expr::Num(2.0),
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                },
            ],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "a".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "b".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "c".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "d".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("a")
            .expect("a track")
            .position
            .evaluate(0),
        [-25.0, -15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("b")
            .expect("b track")
            .position
            .evaluate(0),
        [25.0, -15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("c")
            .expect("c track")
            .position
            .evaluate(0),
        [-25.0, 15.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("d")
            .expect("d track")
            .position
            .evaluate(0),
        [25.0, 15.0]
    );
}

#[test]
fn test_stack_layout_overlaps_children_by_default() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "stack".to_string(),
            ty: "Stack".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "base".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(80.0), Expr::Num(50.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "overlay".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(12.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("base")
            .expect("base track")
            .position
            .evaluate(0),
        [0.0, 0.0]
    );
    assert_eq!(
        timeline
            .tracks
            .get("overlay")
            .expect("overlay track")
            .position
            .evaluate(0),
        [0.0, 0.0]
    );
}

#[test]
fn test_row_with_text_children_uses_measured_bounds() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(20.0),
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "short".to_string(),
                    ty: "Text".to_string(),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("A".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(36.0),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "long".to_string(),
                    ty: "Text".to_string(),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("Measured layout".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(36.0),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let short = timeline.tracks.get("short").expect("short track");
    let long = timeline.tracks.get("long").expect("long track");
    let short_size = short.size.evaluate(0);
    let long_size = long.size.evaluate(0);

    assert!(short_size[0] > 0.0);
    assert!(long_size[0] > short_size[0]);
    assert_eq!(short.position.evaluate(0), [-(long_size[0] + 10.0), 0.0]);
    assert_eq!(long.position.evaluate(0), [short_size[0] + 10.0, 0.0]);
}

#[test]
fn test_row_layout_does_not_reflow_from_scaled_child_animation() {
    let ast = parse_program(
        r#"
row: Row, gap: 20 {
  badge: Circle, radius: 10, color: gold
  label: Text, text: "Stable layout", font_size: 28
}

#1s
label.scale = 2 [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    let badge = timeline
        .tracks
        .get("badge")
        .expect("badge track should exist");
    let label = timeline
        .tracks
        .get("label")
        .expect("label track should exist");

    let badge_position = badge.position.evaluate(0);
    let label_position = label.position.evaluate(0);

    assert_eq!(badge.position.evaluate(1500), badge_position);
    assert_eq!(label.position.evaluate(1500), label_position);
    assert!(label.scale.evaluate(1500) > 1.0);
    assert_eq!(label.scale.evaluate(2000), 2.0);
}

#[test]
fn test_col_with_code_child_uses_measured_bounds() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "col".to_string(),
            ty: "Col".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(12.0),
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "panel".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(40.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "snippet".to_string(),
                    ty: "Code".to_string(),
                    props: vec![
                        Property {
                            name: "code".to_string(),
                            value: Expr::Str("let x = 1".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(22.0),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let panel = timeline.tracks.get("panel").expect("panel track");
    let snippet = timeline.tracks.get("snippet").expect("snippet track");
    let panel_size = panel.size.evaluate(0);
    let snippet_size = snippet.size.evaluate(0);

    assert_eq!(panel_size, [60.0, 20.0]);
    assert!(snippet_size[1] > 0.0);
    assert_eq!(panel.position.evaluate(0), [0.0, -(snippet_size[1] + 6.0)]);
    assert_eq!(snippet.position.evaluate(0), [0.0, panel_size[1] + 6.0]);
}

#[test]
fn test_row_with_mixed_authored_and_measured_children() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "gap".to_string(),
                value: Expr::Num(16.0),
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "dot".to_string(),
                    ty: "Circle".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "label".to_string(),
                    ty: "Text".to_string(),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("Measured".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(28.0),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "image".to_string(),
                    ty: "Image".to_string(),
                    props: vec![
                        Property {
                            name: "url".to_string(),
                            value: Expr::Str(example_path("checker.ppm")),
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(32.0), Expr::Num(24.0)]),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let dot = timeline.tracks.get("dot").expect("dot track");
    let label = timeline.tracks.get("label").expect("label track");
    let image = timeline.tracks.get("image").expect("image track");
    let dot_size = dot.size.evaluate(0);
    let label_size = label.size.evaluate(0);
    let image_size = image.size.evaluate(0);
    let gap = 16.0;
    let total_width = dot_size[0] * 2.0 + label_size[0] * 2.0 + image_size[0] * 2.0 + gap * 2.0;
    let start = -total_width / 2.0;

    assert_eq!(dot_size, [10.0, 10.0]);
    assert!(label_size[0] > 0.0);
    assert_eq!(image_size, [16.0, 12.0]);
    assert_eq!(dot.position.evaluate(0), [start + dot_size[0], 0.0]);
    assert_eq!(
        label.position.evaluate(0),
        [start + dot_size[0] * 2.0 + gap + label_size[0], 0.0]
    );
    assert_eq!(
        image.position.evaluate(0),
        [
            start + dot_size[0] * 2.0 + gap + label_size[0] * 2.0 + gap + image_size[0],
            0.0,
        ]
    );
}

#[test]
fn test_row_align_start_uses_measured_child_height() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "row".to_string(),
            ty: "Row".to_string(),
            props: vec![Property {
                name: "align".to_string(),
                value: Expr::Str("start".to_string()),
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "small".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(20.0)]),
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "tall".to_string(),
                    ty: "Code".to_string(),
                    props: vec![
                        Property {
                            name: "code".to_string(),
                            value: Expr::Str("fn main() {\n    println!(\"hi\");\n}".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(26.0),
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let small = timeline.tracks.get("small").expect("small track");
    let tall = timeline.tracks.get("tall").expect("tall track");
    let small_size = small.size.evaluate(0);
    let tall_size = tall.size.evaluate(0);

    assert!(tall_size[1] > small_size[1]);
    assert_eq!(small.position.evaluate(0)[1], -tall_size[1] + small_size[1]);
    assert_eq!(tall.position.evaluate(0)[1], 0.0);
}

#[test]
fn test_root_grid_and_stack_without_at_use_container_default_center_binding() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "grid".to_string(),
                ty: "Grid".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "stack".to_string(),
                ty: "Stack".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    for label in ["grid", "stack"] {
        assert_eq!(
            timeline
                .tracks
                .get(label)
                .expect("container track")
                .position_binding
                .evaluate(0),
            PositionBinding::ContainerDefault {
                anchor: SceneAnchor::Center,
            }
        );
    }
}

#[test]
fn test_parser_built_row_with_inline_text_and_image_uses_measured_layout() {
    let src = format!(
        r#"
        #0s
        row: Row, gap: 20 {{
          label: Text, text: "Measured", font_size: 28
          photo: Image, url: "{}", size: (32, 24)
        }}
    "#,
        example_path("checker.ppm")
    );

    let ast = parser()
        .parse(src.as_str())
        .into_result()
        .expect("inline measured layout source should parse");
    let timeline = Timeline::build(&ast);
    let label = timeline.tracks.get("label").expect("label track");
    let photo = timeline.tracks.get("photo").expect("photo track");
    let label_size = label.size.evaluate(0);
    let photo_size = photo.size.evaluate(0);

    assert_eq!(
        label.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        photo.placement_mode.evaluate(0),
        PlacementMode::LayoutManaged
    );
    assert!(label_size[0] > 0.0);
    assert_eq!(photo_size, [16.0, 12.0]);
    assert_eq!(label.position.evaluate(0), [-(photo_size[0] + 10.0), 0.0]);
    assert_eq!(photo.position.evaluate(0), [label_size[0] + 10.0, 0.0]);
}

#[test]
fn test_scene_relative_bindings_are_recorded_on_tracks() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "anchored".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "anchor".to_string(),
                        value: Expr::Path(vec!["scene".to_string(), "top".to_string()]),
                    },
                    Property {
                        name: "offset".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "percent".to_string(),
                ty: "Circle".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)]),
                }],
                modifiers: vec![],
                children: vec![],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    assert_eq!(
        timeline
            .tracks
            .get("anchored")
            .expect("anchored track")
            .position_binding
            .evaluate(0),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Top,
            offset: [0.0, 48.0],
        }
    );
    assert_eq!(
        timeline
            .tracks
            .get("percent")
            .expect("percent track")
            .position_binding
            .evaluate(0),
        PositionBinding::ScenePercent {
            x: 0.5,
            y: 0.25,
            offset: [0.0, 0.0],
        }
    );
}

#[test]
fn test_plot_without_at_stays_local_to_parent_graph() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "graph".to_string(),
            ty: "Graph".to_string(),
            props: vec![Property {
                name: "func".to_string(),
                value: Expr::Closure(
                    vec!["x".to_string()],
                    Box::new(Expr::Ident("x".to_string())),
                ),
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "plot".to_string(),
                ty: "CartesianPlot".to_string(),
                props: vec![Property {
                    name: "func".to_string(),
                    value: Expr::Closure(
                        vec!["x".to_string()],
                        Box::new(Expr::Ident("x".to_string())),
                    ),
                }],
                modifiers: vec![],
                children: vec![],
            }],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("plot")
        .expect("plot track should exist");
    assert_ne!(track.shape_type.evaluate(0), 0);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        track.position_binding.evaluate(0),
        PositionBinding::Absolute
    );
}

#[test]
fn test_evaluate_expr_sin_cos() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    // sin(0) = 0
    let result = evaluate_expr(&Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!({
        let v = result.as_num();
        v.abs() < 1e-10
    });

    // sin(PI/2) ≈ 1
    let result = evaluate_expr(
        &Expr::Call(
            "sin".to_string(),
            vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(0) = 1
    let result = evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(PI) ≈ -1
    let result = evaluate_expr(
        &Expr::Call("cos".to_string(), vec![Expr::Num(std::f64::consts::PI)]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() + 1.0).abs() < 1e-10);

    // sin nested: sin(PI/6) * 2
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call(
                "sin".to_string(),
                vec![Expr::Num(std::f64::consts::FRAC_PI_6)],
            )),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_format() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    // format("value: {}", 42)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![Expr::Str("value: {}".to_string()), Expr::Num(42.0)],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "value: 42");

    // format("x={}, y={}", 10, 20)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("x={}, y={}".to_string()),
                Expr::Num(10.0),
                Expr::Num(20.0),
            ],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "x=10, y=20");

    // format with no args
    let result = evaluate_expr(&Expr::Call("format".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "");

    // format with text and sin
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("sin(π/2) = {}".to_string()),
                Expr::Call(
                    "sin".to_string(),
                    vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
                ),
            ],
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_str(), "sin(π/2) = 1");
}

#[test]
fn test_evaluate_expr_path_uses_dotted_environment_lookup() {
    let mut env = animatix::timeline::Environment::raw_new();
    env.set("left.badge.color", animatix::timeline::Value::Num(7.0));

    let result = evaluate_expr(
        &Expr::Path(vec![
            "left".to_string(),
            "badge".to_string(),
            "color".to_string(),
        ]),
        &env,
    )
    .expect("path lookup should resolve from dotted environment key");

    assert_eq!(result.as_num(), 7.0);
}

#[test]
fn test_evaluate_expr_constants() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    assert!(
        (evaluate_expr(&Expr::Ident("PI".to_string()), &env)
            .unwrap_or(animatix::timeline::Value::Num(0.0))
            .as_num()
            - std::f64::consts::PI)
            .abs()
            < 1e-10
    );
    assert!(
        (evaluate_expr(&Expr::Ident("TAU".to_string()), &env)
            .unwrap_or(animatix::timeline::Value::Num(0.0))
            .as_num()
            - std::f64::consts::TAU)
            .abs()
            < 1e-10
    );
}

#[test]
fn test_evaluate_expr_tuple() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);
    let result = evaluate_expr(&Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [100.0, 200.0]);

    // Tuple with call expressions
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
}

#[test]
fn test_timeline_with_expr_call_properties() {
    // Verify that sin/cos expressions work in property values during timeline build
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::Assignment {
            target: vec!["actor1".to_string()],
            property: "position".to_string(),
            value: Expr::Tuple(vec![
                Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
                Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
            ]),
            modifiers: vec![],
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("actor1").expect("actor1 should exist");
    let pos = track.position.evaluate(0);
    // sin(0)=0, cos(0)=1
    assert!((pos[0] - 0.0).abs() < 1e-6);
    assert!((pos[1] - 1.0).abs() < 1e-6);
}

#[test]
fn test_evaluate_expr_modulo() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Basic modulo: 10 % 3 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(10.0)),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(3.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with division: 7 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(7.0)),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with sin result: sin(PI/2) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call(
                "sin".to_string(),
                vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
            )),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Nested modulo: (10 % 3) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Num(10.0)),
                animatix::ast::BinaryOp::Mod,
                Box::new(Expr::Num(3.0)),
            )),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_vec2_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Vec2 addition: (10, 20) + (5, 5) = (15, 25)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(5.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [15.0, 25.0]);

    // Vec2 subtraction: (10, 20) - (3, 8) = (7, 12)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Sub,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(8.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [7.0, 12.0]);

    // Vec2 multiplication: (10, 20) * (2, 3) = (20, 60)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(3.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 60.0]);

    // Vec2 division: (10, 20) / (2, 4) = (5, 5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            animatix::ast::BinaryOp::Div,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [5.0, 5.0]);

    // Vec2 modulo: (10, 21) % (3, 4) = (1, 1)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(21.0)])),
            animatix::ast::BinaryOp::Mod,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [1.0, 1.0]);

    // Scalar-Vec2 multiplication: 2 * (10, 20) = (20, 40)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 40.0]);

    // Vec2 with sin/cos: (sin(0), cos(0)) = (0, 1)
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
}

#[test]
fn test_evaluate_expr_vec3_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Vec3 addition: (1, 2, 3) + (4, 5, 6) = (5, 7, 9)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![
                Expr::Num(1.0),
                Expr::Num(2.0),
                Expr::Num(3.0),
            ])),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Tuple(vec![
                Expr::Num(4.0),
                Expr::Num(5.0),
                Expr::Num(6.0),
            ])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec3(), [5.0, 7.0, 9.0]);

    // Vec3 scalar multiplication: 2 * (1, 2, 3) = (2, 4, 6)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![
                Expr::Num(1.0),
                Expr::Num(2.0),
                Expr::Num(3.0),
            ])),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    assert_eq!(result.as_vec3(), [2.0, 4.0, 6.0]);
}

#[test]
fn test_evaluate_expr_color_operations() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // Color addition: RED + GREEN = (1, 1, 0, 2)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("RED".to_string())),
            animatix::ast::BinaryOp::Add,
            Box::new(Expr::Ident("GREEN".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 1.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 0.0).abs() < 1e-10);
    assert!((color[3] - 2.0).abs() < 1e-10);

    // Color scalar multiplication: 0.5 * BLUE = (0, 0, 0.5, 0.5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(0.5)),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Ident("BLUE".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 0.0).abs() < 1e-10);
    assert!((color[2] - 0.5).abs() < 1e-10);
    assert!((color[3] - 0.5).abs() < 1e-10);

    // Color subtraction: WHITE - RED = (0, 1, 1, 0) - alpha fades out
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("WHITE".to_string())),
            animatix::ast::BinaryOp::Sub,
            Box::new(Expr::Ident("RED".to_string())),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_rand() {
    let mut env = animatix::timeline::Environment::raw_new();
    animatix::timeline::load_standard_library(&mut env);

    // rand() should return a value between 0 and 1
    let result = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    let val = result.as_num();
    assert!(
        val >= 0.0 && val < 1.0,
        "rand() should return value in [0, 1), got {}",
        val
    );

    // rand() should be called multiple times and produce different results
    let result1 = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    let result2 = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
        .unwrap_or(animatix::timeline::Value::Num(0.0));
    // Note: This test might occasionally fail due to random collision, but extremely unlikely
    // In practice, rand() should produce different values; we test the range here
    let val1 = result1.as_num();
    let val2 = result2.as_num();
    assert!(val1 >= 0.0 && val1 < 1.0);
    assert!(val2 >= 0.0 && val2 < 1.0);

    // rand() with expressions: rand() * 100 should be in [0, 100)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call("rand".to_string(), vec![])),
            animatix::ast::BinaryOp::Mul,
            Box::new(Expr::Num(100.0)),
        ),
        &env,
    )
    .unwrap_or(animatix::timeline::Value::Num(0.0));
    let val = result.as_num();
    assert!(
        val >= 0.0 && val < 100.0,
        "rand() * 100 should be in [0, 100), got {}",
        val
    );
}

#[test]
fn test_namespace_export_resolution_in_expressions() {
    let ast = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "panel_color".to_string(),
            value: Expr::Path(vec!["theme".to_string(), "accent".to_string()]),
        },
        Stmt::ActorDecl {
            is_pub: false,
            label: "panel".to_string(),
            ty: "Rect".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["theme".to_string(), "accent".to_string()]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        },
    ];

    let mut namespaces = std::collections::HashMap::new();
    let mut theme_ns = animatix::module::Namespace::default();
    theme_ns.exports.insert(
        "accent".to_string(),
        Expr::Tuple(vec![
            Expr::Num(0.38),
            Expr::Num(0.78),
            Expr::Num(1.0),
            Expr::Num(1.0),
        ]),
    );
    namespaces.insert("theme".to_string(), theme_ns);

    let report = Timeline::build_with_diagnostics(&ast,
        &namespaces,
    );

    let timeline = report.output;
    let track = timeline.tracks.get("panel").unwrap();
    let color = track.color.last_value();
    assert_eq!(color, [0.38, 0.78, 1.0, 1.0]);
}
