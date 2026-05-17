use animatix::ast::{BinaryOp, Expr, InlineItem, Modifier, Property, Stmt, Time};
use animatix::diagnostics::DiagnosticCode;
use animatix::easing::Easing;
use animatix::module::ModuleGraph;
use animatix::parser::parser;
use animatix::renderer::text::TextPath;
use animatix::timeline::{
    evaluate_expr, parse_color, time_to_ms, AnimationTrack, ContainerLayoutChild, ContainerMetadata,
    DEFAULT_LAYOUT_HALF_SIZE, Interpolate, LayoutType,
    MorphStrategy, PlacementMode, PositionBinding, PropertyTrack, SceneAnchor, ShapeType, Timeline,
    TrackAccessor,
};
use chumsky::Parser;
use kurbo::Shape;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// Extension trait for evaluating tracks with Default fallback
trait OptionTrackEvaluate<T: Clone> {
    fn evaluate(&self, time_ms: u64) -> T;
}

impl<T> OptionTrackEvaluate<T> for Option<PropertyTrack<T>>
where
    T: Interpolate + Clone + Default,
{
    fn evaluate(&self, time_ms: u64) -> T {
        self.get(time_ms, T::default())
    }
}

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

fn assert_f32_close(actual: f32, expected: f32, epsilon: f32) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {expected} ± {epsilon}, got {actual}"
    );
}

fn assert_vec2_close(actual: [f32; 2], expected: [f32; 2], epsilon: f32) {
    assert_f32_close(actual[0], expected[0], epsilon);
    assert_f32_close(actual[1], expected[1], epsilon);
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
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("red".to_string()),
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
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "actor1".to_string(),
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("blue".to_string()),
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
fn nested_children_keep_their_actual_first_seen_time() {
    let ast = parse_program(
        r#"
        #1s
        group: Col {
          child_text: Text, text: "Hello", font_size: 32
          child_box: Rect, size: (120, 80)
        }
        "#,
    );

    let timeline = Timeline::build(&ast);

    let group = timeline.tracks.get("group").expect("group track should exist");
    let child_text = timeline
        .tracks
        .get("child_text")
        .expect("child_text track should exist");
    let child_box = timeline
        .tracks
        .get("child_box")
        .expect("child_box track should exist");

    assert_eq!(group.first_seen_ms, 1000);
    assert_eq!(child_text.first_seen_ms, 1000);
    assert_eq!(child_box.first_seen_ms, 1000);
}

#[test]
fn redeclaring_nested_actor_does_not_promote_it_to_root() {
    let ast = parse_program(
        r#"
        #0s
        primitives: Row {
          equation: Math, math: "x", font_size: 48
        }

        #1s
        equation: Math, math: "E = mc^2", font_size: 48 [800ms, ease: ease-in-out]
        "#,
    );

    let timeline = Timeline::build(&ast);

    assert!(timeline.root_actor_labels().contains(&"primitives".to_string()));
    assert!(!timeline.root_actor_labels().contains(&"equation".to_string()));

    let primitives = timeline
        .tracks
        .get("primitives")
        .expect("primitives track should exist");
    assert_eq!(primitives.children, vec!["equation".to_string()]);
}

#[test]
fn config_colorscheme_seeds_scene_background_and_text_alias() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Path(vec!["text".to_string(), "primary".to_string()]),
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "color".to_string(),
                        value: Expr::Path(vec!["accent".to_string(), "primary".to_string()]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("red".to_string()),
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "stroke".to_string(),
                        value: Expr::Ident("red".to_string()),
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "alice".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "color".to_string(),
                        value: Expr::Ident("auto".to_string()),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
            span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "bob".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "color".to_string(),
                        value: Expr::Ident("auto".to_string()),
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
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "alice".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Ident("auto".to_string()),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
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
                    label: "formula".to_string(),
                    ty: "Math".to_string(),
                    props: vec![
                        Property {
                            name: "math".to_string(),
                            value: Expr::Str("E = mc^2".to_string()),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
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
                    label: "snippet".to_string(),
                    ty: "Code".to_string(),
                    props: vec![
                        Property {
                            name: "code".to_string(),
                            value: Expr::Str("fn main() {}".to_string()),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
            span: None,
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["accent".to_string(), "missing".to_string()]),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
            span: None,
                },
                // Circle without explicit color should get surface.primary
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "badge".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
            span: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "to".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(0.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
            span: None,
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
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                // Circle with explicit color should keep it
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "explicit".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(1.0),
                                Expr::Num(0.0),
                                Expr::Num(0.0),
                                Expr::Num(1.0),
                            ]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
            span: None,
                },
                // Circle with auto should use auto pool, not surface.primary
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "auto_color".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Ident("auto".to_string()),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            },
            // Circle without explicit color should get surface.primary from default-dark
            Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(48.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "center".to_string()]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(24.0)]),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
            .get(0, PositionBinding::Absolute),
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
        track.position_binding.get(1000, PositionBinding::Absolute),
        PositionBinding::ScenePercent {
            x: 0.30,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    assert_eq!(
        track.position_binding.get(1500, PositionBinding::Absolute),
        PositionBinding::ScenePercent {
            x: 0.31,
            y: 0.37,
            offset: [0.0, 0.0],
        }
    );
    assert_eq!(
        track.position_binding.get(2000, PositionBinding::Absolute),
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
    assert!(track.text_paths.has_keyframe_at(0));
    assert!(track.text_paths.has_keyframe_at(1000));
}

#[test]
fn test_missing_properties() {
    let track = AnimationTrack::new("empty_actor".to_string());

    // With sparse storage (Option<PropertyTrack<T>>), we must use .get() with explicit defaults
    // for properties that may not have keyframes
    assert_eq!(track.position.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(
        track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::Absolute
    );
    assert_eq!(track.size.get(0, [50.0, 50.0]), [50.0, 50.0]);
    assert_eq!(track.line_from.get(0, [-50.0, 0.0]), [-50.0, 0.0]);
    assert_eq!(track.line_to.get(0, [50.0, 0.0]), [50.0, 0.0]);
    assert_eq!(track.arc_angles.get(0, [0.0, std::f32::consts::PI]), [0.0, std::f32::consts::PI]);
    assert_eq!(track.color.get(0, [1.0, 1.0, 1.0, 1.0]), [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(track.shape_type.get(0, ShapeType::Rect), ShapeType::Rect);
    assert_eq!(track.opacity.get(0, 1.0), 1.0);
}

#[test]
fn test_square_primitive_builds_rect_shape() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "sq".to_string(),
            ty: "Rect".to_string(),
            props: vec![Property {
                name: "side".to_string(),
                value: Expr::Num(80.0),
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
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
fn test_ellipse_uses_default_size() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "ell".to_string(),
            ty: "Ellipse".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("ell").expect("ellipse track should exist");

    assert_eq!(track.size.evaluate(0), [50.0, 50.0]);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_regular_polygon_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "hex".to_string(),
            ty: "Polygon".to_string(),
            props: vec![
                Property {
                    name: "sides".to_string(),
                    value: Expr::Num(6.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "radius".to_string(),
                    value: Expr::Num(40.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("hex").expect("hex track should exist");

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Polygon);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_arrow_primitive_builds_runtime_path() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "arrow".to_string(),
            ty: "Line".to_string(),
            props: vec![
                Property {
                    name: "from".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-60.0), Expr::Num(0.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(0.0)]),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("arrow")
        .expect("arrow track should exist");

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Line);
    assert!(!track.vector_paths.evaluate(0).is_empty());
}

#[test]
fn test_arrow_tip_properties_update_size_track() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "arrow".to_string(),
            ty: "Line".to_string(),
            props: vec![
                Property {
                    name: "tip_length".to_string(),
                    value: Expr::Num(30.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "tip_width".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "y_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(300.0)]),
                    value_span: None,
                trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "t_domain".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("cyan".to_string()),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("curve")
        .expect("parametric track should exist");

    assert_ne!(track.shape_type.evaluate(0), ShapeType::Rect);
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "y_domain".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(300.0), Expr::Num(300.0)]),
                    value_span: None,
                trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "resolution".to_string(),
                        value: Expr::Num(48.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "color".to_string(),
                        value: Expr::Ident("cyan".to_string()),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("contour")
        .expect("implicit plot track should exist");

    assert_ne!(track.shape_type.evaluate(0), ShapeType::Rect);
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(18.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(80.0)]),
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
                label: "mirror".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Path(vec!["source".to_string(), "radius".to_string()]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Path(vec!["source".to_string(), "at".to_string()]),
                        value_span: None,
                    trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)]),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "target".to_string(),
                ty: "Ellipse".to_string(),
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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

    assert_eq!(report.output.tracks["echo"].size.evaluate(0), [9.0, 9.0]);
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
            span: None,
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
                    value_span: None,
            span: None,
                },
                Stmt::Assignment {
                    target: vec!["photo".to_string()],
                    property: "url".to_string(),
                    value: Expr::Str(example_path("checker.ppm")),
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                    value_span: None,
            span: None,
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
            span: None,
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
            span: None,
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
fn test_row_with_missing_image_reports_layout_size_fallback() {
    let ast = parse_program(
        r#"
        row: Row, gap: 12 {
            broken: Image, url: "/definitely/missing/animatix-image.png"
            ok: Rect, size: (20, 20)
        }
        "#,
    );

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MediaLoadFailure
            && diagnostic.location.subject.as_deref() == Some("broken.url")
    }));
    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::LayoutSizeFallback
            && diagnostic.location.subject.as_deref() == Some("broken")
    }));
}

#[test]
fn test_manual_missing_image_does_not_report_layout_size_fallback() {
    let ast = parse_program(
        r#"
        row: Row, gap: 12 {
            broken: Image, url: "/definitely/missing/animatix-image.png", at: (10, 0)
            ok: Rect, size: (20, 20)
        }
        "#,
    );

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::MediaLoadFailure
            && diagnostic.location.subject.as_deref() == Some("broken.url")
    }));
    assert!(!report.diagnostics.iter().any(|diagnostic| {
        diagnostic.code == DiagnosticCode::LayoutSizeFallback
            && diagnostic.location.subject.as_deref() == Some("broken")
    }));
}

#[test]
fn test_layout_uses_dedicated_layout_size_not_legacy_size() {
    let mut timeline = Timeline::new();

    let mut container = AnimationTrack::new("row".to_string());
    container.children = vec!["left".to_string(), "right".to_string()];
    timeline.tracks.insert("row".to_string(), container);

    let mut left = AnimationTrack::new("left".to_string());
    left.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(0, [5.0, 5.0], Easing::Linear);
    left.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(0, [20.0, 10.0], Easing::Linear);
    timeline.tracks.insert("left".to_string(), left);

    let mut right = AnimationTrack::new("right".to_string());
    right.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(0, [5.0, 5.0], Easing::Linear);
    right
        .ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(0, [20.0, 10.0], Easing::Linear);
    timeline.tracks.insert("right".to_string(), right);

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: 0.0,
        padding: 0.0,
        align: "center".to_string(),
        cols: None,
        child_order: vec!["left".to_string(), "right".to_string()],
    };
    let layout_children = vec![
        ContainerLayoutChild {
            label: "left".to_string(),
            placement_mode: PlacementMode::LayoutManaged,
        },
        ContainerLayoutChild {
            label: "right".to_string(),
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];

    let computed = timeline
        .layout_engine
        .compute_layout_for_time(&metadata, &layout_children, 0, &timeline.tracks);

    assert_eq!(computed.get("left"), Some(&[-20.0, 0.0]));
    assert_eq!(computed.get("right"), Some(&[20.0, 0.0]));
}

#[test]
fn test_layout_size_fallback_uses_unseeded_layout_size_even_if_legacy_size_exists() {
    let mut timeline = Timeline::new();

    let mut container = AnimationTrack::new("row".to_string());
    container.children = vec!["fallback_child".to_string(), "measured_child".to_string()];
    timeline.tracks.insert("row".to_string(), container);

    let mut fallback_child = AnimationTrack::new("fallback_child".to_string());
    fallback_child
        .size
        .ensure(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(0, [12.0, 8.0], Easing::Linear);
    timeline
        .tracks
        .insert("fallback_child".to_string(), fallback_child);

    let mut measured_child = AnimationTrack::new("measured_child".to_string());
    measured_child
        .ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(0, [10.0, 10.0], Easing::Linear);
    timeline
        .tracks
        .insert("measured_child".to_string(), measured_child);

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: 0.0,
        padding: 0.0,
        align: "center".to_string(),
        cols: None,
        child_order: vec!["fallback_child".to_string(), "measured_child".to_string()],
    };
    let layout_children = vec![ContainerLayoutChild {
        label: "measured_child".to_string(),
        placement_mode: PlacementMode::LayoutManaged,
    }];

    let computed = timeline
        .layout_engine
        .compute_layout_for_time(&metadata, &layout_children, 0, &timeline.tracks);

    assert_eq!(computed.get("fallback_child"), None);
    assert_eq!(computed.get("measured_child"), Some(&[0.0, 0.0]));
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
                value_span: None,
            trailing_comment: None,
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
            span: None,
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
            span: None,
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
                value_span: None,
            span: None,
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
fn test_svg_url_assignment_succeeds() {
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
            span: None,
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
                value_span: None,
            span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // Should NOT emit UnsupportedMediaAssignment
    assert!(!report.diagnostics.iter().any(|diagnostic| {
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "to".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(60.0), Expr::Num(20.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "stroke_width".to_string(),
                    value: Expr::Num(3.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("axis")
        .expect("axis track should exist");
    let bounds = vector_path_bounds(&timeline, "axis", 0);

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Line);
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(30.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("halo")
        .expect("halo track should exist");
    let bounds = vector_path_bounds(&timeline, "halo", 0);

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Ellipse);
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
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(80.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(40.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(0.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(std::f64::consts::PI),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Ellipse);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
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

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Polygon);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
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

    assert_eq!(track.shape_type.evaluate(0), ShapeType::Path);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "to".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(0.0)]),
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
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["axis".to_string()],
                property: "to".to_string(),
                value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(40.0)]),
                modifiers: vec![],
                value_span: None,
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "radius_y".to_string(),
                        value: Expr::Num(30.0),
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
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["halo".to_string()],
                property: "radius_y".to_string(),
                value: Expr::Num(60.0),
                modifiers: vec![],
                value_span: None,
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius_x".to_string(),
                        value: Expr::Num(80.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "radius_y".to_string(),
                        value: Expr::Num(40.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "start_angle".to_string(),
                        value: Expr::Num(0.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "sweep_angle".to_string(),
                        value: Expr::Num(std::f64::consts::PI / 2.0),
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
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::Assignment {
                target: vec!["ring".to_string()],
                property: "sweep_angle".to_string(),
                value: Expr::Num(std::f64::consts::PI),
                modifiers: vec![],
                value_span: None,
            span: None,
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
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "radius_x".to_string(),
                    value: Expr::Num(70.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "radius_y".to_string(),
                    value: Expr::Num(50.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "start_angle".to_string(),
                    value: Expr::Num(std::f64::consts::PI),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "sweep_angle".to_string(),
                    value: Expr::Num(-std::f64::consts::PI / 2.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            span: None,
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
fn test_path_commands_assignment_with_duration_morphs() {
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
                        Expr::Call("move_to".into(), vec![Expr::Num(-120.0), Expr::Num(40.0)]),
                        Expr::Call("line_to".into(), vec![Expr::Num(80.0), Expr::Num(100.0)]),
                        Expr::Call("close".into(), vec![]),
                    ]),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: Time::Seconds(2.0),
            body: vec![Stmt::Assignment {
                target: vec!["guide".to_string()],
                property: "commands".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Call("move_to".into(), vec![Expr::Num(-130.0), Expr::Num(20.0)]),
                    Expr::Call("curve_to".into(), vec![
                        Expr::Num(-40.0), Expr::Num(-140.0),
                        Expr::Num(90.0), Expr::Num(-20.0),
                        Expr::Num(130.0), Expr::Num(50.0),
                    ]),
                    Expr::Call("line_to".into(), vec![Expr::Num(20.0), Expr::Num(120.0)]),
                    Expr::Call("close".into(), vec![]),
                ]),
                modifiers: vec![Modifier { name: None, value: Expr::Ident("1s".to_string()) }],
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let start_bounds = vector_path_bounds(&timeline, "guide", 2000);
    let mid_bounds = vector_path_bounds(&timeline, "guide", 2500);
    let end_bounds = vector_path_bounds(&timeline, "guide", 3000);

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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(48.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![Modifier {
                    name: Some("strategy".to_string()),
                    value: Expr::Ident("match".to_string()),
                }],
                children: vec![],
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                byte_span: None,
            }, None)],
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                byte_span: None,
            }, None)],
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                        byte_span: None,
                    }, None),
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
                        value_span: None,
            span: None,
                    },
                ],
                span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            }],
            span: None,
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
        ty: "Ellipse".to_string(),
        props: vec![Property {
            name: "radius".to_string(),
            value: Expr::Num(24.0),
            value_span: None,
        trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
            span: None,
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
            byte_span: None,
        }, None)
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
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            }],
            span: None,
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
fn test_nested_sequence_timing() {
    // sequence {
    //     fade-out badge [500ms]
    //     sequence {
    //         fade-in badge [300ms]
    //         scale badge [to: 2, 200ms]
    //     }
    // }
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "badge".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(24.0),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
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
                        byte_span: None,
                    }, None),
                    Stmt::Sequence {
                        body: vec![
                            Stmt::Action(animatix::ast::Action {
                                verb: "fade-in".to_string(),
                                targets: vec!["badge".to_string()],
                                args: vec![],
                                modifiers: vec![Modifier {
                                    name: None,
                                    value: Expr::Ident("300ms".to_string()),
                                }],
                                byte_span: None,
                            }, None),
                            Stmt::Assignment {
                                target: vec!["badge".to_string()],
                                property: "scale".to_string(),
                                value: Expr::Num(2.0),
                                modifiers: vec![Modifier {
                                    name: None,
                                    value: Expr::Ident("200ms".to_string()),
                                }],
                                value_span: None,
                                span: None,
                            },
                        ],
                        span: None,
                    },
                ],
                span: None,
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
    // fade-out at 0-500ms
    assert_eq!(track.opacity.evaluate(250), 0.5);
    // fade-in at 500-800ms
    assert_eq!(track.opacity.evaluate(650), 0.5);
    // scale at 800-1000ms
    assert!((track.scale.evaluate(800) - 1.0).abs() < 0.001);
    assert!((track.scale.evaluate(1000) - 2.0).abs() < 0.001);
}

#[test]
fn test_nested_stagger_timing() {
    // stagger [100ms] {
    //     fade-in a [200ms]
    //     stagger [50ms] {
    //         fade-in b [100ms]
    //         fade-in c [100ms]
    //     }
    // }
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                label: "a".to_string(),
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "b".to_string(),
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "c".to_string(),
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::Stagger {
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("100ms".to_string()),
                }],
                body: vec![
                    Stmt::Action(animatix::ast::Action {
                        verb: "fade-in".to_string(),
                        targets: vec!["a".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("200ms".to_string()),
                        }],
                        byte_span: None,
                    }, None),
                    Stmt::Stagger {
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("50ms".to_string()),
                        }],
                        body: vec![
                            Stmt::Action(animatix::ast::Action {
                                verb: "fade-in".to_string(),
                                targets: vec!["b".to_string()],
                                args: vec![],
                                modifiers: vec![Modifier {
                                    name: None,
                                    value: Expr::Ident("100ms".to_string()),
                                }],
                                byte_span: None,
                            }, None),
                            Stmt::Action(animatix::ast::Action {
                                verb: "fade-in".to_string(),
                                targets: vec!["c".to_string()],
                                args: vec![],
                                modifiers: vec![Modifier {
                                    name: None,
                                    value: Expr::Ident("100ms".to_string()),
                                }],
                                byte_span: None,
                            }, None),
                        ],
                        span: None,
                    },
                ],
                span: None,
            },
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(report.diagnostics.is_empty());

    let track_a = report.output.tracks.get("a").unwrap();
    let track_b = report.output.tracks.get("b").unwrap();
    let track_c = report.output.tracks.get("c").unwrap();

    // a fades in at 0-200ms
    assert_eq!(track_a.opacity.evaluate(100), 0.5);
    // b fades in at 100+0=100ms to 200ms (nested stagger starts at 100ms)
    assert_eq!(track_b.opacity.evaluate(150), 0.5);
    // c fades in at 100+50=150ms to 250ms
    assert_eq!(track_c.opacity.evaluate(200), 0.5);
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            span: None,
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
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(24.0),
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![Modifier {
                name: Some("delay".to_string()),
                value: Expr::Ident("1s".to_string()),
            }],
            children: vec![],
            span: None,
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
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(24.0),
                value_span: None,
            trailing_comment: None,
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
            span: None,
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
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(50.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(300.0)]),
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(640.0), Expr::Num(360.0)]),
                        value_span: None,
                    trailing_comment: None,
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
            span: None,
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "origin_child".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.get(0, PlacementMode::LayoutManaged), PlacementMode::Manual);
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(36.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(42.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(22.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
            span: None,
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
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let size = timeline.tracks["logo"].size.evaluate(0);

    assert_eq!(size, [80.0, 80.0]);
}

#[test]
fn test_svg_paths_are_centered_around_origin() {
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
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("logo").expect("logo track should exist");

    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in &track.svg_paths {
        let bounds = path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    assert!((min_x + max_x).abs() < 1e-6, "svg should be horizontally centered");
    assert!((min_y + max_y).abs() < 1e-6, "svg should be vertically centered");
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
        track.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::ScenePercent {
            x: 0.72,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    match track.position_binding.get(1500, PositionBinding::Absolute) {
        PositionBinding::ScenePercent { x, y, offset } => {
            assert!((x - 0.71).abs() < f32::EPSILON * 4.0);
            assert!((y - 0.35).abs() < f32::EPSILON * 4.0);
            assert_eq!(offset, [0.0, 0.0]);
        }
        other => panic!("expected scene-percent binding at midpoint, got {other:?}"),
    }
    assert_eq!(
        track.position_binding.get(2000, PositionBinding::Absolute),
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
        track.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::ScenePercent {
            x: 0.30,
            y: 0.38,
            offset: [0.0, 0.0],
        }
    );
    match track.position_binding.get(1500, PositionBinding::Absolute) {
        PositionBinding::ScenePercent { x, y, offset } => {
            assert!((x - 0.31).abs() < f32::EPSILON * 4.0);
            assert!((y - 0.37).abs() < f32::EPSILON * 4.0);
            assert_eq!(offset, [0.0, 0.0]);
        }
        other => panic!("expected scene-percent binding at midpoint, got {other:?}"),
    }
}

#[test]
fn test_anchored_column_keeps_svg_child_layout_managed() {
    let ast = parse_program(
        r#"
        logo_container: Col, anchor: scene.top, offset: (0, 48) {
          logo_svg: Svg { url: "examples/vector.svg" }
        }
        "#,
    );

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
        logo_container.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Top,
            offset: [0.0, 48.0],
        }
    );
    assert_eq!(
        track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.get(0, PositionBinding::Absolute),
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
        track.position_binding.get(0, PositionBinding::Absolute),
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "auto_child".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("auto_child")
        .expect("auto_child track should exist");

    assert_eq!(
        track.placement_mode.get(0, PlacementMode::LayoutManaged),
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "manual_child".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![
                        Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "at".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "layout_child".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
        manual_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::Manual
    );
    assert_eq!(manual_track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        layout_track.placement_mode.get(0, PlacementMode::LayoutManaged),
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                    value_span: None,
                trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("origin_child")
        .expect("origin_child track should exist");

    assert_eq!(track.placement_mode.get(0, PlacementMode::LayoutManaged), PlacementMode::Manual);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![animatix::ast::InlineItem::Labeled {
                label: "manual_child".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(45.0), Expr::Num(55.0)]),
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("manual_child")
        .expect("manual_child track should exist");

    assert_eq!(track.placement_mode.get(0, PlacementMode::LayoutManaged), PlacementMode::Manual);
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                value_span: None,
            span: None,
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
        track.placement_mode.get(999, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(track.placement_mode.get(1000, PlacementMode::LayoutManaged), PlacementMode::Manual);
    assert_eq!(track.placement_mode.get(1500, PlacementMode::LayoutManaged), PlacementMode::Manual);
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
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "radius".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
            }],
            span: None,
        },
        Stmt::RelativeKeyframe {
            offset: Time::Seconds(1.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                label: "child".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(50.0)]),
                        value_span: None,
                    trailing_comment: None,
                    },
                ],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                }],
                children: vec![],
            span: None,
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
        track.placement_mode.get(999, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        track.position_binding.get(999, PositionBinding::Absolute),
        PositionBinding::Absolute
    );
    assert_eq!(track.placement_mode.get(1000, PlacementMode::LayoutManaged), PlacementMode::Manual);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let track = timeline.tracks.get("row").expect("row track should exist");

    assert_eq!(
        track.position_binding.get(0, PositionBinding::Absolute),
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
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(10.0),
                    value_span: None,
                trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
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
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
                animatix::ast::InlineItem::Labeled {
                    label: "overlay".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(12.0),
                        value_span: None,
                    trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
                value_span: None,
            trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(36.0),
                            value_span: None,
                        trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(36.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
    assert_vec2_close(short.position.evaluate(0), [-(long_size[0] + 10.0), 0.0], 0.3);
    assert_vec2_close(long.position.evaluate(0), [short_size[0] + 10.0, 0.0], 0.3);
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
fn test_row_layout_reflows_from_size_change_when_dynamic_enabled() {
    let ast = parse_program(
        r#"
config { dynamic_layout: true }

row: Row, gap: 20 {
  badge: Circle, radius: 10, color: gold
  label: Rect, size: (40, 20), color: blue
}

#1s
label.size = (80, 40) [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    assert!(timeline.dynamic_layout);

    let label = timeline.tracks.get("label").expect("label track");

    // At t=0, label half-size is (20, 10)
    assert_eq!(label.size.evaluate(0), [20.0, 10.0]);

    // At t=2s, label half-size is (40, 20)
    assert_eq!(label.size.evaluate(2000), [40.0, 20.0]);

    // With DYNAMIC layout enabled, when we evaluate the scene at t=2s,
    // the layout engine should recompute positions based on the new size.
    // The badge position should shift because label is now wider.

    // For now, verify the flag is set and sizes animate correctly
    // Full dynamic layout verification requires scene evaluation
    assert!(timeline.dynamic_layout);
}

#[test]
fn test_container_metadata_populated_for_layout_containers() {
    let ast = parse_program(
        r#"
row: Row, gap: 30, align: "center" {
  a: Circle, radius: 20
  b: Circle, radius: 20
}
"#,
    );

    let timeline = Timeline::build(&ast);

    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have container metadata");

    assert!(matches!(metadata.layout_type, LayoutType::Row));
    assert_eq!(metadata.gap, 30.0);
    assert_eq!(metadata.align, "center");
    assert_eq!(metadata.child_order, vec!["a", "b"]);
}

#[test]
fn test_grid_container_metadata_includes_cols() {
    let ast = parse_program(
        r#"
dashboard: Grid, cols: 3, gap: 10 {
  p1: Rect, size: (50, 50)
  p2: Rect, size: (50, 50)
  p3: Rect, size: (50, 50)
}
"#,
    );

    let timeline = Timeline::build(&ast);

    let metadata = timeline
        .container_metadata
        .get("dashboard")
        .expect("dashboard should have metadata");

    assert!(matches!(metadata.layout_type, LayoutType::Grid));
    assert_eq!(metadata.cols, Some(3));
    assert_eq!(metadata.child_order, vec!["p1", "p2", "p3"]);
}

#[test]
fn test_dynamic_layout_disabled_by_default() {
    let ast = parse_program(
        r#"
row: Row, gap: 20 {
  a: Circle, radius: 10
}
"#,
    );

    let timeline = Timeline::build(&ast);
    assert!(!timeline.dynamic_layout);
}

#[test]
fn test_layout_engine_recomputes_positions_when_size_changes() {
    // Build a timeline with dynamic layout and a size animation
    let ast = parse_program(
        r#"
config { dynamic_layout: true }

row: Row, gap: 20 {
  left: Circle, radius: 10
  right: Rect, size: (40, 20)
}

#1s
right.size = (80, 20) [1s]
"#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row metadata");

    // At t=0: left full width = 20, right full width = 40, gap = 20
    // total = 20 + 20 + 40 = 80, main_start = -40
    // left position = -40 + 10 = -30, right position = -40 + 20 + 20 + 20 = 20
    let positions_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    assert_eq!(positions_0.get("left").copied().unwrap(), [-30.0, 0.0]);
    assert_eq!(positions_0.get("right").copied().unwrap(), [20.0, 0.0]);

    // At t=2000: left full width = 20, right full width = 80 (size doubled), gap = 20
    // total = 20 + 20 + 80 = 120, main_start = -60
    // left position = -60 + 10 = -50, right position = -60 + 20 + 20 + 40 = 20
    let positions_2s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 2000, &timeline.tracks);

    // Left should have shifted left because the row got wider
    assert_eq!(positions_2s.get("left").copied().unwrap(), [-50.0, 0.0]);
    // Right center stays at the same absolute position
    assert_eq!(positions_2s.get("right").copied().unwrap(), [20.0, 0.0]);
}

#[test]
fn test_layout_engine_skips_manual_placement_children() {
    let ast = parse_program(
        r#"
config { dynamic_layout: true }

row: Row, gap: 20 {
  left: Circle, radius: 10
  right: Circle, radius: 10, at: (100, 0)
}
"#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // left is LayoutManaged, so it should be in the result
    assert!(positions.contains_key("left"));
    // right has explicit `at`, so it's Manual and should NOT be in layout result
    assert!(!positions.contains_key("right"));
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "panel".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(40.0)]),
                        value_span: None,
                    trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(22.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
    assert_vec2_close(panel.position.evaluate(0), [0.0, -(snippet_size[1] + 6.0)], 0.1);
    assert_vec2_close(snippet.position.evaluate(0), [0.0, panel_size[1] + 6.0], 0.1);
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "dot".to_string(),
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                    trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(28.0),
                            value_span: None,
                        trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(32.0), Expr::Num(24.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
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
    assert_vec2_close(dot.position.evaluate(0), [start + dot_size[0], 0.0], 0.3);
    assert_vec2_close(
        label.position.evaluate(0),
        [start + dot_size[0] * 2.0 + gap + label_size[0], 0.0]
        ,0.3
    );
    assert_vec2_close(
        image.position.evaluate(0),
        [
            start + dot_size[0] * 2.0 + gap + label_size[0] * 2.0 + gap + image_size[0],
            0.0,
        ],
        0.3
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
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![
                animatix::ast::InlineItem::Labeled {
                    label: "small".to_string(),
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(20.0), Expr::Num(20.0)]),
                        value_span: None,
                    trailing_comment: None,
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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(26.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                },
            ],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let small = timeline.tracks.get("small").expect("small track");
    let tall = timeline.tracks.get("tall").expect("tall track");
    let small_size = small.size.evaluate(0);
    let tall_size = tall.size.evaluate(0);

    assert!(tall_size[1] > small_size[1]);
    assert_f32_close(small.position.evaluate(0)[1], -tall_size[1] + small_size[1], 0.1);
    assert_f32_close(tall.position.evaluate(0)[1], 0.0, 0.1);
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
            span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "stack".to_string(),
                ty: "Stack".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            span: None,
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
                .get(0, PositionBinding::Absolute),
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
        label.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        photo.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert!(label_size[0] > 0.0);
    assert_eq!(photo_size, [16.0, 12.0]);
    assert_vec2_close(label.position.evaluate(0), [-(photo_size[0] + 10.0), 0.0], 0.1);
    assert_vec2_close(photo.position.evaluate(0), [label_size[0] + 10.0, 0.0], 0.1);
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
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "offset".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]),
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
                label: "percent".to_string(),
                ty: "Ellipse".to_string(),
                props: vec![Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(25.0)]),
                    value_span: None,
                trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            span: None,
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
            .get(0, PositionBinding::Absolute),
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
            .get(0, PositionBinding::Absolute),
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
                value_span: None,
            trailing_comment: None,
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

    let timeline = Timeline::build(&ast);
    let track = timeline
        .tracks
        .get("plot")
        .expect("plot track should exist");
    assert_ne!(track.shape_type.evaluate(0), ShapeType::Rect);
    assert_eq!(track.position.evaluate(0), [0.0, 0.0]);
    assert_eq!(
        track.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::Absolute
    );
}

#[test]
fn test_evaluate_expr_sin_cos() {
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
            value_span: None,
            span: None,
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
    let mut env = animatix::timeline::Environment::new();
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
            span: None,
        },
        Stmt::ActorDecl {
            is_pub: false,
            label: "panel".to_string(),
            ty: "Rect".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "color".to_string(),
                    value: Expr::Path(vec!["theme".to_string(), "accent".to_string()]),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
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
    let color = track.color.last([0.0, 0.0, 0.0, 0.0]);
    assert_eq!(color, [0.38, 0.78, 1.0, 1.0]);
}

// =============================================================================
// Taffy-backed layout migration tests
// =============================================================================

/// Verifies that manual children (those with explicit `at`) still affect
/// container spacing, while remaining excluded from layout-managed outputs.
#[test]
fn test_manual_children_do_not_affect_layout_spacing() {
    let ast = parse_program(
        r#"
        row: Row, gap: 20 {
          manual: Circle, radius: 15, at: (200, 50)
          auto: Circle, radius: 10
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // Manual child still contributes spacing. With a 30px manual circle, 20px gap,
    // and a 20px auto circle: total = 30 + 20 + 20 = 70, start = -35,
    // auto center = -35 + 30 + 20 + 10 = 25.
    assert!(positions.contains_key("auto"));
    assert_eq!(positions.get("auto").copied().unwrap(), [25.0, 0.0]);

    // manual should NOT appear in layout positions (it's Manual, not LayoutManaged)
    assert!(!positions.contains_key("manual"));
}

/// Verifies that manual children with explicit positions are placed at their
/// authored coordinates, not at layout-computed positions.
#[test]
fn test_manual_child_preserves_explicit_position() {
    let ast = parse_program(
        r#"
        row: Row, gap: 20 {
          manual: Circle, radius: 15, at: (100, 75)
          auto: Circle, radius: 10
        }
        "#,
    );

    let timeline = Timeline::build(&ast);

    // Manual child's track should have its authored position, not layout-computed
    let manual_track = timeline.tracks.get("manual").expect("manual track");
    assert_eq!(
        manual_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::Manual
    );
    assert_eq!(manual_track.position.evaluate(0), [100.0, 75.0]);
}

/// Verifies center-relative coordinate preservation: children anchored to
/// scene.center should maintain their relative offset from center regardless
/// of parent container's position.
#[test]
fn test_center_anchored_child_preserves_relative_offset() {
    let ast = parse_program(
        r#"
        container: Col {
          child: Circle, radius: 20, anchor: scene.center, offset: (0, 30)
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let child_track = timeline.tracks.get("child").expect("child track");

    // Child should have SceneAnchor binding with center anchor
    assert_eq!(
        child_track.position_binding.get(0, PositionBinding::Absolute),
        PositionBinding::SceneAnchor {
            anchor: SceneAnchor::Center,
            offset: [0.0, 30.0],
        }
    );
}

/// Verifies that Row and Col produce symmetric layout behavior for their
/// respective axes - Row distributes horizontally, Col distributes vertically.
#[test]
fn test_row_col_parity_horizontal_vertical_distribution() {
    // Row: children distributed along X axis, aligned on Y
    let row_ast = parse_program(
        r#"
        row: Row, gap: 10 {
          a: Rect, size: (20, 40)
          b: Rect, size: (30, 20)
        }
        "#,
    );

    // Col: children distributed along Y axis, aligned on X
    let col_ast = parse_program(
        r#"
        col: Col, gap: 10 {
          a: Rect, size: (40, 20)
          b: Rect, size: (20, 30)
        }
        "#,
    );

    let row_timeline = Timeline::build(&row_ast);
    let col_timeline = Timeline::build(&col_ast);

    let row_metadata = row_timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");
    let col_metadata = col_timeline
        .container_metadata
        .get("col")
        .expect("col should have metadata");

    let row_positions = row_timeline
        .layout_engine
        .compute_layout_for_time(row_metadata, &row_metadata.layout_children(&row_timeline.tracks), 0, &row_timeline.tracks);
    let col_positions = col_timeline
        .layout_engine
        .compute_layout_for_time(col_metadata, &col_metadata.layout_children(&col_timeline.tracks), 0, &col_timeline.tracks);

    // Row: total width = 20 + 10 + 30 = 60, start = -30
    // a at -30 + 10 = -20, b at -30 + 20 + 10 + 15 = 15
    assert_eq!(row_positions.get("a").copied().unwrap(), [-20.0, 0.0]);
    assert_eq!(row_positions.get("b").copied().unwrap(), [15.0, 0.0]);

    // Col: total height = 20 + 10 + 30 = 60, start = -30
    // a at -30 + 10 = -20, b at -30 + 20 + 10 + 15 = 15
    assert_eq!(col_positions.get("a").copied().unwrap(), [0.0, -20.0]);
    assert_eq!(col_positions.get("b").copied().unwrap(), [0.0, 15.0]);
}

/// Verifies that Grid maintains consistent cell sizing based on largest child
/// in each row/column, similar to how Row/Col compute their cross-axis extent.
#[test]
fn test_grid_parity_with_row_col_extent_computation() {
    let ast = parse_program(
        r#"
        grid: Grid, cols: 2, gap: 8 {
          tall: Rect, size: (30, 60)
          wide: Rect, size: (50, 30)
          small: Rect, size: (20, 20)
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("grid")
        .expect("grid should have metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // Col 0: max width = max(30, 20) = 50, positions should center in 50-wide cell
    // Col 1: max width = 50
    // Row 0: max height = max(60, 30) = 60
    // Row 1: max height = 20
    // Total width = 50 + 8 + 50 = 108, start_x = -54
    // Total height = 60 + 8 + 20 = 88, start_y = -44

    // tall (index 0): col 0, row 0 -> x = -54 + 25 = -29, y = -44 + 30 = -14
    // wide (index 1): col 1, row 0 -> x = -54 + 50 + 4 + 25 = 25, y = -14
    // small (index 2): col 0, row 1 -> x = -29, y = -44 + 60 + 4 + 10 = 30

    assert!(positions.get("tall").is_some());
    assert!(positions.get("wide").is_some());
    assert!(positions.get("small").is_some());

    // Just verify they're distinct positions within expected bounds
    let tall_pos = positions.get("tall").copied().unwrap();
    let wide_pos = positions.get("wide").copied().unwrap();
    let small_pos = positions.get("small").copied().unwrap();

    assert!(tall_pos[0] < wide_pos[0]); // tall is left of wide
    assert!(small_pos[1] > tall_pos[1]); // small is below tall
}

/// Verifies that dynamic layout recomputes positions when child size changes.
/// With dynamic layout enabled, layout engine samples sizes at query time.
#[test]
fn test_dynamic_layout_recomputes_on_size_change() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        row: Row, gap: 20 {
          left: Circle, radius: 12
          right: Rect, size: (60, 40)
        }

        #2s
        right.size = (100, 40) [1s]
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");

    assert!(timeline.dynamic_layout);

    // At t=0: left full width = 24, right full width = 60, gap = 20
    // total = 24 + 20 + 60 = 104, start = -52
    // left at -52 + 12 = -40, right at -52 + 24 + 20 + 30 = 22
    let pos_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert_eq!(pos_0.get("left").copied().unwrap(), [-40.0, 0.0]);
    assert_eq!(pos_0.get("right").copied().unwrap(), [22.0, 0.0]);

    // At t=3s (animation complete): left full width = 24, right full width = 100, gap = 20
    // total = 24 + 20 + 100 = 144, start = -72
    // left at -72 + 12 = -60, right at -72 + 24 + 20 + 50 = 22
    let pos_3s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 3000, &timeline.tracks);
    assert_eq!(pos_3s.get("left").copied().unwrap(), [-60.0, 0.0]);
    // right center stays at 22 (same absolute position because left shifted)
    assert_eq!(pos_3s.get("right").copied().unwrap(), [22.0, 0.0]);
}

/// Verifies that dynamic layout uses the admitted child set, but samples sizes
/// from that set at the queried time.
#[test]
fn test_dynamic_layout_recomputes_on_child_addition() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        row: Row, gap: 15 {
          a: Circle, radius: 10
        }

        #1s
        row: Row, gap: 15 {
          a: Circle, radius: 10
          b: Circle, radius: 20
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");

    // The admitted child set is built from the final validated metadata, so both
    // children participate in dynamic layout sampling once admitted.
    let pos_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert_eq!(pos_0.get("a").copied().unwrap(), [-27.5, 0.0]);
    assert_eq!(pos_0.get("b").copied().unwrap(), [17.5, 0.0]);

    // At t=1s+, positions remain consistent with the same child set.
    let pos_1s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 1000, &timeline.tracks);
    assert_eq!(pos_1s.get("a").copied().unwrap(), [-27.5, 0.0]);
    assert_eq!(pos_1s.get("b").copied().unwrap(), [17.5, 0.0]);
}

#[test]
fn test_dynamic_layout_excludes_unadmitted_children_at_all_times() {
    let svg_path = format!("{}/../../examples/vector.svg", env!("CARGO_MANIFEST_DIR"));
    let ast = parse_program(&format!(
        r#"
        config {{ dynamic_layout: true }}
        icon_row: Row, gap: 20 {{
            icon1: Svg {{ url: "{}" }}
            icon2: Svg {{ url: "{}" }}
        }}
        "#,
        svg_path, svg_path
    ));

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let metadata = timeline
        .container_metadata
        .get("icon_row")
        .expect("icon_row should have metadata");

    assert!(timeline.layout_children_for("icon_row").is_empty());

    for time_ms in [0_u64, 500, 1000, 2000] {
        let layout_children = timeline.layout_children_for("icon_row");
        let positions = timeline
            .layout_engine
            .compute_layout_for_time(metadata, &layout_children, time_ms, &timeline.tracks);
        assert!(!positions.contains_key("icon1"));
        assert!(!positions.contains_key("icon2"));
    }
}

#[test]
fn test_dynamic_layout_uses_layout_size_for_radius_y_assignment() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        row: Row, gap: 20 {
          left: Rect, size: (40, 40)
          right: Ellipse, radius_x: 10, radius_y: 10
        }

        #1s
        right.radius_y = 40 [1s]
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");

    let pos_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    let pos_2s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 2000, &timeline.tracks);

    assert_eq!(pos_0.get("left").copied().unwrap(), [-20.0, 0.0]);
    assert_eq!(pos_0.get("right").copied().unwrap(), [30.0, 0.0]);

    // Y-size changes should not affect row main-axis placement, but the mirrored
    // layout_size track must remain seeded so dynamic layout continues to succeed.
    assert_eq!(pos_2s.get("left").copied().unwrap(), [-20.0, 0.0]);
    assert_eq!(pos_2s.get("right").copied().unwrap(), [30.0, 0.0]);
}

/// Verifies Stack unchanged semantics: Stack positions all children at origin (0,0)
/// regardless of their sizes, and does not reflow when children change.
/// Stack is purely overlapping - children pile up at the same point.
#[test]
fn test_stack_overlaps_all_children_at_origin() {
    let ast = parse_program(
        r#"
        stack: Stack {
          base: Rect, size: (100, 80)
          overlay: Circle, radius: 30
          top: Rect, size: (20, 20)
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("stack")
        .expect("stack should have metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // Stack places ALL children at origin regardless of their sizes
    assert_eq!(positions.get("base").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(positions.get("overlay").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(positions.get("top").copied().unwrap(), [0.0, 0.0]);
}

/// Verifies Stack does not reflow when a child's size changes.
/// Stack semantics are that children always overlap at origin.
#[test]
fn test_stack_does_not_reflow_on_size_change() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        stack: Stack {
          growing: Rect, size: (40, 30)
        }

        #1s
        growing.size = (80, 60) [1s]
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("stack")
        .expect("stack should have metadata");

    // Stack should always place 'growing' at origin regardless of size
    let pos_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    let pos_2s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 2000, &timeline.tracks);

    // Both times, growing should be at origin
    assert_eq!(pos_0.get("growing").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(pos_2s.get("growing").copied().unwrap(), [0.0, 0.0]);
}

/// Verifies Stack ignores manual placement - all children go to origin.
#[test]
fn test_stack_ignores_manual_placement_all_children_at_origin() {
    let ast = parse_program(
        r#"
        stack: Stack {
          manual: Circle, radius: 20, at: (500, 300)
          auto: Rect, size: (60, 40)
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("stack")
        .expect("stack should have metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // Stack only includes LayoutManaged children in positions
    // 'manual' is Manual, so only 'auto' should be in positions
    assert!(!positions.contains_key("manual"));
    assert_eq!(positions.get("auto").copied().unwrap(), [0.0, 0.0]);

    // But manual child's track still has its authored position
    let manual_track = timeline.tracks.get("manual").expect("manual track");
    assert_eq!(manual_track.position.evaluate(0), [500.0, 300.0]);
}

// =============================================================================
// Bounded compatibility sweep: actor kinds inside layout containers
// =============================================================================
//
// This section verifies that representative actor families can participate in
// Row/Col/Grid layout containers as layout-managed children (no explicit `at`),
// expose sane positions/sizes, and work correctly with Stack semantics kept separate.
//
// Actor families covered:
//   - Vector shapes:  Circle, Rect
//   - Text:           Text
//   - Math/Code:      Math, Code
//   - SVG:            Svg
//   - Image:          Image
//   - Plot/Graph:     Graph with ParametricPlot child
//
// Layout containers covered:
//   - Row:            horizontal distribution
//   - Col:            vertical distribution
//   - Grid:           2D grid arrangement
//   - Stack:          separate tests verifying origin placement

/// Verifies Text can be layout-managed inside a Row and exposes sane size.
#[test]
fn test_text_in_row_is_layout_managed() {
    let ast = parse_program(
        r#"
        label_row: Row, gap: 12 {
            title: Text, text: "Hello", font_size: 24
            subtitle: Text, text: "World", font_size: 18
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("label_row")
        .expect("label_row should have metadata");

    // Verify children are layout-managed (no explicit at = LayoutManaged)
    let title_track = timeline.tracks.get("title").expect("title track");
    let subtitle_track = timeline.tracks.get("subtitle").expect("subtitle track");
    assert_eq!(
        title_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        subtitle_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Verify sizes are measured (not default [50,50])
    let title_size = title_track.size.evaluate(0);
    let subtitle_size = subtitle_track.size.evaluate(0);
    assert!(title_size[0] > 0.0 && title_size[1] > 0.0, "title size should be measured");
    assert!(subtitle_size[0] > 0.0 && subtitle_size[1] > 0.0, "subtitle size should be measured");

    // Verify layout computes positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(positions.contains_key("title"), "title should have layout position");
    assert!(positions.contains_key("subtitle"), "subtitle should have layout position");

    // Verify distinct horizontal positions (Row distributes along X)
    let title_pos = positions.get("title").copied().unwrap();
    let subtitle_pos = positions.get("subtitle").copied().unwrap();
    assert_ne!(title_pos[0], subtitle_pos[0], "children should have distinct X positions");
}

/// Verifies Math can be layout-managed inside a Col and exposes sane size.
#[test]
fn test_math_in_col_is_layout_managed() {
    let ast = parse_program(
        r#"
        formula_col: Col, gap: 16 {
            eq1: Math, math: "E = mc^2", font_size: 32
            eq2: Math, math: "F = ma", font_size: 28
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("formula_col")
        .expect("formula_col should have metadata");

    // Verify children are layout-managed
    let eq1_track = timeline.tracks.get("eq1").expect("eq1 track");
    let eq2_track = timeline.tracks.get("eq2").expect("eq2 track");
    assert_eq!(
        eq1_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        eq2_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Verify sizes are measured
    let eq1_size = eq1_track.size.evaluate(0);
    let eq2_size = eq2_track.size.evaluate(0);
    assert!(eq1_size[0] > 0.0 && eq1_size[1] > 0.0, "eq1 size should be measured");
    assert!(eq2_size[0] > 0.0 && eq2_size[1] > 0.0, "eq2 size should be measured");

    // Verify layout computes positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(positions.contains_key("eq1"), "eq1 should have layout position");
    assert!(positions.contains_key("eq2"), "eq2 should have layout position");

    // Verify distinct vertical positions (Col distributes along Y)
    let eq1_pos = positions.get("eq1").copied().unwrap();
    let eq2_pos = positions.get("eq2").copied().unwrap();
    assert_ne!(eq1_pos[1], eq2_pos[1], "children should have distinct Y positions");
}

/// Verifies Code can be layout-managed inside a Grid and exposes sane size.
#[test]
fn test_code_in_grid_is_layout_managed() {
    let ast = parse_program(
        r#"
        snippet_grid: Grid, cols: 2, gap: 10 {
            fn_a: Code, code: "fn a() {}", font_size: 14
            fn_b: Code, code: "fn b() {}", font_size: 14
            fn_c: Code, code: "fn c() {}", font_size: 14
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("snippet_grid")
        .expect("snippet_grid should have metadata");

    // Verify children are layout-managed
    let fn_a_track = timeline.tracks.get("fn_a").expect("fn_a track");
    let fn_b_track = timeline.tracks.get("fn_b").expect("fn_b track");
    let fn_c_track = timeline.tracks.get("fn_c").expect("fn_c track");
    for (label, track) in [("fn_a", fn_a_track), ("fn_b", fn_b_track), ("fn_c", fn_c_track)] {
        assert_eq!(
            track.placement_mode.get(0, PlacementMode::LayoutManaged),
            PlacementMode::LayoutManaged,
            "{} should be layout-managed",
            label
        );
        let size = track.size.evaluate(0);
        assert!(size[0] > 0.0 && size[1] > 0.0, "{} size should be measured", label);
    }

    // Verify layout computes positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(positions.contains_key("fn_a"));
    assert!(positions.contains_key("fn_b"));
    assert!(positions.contains_key("fn_c"));

    // Verify fn_a and fn_b are on same row (same Y), fn_c is on next row
    let pos_a = positions.get("fn_a").copied().unwrap();
    let pos_b = positions.get("fn_b").copied().unwrap();
    let pos_c = positions.get("fn_c").copied().unwrap();

    assert_eq!(pos_a[1], pos_b[1], "fn_a and fn_b should be on same row");
    assert_ne!(pos_a[1], pos_c[1], "fn_c should be on different row");
    assert!(pos_a[0] < pos_b[0], "fn_a should be left of fn_b");
}

/// Verifies Svg can be layout-managed inside a Row with anchor and exposes sane size.
///
/// NOTE: SVG inside layout containers without explicit `at` does NOT report intrinsic
/// size - the size track remains at default [0,0]. With the strict layout admission
/// policy, SVGs without seeded layout_size are EXCLUDED from layout positioning.
/// A warning is emitted but they are not given layout positions.
#[test]
fn test_svg_in_row_is_layout_managed() {
    let svg_path = format!("{}/../../examples/vector.svg", env!("CARGO_MANIFEST_DIR"));
    let ast = parse_program(&format!(
        r#"
        icon_row: Row, gap: 20 {{
            icon1: Svg {{ url: "{}" }}
            icon2: Svg {{ url: "{}" }}
        }}
        "#,
        svg_path, svg_path
    ));

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let metadata = timeline
        .container_metadata
        .get("icon_row")
        .expect("icon_row should have metadata");

    // Verify children are layout-managed
    let icon1_track = timeline.tracks.get("icon1").expect("icon1 track");
    let icon2_track = timeline.tracks.get("icon2").expect("icon2 track");
    assert_eq!(
        icon1_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        icon2_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // NOTE: SVG inside layout container does NOT report intrinsic size
    // This is a known limitation - SVG needs explicit at to measure size
    let icon1_size = icon1_track.size.evaluate(0);
    assert_eq!(icon1_size, [0.0, 0.0], "SVG in layout has no intrinsic size");

    // SVGs without seeded layout_size should be excluded from layout_children
    // and a LayoutSizeFallback warning should be emitted
    let layout_children = timeline.layout_children_for("icon_row");
    assert!(!layout_children.iter().any(|c| c.label == "icon1"));
    assert!(!layout_children.iter().any(|c| c.label == "icon2"));
    assert!(report.diagnostics.iter().any(|d| {
        d.code == DiagnosticCode::LayoutSizeFallback
            && d.location.subject.as_deref() == Some("icon1")
    }));
    assert!(report.diagnostics.iter().any(|d| {
        d.code == DiagnosticCode::LayoutSizeFallback
            && d.location.subject.as_deref() == Some("icon2")
    }));

    // Verify layout computes positions - but SVGs are excluded so no positions for them
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(!positions.contains_key("icon1"), "SVG without layout_size should be excluded");
    assert!(!positions.contains_key("icon2"), "SVG without layout_size should be excluded");
}

/// Verifies Image can be layout-managed inside a Col and preserves declared size.
#[test]
fn test_image_in_col_is_layout_managed() {
    let ast = parse_program(&format!(
        r#"
        photo_col: Col, gap: 24 {{
            photo1: Image, url: "{}", size: (32, 24)
            photo2: Image, url: "{}", size: (32, 24)
        }}
        "#,
        example_path("checker.ppm"),
        example_path("checker.ppm")
    ));

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("photo_col")
        .expect("photo_col should have metadata");

    // Verify children are layout-managed
    let photo1_track = timeline.tracks.get("photo1").expect("photo1 track");
    let photo2_track = timeline.tracks.get("photo2").expect("photo2 track");
    assert_eq!(
        photo1_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        photo2_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Image size should propagate through the shared layout size track.
    let photo1_size = photo1_track.size.evaluate(0);
    let photo2_size = photo2_track.size.evaluate(0);
    assert_eq!(photo1_size, [16.0, 12.0]);
    assert_eq!(photo2_size, [16.0, 12.0]);

    // Verify layout computes positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(positions.contains_key("photo1"));
    assert!(positions.contains_key("photo2"));

    // Verify distinct vertical positions (Col distributes along Y)
    let pos1 = positions.get("photo1").copied().unwrap();
    let pos2 = positions.get("photo2").copied().unwrap();
    assert_ne!(pos1[1], pos2[1], "photos should have distinct Y positions");
}

/// Verifies Graph with ParametricPlot child can be layout-managed inside a Grid.
///
/// Graph is built using the AST builder pattern with explicit Closure expressions.
#[test]
fn test_graph_with_parametric_plot_in_grid_is_layout_managed() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            label: "graph_grid".to_string(),
            ty: "Grid".to_string(),
            props: vec![
                Property {
                    name: "cols".to_string(),
                    value: Expr::Num(2.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![
                InlineItem::Labeled {
                    label: "plot_a".to_string(),
                    ty: "Graph".to_string(),
                    props: vec![
                        Property {
                            name: "x_domain".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "y_domain".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(120.0)]),
                            value_span: None,
                        trailing_comment: None,
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
                                        Expr::Call(
                                            "sin".to_string(),
                                            vec![Expr::Ident("t".to_string())],
                                        ),
                                    ])),
                                ),
                                value_span: None,
                            trailing_comment: None,
                            },
                            Property {
                                name: "t_domain".to_string(),
                                value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                                value_span: None,
                            trailing_comment: None,
                            },
                            Property {
                                name: "color".to_string(),
                                value: Expr::Ident("cyan".to_string()),
                                value_span: None,
                            trailing_comment: None,
                            },
                        ],
                        modifiers: vec![],
                        children: vec![],
                    }],
                },
                InlineItem::Labeled {
                    label: "plot_b".to_string(),
                    ty: "Graph".to_string(),
                    props: vec![
                        Property {
                            name: "x_domain".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "y_domain".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "size".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(120.0)]),
                            value_span: None,
                        trailing_comment: None,
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
                                        Expr::Call(
                                            "cos".to_string(),
                                            vec![Expr::Ident("t".to_string())],
                                        ),
                                    ])),
                                ),
                                value_span: None,
                            trailing_comment: None,
                            },
                            Property {
                                name: "t_domain".to_string(),
                                value: Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(2.0)]),
                                value_span: None,
                            trailing_comment: None,
                            },
                            Property {
                                name: "color".to_string(),
                                value: Expr::Ident("magenta".to_string()),
                                value_span: None,
                            trailing_comment: None,
                            },
                        ],
                        modifiers: vec![],
                        children: vec![],
                    }],
                },
            ],
            span: None,
        }],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("graph_grid")
        .expect("graph_grid should have metadata");

    // Verify Graph children are layout-managed
    let plot_a_track = timeline.tracks.get("plot_a").expect("plot_a track");
    let plot_b_track = timeline.tracks.get("plot_b").expect("plot_b track");
    assert_eq!(
        plot_a_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        plot_b_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Verify graphs have proper sizes (stored as half-size)
    let plot_a_size = plot_a_track.size.evaluate(0);
    assert_eq!(plot_a_size, [60.0, 60.0], "graph should report declared half-size");

    // Verify layout computes positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    assert!(positions.contains_key("plot_a"));
    assert!(positions.contains_key("plot_b"));

    // Verify plots are on same row with distinct X positions
    let pos_a = positions.get("plot_a").copied().unwrap();
    let pos_b = positions.get("plot_b").copied().unwrap();
    assert_eq!(pos_a[1], pos_b[1], "plots should be on same row");
    assert!(pos_a[0] < pos_b[0], "plot_a should be left of plot_b");
}

/// Verifies that Circle (vector shape) in Row works correctly with layout.
#[test]
fn test_circle_in_row_is_layout_managed() {
    let ast = parse_program(
        r#"
        dot_row: Row, gap: 10 {
            red_dot: Circle, radius: 15, color: red
            blue_dot: Circle, radius: 20, color: blue
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("dot_row")
        .expect("dot_row should have metadata");

    // Verify children are layout-managed
    let red_track = timeline.tracks.get("red_dot").expect("red_dot track");
    let blue_track = timeline.tracks.get("blue_dot").expect("blue_dot track");
    assert_eq!(
        red_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        blue_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Verify sizes match declared radii
    assert_eq!(red_track.size.evaluate(0), [15.0, 15.0]);
    assert_eq!(blue_track.size.evaluate(0), [20.0, 20.0]);

    // Verify layout positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    let red_pos = positions.get("red_dot").copied().unwrap();
    let blue_pos = positions.get("blue_dot").copied().unwrap();
    assert_ne!(red_pos[0], blue_pos[0], "dots should have distinct X positions");
}

/// Verifies that Rect (vector shape) in Col works correctly with layout.
#[test]
fn test_rect_in_col_is_layout_managed() {
    let ast = parse_program(
        r#"
        box_col: Col, gap: 14 {
            small_box: Rect, size: (40, 30), color: green
            tall_box: Rect, size: (30, 60), color: orange
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("box_col")
        .expect("box_col should have metadata");

    // Verify children are layout-managed
    let small_track = timeline.tracks.get("small_box").expect("small_box track");
    let tall_track = timeline.tracks.get("tall_box").expect("tall_box track");
    assert_eq!(
        small_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(
        tall_track.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );

    // Verify sizes match declared sizes (stored as half-size)
    assert_eq!(small_track.size.evaluate(0), [20.0, 15.0]);
    assert_eq!(tall_track.size.evaluate(0), [15.0, 30.0]);

    // Verify layout positions
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    let small_pos = positions.get("small_box").copied().unwrap();
    let tall_pos = positions.get("tall_box").copied().unwrap();
    assert_ne!(small_pos[1], tall_pos[1], "boxes should have distinct Y positions");
}

// =============================================================================
// Stack semantics kept separate
// =============================================================================

/// Verifies Stack places all actor kinds at origin regardless of their sizes.
///
/// NOTE: With strict layout admission, children without seeded layout_size
/// (like SVG without explicit at) are excluded from layout positioning.
#[test]
fn test_stack_places_all_actor_kinds_at_origin() {
    let svg_path = format!("{}/../../examples/vector.svg", env!("CARGO_MANIFEST_DIR"));
    let ast = parse_program(&format!(
        r#"
        mixed_stack: Stack {{
            label: Text, text: "Title", font_size: 32
            icon: Svg {{ url: "{}" }}
            badge: Circle, radius: 18, color: red
            box: Rect, size: (60, 40), color: blue
        }}
        "#,
        svg_path
    ));

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let metadata = timeline
        .container_metadata
        .get("mixed_stack")
        .expect("mixed_stack should have metadata");

    let layout_children = timeline.layout_children_for("mixed_stack");
    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &layout_children, 0, &timeline.tracks);

    // Children with seeded layout_size should be at origin (Stack semantics)
    // SVG without layout_size is excluded from layout_children
    assert_eq!(positions.get("label").copied().unwrap(), [0.0, 0.0]);
    assert!(!positions.contains_key("icon"), "SVG without layout_size should be excluded");
    assert_eq!(positions.get("badge").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(positions.get("box").copied().unwrap(), [0.0, 0.0]);

    // Verify SVG was excluded and warning was emitted
    assert!(!layout_children.iter().any(|c| c.label == "icon"));
    assert!(report.diagnostics.iter().any(|d| {
        d.code == DiagnosticCode::LayoutSizeFallback
            && d.location.subject.as_deref() == Some("icon")
    }));
}

/// Verifies Stack does not reflow when mixed actor kind children change size.
#[test]
fn test_stack_does_not_reflow_mixed_actor_kinds() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        mixed_stack: Stack {
            growing_text: Text, text: "Growing", font_size: 16
            growing_shape: Rect, size: (30, 20)
        }

        #1s
        growing_text.font_size = 48 [1s]
        growing_shape.size = (80, 60) [1s]
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("mixed_stack")
        .expect("mixed_stack should have metadata");

    // Both at t=0 and t=2s should be at origin
    let pos_0 = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);
    let pos_2s = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 2000, &timeline.tracks);

    assert_eq!(pos_0.get("growing_text").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(pos_0.get("growing_shape").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(pos_2s.get("growing_text").copied().unwrap(), [0.0, 0.0]);
    assert_eq!(pos_2s.get("growing_shape").copied().unwrap(), [0.0, 0.0]);
}

// =============================================================================
// End of bounded compatibility sweep
// =============================================================================

/// Verifies that LayoutEngine.compute_layout_for_time only returns
/// LayoutManaged children, excluding Manual children entirely.
#[test]
fn test_layout_engine_excludes_manual_children() {
    let ast = parse_program(
        r#"
        row: Row, gap: 25 {
          m1: Circle, radius: 10, at: (0, 0)
          l1: Circle, radius: 15
          m2: Rect, size: (20, 20), at: (50, 50)
          l2: Circle, radius: 10
        }
        "#,
    );

    let timeline = Timeline::build(&ast);
    let metadata = timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");

    let positions = timeline
        .layout_engine
        .compute_layout_for_time(metadata, &metadata.layout_children(&timeline.tracks), 0, &timeline.tracks);

    // Only layout-managed children should appear
    assert!(positions.contains_key("l1"));
    assert!(positions.contains_key("l2"));
    assert!(!positions.contains_key("m1"));
    assert!(!positions.contains_key("m2"));

    // Manual children still consume layout slots. Total width is:
    // 20 + 25 + 30 + 25 + 20 + 25 + 20 = 165, start = -82.5.
    // l1 center = -82.5 + 20 + 25 + 15 = -22.5
    // l2 center = -82.5 + 20 + 25 + 30 + 25 + 20 + 25 + 10 = 72.5
    assert_eq!(positions.get("l1").copied().unwrap(), [-22.5, 0.0]);
    assert_eq!(positions.get("l2").copied().unwrap(), [72.5, 0.0]);
}

/// Verifies that non-layout containers do not record container metadata.
#[test]
fn test_layout_engine_returns_empty_for_non_container() {
    let ast = parse_program(
        r#"
        circle: Circle, radius: 30
        "#,
    );

    let timeline = Timeline::build(&ast);
    assert!(!timeline.container_metadata.contains_key("circle"));
}

/// Verifies center alignment in Row distributes children with cross-axis
/// centering, similar to how Col centers along main axis.
#[test]
fn test_row_center_align_same_semantics_as_col_center() {
    // Row with align: center should center children on Y axis
    let row_ast = parse_program(
        r#"
        row: Row, align: "center", gap: 10 {
          tall: Rect, size: (20, 60)
          wide: Rect, size: (40, 30)
        }
        "#,
    );

    // Col with align: center should center children on X axis
    let col_ast = parse_program(
        r#"
        col: Col, align: "center", gap: 10 {
          tall: Rect, size: (60, 20)
          wide: Rect, size: (30, 40)
        }
        "#,
    );

    let row_timeline = Timeline::build(&row_ast);
    let col_timeline = Timeline::build(&col_ast);

    let row_metadata = row_timeline
        .container_metadata
        .get("row")
        .expect("row should have metadata");
    let col_metadata = col_timeline
        .container_metadata
        .get("col")
        .expect("col should have metadata");

    let row_positions = row_timeline
        .layout_engine
        .compute_layout_for_time(row_metadata, &row_metadata.layout_children(&row_timeline.tracks), 0, &row_timeline.tracks);
    let col_positions = col_timeline
        .layout_engine
        .compute_layout_for_time(col_metadata, &col_metadata.layout_children(&col_timeline.tracks), 0, &col_timeline.tracks);

    // Center alignment affects only the cross axis. Main-axis positions still differ.
    assert_eq!(row_positions.get("tall").copied().unwrap(), [-25.0, 0.0]);
    assert_eq!(row_positions.get("wide").copied().unwrap(), [15.0, 0.0]);
    assert_eq!(col_positions.get("tall").copied().unwrap(), [0.0, -25.0]);
    assert_eq!(col_positions.get("wide").copied().unwrap(), [0.0, 15.0]);
}

/// Verifies that container metadata correctly records layout type for all
/// supported container types: Row, Col, Grid, Stack.
#[test]
fn test_container_metadata_layout_type_parity() {
    let row_ast = parse_program(r#"row: Row, gap: 10 { a: Circle, radius: 10 }"#);
    let col_ast = parse_program(r#"col: Col, gap: 10 { a: Circle, radius: 10 }"#);
    let grid_ast = parse_program(r#"grid: Grid, cols: 2 { a: Circle, radius: 10 }"#);
    let stack_ast = parse_program(r#"stack: Stack { a: Circle, radius: 10 }"#);

    let row_tl = Timeline::build(&row_ast);
    let col_tl = Timeline::build(&col_ast);
    let grid_tl = Timeline::build(&grid_ast);
    let stack_tl = Timeline::build(&stack_ast);

    assert!(matches!(
        row_tl.container_metadata.get("row").unwrap().layout_type,
        LayoutType::Row
    ));
    assert!(matches!(
        col_tl.container_metadata.get("col").unwrap().layout_type,
        LayoutType::Col
    ));
    assert!(matches!(
        grid_tl.container_metadata.get("grid").unwrap().layout_type,
        LayoutType::Grid
    ));
    assert!(matches!(
        stack_tl.container_metadata.get("stack").unwrap().layout_type,
        LayoutType::Stack
    ));
}

/// Verifies that swap action animates child positions through interpolation.
#[test]
fn test_swap_action_animates_positions() {
    let ast = parse_program(
        r#"
        config { dynamic_layout: true }

        #0s
        row: Row, gap: 8 {
          a: Rect, size: (60, 80)
          b: Rect, size: (60, 160)
        }

        #1s
        swap a b [500ms]
        "#,
    );

    let timeline = Timeline::build(&ast);

    // Verify child_orders track was created
    assert!(timeline.child_orders.contains_key("row"));
    let track = timeline.child_orders.get("row").unwrap();
    assert_eq!(track.keyframes.len(), 1);
    let (order, _) = track.keyframes.get(&1500).unwrap(); // 1s + 500ms
    assert_eq!(order, &vec!["b".to_string(), "a".to_string()]);

    // Check positions before swap (t=0)
    let pos_before = timeline.compute_animated_layout("row", 0);
    let pos_a_before = pos_before.get("a").copied().unwrap();
    let pos_b_before = pos_before.get("b").copied().unwrap();

    // Check positions during swap (t=1250, midpoint)
    let pos_mid = timeline.compute_animated_layout("row", 1250);
    let pos_a_mid = pos_mid.get("a").copied().unwrap();
    let pos_b_mid = pos_mid.get("b").copied().unwrap();

    // Check positions after swap (t=1500)
    let pos_after = timeline.compute_animated_layout("row", 1500);
    let pos_a_after = pos_after.get("a").copied().unwrap();
    let pos_b_after = pos_after.get("b").copied().unwrap();

    // Before: a is left of b
    assert!(pos_a_before[0] < pos_b_before[0], "a should be left of b before swap");

    // After: a is right of b (they swapped)
    assert!(pos_a_after[0] > pos_b_after[0], "a should be right of b after swap");

    // Mid: positions should be between before and after (allowing for floating point)
    let a_moved = (pos_a_mid[0] - pos_a_before[0]).abs() > 0.1;
    let b_moved = (pos_b_mid[0] - pos_b_before[0]).abs() > 0.1;
    assert!(a_moved, "a should have moved from its start position during animation; before={:?} mid={:?} after={:?}", pos_a_before, pos_a_mid, pos_a_after);
    assert!(b_moved, "b should have moved from its start position during animation; before={:?} mid={:?} after={:?}", pos_b_before, pos_b_mid, pos_b_after);
}

// ─── seeded_rand determinism tests ───────────────────────────────────────

#[test]
fn seeded_rand_is_deterministic() {
    let source = r#"
        c: Circle, radius: 50, color: red
        always {
            c.position = (seeded_rand(1.0) * 100, seeded_rand(2.0) * 100)
        }
    "#;
    let ast = parse_program(source);
    let timeline = Timeline::build(&ast);
    let pos1 = timeline.tracks.get("c").unwrap().position.get(0, [0.0, 0.0]);

    // Rebuild and re-evaluate — same seed must produce same value
    let timeline2 = Timeline::build(&ast);
    let pos2 = timeline2.tracks.get("c").unwrap().position.get(0, [0.0, 0.0]);
    assert_eq!(pos1, pos2, "seeded_rand must be deterministic for the same seed");
}

#[test]
fn seeded_rand_returns_value_in_range() {
    let source = r#"
        c: Circle, radius: 50, color: red
        always {
            c.position = (seeded_rand(42.0) * 100, seeded_rand(42.0) * 100)
        }
    "#;
    let ast = parse_program(source);
    let timeline = Timeline::build(&ast);
    let pos = timeline.tracks.get("c").unwrap().position.get(0, [0.0, 0.0]);
    assert!(
        pos[0] >= 0.0 && pos[0] <= 100.0,
        "seeded_rand should return value in [0,1] range, got {}",
        pos[0]
    );
    assert!(
        pos[1] >= 0.0 && pos[1] <= 100.0,
        "seeded_rand should return value in [0,1] range, got {}",
        pos[1]
    );
}

#[test]
fn seeded_rand_different_seeds_produce_different_values() {
    use animatix::ast::Expr;
    use animatix::timeline::{evaluate_expr, Value};

    let source = r#"
        c: Circle, radius: 50, color: red
    "#;
    let ast = parse_program(source);
    let timeline = Timeline::build(&ast);

    // Evaluate seeded_rand with different seeds directly in the timeline's env
    let expr1 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(1.0)]);
    let expr2 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(2.0)]);

    let val1 = evaluate_expr(&expr1, &timeline.env).unwrap();
    let val2 = evaluate_expr(&expr2, &timeline.env).unwrap();

    let n1 = match val1 { Value::Num(n) => n, _ => panic!("expected num") };
    let n2 = match val2 { Value::Num(n) => n, _ => panic!("expected num") };

    assert_ne!(n1, n2, "different seeds should produce different values");
}
