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
//! * `typst_shorthand` — `label: $$ content $$ [modifiers]`
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

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{
    self, ExprParser, InlineItemsParser, ModifiersParser, ParserExtra, PropertyParser, StrInput,
};
use crate::ast::*;

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
                text::keyword("Any").to(TypeAnnotation::Any),
            ));
            let list = text::keyword("List")
                .ignore_then(just('<').padded())
                .ignore_then(type_annotation.clone())
                .then_ignore(just('>').padded())
                .map(|inner| TypeAnnotation::List(Box::new(inner)));
            let atom = simple
                .or(list)
                .or(common::type_ident().map(TypeAnnotation::Alias))
                .or(common::ident()
                    .then(
                        just('.')
                            .padded()
                            .ignore_then(common::ident().filter(|s: &String| {
                                s.chars().next().is_some_and(|c| c.is_lowercase() || c == '_')
                            }))
                            .repeated()
                            .collect::<Vec<_>>(),
                    )
                    .then_ignore(just('.').padded())
                    .then(common::type_ident())
                    .map(|((first, middle), last)| {
                        let mut parts = vec![first];
                        parts.extend(middle);
                        parts.push(last);
                        TypeAnnotation::Alias(parts.join("."))
                    }));
            let union = atom
                .clone()
                .then(
                    just('|')
                        .padded()
                        .ignore_then(atom.clone())
                        .repeated()
                        .collect::<Vec<_>>(),
                )
                .map(|(first, rest)| {
                    if rest.is_empty() {
                        first
                    } else {
                        let mut types = vec![first];
                        types.extend(rest);
                        TypeAnnotation::Union(types)
                    }
                });
            union.or(atom)
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

        let type_alias = text::keyword("pub")
            .padded()
            .or_not()
            .then(
                text::keyword("type")
                    .padded()
                    .ignore_then(ident.clone())
                    .then_ignore(just('=').padded())
                    .then(type_annotation.clone()),
            )
            .map(|(is_pub, (name, annotation))| Stmt::TypeAlias {
                is_pub: is_pub.is_some(),
                name,
                annotation,
                span: None,
            })
            .labelled("type alias")
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

        let indexed_dotted_ident = common::indexed_dotted_ident();
        // For assignments/reactive-bindings we need full expr parsing in brackets.
        let indexed_dotted_ident_with_expr = common::indexed_dotted_ident_with_expr(expr.clone());

        let assignment = indexed_dotted_ident_with_expr
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
                    let property = match &path[0] {
                        TargetSegment::Static(s) => s.clone(),
                        TargetSegment::Indexed { .. } => return Err(Rich::custom(
                            span,
                            "indexed target cannot be a property name",
                        )),
                    };
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
                    let property = match path.last().unwrap() {
                        TargetSegment::Static(s) => s.clone(),
                        TargetSegment::Indexed { .. } => return Err(Rich::custom(
                            span,
                            "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                        )),
                    };
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
        let reactive_binding = indexed_dotted_ident_with_expr
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
                    let property = match path.last().unwrap() {
                        TargetSegment::Static(s) => s.clone(),
                        TargetSegment::Indexed { .. } => return Err(Rich::custom(
                            span,
                            "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                        )),
                    };
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

        let callout_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Callout"))
            .then(block_props.clone())
            .map(|(label, props)| Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: label.unwrap_or_else(|| "unnamed_callout".to_string()),
                array_index: None,
                ty: "Callout".to_string(),
                props,
                modifiers: vec![],
                children: vec![],
                span: None,
            })
            .padded();

        // Shorthand: label: $$ content $$ → label: Typst, content: content
        let typst_shorthand = ident
            .clone()
            .then_ignore(just(':').padded())
            .then_ignore(just("$$"))
            .then(
                just("$$").not()
                    .ignore_then(any())
                    .repeated()
                    .collect::<String>()
                    .map(|s: String| s.trim().to_string()),
            )
            .then_ignore(just("$$"))
            .then(modifiers.clone())
            .map(|((label, content), modifiers)| Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label,
                array_index: None,
                ty: "Typst".to_string(),
                props: vec![Property {
                    name: "content".to_string(),
                    value: Expr::Str(content),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers,
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

        // Action target: dotted path where segments may carry integer array indices.
        // `dots[0].at` is resolved to `dots__0.at` by the indexed parser, matching
        // the `__` scheme used by `resolve_array_index` in timeline/build/process.rs.
        let action_target = indexed_dotted_ident
            .clone()
            .map(|segments| {
                target_segments_static_key(&segments)
            });

        let action = ident
            .clone()
            .then(
                // Comma-separated targets: `pulse btn, icon [200ms]`
                action_target
                    .clone()
                    .then(
                        just(',').padded().ignore_then(action_target.clone()).repeated().at_least(1).collect::<Vec<_>>()
                    )
                    .map(|(first, rest)| {
                        let mut targets = vec![first];
                        targets.extend(rest);
                        targets
                    })
                    .or(
                        // Space-separated targets (backward compat): `swap bar1 bar2 [500ms]`
                        action_target.clone().repeated().at_least(1).collect::<Vec<_>>()
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

        // Match statement: match <expr> { <pat> => { <stmts> }, ..., _ => {} }
        // The match pattern parser reuses the same logic as the expression form.
        // We inline a simplified pattern parser here since `match_pat` references
        // itself recursively and needs to be inside the recursive block.
        let match_pat = recursive(|match_pat| {
            let wildcard = just('_').to(MatchPattern::Wildcard).padded();

            let num_pat = text::int(10)
                .then(just('.').ignore_then(text::digits(10)).or_not())
                .to_slice()
                .from_str()
                .unwrapped()
                .map(MatchPattern::Num)
                .padded();

            let str_pat = super::common::string_literal()
                .map(|e| match e {
                    Expr::Str(s) => MatchPattern::Str(s),
                    _ => unreachable!(),
                })
                .padded();

            let bool_pat = text::keyword("true")
                .to(MatchPattern::Bool(true))
                .or(text::keyword("false").to(MatchPattern::Bool(false)))
                .padded();

            let literal_pat = choice((num_pat, str_pat, bool_pat)).boxed();

            let range_pat = literal_pat
                .clone()
                .then_ignore(just("..=").padded())
                .then(literal_pat.clone())
                .map(|(lo, hi)| MatchPattern::Range(Box::new(lo), Box::new(hi)))
                .boxed();

            let tuple_pat = match_pat
                .clone()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just('(').padded(), just(')').padded())
                .map(MatchPattern::Tuple)
                .boxed();

            let atom = choice((wildcard.clone(), range_pat, tuple_pat, literal_pat)).boxed();

            atom.clone()
                .foldl(
                    just('|').padded().ignore_then(atom.clone()).repeated(),
                    |left, right| match left {
                        MatchPattern::Or(mut items) => {
                            items.push(right);
                            MatchPattern::Or(items)
                        }
                        other => MatchPattern::Or(vec![other, right]),
                    },
                )
                .boxed()
        });

        let match_stmt = text::keyword("match")
            .ignore_then(expr.clone().padded())
            .then(
                match_pat
                    .clone()
                    .then_ignore(just("=>").padded())
                    .then(always_body.clone())
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<(MatchPattern, Vec<Stmt>)>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(scrutinee, arms)| Stmt::Match {
                scrutinee,
                arms,
                span: None,
            })
            .labelled("match statement")
            .as_context()
            .padded();

        // Loop variable pattern: single ident or tuple (a, b, c)
        let loop_var_pat = ident
            .clone()
            .map(LoopPattern::Single)
            .or(ident
                .clone()
                .separated_by(just(',').padded())
                .collect::<Vec<_>>()
                .delimited_by(just('(').padded(), just(')').padded())
                .map(LoopPattern::Tuple));

        // For loop: for i in items { } or for (a, b) in items { }
        let for_stmt = text::keyword("for")
            .ignore_then(loop_var_pat.clone())
            .then(
                // Index variable only valid after single ident, not after tuple pattern
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
            type_alias,
            import_stmt,
            assignment,
            reactive_binding,
            svg_stmt,
            image_stmt,
            callout_stmt,
            always_stmt,
            conditional_stmt,
            match_stmt,
            for_stmt,
            sequence_stmt,
            stagger_stmt,
            component_action_stmt,
            component_def,
            typst_shorthand,
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
