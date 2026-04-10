use crate::ast::*;
use chumsky::prelude::*;

pub fn parser<'src>() -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let ident = text::ident()
        .then(just('-').then(text::ident()).repeated())
        .to_slice()
        .map(String::from)
        .padded();

    let num = text::int(10)
        .then(just('.').ignore_then(text::digits(10)).or_not())
        .to_slice()
        .from_str()
        .unwrapped()
        .map(Expr::Num)
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
            .map(Expr::Tuple);

        let array = expr
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded())
            .map(Expr::Tuple); // Using Tuple for arrays as well per AST

        let atom = choice((
            num,
            str_val,
            bool_val,
            null_val,
            tuple,
            array,
            ident.clone().map(Expr::Ident),
        ))
        .padded();

        // Method calls and indexing
        let access = atom.foldl(
            choice((just('.')
                .ignore_then(ident.clone())
                .then(
                    expr.clone()
                        .separated_by(just(',').padded())
                        .collect::<Vec<_>>()
                        .delimited_by(just('(').padded(), just(')').padded())
                        .or_not(),
                )
                .map(|(name, args)| {
                    if let Some(_args) = args {
                        // Method call: obj.method(args)
                        vec![name, "()".to_string()] // A bit of a hack to pass through foldl
                    } else {
                        // Path access: obj.field
                        vec![name]
                    }
                }),))
            .repeated(),
            |acc, parts| {
                // Simplified Path construction for AST matching
                if parts.len() == 1 {
                    Expr::Path(vec![
                        match acc {
                            Expr::Ident(s) => s,
                            _ => "".to_string(), // Simplified
                        },
                        parts[0].clone(),
                    ])
                } else {
                    acc
                }
            },
        );

        // Comparison operators
        let compare_op = choice((
            just(">=").to(BinaryOp::Gte),
            just("<=").to(BinaryOp::Lte),
            just("==").to(BinaryOp::Eq),
            just("!=").to(BinaryOp::Neq),
            just('>').to(BinaryOp::Gt),
            just('<').to(BinaryOp::Lt),
        ));

        let comparison = access
            .clone()
            .then(compare_op)
            .then(access.clone())
            .map(|((left, op), right)| Expr::Binary(Box::new(left), op, Box::new(right)));

        // We can add operators here, but for brevity we stick to the basic atoms and paths
        comparison.or(access)
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
            .then(expr.clone())
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
    ));

    let modifiers = modifier
        .separated_by(just(',').padded())
        .collect::<Vec<_>>()
        .delimited_by(just('[').padded(), just(']').padded())
        .or_not()
        .map(|m| m.unwrap_or_default());

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

        let assignment = ident
            .clone()
            .then_ignore(just('.'))
            .then(ident.clone())
            .then_ignore(just('=').padded())
            .then(expr.clone())
            .then(modifiers.clone())
            .map(
                |(((target, property), value), modifiers)| Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                },
            )
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

        let actor_decl = text::keyword("pub")
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
                    .map(|p| p.unwrap_or_default()),
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

        // Loop statement: loop { } | loop N times { } | loop Ns { }
        let loop_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        // Helper enum to carry suffix info through the count parser
        #[derive(Clone)]
        enum LoopKindKind {
            Count,
            BoundedSec,
            BoundedMs,
        }

        // Parse loop with a number, then look ahead to decide kind.
        // Captures the number FIRST, then checks suffix — no backtracking issues.
        let loop_with_num = text::keyword("loop")
            .ignore_then(just(' '))
            .then(text::int(10).map(|s: &str| s.parse::<u32>().unwrap_or(1)))
            .then_ignore(
                just(' ')
                    .ignore_then(text::keyword("times"))
                    .ignore_then(just(' ')),
            )
            .padded()
            .map(|(_, count)| (count, LoopKindKind::Count));

        // Count loop: "loop N times { }"
        let loop_count = loop_with_num
            .then(loop_body.clone())
            .map(|((count, _), body)| Stmt::Loop {
                kind: LoopKind::Count(count),
                label: None,
                body,
            })
            .padded();

        // Bounded loop: "loop Ns { }" or "loop Nms { }"
        let loop_bounded = text::keyword("loop")
            .ignore_then(just(' '))
            .ignore_then(time.clone())
            .then(loop_body.clone())
            .map(|(kind, body)| Stmt::Loop {
                kind: LoopKind::Bounded(kind),
                label: None,
                body,
            })
            .padded();

        // Infinite loop: "loop { }"
        let loop_infinite = text::keyword("loop")
            .ignore_then(loop_body.clone())
            .map(|body| Stmt::Loop {
                kind: LoopKind::Infinite,
                label: None,
                body,
            })
            .padded();

        let loop_stmt = choice((loop_infinite, loop_count, loop_bounded));

        // Labeled loop: job: loop N times { } | job: loop Ns { }
        let labeled_loop_with_num = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(text::keyword("loop"))
            .then(just(' '))
            .then(text::int(10).map(|s: &str| s.parse::<u32>().unwrap_or(1)))
            .then_ignore(
                just(' ')
                    .ignore_then(text::keyword("times"))
                    .ignore_then(just(' ')),
            )
            .padded()
            .map(|(((label, _), _), count)| (label, count, LoopKindKind::Count));

        // Labeled count loop: "job: loop N times { }"
        let labeled_loop_count = labeled_loop_with_num
            .then(loop_body.clone())
            .map(|((label, count, _), body)| Stmt::Loop {
                kind: LoopKind::Count(count),
                label: Some(label),
                body,
            })
            .padded();

        // Labeled bounded loop: "job: loop Ns { }"
        let labeled_loop_bounded = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(text::keyword("loop"))
            .then(just(' '))
            .then(time.clone())
            .then(loop_body.clone())
            .map(|((((label, _), _), time_val), body)| Stmt::Loop {
                kind: LoopKind::Bounded(time_val),
                label: Some(label),
                body,
            })
            .padded();

        let labeled_loop_stmt = choice((labeled_loop_count, labeled_loop_bounded));

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
            .or_not()
            .map(|p| p.is_some())
            .then(text::keyword("component"))
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
            .map(|((((is_pub, _), name), params), body)| {
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
            svg_stmt,
            labeled_loop_stmt,
            loop_stmt,
            labeled_always_stmt,
            always_stmt,
            conditional_stmt,
            for_stmt,
            actor_decl,
            component_def,
            action,
            comment,
        ))
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
}
