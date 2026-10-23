use animatix_syntax::ast::{ComponentDef, Expr, InlineItem, Property, Stmt};
use animatix_syntax::parser::parse_simple;

use super::super::apply::{
    SourceEdit, apply_edit, find_actor_decl_mut, find_assignment_mut, find_prop_mut,
    time_to_seconds,
};

fn parse(source: &str) -> Vec<Stmt> {
    parse_simple(source).0.expect("failed to parse test source")
}

#[test]
fn set_existing_property() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200), color: red"#,
    );
    let edit = SourceEdit::SetProperty {
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("blue".into()),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
    let prop = find_prop_mut(actor, "color").expect("property 'color' should exist");
    assert_eq!(prop.value, Expr::Ident("blue".into()));
}

#[test]
fn set_property_uses_canonical_name() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, at: (100, 200)"#,
    );
    let edit = SourceEdit::SetProperty {
        actor: "btn".into(),
        property: "position".into(),
        value: Expr::Tuple(vec![Expr::Num(150.0), Expr::Num(250.0)]),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
    let prop = find_prop_mut(actor, "at").expect("property 'at' should exist");
    assert_eq!(prop.value, Expr::Tuple(vec![Expr::Num(150.0), Expr::Num(250.0)]));
}

#[test]
fn set_assignment_value() {
    let mut stmts = parse(
        r#"#2s
btn.color = red"#,
    );
    let edit = SourceEdit::SetProperty {
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("blue".into()),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let assignment = find_assignment_mut(&mut stmts, "btn", "color")
        .expect("assignment 'btn.color' should exist");
    if let Stmt::Assignment { value, .. } = assignment {
        assert_eq!(*value, Expr::Ident("blue".into()));
    } else {
        panic!("Expected Assignment");
    }
}

#[test]
fn insert_new_property() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)"#,
    );
    let edit = SourceEdit::InsertProperty {
        actor: "btn".into(),
        property: "color".into(),
        value: Expr::Ident("blue".into()),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
    let prop = find_prop_mut(actor, "color").expect("property 'color' should exist");
    assert_eq!(prop.value, Expr::Ident("blue".into()));
}

#[test]
fn insert_property_returns_false_if_already_exists() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)"#,
    );
    let edit = SourceEdit::InsertProperty {
        actor: "btn".into(),
        property: "size".into(),
        value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
    };
    // Should fail because "size" already exists
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn insert_actor_top_level() {
    let mut stmts = parse(r#"btn: Rect, size: (100, 200)"#);
    let edit = SourceEdit::InsertActor {
        ty: "Ellipse".into(),
        label: "circle1".into(),
        props: vec![Property {
            name: "at".into(),
            value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
            value_span: None,
            trailing_comment: None,
        }],
        container: None,
        time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Should have 2 statements now
    assert_eq!(stmts.len(), 2);

    // Second statement should be the new actor
    if let Stmt::ActorDecl { label, ty, .. } = &stmts[1] {
        assert_eq!(label, "circle1");
        assert_eq!(ty, "Ellipse");
    } else {
        panic!("Expected ActorDecl at index 1");
    }
}

#[test]
fn insert_actor_into_container() {
    let mut stmts = parse(
        r#"row1: Row, gap: 8 {
  btn: Rect, size: (100, 200)
}"#,
    );
    let edit = SourceEdit::InsertActor {
        ty: "Ellipse".into(),
        label: "circle1".into(),
        props: vec![Property {
            name: "at".into(),
            value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
            value_span: None,
            trailing_comment: None,
        }],
        container: Some("row1".into()),
        time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Find the container and verify it has the new child
    let container = find_actor_decl_mut(&mut stmts, "row1").expect("container 'row1' should exist");
    if let Stmt::ActorDecl { children, .. } = container {
        assert_eq!(children.len(), 2);
        if let InlineItem::Labeled { label, ty, .. } = &children[1] {
            assert_eq!(label, "circle1");
            assert_eq!(ty, "Ellipse");
        } else {
            panic!("Expected Labeled child at index 1");
        }
    } else {
        panic!("Expected ActorDecl for row1");
    }
}

#[test]
fn insert_effect_lands_before_content() {
    let mut stmts = parse(
        r#"bg: Filter {
  img: Rect, size: (100, 100)
}"#,
    );
    let edit = SourceEdit::InsertEffect {
        scope: "bg".into(),
        ty: "Blur".into(),
        label: "soft1".into(),
        props: vec![],
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let container = find_actor_decl_mut(&mut stmts, "bg").expect("scope 'bg' should exist");
    if let Stmt::ActorDecl { children, .. } = container {
        assert_eq!(children.len(), 2);
        match &children[0] {
            InlineItem::Labeled { label, ty, .. } => {
                assert_eq!(label, "soft1");
                assert_eq!(ty, "Blur");
            },
            other => panic!("expected the effect first, got {other:?}"),
        }
        assert!(matches!(&children[1], InlineItem::Labeled { label, .. } if label == "img"));
    } else {
        panic!("Expected ActorDecl for bg");
    }
}

#[test]
fn insert_effect_follows_existing_effects() {
    let mut stmts = parse(
        r#"bg: Filter {
  soft: Blur, radius: 4
  img: Rect, size: (100, 100)
}"#,
    );
    let edit = SourceEdit::InsertEffect {
        scope: "bg".into(),
        ty: "ColorGrade".into(),
        label: "warm".into(),
        props: vec![],
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let container = find_actor_decl_mut(&mut stmts, "bg").expect("scope 'bg' should exist");
    if let Stmt::ActorDecl { children, .. } = container {
        let labels: Vec<&str> = children
            .iter()
            .map(|child| match child {
                InlineItem::Labeled { label, .. } => label.as_str(),
                _ => "<anon>",
            })
            .collect();
        assert_eq!(labels, vec!["soft", "warm", "img"]);
    } else {
        panic!("Expected ActorDecl for bg");
    }
}

#[test]
fn insert_effect_reports_missing_scope() {
    let mut stmts = parse(r#"btn: Rect, size: (10, 10)"#);
    let edit = SourceEdit::InsertEffect {
        scope: "nope".into(),
        ty: "Blur".into(),
        label: "soft1".into(),
        props: vec![],
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn rename_actor_and_references() {
    let mut stmts = parse(
        r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.color = blue
btn.position = (200, 100)"#,
    );

    let edit = SourceEdit::RenameActor {
        old_label: "btn".into(),
        new_label: "my_box".into(),
    };
    let _ = apply_edit(&mut stmts, edit);

    // Actor decl should be renamed
    let actor =
        find_actor_decl_mut(&mut stmts, "my_box").expect("renamed actor 'my_box' should exist");
    if let Stmt::ActorDecl { label, .. } = actor {
        assert_eq!(label, "my_box");
    } else {
        panic!("Expected ActorDecl");
    }

    // Old name should not exist
    assert!(find_actor_decl_mut(&mut stmts, "btn").is_none());

    // Assignments should be renamed (search recursively through keyframes)
    let mut found_color = false;
    let mut found_position = false;
    fn walk(stmts: &[Stmt], found_color: &mut bool, found_position: &mut bool) {
        for stmt in stmts {
            match stmt {
                Stmt::Assignment {
                    target, property, ..
                } => {
                    if target.last()
                        == Some(&animatix_syntax::ast::TargetSegment::Static("my_box".to_string()))
                    {
                        if property == "color" {
                            *found_color = true;
                        }
                        if property == "position" {
                            *found_position = true;
                        }
                    }
                    assert!(
                        target.last()
                            != Some(&animatix_syntax::ast::TargetSegment::Static(
                                "btn".to_string()
                            )),
                        "Old reference 'btn' should have been renamed"
                    );
                },
                Stmt::Keyframe { body, .. }
                | Stmt::RelativeKeyframe { body, .. }
                | Stmt::Sequence { body, .. }
                | Stmt::Stagger { body, .. }
                | Stmt::Always { body, .. }
                | Stmt::FnDecl { body, .. }
                | Stmt::Block { body, .. } => {
                    walk(body, found_color, found_position);
                },
                Stmt::ComponentDef(ComponentDef { body, .. }, _) => {
                    walk(body, found_color, found_position);
                },
                Stmt::Conditional {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    walk(then_branch, found_color, found_position);
                    if let Some(else_b) = else_branch {
                        walk(else_b, found_color, found_position);
                    }
                },
                Stmt::ForLoop { body, .. } => {
                    walk(body, found_color, found_position);
                },
                Stmt::ActorDecl { children, .. } => {
                    walk_inline(children, found_color, found_position);
                },
                _ => {},
            }
        }
    }
    fn walk_inline(items: &[InlineItem], _found_color: &mut bool, _found_position: &mut bool) {
        for item in items {
            match item {
                InlineItem::Labeled { children, .. }
                | InlineItem::Anonymous { children, .. }
                | InlineItem::ForLoop { body: children, .. } => {
                    walk_inline(children, _found_color, _found_position);
                },
                InlineItem::SlotFill {
                    items: slot_items, ..
                } => {
                    walk_inline(slot_items, _found_color, _found_position);
                },
                _ => {},
            }
        }
    }
    walk(&stmts, &mut found_color, &mut found_position);
    assert!(found_color, "Color assignment should reference my_box");
    assert!(found_position, "Position assignment should reference my_box");
}

#[test]
fn insert_snippet_at_top_level() {
    use animatix_syntax::ast::{Expr, Property, Stmt};

    let mut stmts = vec![];
    let fragment = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "title".into(),
        array_index: None,
        ty: "Text".into(),
        props: vec![Property::new("content", Expr::Str("Hello".into()))],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let result = super::insert_snippet(&mut stmts, fragment, None, None);
    assert!(result.is_ok());
    assert_eq!(stmts.len(), 1);
    if let Stmt::ActorDecl { label, .. } = &stmts[0] {
        assert_eq!(label, "title");
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn insert_snippet_into_keyframe() {
    use animatix_syntax::ast::{Stmt, Time};

    let mut stmts = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![],
        span: None,
    }];
    let fragment = vec![Stmt::Action(
        animatix_syntax::ast::Action {
            verb: "fade-in".into(),
            targets: vec!["title".into()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        },
        None,
    )];
    let result = super::insert_snippet(&mut stmts, fragment, Some(0.0), None);
    assert!(result.is_ok());
    if let Stmt::Keyframe { body, .. } = &stmts[0] {
        assert_eq!(body.len(), 1);
    } else {
        panic!("Expected Keyframe");
    }
}

#[test]
fn insert_snippet_into_container() {
    use animatix_syntax::ast::Stmt;

    let mut stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "container".into(),
        array_index: None,
        ty: "Row".into(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let fragment = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "child".into(),
        array_index: None,
        ty: "Text".into(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let result = super::insert_snippet(&mut stmts, fragment, None, Some("container"));
    assert!(result.is_ok());
    if let Stmt::ActorDecl { children, .. } = &stmts[0] {
        assert_eq!(children.len(), 1);
    } else {
        panic!("Expected ActorDecl");
    }
}

#[test]
fn duplicate_actor_inserts_declaration_with_new_label() {
    let mut stmts = parse(
        r#"#0s
box1: Rect, size: (100, 100)
box1.color = red"#,
    );
    let edit = SourceEdit::DuplicateActor {
        original_label: "box1".into(),
        new_label: "box2".into(),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    assert!(
        stmts
            .iter()
            .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "box2")),
        "duplicate should add box2 declaration"
    );
    assert!(
        super::super::apply::find_actor_decl(&stmts, "box1").is_some(),
        "original declaration should remain"
    );
}

#[test]
fn paste_actors_renames_keyframes_and_shifts_times() {
    let mut stmts = parse(
        r#"#0s
box1: Rect, size: (100, 100)

#1s
box1.color = red

#2s
box1.opacity = 0.5"#,
    );
    let edit = SourceEdit::PasteActors {
        clipboard: vec![("box1".to_string(), "box1_copy".to_string())],
        time_s: 1.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    assert!(
        stmts
            .iter()
            .any(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "box1_copy")),
        "paste should add box1_copy declaration"
    );

    let keyframe_times: Vec<f64> = stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Keyframe { time, body, .. }
                if body.iter().any(|b| {
                    matches!(b, Stmt::Assignment { target, .. }
                        if target.iter().any(|t| t.label_str() == "box1_copy"))
                }) =>
            {
                Some(time_to_seconds(time))
            },
            _ => None,
        })
        .collect();
    assert_eq!(keyframe_times, vec![2.0, 3.0], "keyframes should shift by +1s");
}

#[test]
fn paste_actors_with_missing_actor_fails() {
    let mut stmts = parse("#0s\nbox1: Rect, size: (100, 100)");
    let edit = SourceEdit::PasteActors {
        clipboard: vec![("missing".to_string(), "missing_copy".to_string())],
        time_s: 0.0,
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn remove_existing_property() {
    let mut stmts = parse(
        r#"#0s
callout: Callout, target: box1, place: right"#,
    );
    let edit = SourceEdit::RemoveProperty {
        actor: "callout".into(),
        property: "target".into(),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    let actor = find_actor_decl_mut(&mut stmts, "callout").expect("actor should exist");
    assert!(find_prop_mut(actor, "target").is_none(), "'target' should have been removed");
    // Other properties remain
    assert!(find_prop_mut(actor, "place").is_some(), "'place' should still exist");
}

#[test]
fn remove_property_actor_not_found() {
    let mut stmts = parse(
        r#"#0s
box1: Rect, size: (100, 100)"#,
    );
    let edit = SourceEdit::RemoveProperty {
        actor: "missing".into(),
        property: "size".into(),
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn remove_property_not_found_on_actor() {
    let mut stmts = parse(
        r#"#0s
box1: Rect, size: (100, 100)"#,
    );
    let edit = SourceEdit::RemoveProperty {
        actor: "box1".into(),
        property: "color".into(),
    };
    assert!(apply_edit(&mut stmts, edit).is_err());
}

#[test]
fn remove_property_does_not_touch_assignments() {
    // Keyframed assignments should remain untouched
    let mut stmts = parse(
        r#"#0s
callout: Callout, target: box1
#1s
callout.target = box2"#,
    );
    let edit = SourceEdit::RemoveProperty {
        actor: "callout".into(),
        property: "target".into(),
    };
    assert!(apply_edit(&mut stmts, edit).is_ok());

    // Declaration prop removed
    let actor = find_actor_decl_mut(&mut stmts, "callout").expect("actor should exist");
    assert!(find_prop_mut(actor, "target").is_none());

    // Assignment still present
    let assignment = find_assignment_mut(&mut stmts, "callout", "target");
    assert!(assignment.is_some(), "keyframed assignment should remain");
}
