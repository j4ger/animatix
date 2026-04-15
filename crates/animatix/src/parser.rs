use crate::ast::*;
use chumsky::prelude::*;

pub fn parser<'src>() -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let ident = text::ident()
        .then(just('-').then(text::ident()).repeated())
        .to_slice()
        .try_map(|ident: &str, span| {
            let reserved = [
                "let",
                "import",
                "always",
                "if",
                "else",
                "for",
                "in",
                "pub",
                "component",
                "true",
                "false",
                "null",
                "loop",
                "yield",
                "stop",
                "pause",
                "resume",
                "action",
            ];
            if reserved.contains(&ident) {
                Err(Rich::custom(
                    span,
                    format!("'{}' is a reserved keyword", ident),
                ))
            } else {
                Ok(String::from(ident))
            }
        })
        .padded();

    let dotted_ident = ident
        .clone()
        .separated_by(just('.').padded())
        .at_least(1)
        .collect::<Vec<_>>()
        .boxed();

    let num = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .map(Expr::Num)
        .padded();

    let percent = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .then_ignore(just('%'))
        .map(Expr::Percent)
        .padded();

    let str_val = just('"')
        .ignore_then(none_of('"').repeated().collect::<String>())
        .then_ignore(just('"'))
        .map(Expr::Str)
        .padded();

    let bool_val = text::keyword("true")
        .to(Expr::Bool(true))
        .or(text::keyword("false").to(Expr::Bool(false)))
        .padded();

    let null_val = text::keyword("null").to(Expr::Null).padded();

    let time = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str::<f64>()
        .unwrapped()
        .then(choice((just("ms").to(true), just("s").to(false))))
        .map(|(v, is_ms)| {
            if is_ms {
                Time::Milliseconds(v as u64)
            } else {
                Time::Seconds(v)
            }
        })
        .padded();

    let expr = recursive(|expr| {
        let tuple = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('(').padded(), just(')').padded())
            .map(|items| {
                if items.len() == 1 {
                    items.into_iter().next().unwrap()
                } else {
                    Expr::Tuple(items)
                }
            })
            .boxed();

        let array = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded())
            .map(Expr::Tuple) // Using Tuple for arrays as well per AST
            .boxed();

        let call = ident
            .clone()
            .then(
                expr.clone()
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just('(').padded(), just(')').padded()),
            )
            .map(|(name, args)| Expr::Call(name, args))
            .boxed();

        // Prefix operators for unary negation and logical NOT
        let prefix_op = just('-').to(UnaryOp::Neg).or(just('!').to(UnaryOp::Not));

        let base_atom = choice((
            percent,
            num,
            str_val,
            bool_val,
            null_val,
            call,
            tuple,
            array,
            ident.clone().map(Expr::Ident),
        ));

        // Prefix expressions: fold multiple prefix ops around an atom
        let atom = prefix_op
            .padded()
            .repeated()
            .collect::<Vec<_>>()
            .then(base_atom)
            .map(|(ops, expr)| {
                ops.into_iter()
                    .fold(expr, |acc, op| Expr::Unary(op, Box::new(acc)))
            })
            .padded();

        let access = atom
            .clone()
            .then(
                just('.')
                    .padded()
                    .ignore_then(ident.clone())
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .map(|(base, segments)| {
                if segments.is_empty() {
                    base
                } else {
                    match base {
                        Expr::Ident(name) => {
                            let mut parts = Vec::with_capacity(segments.len() + 1);
                            parts.push(name);
                            parts.extend(segments);
                            Expr::Path(parts)
                        }
                        Expr::Path(mut parts) => {
                            parts.extend(segments);
                            Expr::Path(parts)
                        }
                        other => other,
                    }
                }
            });

        // Mathematical and logical operators precedence
        let pow = recursive(|pow| {
            access
                .clone()
                .then(just('^').padded().to(BinaryOp::Pow).then(pow).or_not())
                .map(|(lhs, rhs)| {
                    if let Some((op, rhs)) = rhs {
                        Expr::Binary(Box::new(lhs), op, Box::new(rhs))
                    } else {
                        lhs
                    }
                })
        });

        let product = pow.clone().foldl(
            choice((
                just('*').to(BinaryOp::Mul),
                just('/').to(BinaryOp::Div),
                just('%').to(BinaryOp::Mod),
            ))
            .padded()
            .then(pow.clone())
            .repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let sum = product.clone().foldl(
            choice((just('+').to(BinaryOp::Add), just('-').to(BinaryOp::Sub)))
                .padded()
                .then(product.clone())
                .repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let compare_op = choice((
            just(">=").to(BinaryOp::Gte),
            just("<=").to(BinaryOp::Lte),
            just("==").to(BinaryOp::Eq),
            just("!=").to(BinaryOp::Neq),
            just('>').to(BinaryOp::Gt),
            just('<').to(BinaryOp::Lt),
        ));

        let comparison = sum.clone().foldl(
            compare_op.padded().then(sum.clone()).repeated(),
            |lhs, (op, rhs)| Expr::Binary(Box::new(lhs), op, Box::new(rhs)),
        );

        let conditional_expr = text::keyword("if")
            .ignore_then(expr.clone())
            .then(
                expr.clone()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .then(
                text::keyword("else").ignore_then(
                    expr.clone()
                        .delimited_by(just('{').padded(), just('}').padded()),
                ),
            )
            .map(|((condition, then_branch), else_branch)| {
                Expr::Conditional(
                    Box::new(condition),
                    Box::new(then_branch),
                    Box::new(else_branch),
                )
            })
            .boxed();

        let closure = choice((
            ident
                .clone()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just('(').padded(), just(')').padded()),
            ident.clone().map(|i| vec![i]),
        ))
        .then_ignore(just("=>").padded())
        .then(expr.clone())
        .map(|(args, body)| Expr::Closure(args, Box::new(body)))
        .boxed();

        choice((closure, conditional_expr, comparison)).boxed()
    });

    let property = ident
        .clone()
        .then_ignore(just(':').padded())
        .then(expr.clone())
        .map(|(name, value)| Property { name, value });

    let modifier = choice((
        // named modifier: ease: bounce
        ident
            .clone()
            .then_ignore(just(':').padded())
            .then(choice((
                time.clone().map(|t| match t {
                    Time::Seconds(s) => Expr::Ident(format!("{}s", s)),
                    Time::Milliseconds(ms) => Expr::Ident(format!("{}ms", ms)),
                }),
                expr.clone(),
            )))
            .map(|(name, value)| Modifier {
                name: Some(name),
                value,
            }),
        // positional modifier: 2s
        time.clone().map(|t| Modifier {
            name: None,
            value: match t {
                Time::Seconds(s) => Expr::Ident(format!("{}s", s)),
                Time::Milliseconds(ms) => Expr::Ident(format!("{}ms", ms)),
            },
        }),
        expr.clone().map(|value| Modifier { name: None, value }),
    ))
    .boxed();

    let modifiers = modifier
        .separated_by(just(',').padded())
        .collect::<Vec<_>>()
        .delimited_by(just('[').padded(), just(']').padded())
        .or_not()
        .map(|m: Option<Vec<Modifier>>| m.unwrap_or_default());

    let type_ident = ident
        .clone()
        .filter(|s: &String| s.chars().next().map_or(false, |c| c.is_uppercase()));

    #[derive(Clone)]
    enum FlatItem {
        Labeled(String, String, Vec<Modifier>, Vec<InlineItem>),
        Anonymous(String, Vec<Modifier>, Vec<InlineItem>),
        Prop(Property),
    }

    let inline_items = recursive(|inline_items| {
        let children_block = inline_items
            .clone()
            .delimited_by(just('{').padded(), just('}').padded())
            .or_not()
            .map(|c| c.unwrap_or_default());

        let flat_item = choice((
            ident
                .clone()
                .then_ignore(just(':').padded())
                .then(type_ident.clone())
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|(((label, ty), mods), children)| {
                    FlatItem::Labeled(label, ty, mods, children)
                }),
            type_ident
                .clone()
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|((ty, mods), children)| FlatItem::Anonymous(ty, mods, children)),
            property.clone().map(FlatItem::Prop),
        ))
        .padded();

        flat_item
            .separated_by(just(',').padded().or_not())
            .allow_trailing()
            .collect::<Vec<_>>()
            .map(|items| {
                let mut result = Vec::new();
                for item in items {
                    match item {
                        FlatItem::Labeled(label, ty, mods, children) => {
                            result.push(InlineItem::Labeled {
                                label,
                                ty,
                                props: Vec::new(),
                                modifiers: mods,
                                children,
                            });
                        }
                        FlatItem::Anonymous(ty, mods, children) => {
                            result.push(InlineItem::Anonymous {
                                ty,
                                props: Vec::new(),
                                modifiers: mods,
                                children,
                            });
                        }
                        FlatItem::Prop(p) => {
                            if let Some(last) = result.last_mut() {
                                match last {
                                    InlineItem::Labeled { props, .. } => props.push(p),
                                    InlineItem::Anonymous { props, .. } => props.push(p),
                                }
                            }
                        }
                    }
                }
                result
            })
    });

    let stmt = recursive(|_stmt| {
        let let_decl = text::keyword("let")
            .ignore_then(ident.clone())
            .then_ignore(just('=').padded())
            .then(expr.clone())
            .map(|(name, value)| Stmt::LetDecl { name, value })
            .padded();

        let import_stmt = text::keyword("import")
            .padded()
            .ignore_then(
                just('"')
                    .ignore_then(none_of('"').repeated().collect::<String>())
                    .then_ignore(just('"')),
            )
            .map(|path| Stmt::Import { path })
            .padded();

        let assignment = dotted_ident
            .clone()
            .then_ignore(just('=').padded())
            .then(expr.clone())
            .then(modifiers.clone())
            .try_map(|((path, value), modifiers), span| {
                if path.len() < 2 {
                    Err(Rich::custom(
                        span,
                        "assignment target must include at least one '.' before the property",
                    ))
                } else {
                    let property = path.last().cloned().unwrap_or_default();
                    let target = path[..path.len() - 1].to_vec();
                    Ok(Stmt::Assignment {
                        target,
                        property,
                        value,
                        modifiers,
                    })
                }
            })
            .padded();

        let block_props = property
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        let text_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Text"))
            .then(block_props.clone())
            .then(modifiers.clone())
            .map(|((label, props), modifiers)| Stmt::Text {
                label,
                props,
                modifiers,
            })
            .padded();

        let math_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Math"))
            .then(block_props.clone())
            .then(modifiers.clone())
            .map(|((label, props), modifiers)| Stmt::Math {
                label,
                props,
                modifiers,
            })
            .padded();

        let code_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Code"))
            .then(block_props.clone())
            .then(modifiers.clone())
            .map(|((label, props), modifiers)| Stmt::Code {
                label,
                props,
                modifiers,
            })
            .padded();

        let svg_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Svg"))
            .then(block_props.clone())
            .map(|(label, props)| {
                let mut url = String::new();
                let mut at = (0.0, 0.0);
                let mut scale = 1.0;
                for p in props {
                    match p.name.as_str() {
                        "url" => {
                            if let Expr::Str(s) = p.value {
                                url = s;
                            }
                        }
                        "at" => {
                            if let Expr::Tuple(t) = p.value {
                                if t.len() == 2 {
                                    if let Expr::Num(x) = t[0] {
                                        if let Expr::Num(y) = t[1] {
                                            at = (x as f32, y as f32);
                                        }
                                    }
                                }
                            }
                        }
                        "scale" => {
                            if let Expr::Num(n) = p.value {
                                scale = n as f32;
                            }
                        }
                        _ => {}
                    }
                }
                Stmt::Svg {
                    label,
                    url,
                    at,
                    scale,
                }
            })
            .padded();

        let image_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Image"))
            .then(block_props.clone())
            .map(|(label, props)| {
                let mut url = String::new();
                let mut at = (0.0, 0.0);
                let mut size = None;
                for p in props {
                    match p.name.as_str() {
                        "url" => {
                            if let Expr::Str(s) = p.value {
                                url = s;
                            }
                        }
                        "at" => {
                            if let Expr::Tuple(t) = p.value {
                                if t.len() == 2 {
                                    if let Expr::Num(x) = t[0] {
                                        if let Expr::Num(y) = t[1] {
                                            at = (x as f32, y as f32);
                                        }
                                    }
                                }
                            }
                        }
                        "size" => {
                            if let Expr::Tuple(t) = p.value {
                                if t.len() == 2 {
                                    if let Expr::Num(width) = t[0] {
                                        if let Expr::Num(height) = t[1] {
                                            size = Some((width as f32, height as f32));
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Stmt::Image {
                    label,
                    url,
                    at,
                    size,
                }
            })
            .padded();

        let actor_decl = text::keyword("pub")
            .padded()
            .or_not()
            .map(|p| p.is_some())
            .then(ident.clone())
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
                |(((((is_pub, label), ty), props), modifiers), children)| Stmt::ActorDecl {
                    is_pub,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                },
            )
            .padded();

        let action = ident
            .clone()
            .then(ident.clone().repeated().collect::<Vec<_>>()) // Simplified targets
            .then(modifiers.clone())
            .map(|((verb, targets), modifiers)| {
                Stmt::Action(Action {
                    verb,
                    targets,
                    args: vec![],
                    modifiers,
                })
            })
            .padded();

        let comment = just("//")
            .ignore_then(none_of("\r\n").repeated().to_slice().map(String::from))
            .map(Stmt::Comment)
            .padded();

        // Always statement: always { }
        let always_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        let always_stmt = text::keyword("always")
            .ignore_then(always_body.clone())
            .map(|body| Stmt::Always { body })
            .padded();

        let labeled_always_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(text::keyword("always"))
            .then(always_body.clone())
            .map(|((label, _), body)| Stmt::LabeledAlways { label, body })
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
                },
            )
            .padded();

        // For loop: for i in items { }
        let for_loop_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        let for_stmt = text::keyword("for")
            .ignore_then(ident.clone())
            .then_ignore(text::keyword("in").padded())
            .then(expr.clone())
            .then(for_loop_body)
            .map(|((var, iterable), body)| Stmt::ForLoop {
                var,
                iterable,
                body,
            })
            .padded();

        let param_def = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(
                str_val
                    .clone()
                    .map(|e| Some(e))
                    .or(text::keyword("null").to(Some(Expr::Null))),
            )
            .map(|(name, default): (String, Option<Expr>)| ParamDef {
                name,
                param_type: None,
                default: default,
            });

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
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(((is_pub, name), params), body)| {
                Stmt::ComponentDef(ComponentDef {
                    is_pub,
                    name,
                    params,
                    body,
                })
            })
            .padded();

        choice((
            let_decl,
            import_stmt,
            assignment,
            text_stmt,
            math_stmt,
            code_stmt,
            svg_stmt,
            image_stmt,
            labeled_always_stmt,
            always_stmt,
            conditional_stmt,
            for_stmt,
            component_def,
            actor_decl,
            action,
            comment,
        ))
        .boxed()
    });

    let keyframe = just('#')
        .ignore_then(just('+').or_not())
        .then(time.clone())
        .then(stmt.clone().repeated().collect::<Vec<_>>())
        .map(|((is_relative, t), body)| {
            if is_relative.is_some() {
                Stmt::RelativeKeyframe { offset: t, body }
            } else {
                Stmt::Keyframe { time: t, body }
            }
        })
        .padded();

    // Top-level can be keyframes or standalone statements
    choice((
        keyframe,
        stmt.map(|s| Stmt::Keyframe {
            time: Time::Seconds(0.0), // default timeline wrapper
            body: vec![s],
        }),
    ))
    .repeated()
    .collect::<Vec<_>>()
    .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chumsky::Parser;

    #[test]
    fn test_closure_parser() {
        let input = "let f = (x) => x ^ 2";
        let res = parser().parse(input).unwrap();

        // Find the LetDecl stmt
        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::LetDecl { name, value } = &body[0] {
                assert_eq!(name, "f");
                assert_eq!(
                    *value,
                    Expr::Closure(
                        vec!["x".to_string()],
                        Box::new(Expr::Binary(
                            Box::new(Expr::Ident("x".to_string())),
                            BinaryOp::Pow,
                            Box::new(Expr::Num(2.0))
                        ))
                    )
                );
            } else {
                panic!("Expected LetDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }
}
