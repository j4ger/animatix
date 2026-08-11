use super::*;
use crate::ast::Property;

#[cfg(test)]
mod bar_chart;
#[cfg(test)]
mod build;
#[cfg(test)]
mod build_diagnostics;
#[cfg(test)]
mod callout;
#[cfg(test)]
mod colorscheme;
#[cfg(test)]
mod container_helpers;
#[cfg(test)]
mod keyframe_times;
#[cfg(test)]
mod layout;
#[cfg(test)]
mod legend;
#[cfg(test)]
mod media;
#[cfg(test)]
mod modifiers;
#[cfg(test)]
mod plot_transitions;
#[cfg(test)]
mod property_lookup;
#[cfg(test)]
mod scene_eval;
#[cfg(test)]
mod taffy_layout;
#[cfg(test)]
mod variable_tracks;

#[test]
fn get_track_mut_invalidates_frame_cache() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "box0".to_string(),
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
            span: None,
        }],
        span: None,
    }];
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let mut timeline = report.output;
    let dims = SceneDimensions {
        width: 100,
        height: 100,
    };
    let _scene = timeline.evaluate(0.0, dims);
    assert!(timeline.frame_cache.borrow().is_some());
    let _track = timeline.get_track_mut("box0");
    assert!(
        timeline.frame_cache.borrow().is_none(),
        "get_track_mut should invalidate the frame cache"
    );
}

#[test]
fn test_animated_scene_has_keyframes() {
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
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "box0".to_string(),
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
                        name: "color".to_string(),
                        value: Expr::Ident("accent.primary".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
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
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(5.0),
            body: vec![
                Stmt::Assignment {
                    target: vec![crate::ast::TargetSegment::Static("box0".to_string())],
                    property: "position".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(200.0)]),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
                Stmt::Assignment {
                    target: vec![crate::ast::TargetSegment::Static("box0".to_string())],
                    property: "color".to_string(),
                    value: Expr::Ident("accent.success".to_string()),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
                Stmt::Assignment {
                    target: vec![crate::ast::TargetSegment::Static("box0".to_string())],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.5),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
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

    let track = timeline.get_track("box0").expect("box0 should exist");
    assert!(track.has_any_keyframes(), "box0 should have animated keyframes");
    assert!(
        track.geometry.position.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "position should have at least 2 keyframes"
    );
    assert!(
        track.style.color.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "color should have at least 2 keyframes"
    );
    assert!(
        track.style.opacity.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "opacity should have at least 2 keyframes"
    );
}
