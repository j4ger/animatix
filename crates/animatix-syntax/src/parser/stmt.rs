//!
//! Statement parsers for the Animatix DSL.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{
    self, ExprParser, InlineItemsParser, ModifiersParser, ParserExtra, PropertyParser, StrInput,
};
use super::token_parser::*;
use crate::ast::*;

/// Build the recursive statement parser.
pub(crate) fn parser<'src>(
    expr: ExprParser<'src>,
    property: PropertyParser<'src>,
    modifiers: ModifiersParser<'src>,
    inline_items: InlineItemsParser<'src>,
) -> Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>> {
    fn type_keyword<'src>(
        name: &'static str,
    ) -> impl Parser<'src, StrInput<'src>, (), ParserExtra<'src>> + Clone {
        common::ident().filter(move |s: &String| s == name).to(())
    }

    recursive(|_stmt| {
        let ident = common::ident();
        let label_expr = common::label_expr(expr.clone());

        let block_props = property
            .clone()
            .separated_by(comma())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(lbrace(), rbrace());

        let always_body = _stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace());

        let for_loop_body = _stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace());

        let type_annotation = recursive(|type_annotation| {
            let simple = choice((
                type_keyword("Num").to(TypeAnnotation::Num),
                type_keyword("Str").to(TypeAnnotation::Str),
                type_keyword("Bool").to(TypeAnnotation::Bool),
                type_keyword("Vec2").to(TypeAnnotation::Vec2),
                type_keyword("Vec3").to(TypeAnnotation::Vec3),
                type_keyword("Vec4").to(TypeAnnotation::Vec4),
                type_keyword("Color").to(TypeAnnotation::Color),
                type_keyword("Actor").to(TypeAnnotation::Actor),
                type_keyword("Scene").to(TypeAnnotation::Scene),
                type_keyword("Any").to(TypeAnnotation::Any),
            ));
            let list = type_keyword("List")
                .ignore_then(lt())
                .ignore_then(type_annotation.clone())
                .then_ignore(gt())
                .map(|inner| TypeAnnotation::List(Box::new(inner)));
            let alias_middle = common::ident().filter(|s: &String| {
                s.chars().next().is_some_and(|c| c.is_lowercase() || c == '_')
            });
            let canonical_alias = common::ident()
                .then(
                    colon_colon()
                        .ignore_then(alias_middle.clone())
                        .repeated()
                        .collect::<Vec<_>>(),
                )
                .then_ignore(colon_colon())
                .then(common::type_ident())
                .map(|((first, middle), last)| {
                    let mut parts = vec![first];
                    parts.extend(middle);
                    parts.push(last);
                    TypeAnnotation::Alias(parts.join("::"))
                });
            let legacy_alias = common::ident()
                .then(dot().ignore_then(alias_middle).repeated().collect::<Vec<_>>())
                .then_ignore(dot())
                .then(common::type_ident())
                .map(|((first, middle), last)| {
                    let mut parts = vec![first];
                    parts.extend(middle);
                    parts.push(last);
                    TypeAnnotation::Alias(parts.join("::"))
                });
            let tuple = type_keyword("Tuple")
                .ignore_then(lt())
                .ignore_then(
                    type_annotation
                        .clone()
                        .separated_by(comma())
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .or_not()
                        .map(|items| items.unwrap_or_default()),
                )
                .then_ignore(gt())
                .map(TypeAnnotation::Tuple)
                .boxed();
            let function = type_keyword("Fn")
                .ignore_then(
                    type_annotation
                        .clone()
                        .separated_by(comma())
                        .allow_trailing()
                        .collect::<Vec<_>>()
                        .or_not()
                        .map(|items| items.unwrap_or_default())
                        .delimited_by(lparen(), rparen()),
                )
                .then_ignore(arrow())
                .then(type_annotation.clone())
                .map(|(params, ret)| TypeAnnotation::Function {
                    params,
                    ret: Box::new(ret),
                })
                .boxed();
            let atom = simple
                .or(list)
                .or(tuple)
                .or(function)
                .or(common::type_ident().map(TypeAnnotation::Alias))
                .or(canonical_alias)
                .or(legacy_alias);
            let union = atom
                .clone()
                .then(pipe().ignore_then(atom.clone()).repeated().collect::<Vec<_>>())
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
            .then_ignore(colon())
            .then(
                type_annotation
                    .clone()
                    .then(assign().ignore_then(expr.clone()).or_not())
                    .map(|(ty, default)| (Some(ty), default))
                    .or(expr.clone().map(|e| (None, Some(e)))),
            )
            .map(|(name, (param_type, default))| ParamDef {
                name,
                param_type,
                default,
            });

        let let_decl = keyword("pub")
            .or_not()
            .then(keyword("let").ignore_then(ident.clone()).then_ignore(assign()).then(expr.clone()))
            .map(|(pub_kw, (name, value))| Stmt::LetDecl {
                is_pub: pub_kw.is_some(),
                name,
                value,
                span: None,
            })
            .labelled("let declaration")
            .as_context();

        let type_alias = keyword("pub")
            .or_not()
            .then(keyword("type").ignore_then(ident.clone()).then_ignore(assign()).then(type_annotation.clone()))
            .map(|(is_pub, (name, annotation))| Stmt::TypeAlias {
                is_pub: is_pub.is_some(),
                name,
                annotation,
                span: None,
            })
            .labelled("type alias")
            .as_context();

        let import_stmt = keyword("import")
            .ignore_then(string())
            .then(keyword("as").ignore_then(ident.clone()).or_not())
            .map(|(path, alias)| Stmt::Import { path, alias, span: None })
            .labelled("import")
            .as_context();

        let indexed_dotted_ident = common::indexed_dotted_ident();
        let indexed_dotted_ident_with_expr = common::indexed_dotted_ident_with_expr(expr.clone());

        let assignment = indexed_dotted_ident_with_expr
            .clone()
            .then_ignore(assign())
            .then(expr.clone().map_with(
                |value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                    (value, extra.span())
                },
            ))
            .then(modifiers.clone())
            .try_map(|((path, (value, value_span)), mut modifiers), span| {
                if path.is_empty() {
                    Err(Rich::custom(
                        span,
                        "assignment target must be a property name or actor.property path",
                    ))
                } else if path.len() == 1 {
                    let property = match &path[0] {
                        TargetSegment::Static(s) => s.clone(),
                        TargetSegment::Indexed { .. } => {
                            return Err(Rich::custom(span, "indexed target cannot be a property name"));
                        },
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
                    let property = match path.last() {
                        Some(TargetSegment::Static(s)) => s.clone(),
                        Some(TargetSegment::Indexed { .. }) => {
                            return Err(Rich::custom(
                                span,
                                "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                            ));
                        },
                        None => {
                            return Err(Rich::custom(
                                span,
                                "assignment target must include a property name",
                            ));
                        },
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
            .as_context();

        let reactive_binding = indexed_dotted_ident_with_expr
            .clone()
            .then_ignore(reactive_assign())
            .then(expr.clone().map_with(
                |value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                    (value, extra.span())
                },
            ))
            .try_map(|(path, (value, value_span)), span| {
                if path.len() < 2 {
                    Err(Rich::custom(
                        span,
                        "reactive binding target must include an actor label and a property (e.g. 'actor.prop := expr')",
                    ))
                } else {
                    let property = match path.last() {
                        Some(TargetSegment::Static(s)) => s.clone(),
                        Some(TargetSegment::Indexed { .. }) => {
                            return Err(Rich::custom(
                                span,
                                "array index cannot appear on the property segment (e.g. use bars[i].color, not a.b[i])",
                            ));
                        },
                        None => {
                            return Err(Rich::custom(
                                span,
                                "reactive binding target must include a property name",
                            ));
                        },
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
            .labelled("reactive binding");

        let svg_stmt = ident
            .clone()
            .then_ignore(colon())
            .or_not()
            .then_ignore(type_keyword("Svg"))
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
            });

        let image_stmt = ident
            .clone()
            .then_ignore(colon())
            .or_not()
            .then_ignore(type_keyword("Image"))
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
            });

        let callout_stmt = ident
            .clone()
            .then_ignore(colon())
            .or_not()
            .then_ignore(type_keyword("Callout"))
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
            });

        let typst_shorthand = ident
            .clone()
            .then_ignore(colon())
            .then(typst())
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
            });

        let text_shorthand = ident
            .clone()
            .then_ignore(colon())
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
            });

        let actor_decl = keyword("pub")
            .or_not()
            .map(|p| p.is_some())
            .then(label_expr.clone())
            .then_ignore(colon())
            .then(ident.clone())
            .then(
                comma()
                    .ignore_then(property.clone().separated_by(comma()).collect::<Vec<_>>())
                    .or_not()
                    .map(|p: Option<Vec<Property>>| p.unwrap_or_default()),
            )
            .then(modifiers.clone())
            .then(
                inline_items
                    .clone()
                    .delimited_by(lbrace(), rbrace())
                    .or_not()
                    .map(|c| c.unwrap_or_default()),
            )
            .map(
                |(((((is_pub, (label, array_index)), ty), props), modifiers), children)| {
                    Stmt::ActorDecl {
                        is_pub,
                        is_anonymous: false,
                        label,
                        array_index,
                        ty,
                        props,
                        modifiers,
                        children,
                        span: None,
                    }
                },
            )
            .labelled("actor declaration")
            .as_context();

        let action_target = indexed_dotted_ident.clone().map(|segments| target_segments_static_key(&segments));

        let action = ident
            .clone()
            .then(
                action_target
                    .clone()
                    .then(comma().ignore_then(action_target.clone()).repeated().at_least(1).collect::<Vec<_>>())
                    .map(|(first, rest)| {
                        let mut targets = vec![first];
                        targets.extend(rest);
                        targets
                    })
                    .or(action_target.clone().repeated().at_least(1).collect::<Vec<_>>())
                    .or_not()
                    .map(|opt| opt.unwrap_or_default()),
            )
            .then(expr.clone().repeated().collect::<Vec<_>>())
            .then(modifiers.clone())
            .map_with(
                |(((verb, targets), args), modifiers),
                 extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                    Stmt::Action(
                        Action {
                            verb,
                            targets,
                            args,
                            modifiers,
                            byte_span: Some(extra.span()),
                        },
                        None,
                    )
                },
            );

        let sequence_stmt = keyword("sequence")
            .ignore_then(_stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()))
            .map(|body| Stmt::Sequence { body, span: None });

        let stagger_stmt = keyword("stagger")
            .ignore_then(modifiers.clone())
            .then(_stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()))
            .map(|(modifiers, body)| Stmt::Stagger { modifiers, body, span: None });

        let always_stmt = keyword("always")
            .ignore_then(always_body.clone())
            .map(|body| Stmt::Always { body, span: None })
            .labelled("always block")
            .as_context();

        let conditional_stmt = keyword("if")
            .ignore_then(expr.clone())
            .then(always_body.clone())
            .then(keyword("else").ignore_then(always_body.clone()).or_not())
            .map(|((condition, then_branch), else_branch)| Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                span: None,
            })
            .labelled("if statement")
            .as_context();

        let match_pat = recursive(|match_pat| {
            let wildcard = underscore().to(MatchPattern::Wildcard);
            let num_pat = number().map(MatchPattern::Num);
            let str_pat = super::common::string_literal().map(|e| match e {
                Expr::Str(s) => MatchPattern::Str(s),
                _ => unreachable!(),
            });
            let bool_pat = bool_lit().map(MatchPattern::Bool);
            let literal_pat = choice((num_pat, str_pat, bool_pat)).boxed();
            let range_pat = literal_pat
                .clone()
                .then_ignore(range_inclusive())
                .then(literal_pat.clone())
                .map(|(lo, hi)| MatchPattern::Range(Box::new(lo), Box::new(hi)))
                .boxed();
            let tuple_pat = match_pat
                .clone()
                .separated_by(comma())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen())
                .map(MatchPattern::Tuple)
                .boxed();
            let atom = choice((wildcard.clone(), range_pat, tuple_pat, literal_pat)).boxed();
            atom.clone()
                .foldl(pipe().ignore_then(atom.clone()).repeated(), |left, right| match left {
                    MatchPattern::Or(mut items) => {
                        items.push(right);
                        MatchPattern::Or(items)
                    },
                    other => MatchPattern::Or(vec![other, right]),
                })
                .boxed()
        });

        let match_stmt = keyword("match")
            .ignore_then(expr.clone())
            .then({
                let match_arm = match_pat.clone().then_ignore(arrow()).then(always_body.clone());
                match_arm
                    .clone()
                    .then(comma().or_not())
                    .repeated()
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .map(|items| {
                        items
                            .into_iter()
                            .map(|(arm, _comma)| arm)
                            .collect::<Vec<(MatchPattern, Vec<Stmt>)>>()
                    })
                    .delimited_by(lbrace(), rbrace())
            })
            .map(|(scrutinee, arms)| Stmt::Match {
                scrutinee,
                arms,
                span: None,
            })
            .labelled("match statement")
            .as_context();

        let loop_var_pat = ident
            .clone()
            .map(LoopPattern::Single)
            .or(ident
                .clone()
                .separated_by(comma())
                .collect::<Vec<_>>()
                .delimited_by(lparen(), rparen())
                .map(LoopPattern::Tuple));

        let for_stmt = keyword("for")
            .ignore_then(loop_var_pat.clone())
            .then(comma().ignore_then(ident.clone()).or_not())
            .then_ignore(keyword("in"))
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
            .as_context();

        let component_action_stmt = keyword("action")
            .ignore_then(ident.clone())
            .then(
                param_def
                    .clone()
                    .separated_by(comma())
                    .collect::<Vec<_>>()
                    .delimited_by(lparen(), rparen())
                    .or_not()
                    .map(|p| p.unwrap_or_default()),
            )
            .then(_stmt.clone().repeated().collect::<Vec<_>>().delimited_by(lbrace(), rbrace()))
            .map(|((name, params), body)| Stmt::ComponentAction {
                name,
                params,
                body,
                span: None,
            });

        let component_def = keyword("pub")
            .or_not()
            .map(|p| p.is_some())
            .then_ignore(keyword("component"))
            .then(ident.clone())
            .then(
                param_def
                    .separated_by(comma())
                    .collect::<Vec<_>>()
                    .delimited_by(lparen(), rparen())
                    .or_not()
                    .map(|p| p.unwrap_or_default()),
            )
            .then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(lbrace(), rbrace())
                    .try_map(|body: Vec<Stmt>, span| {
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
                Stmt::ComponentDef(ComponentDef { is_pub, name, params, body }, None)
            })
            .labelled("component definition")
            .as_context();

        let block_comment_reject = slash().ignore_then(star()).try_map(|_, span| {
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
        ))
        .labelled("statement")
        .boxed()
    })
    .boxed()
}
