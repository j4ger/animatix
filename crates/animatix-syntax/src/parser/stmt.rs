//!
//! Statement parsers for the Animatix DSL.
//!
//! This module provides the [`parser()`] function which builds the recursive
//! statement parser used by [`super::parser()`]. The function takes shared
//! sub-parsers (`expr`, `property`, `modifiers`, `inline_items`) as arguments
//! and returns a boxed parser for a single [`Stmt`].
//!
//! The statement parsers extracted here are:
//!
//! * `let_decl` — `let name = expr` / `pub let name = expr`
//! * `import_stmt` — `import "path" as alias`
//! * `assignment` — `target.prop = expr [modifiers]`
//! * `reactive_binding` — `actor.prop := expr`
//! * `svg_stmt` — `label: Svg { ... }`
//! * `image_stmt` — `label: Image { ... }`
//! * `text_shorthand` — `label: "string" [modifiers]`
//! * `actor_decl` — `label: Type, props [modifiers] { children }`
//! * `action` — `verb target [args] [modifiers]`
//! * `always_stmt` — `always { ... }`
//! * `conditional_stmt` — `if expr { ... } else { ... }`
//! * `for_stmt` — `for var in expr { ... }`
//! * `sequence_stmt` — `sequence { ... }`
//! * `stagger_stmt` — `stagger [modifiers] { ... }`
//! * `component_action_stmt` — `action name(params) { ... }`
//! * `component_def` — `pub component name(params) { ... }`
//! * `comment` — `// ...`
//! * `block_comment_reject` — rejects `/* */` with a clear diagnostic

use crate::ast::*;
use chumsky::input::MapExtra;
use chumsky::prelude::*;
use super::common::{self, ExprParser, InlineItemsParser, ModifiersParser, ParserExtra, PropertyParser, StrInput};

/// Build the recursive statement parser.
///
/// All shared sub-parsers are passed as arguments so that this module has no
/// dependency on the outer [`super::parser()`] function's local variables.
pub(crate) fn parser<'src>(
    expr: ExprParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
    inline_items: InlineItemsParser<'src>,
) -> Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>> {
    recursive(|_stmt| {
        let ident = common::ident();
        let dotted_ident = common::dotted_ident();
        let label_expr = common::label_expr(expr.clone());

        // Property block: { prop1, prop2, ... }
        let block_props = property
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        // Always body: { stmt1; stmt2; ... }
        let always_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        // For loop body
        let for_loop_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        // Type annotation: Num | Str | Bool | Vec2 | Vec4 | Color | Actor | Scene | List<Type>
        let type_annotation = recursive(|type_annotation| {
            let simple = choice((
                text::keyword("Num").to(TypeAnnotation::Num),
                text::keyword("Str").to(TypeAnnotation::Str),
                text::keyword("Bool").to(TypeAnnotation::Bool),
                text::keyword("Vec2").to(TypeAnnotation::Vec2),
                text::keyword("Vec4").to(TypeAnnotation::Vec4),
                text::keyword("Color").to(TypeAnnotation::Color),
                text::keyword("Actor").to(TypeAnnotation::Actor),
                text::keyword("Scene").to(TypeAnnotation::Scene),
            ));
            let list = text::keyword("List")
                .ignore_then(just('<').padded())
                .ignore_then(type_annotation)
                .then_ignore(just('>').padded())
                .map(|inner| TypeAnnotation::List(Box::new(inner)));
            simple.or(list)
        });

        let param_def = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(
                // Try type annotation + optional default first
                type_annotation
                    .clone()
                    .then(just('=').padded().ignore_then(expr.clone()).or_not())
                    .map(|(ty, default)| (Some(ty), default))
                    // Fall back to expression as default (backward compat)
                    .or(expr.clone().map(|e| (None, Some(e)))),
            )
            .map(|(name, (param_type, default))| ParamDef {
                name,
                param_type,
                default,
            });

        let let_decl = text::keyword("pub")
            .padded()
            .or_not()
            .then(text::keyword("let")
                .padded()
                .ignore_then(ident.clone())
                .then_ignore(just('=').padded())
                .then(expr.clone()))
            .map(|(pub_kw, (name, value))| Stmt::LetDecl {
                is_pub: pub_kw.is_some(),
                name,
                value,
                span: None,
            })
            .labelled("let declaration")
            .as_context()
            .padded();

        let import_stmt = text::keyword("import")
            .padded()
            .ignore_then(
                just('"')
                    .ignore_then(none_of('"').repeated().collect::<String>())
                    .then_ignore(just('"')),
            )
            .then(
                text::keyword("as")
                    .padded()
                    .ignore_then(ident.clone())
                    .or_not(),
            )
            .map(|(path, alias)| Stmt::Import { path, alias, span: None })
            .labelled("import")
            .as_context()
            .padded();

        let assignment = dotted_ident
            .clone()
            .then_ignore(just('=').padded())
            .then(expr.clone().map_with(|value, extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                (value, ByteSpan { start: span.start, end: span.end })
            }))
            .then(modifiers.clone())
            .try_map(|((path, (value, value_span)), mut modifiers), span| {
                if path.is_empty() {
                    Err(Rich::custom(
                        span,
                        "assignment target must be a property name or actor.property path",
                    ))
                } else if path.len() == 1 {
                    // Single-segment assignment: e.g. `at = expr` inside an always block
                    let property = path[0].clone();
                    let easing = common::extract_easing(&mut modifiers);
                    Ok(Stmt::Assignment {
                        target: vec![],
                        property,
                        value,
                        modifiers,
                        easing,
                        value_span: Some(value_span),
                        span: None,
                    })
                } else {
                    let property = path.last().cloned().unwrap_or_default();
                    let target = path[..path.len() - 1].to_vec();
                    let easing = common::extract_easing(&mut modifiers);
                    Ok(Stmt::Assignment {
                        target,
                        property,
                        value,
                        modifiers,
                        easing,
                        value_span: Some(value_span),
                        span: None,
                    })
                }
            })
            .labelled("assignment")
            .as_context()
            .padded();

        // Reactive binding: actor.prop := expr
        let reactive_binding = dotted_ident
            .clone()
            .then_ignore(just(":=").padded())
            .then(expr.clone().map_with(|value, extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                (value, ByteSpan { start: span.start, end: span.end })
            }))
            .try_map(|(path, (value, value_span)), span| {
                if path.len() < 2 {
                    Err(Rich::custom(
                        span,
                        "reactive binding target must include an actor label and a property (e.g. 'actor.prop := expr')",
                    ))
                } else {
                    let property = path.last().cloned().unwrap_or_default();
                    let target = path[..path.len() - 1].to_vec();
                    Ok(Stmt::ReactiveBinding {
                        target,
                        property,
                        value,
                        value_span: Some(value_span),
                        span: None,
                    })
                }
            })
            .labelled("reactive binding")
            .padded();

        let svg_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Svg"))
            .then(block_props.clone())
            .map(|(label, props)| Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: label.unwrap_or_else(|| "unnamed_svg".to_string()),
                array_index: None,
                ty: "Svg".to_string(),
                props,
                modifiers: vec![],
                children: vec![],
                span: None,
            })
            .padded();

        let image_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Image"))
            .then(block_props.clone())
            .map(|(label, props)| Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: label.unwrap_or_else(|| "unnamed_image".to_string()),
                array_index: None,
                ty: "Image".to_string(),
                props,
                modifiers: vec![],
                children: vec![],
                span: None,
            })
            .padded();

        // Shorthand: label: "string" → label: Text, text: "string"
        let text_shorthand = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(common::string_literal())
            .then(modifiers.clone())
            .map(|((label, text), modifiers)| Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label,
                array_index: None,
                ty: "Text".to_string(),
                props: vec![Property {
                    name: "text".to_string(),
                    value: text,
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers,
                children: vec![],
                span: None,
            })
            .padded();

        let actor_decl = text::keyword("pub")
            .padded()
            .or_not()
            .map(|p| p.is_some())
            .then(label_expr.clone())
            .then_ignore(just(':').padded())
            .then(ident.clone())
            .then(
                just(',')
                    .padded()
                    .ignore_then(
                        property
                            .clone()
                            .separated_by(just(',').padded())
                            .collect::<Vec<_>>(),
                    )
                    .or_not()
                    .map(|p: Option<Vec<Property>>| p.unwrap_or_default()),
            )
            .then(modifiers.clone())
            .then(
                inline_items
                    .clone()
                    .delimited_by(just('{').padded(), just('}').padded())
                    .or_not()
                    .map(|c| c.unwrap_or_default()),
            )
            .map(
                |(((((is_pub, (label, array_index)), ty), props), modifiers), children)| Stmt::ActorDecl {
                    is_pub,
                    is_anonymous: false,
                    label,
                    array_index,
                    ty,
                    props,
                    modifiers,
                    children,
                    span: None,
                },
            )
            .labelled("actor declaration")
            .as_context()
            .padded();

        let action = ident
            .clone()
            .then(
                // Comma-separated targets: `pulse btn, icon [200ms]`
                ident
                    .clone()
                    .then(
                        just(',').padded().ignore_then(ident.clone()).repeated().at_least(1).collect::<Vec<_>>()
                    )
                    .map(|(first, rest)| {
                        let mut targets = vec![first];
                        targets.extend(rest);
                        targets
                    })
                    .or(
                        // Space-separated targets (backward compat): `swap bar1 bar2 [500ms]`
                        ident.clone().repeated().at_least(1).collect::<Vec<_>>()
                    )
                    .or_not()
                    .map(|opt| opt.unwrap_or_default())
            ) // targets
            .then(expr.clone().repeated().collect::<Vec<_>>()) // args
            .then(modifiers.clone())
            .map_with(|(((verb, targets), args), modifiers), extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                Stmt::Action(Action {
                    verb,
                    targets,
                    args,
                    modifiers,
                    byte_span: Some(ByteSpan { start: span.start, end: span.end }),
                }, None)
            })
            .padded();

        let sequence_stmt = text::keyword("sequence")
            .ignore_then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|body| Stmt::Sequence { body, span: None })
            .padded();

        let stagger_stmt = text::keyword("stagger")
            .ignore_then(modifiers.clone())
            .then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(modifiers, body)| Stmt::Stagger { modifiers, body, span: None })
            .padded();

        let comment = just("//")
            .ignore_then(none_of("\r\n").repeated().to_slice().map(String::from))
            .map(|text| Stmt::Comment(text, None))
            .padded();

        // Always statement: always { }
        let always_stmt = text::keyword("always")
            .ignore_then(always_body.clone())
            .map(|body| Stmt::Always { body, span: None })
            .labelled("always block")
            .as_context()
            .padded();

        // Conditional: if expr { }
        let conditional_stmt = text::keyword("if")
            .ignore_then(expr.clone())
            .then(always_body.clone())
            .then(
                text::keyword("else")
                    .ignore_then(always_body.clone())
                    .or_not(),
            )
            .map(
                |((condition, then_branch), else_branch)| Stmt::Conditional {
                    condition,
                    then_branch,
                    else_branch,
                    span: None,
                },
            )
            .labelled("if statement")
            .as_context()
            .padded();

        // For loop: for i in items { }
        let for_stmt = text::keyword("for")
            .ignore_then(ident.clone())
            .then(
                just(',')
                    .padded()
                    .ignore_then(ident.clone())
                    .or_not(),
            )
            .then_ignore(text::keyword("in").padded())
            .then(expr.clone())
            .then(for_loop_body)
            .map(|(((var, index_var), iterable), body)| Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                span: None,
            })
            .labelled("for loop")
            .as_context()
            .padded();

        let component_action_stmt = text::keyword("action")
            .ignore_then(ident.clone())
            .then(
                param_def
                    .clone()
                    .separated_by(just(',').padded())
                    .collect::<Vec<_>>()
                    .delimited_by(just('(').padded(), just(')').padded())
                    .or_not()
                    .map(|p| p.unwrap_or_default()),
            )
            .then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|((name, params), body)| Stmt::ComponentAction {
                name,
                params,
                body,
                span: None,
            })
            .padded();

        let component_def = text::keyword("pub")
            .padded()
            .or_not()
            .map(|p| p.is_some())
            .then_ignore(text::keyword("component").padded())
            .then(ident.clone())
            .then(
                param_def
                    .separated_by(just(',').padded())
                    .collect::<Vec<_>>()
                    .delimited_by(just('(').padded(), just(')').padded())
                    .or_not()
                    .map(|p| p.unwrap_or_default()),
            )
            .then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded())
                    .try_map(|body: Vec<Stmt>, span| {
                        // Reject imports inside component bodies — components are
                        // actor templates, not module-level containers.
                        for stmt in &body {
                            if matches!(stmt, Stmt::Import { .. }) {
                                return Err(Rich::custom(
                                    span,
                                    "import statements are not allowed inside component bodies",
                                ));
                            }
                        }
                        Ok(body)
                    }),
            )
            .map(|(((is_pub, name), params), body)| {
                Stmt::ComponentDef(ComponentDef {
                    is_pub,
                    name,
                    params,
                    body,
                }, None)
            })
            .labelled("component definition")
            .as_context()
            .padded();

        // Reject block comments with a clear diagnostic.
        let block_comment_reject = just("/*")
            .try_map(|_, span| {
                Err(Rich::custom(
                    span,
                    "block comments (/* */) are not supported; use // line comments instead",
                ))
            });

        choice((
            block_comment_reject,
            let_decl,
            import_stmt,
            assignment,
            reactive_binding,
            svg_stmt,
            image_stmt,
            always_stmt,
            conditional_stmt,
            for_stmt,
            sequence_stmt,
            stagger_stmt,
            component_action_stmt,
            component_def,
            text_shorthand,
            actor_decl,
            action,
            comment,
        ))
        .labelled("statement")
        .boxed()
    })
    .boxed()
}
