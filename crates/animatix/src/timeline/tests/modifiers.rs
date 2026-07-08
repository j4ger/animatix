use super::*;
use crate::ast::BinaryOp;

#[test]
fn test_apply_modifier_stmt_supports_conditionals_statelessly() {
    let mut timeline = Timeline::new();
    load_standard_library(&mut timeline.env);

    let modifier = Stmt::Conditional {
        condition: Expr::Binary(
            Box::new(Expr::Ident("t".to_string())),
            BinaryOp::Lt,
            Box::new(Expr::Num(1.0)),
        ),
        then_branch: vec![Stmt::Assignment {
            target: vec![crate::ast::TargetSegment::Static("pulse".to_string())],
            property: "opacity".to_string(),
            value: Expr::Num(1.0),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        else_branch: Some(vec![Stmt::Assignment {
            target: vec![crate::ast::TargetSegment::Static("pulse".to_string())],
            property: "opacity".to_string(),
            value: Expr::Num(0.0),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }]),
        span: None,
    };

    let mut first_overrides = std::collections::HashMap::new();
    let mut first_env =
        timeline.build_frame_env_internal(500, SceneDimensions::default(), &first_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut first_env, &mut first_overrides);

    let mut second_overrides = std::collections::HashMap::new();
    let mut second_env =
        timeline.build_frame_env_internal(1500, SceneDimensions::default(), &second_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut second_env, &mut second_overrides);

    let mut repeat_overrides = std::collections::HashMap::new();
    let mut repeat_env =
        timeline.build_frame_env_internal(500, SceneDimensions::default(), &repeat_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut repeat_env, &mut repeat_overrides);

    assert_eq!(first_overrides["pulse"]["opacity"], Value::Num(1.0));
    assert_eq!(second_overrides["pulse"]["opacity"], Value::Num(0.0));
    assert_eq!(first_overrides, repeat_overrides);
}
