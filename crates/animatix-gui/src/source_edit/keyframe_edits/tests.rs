use animatix_syntax::ast::{Expr, Stmt, Time};
use animatix_syntax::parser::parse_simple;
use animatix_syntax::walk::time_to_seconds;

use super::super::apply::{SourceEdit, apply_edit};

fn parse(source: &str) -> Vec<Stmt> {
    parse_simple(source).0.expect("failed to parse test source")
}

#[test]
fn insert_keyframe_block() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#,
    );
    let edit = SourceEdit::InsertKeyframe {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("blue".into()),
        time_s: 3.0,
        prev_time_s: 2.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Should have 3 top-level keyframes now
    assert_eq!(stmts.len(), 3);

    // The new keyframe should be a RelativeKeyframe after the #2s one
    if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
        assert_eq!(*offset, Time::Seconds(1.0));
        assert_eq!(body.len(), 1);
        if let Stmt::Assignment {
            target, property, ..
        } = &body[0]
        {
            assert_eq!(
                target,
                &vec![animatix_syntax::ast::TargetSegment::Static(
                    "btn".to_string()
                )]
            );
            assert_eq!(property, "color");
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected RelativeKeyframe");
    }
}

#[test]
fn insert_keyframe_wraps_declarations_in_zero_keyframe() {
    // No keyframes at all — inserting a relative keyframe must wrap the
    // top-level declarations in a single #0s so they don't get shifted.
    let mut stmts = parse(
        r#"btn: Rect, size: (100, 200)
circle: Ellipse, radius: 50"#,
    );

    let edit = SourceEdit::InsertKeyframe {
        scene: None,
        actor: "btn".into(),
        property: "position".into(),
        value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
        time_s: 0.5,
        prev_time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // All leading declarations are wrapped in one #0s, then the new relative keyframe
    assert_eq!(stmts.len(), 2);

    if let Stmt::Keyframe { time, body, .. } = &stmts[0] {
        assert_eq!(*time, Time::Seconds(0.0));
        assert_eq!(body.len(), 2);
    } else {
        panic!("Expected Keyframe at index 0, got {:?}", stmts[0]);
    }

    // Second statement is the new relative keyframe
    if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[1] {
        assert_eq!(*offset, Time::Milliseconds(500));
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected RelativeKeyframe at index 1");
    }
}

#[test]
fn insert_keyframe_adjusts_subsequent_relative_offset() {
    // Inserting between #0s and #+1s should adjust the #+1s offset to #+500ms
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#+1s
btn.color = red"#,
    );

    let edit = SourceEdit::InsertKeyframe {
        scene: None,
        actor: "btn".into(),
        property: "position".into(),
        value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
        time_s: 0.5,
        prev_time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Should have 3 top-level statements
    assert_eq!(stmts.len(), 3);

    // New keyframe at index 1
    if let Stmt::RelativeKeyframe { offset, .. } = &stmts[1] {
        assert_eq!(*offset, Time::Milliseconds(500));
    } else {
        panic!("Expected RelativeKeyframe at index 1");
    }

    // Existing keyframe at index 2 — offset should be reduced
    if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
        assert_eq!(*offset, Time::Milliseconds(500));
    } else {
        panic!("Expected RelativeKeyframe at index 2");
    }
}

#[test]
fn merge_keyframe_updates_existing_assignment() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.color = red
btn.position = (10, 20)"#,
    );

    let edit = SourceEdit::MergeKeyframe {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("blue".into()),
        time_s: 1.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let mut found = false;
    if let Stmt::Keyframe { body, .. } = &stmts[1] {
        for stmt in body {
            if let Stmt::Assignment {
                property, value, ..
            } = stmt
            {
                if property == "color" {
                    assert_eq!(*value, Expr::Ident("blue".into()));
                    found = true;
                }
            }
        }
    }
    assert!(found);
}

#[test]
fn merge_keyframe_uses_relative_time() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#+500ms
btn.color = red"#,
    );

    let edit = SourceEdit::MergeKeyframe {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("green".into()),
        time_s: 0.5,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    if let Stmt::RelativeKeyframe { body, .. } = &stmts[1] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert_eq!(*value, Expr::Ident("green".into()));
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected RelativeKeyframe");
    }
}

#[test]
fn set_keyframe_value_replaces_rhs_at_exact_time_without_moving_it() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)
btn.opacity = 1

#2s
btn.opacity = 0.5"#,
    );

    let edit = SourceEdit::SetKeyframeValue {
        scene: None,
        actor: "btn".into(),
        property: "opacity".into(),
        value: Expr::Num(0.75),
        time_s: 2.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // The #2s block keeps its time; only the RHS changed.
    assert_eq!(keyframe_time_with_property(&stmts, "opacity"), Some(0.0));
    if let Stmt::Keyframe { time, body, .. } = &stmts[1] {
        assert_eq!(*time, Time::Seconds(2.0));
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert_eq!(*value, Expr::Num(0.75));
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected the #2s keyframe block to survive");
    }
}

#[test]
fn set_keyframe_value_ignores_merge_window_and_reports_missing_time() {
    // A keyframe exists at 1s only. Asking for 1.02s (inside the property
    // merge window) must fail rather than editing the 1s keyframe.
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.opacity = 0.5"#,
    );
    let edit = SourceEdit::SetKeyframeValue {
        scene: None,
        actor: "btn".into(),
        property: "opacity".into(),
        value: Expr::Num(0.9),
        time_s: 1.02,
    };
    assert!(apply_edit(&mut stmts, edit).is_err());

    // The exact time is still editable.
    let edit = SourceEdit::SetKeyframeValue {
        scene: None,
        actor: "btn".into(),
        property: "opacity".into(),
        value: Expr::Num(0.9),
        time_s: 1.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());
}

#[test]
fn set_keyframe_value_is_scoped_to_named_scene() {
    let mut stmts = parse(
        r#"# A
#0s
box: Rect, size: (10, 10)

# B
#0s
box: Rect, size: (10, 10)
#1s
box.opacity = 0.25"#,
    );

    let edit = SourceEdit::SetKeyframeValue {
        scene: Some("B".into()),
        actor: "box".into(),
        property: "opacity".into(),
        value: Expr::Num(0.8),
        time_s: 1.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let scene_b = stmts
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Scene { name, body, .. } if name == "B" => Some(body),
            _ => None,
        })
        .expect("scene B");
    if let Stmt::Keyframe { body, .. } = &scene_b[1] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert_eq!(*value, Expr::Num(0.8));
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected keyframe in scene B");
    }
}

#[test]
fn move_keyframe_time_absolute() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red

#+2s
btn.color = blue"#,
    );

    let edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        old_time_s: 2.0,
        new_time_s: 3.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // First keyframe should now be at 3s
    if let Stmt::Keyframe { time, .. } = &stmts[1] {
        assert_eq!(*time, Time::Seconds(3.0));
    } else {
        panic!("Expected Keyframe at index 1");
    }

    // The relative keyframe after it should have its offset adjusted
    // Old: 2s + 2s = 4s. New: 3s + offset = 4s → offset = 1s
    if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
        assert_eq!(*offset, Time::Seconds(1.0));
    } else {
        panic!("Expected RelativeKeyframe at index 2");
    }
}

#[test]
fn move_keyframe_time_relative() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#+2s
btn.color = red

#+3s
btn.color = blue"#,
    );

    let edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        old_time_s: 2.0, // 0s + 2s
        new_time_s: 3.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // First relative keyframe should now have offset 3s
    if let Stmt::RelativeKeyframe { offset, .. } = &stmts[1] {
        assert_eq!(*offset, Time::Seconds(3.0));
    } else {
        panic!("Expected RelativeKeyframe at index 1");
    }

    // Second relative keyframe: old total was 0+2+3=5s.
    // New: first is at 3s, so second should stay at 5s → offset = 2s
    if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
        assert_eq!(*offset, Time::Seconds(2.0));
    } else {
        panic!("Expected RelativeKeyframe at index 2");
    }
}

#[test]
fn move_keyframe_time_no_change() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#,
    );

    let edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        old_time_s: 2.0,
        new_time_s: 2.0, // same time
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Should still be at 2s
    if let Stmt::Keyframe { time, .. } = &stmts[1] {
        assert_eq!(*time, Time::Seconds(2.0));
    } else {
        panic!("Expected Keyframe at index 1");
    }
}

#[test]
fn move_keyframe_time_not_found() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#,
    );

    let edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "btn".into(),
        property: "color".into(),
        old_time_s: 999.0, // doesn't exist
        new_time_s: 3.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn move_keyframe_time_wrong_actor() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#,
    );

    let edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "other".into(),
        property: "color".into(),
        old_time_s: 2.0,
        new_time_s: 3.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn delete_keyframe_is_scoped_to_named_scene() {
    let mut stmts = parse(
        r#"# A
#0s
box: Rect, size: (100, 100)
#2s
box.color = red
# B
#0s
box: Rect, size: (100, 100)
#2s
box.color = blue"#,
    );

    let edit = SourceEdit::DeleteKeyframe {
        scene: Some("A".to_string()),
        actor: "box".into(),
        property: "color".into(),
        time_s: 2.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Scene B keeps its color keyframe.
    let scene_b = stmts.iter().find_map(|stmt| match stmt {
        Stmt::Scene { name, body, .. } if name == "B" => Some(body),
        _ => None,
    });
    let b_has_color = scene_b.is_some_and(|body| {
        body.iter().any(|stmt| match stmt {
            Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => body.iter().any(
                |stmt| matches!(stmt, Stmt::Assignment { property, .. } if property == "color"),
            ),
            _ => false,
        })
    });
    assert!(b_has_color, "scene B color keyframe must survive scene A deletion");
}

#[test]
fn move_and_easing_are_scoped_to_named_scene() {
    let mut stmts = parse(
        r#"# A
#0s
box: Rect, size: (100, 100)
#2s
box.color = red
# B
#0s
box: Rect, size: (100, 100)
#2s
box.color = blue"#,
    );

    let move_edit = SourceEdit::MoveKeyframeTime {
        scene: Some("B".to_string()),
        actor: "box".into(),
        property: "color".into(),
        old_time_s: 2.0,
        new_time_s: 3.0,
    };
    assert!(apply_edit(&mut stmts, move_edit).is_ok());

    let easing_edit = SourceEdit::SetKeyframeEasing {
        scene: Some("A".to_string()),
        actor: "box".into(),
        property: "color".into(),
        time_s: 2.0,
        easing: animatix_syntax::easing::Easing::Bounce,
    };
    assert!(apply_edit(&mut stmts, easing_edit).is_ok());

    // Scene A keyframe stays at 2s; scene B moved its color keyframe to 3s.
    let scene_a_color_time = stmts
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Scene { name, body, .. } if name == "A" => Some(body),
            _ => None,
        })
        .and_then(|body| keyframe_time_with_property(body, "color"));
    assert_eq!(scene_a_color_time, Some(2.0));

    let scene_b_color_time = stmts
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Scene { name, body, .. } if name == "B" => Some(body),
            _ => None,
        })
        .and_then(|body| keyframe_time_with_property(body, "color"));
    assert_eq!(scene_b_color_time, Some(3.0));
}

#[test]
fn effect_param_keyframes_use_dotted_target() {
    let mut stmts = parse(
        r#"#0s
bg: Filter {
  soft: Blur, radius: 4
  img: Rect, size: (100, 100)
}
"#,
    );

    let insert = SourceEdit::InsertKeyframe {
        scene: None,
        actor: "bg".into(),
        property: "soft.radius".into(),
        value: Expr::Num(16.0),
        time_s: 1.0,
        prev_time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, insert).is_ok());

    // The keyframed assignment targets `[bg, soft]` with property `radius`.
    if let Stmt::RelativeKeyframe { body, .. } = &stmts[1] {
        if let Stmt::Assignment {
            target, property, ..
        } = &body[0]
        {
            assert_eq!(
                target,
                &vec![
                    animatix_syntax::ast::TargetSegment::Static("bg".to_string()),
                    animatix_syntax::ast::TargetSegment::Static("soft".to_string()),
                ]
            );
            assert_eq!(property, "radius");
        } else {
            panic!("Expected Assignment");
        }
    } else {
        panic!("Expected RelativeKeyframe");
    }

    // Merging at the same time updates the effect assignment in place.
    let merge = SourceEdit::MergeKeyframe {
        scene: None,
        actor: "bg".into(),
        property: "soft.radius".into(),
        value: Expr::Num(24.0),
        time_s: 1.0,
    };
    assert!(apply_edit(&mut stmts, merge).is_ok());
    if let Stmt::RelativeKeyframe { body, .. } = &stmts[1] {
        if let Stmt::Assignment { value, .. } = &body[0] {
            assert_eq!(*value, Expr::Num(24.0));
        }
    }

    // Easing and moving resolve the same dotted target.
    let easing = SourceEdit::SetKeyframeEasing {
        scene: None,
        actor: "bg".into(),
        property: "soft.radius".into(),
        time_s: 1.0,
        easing: animatix_syntax::easing::Easing::EaseInOut,
    };
    assert!(apply_edit(&mut stmts, easing).is_ok());

    let move_edit = SourceEdit::MoveKeyframeTime {
        scene: None,
        actor: "bg".into(),
        property: "soft.radius".into(),
        old_time_s: 1.0,
        new_time_s: 2.0,
    };
    assert!(apply_edit(&mut stmts, move_edit).is_ok());

    // Deleting removes the effect assignment (and the emptied block).
    let delete = SourceEdit::DeleteKeyframe {
        scene: None,
        actor: "bg".into(),
        property: "soft.radius".into(),
        time_s: 2.0,
    };
    assert!(apply_edit(&mut stmts, delete).is_ok());
    let still_has_effect_assignment = stmts.iter().any(|stmt| match stmt {
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => body
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Assignment { property, .. } if property == "radius")),
        _ => false,
    });
    assert!(!still_has_effect_assignment);
}

fn keyframe_time_with_property(stmts: &[Stmt], property: &str) -> Option<f64> {
    let mut current_time = 0.0;
    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if body.iter().any(|stmt| {
                    matches!(
                        stmt,
                        Stmt::Assignment { property: prop, .. } if prop == property
                    )
                }) {
                    return Some(current_time);
                }
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if body.iter().any(|stmt| {
                    matches!(
                        stmt,
                        Stmt::Assignment { property: prop, .. } if prop == property
                    )
                }) {
                    return Some(current_time);
                }
            },
            _ => {},
        }
    }
    None
}
