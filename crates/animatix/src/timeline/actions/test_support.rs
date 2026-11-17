//! Shared AST fixtures for the action tests.

use crate::ast::{Expr, Property, Stmt};

pub(crate) fn text_decl(label: &str) -> Stmt {
    text_decl_with(label, "Hello")
}

/// A `Text` declaration with caller-chosen sample content (same geometry).
pub(crate) fn text_decl_with(label: &str, text: &str) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![
            Property {
                name: "text".to_string(),
                value: Expr::Str(text.to_string()),
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

/// Build the ordinary-call AST used by integration fixtures for backend tests.
pub(crate) fn invocation_stmt(
    call: super::BuiltinInvocation,
    span: Option<crate::ast::Span>,
) -> Stmt {
    use crate::ast::CallArgument;
    let targets = call
        .targets
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let expr = Expr::Ident(label.clone());
            match call.target_index.get(i).and_then(Option::as_ref) {
                Some(index) => Expr::Index(Box::new(expr), Box::new(index.clone())),
                None => expr,
            }
        })
        .collect::<Vec<_>>();
    let mut args = Vec::new();
    if call.verb == "swap" {
        for (name, value) in ["a", "b"].into_iter().zip(targets) {
            args.push(CallArgument::named(name, value));
        }
    } else if targets.len() == 1 {
        args.push(CallArgument::named("target", targets[0].clone()));
    } else {
        args.push(CallArgument::named("targets", Expr::List(targets)));
    }
    let descriptor = animatix_std::callable::callable(&call.verb).expect("standard callable");
    let positional_names = descriptor
        .params
        .iter()
        .filter(|p| !matches!(p.name.as_str(), "target" | "targets" | "a" | "b"));
    for (param, value) in positional_names.zip(call.args) {
        args.push(CallArgument::named(&param.name, value));
    }
    for modifier in call.modifiers {
        let name = modifier.name.unwrap_or_else(|| "duration".into());
        let value = match modifier.value {
            Expr::Ident(ref raw)
                if matches!(name.as_str(), "duration" | "delay" | "anticipate" | "stagger") =>
            {
                crate::timeline::timing::parse_duration_literal(raw)
                    .map(|ms| Expr::Duration(ms / 1000.0))
                    .unwrap_or(modifier.value)
            },
            value => value,
        };
        args.push(CallArgument::named(name, value));
    }
    Stmt::Expr(Expr::Call(Box::new(Expr::Ident(call.verb)), args), span)
}
