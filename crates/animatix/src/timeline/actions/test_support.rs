//! Shared AST fixtures for the action tests.

use crate::ast::{Expr, Property, Stmt};

pub(crate) fn text_decl(label: &str) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![
            Property {
                name: "text".to_string(),
                value: Expr::Str("Hello".to_string()),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "font_size".to_string(),
                value: Expr::Num(32.0),
                value_span: None,
                trailing_comment: None,
            },
            Property {
                name: "at".to_string(),
                value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                value_span: None,
                trailing_comment: None,
            },
        ],
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}

pub(crate) fn circle_decl(label: &str) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Ellipse".to_string(),
        props: vec![Property {
            name: "size".to_string(),
            value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(40.0)]),
            value_span: None,
            trailing_comment: None,
        }],
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}
