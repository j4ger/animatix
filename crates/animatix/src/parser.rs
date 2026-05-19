//!
//! # Animatix Parser
//!
//! Combinator-based recursive descent parser using [`chumsky`]. This parser is the
//! executable source of truth for accepted `.amx` syntax.
//!
//! ## Entry Points
//!
//! - [`parse_program()`] — parses a full `.amx` file into `Vec<Stmt>`
//! - [`parse_expr()`] — parses a single expression
//!
//! ## Key Design Notes
//!
//! - The grammar is expression-heavy with prefix/infix operator precedence handled via
//!   combinator chaining in `chumsky`.
//! - Actor declarations, actions, and assignments share a generic modifier syntax in
//!   brackets `[...]`.
//! - `Text`, `Math`, `Code` are parsed as generic actor declarations.
//! - The parser accepts some syntax that the runtime may reject (e.g., method/index/construct
//!   expressions) — honest runtime diagnostics handle the mismatch.
//!
//! ## Relationship to Other Systems
//!
//! - [`crate::ast`] defines the AST nodes this parser produces.
//! - `tree-sitter-animatix/` is a synchronized derivative for editor tooling.
//! - Parser tests in `tests/parser_tests.rs` are the authority on accepted syntax.

use crate::ast::*;
use chumsky::input::MapExtra;
use chumsky::prelude::*;
use std::ops::Range;

/// A structured parse error with human-readable location and context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Range<usize>,
    pub line: usize,
    pub column: usize,
    pub expected: Vec<String>,
    pub found: Option<String>,
    pub context: Vec<String>,
}

impl ParseError {
    /// Convert a chumsky `Rich` error into a structured `ParseError`.
    pub fn from_rich(source: &str, err: &Rich<'_, char>) -> Self {
        let span = err.span();
        let start = span.start;
        let end = span.end;
        let (line, column) = byte_offset_to_line_col(source, start);

        let mut _message = String::new();
        let mut expected = Vec::new();
        let mut found = None;

        match err.reason() {
            chumsky::error::RichReason::ExpectedFound { expected: exp, found: f } => {
                expected = exp.iter().map(|p| p.to_string()).collect();
                found = f.as_ref().map(|c| c.to_string());
                let expected_str = expected.join(", ");
                match (expected_str.is_empty(), found.as_ref()) {
                    (false, Some(f)) => _message = format!("expected {expected_str}, found '{f}'"),
                    (false, None) => _message = format!("expected {expected_str}, found end of input"),
                    (true, Some(f)) => _message = format!("unexpected '{f}'"),
                    (true, None) => _message = "unexpected end of input".to_string(),
                }
            }
            chumsky::error::RichReason::Custom(msg) => {
                _message = msg.clone();
            }
        }

        let context: Vec<String> = err
            .contexts()
            .map(|(pattern, _)| pattern.to_string())
            .collect();

        Self {
            message: _message,
            span: start..end,
            line,
            column,
            expected,
            found,
            context,
        }
    }
}

/// Convert a byte offset into a 1-based (line, column) pair.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn parser<'src>() -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let ident = text::ident()
        .then(just('-').then(text::ident()).repeated())
        .to_slice()
        .try_map(|ident: &str, span| {
            let reserved = [
                "let",
                "import",
                "always",
                "drive",
                "if",
                "else",
                "for",
                "in",
                "pub",
                "component",
                "config",
                "true",
                "false",
                "null",
                "loop",
                "yield",
                "stop",
                "pause",
                "resume",
                "action",
                "play",
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

    // Dotted identifier for assignment targets and property keys with dots
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

        // Type construction expression: TypeName { prop1: val1, prop2: val2 }
        // Inline property parsing since property is defined after expr
        let construct = ident
            .filter(|s: &String| s.chars().next().map_or(false, |c| c.is_uppercase()))
            .then(
                ident
                    .clone()
                    .then_ignore(just(':').padded())
                    .then(expr.clone())
                    .map(|(name, value)| Property { name, value, value_span: None, trailing_comment: None })
                    .separated_by(just(',').padded())
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(name, props)| Expr::Construct(name, props))
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
            construct,
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

        choice((closure, conditional_expr, comparison))
            .labelled("expression")
            .as_context()
            .boxed()
    });

    // Property name can be simple ident or dotted (e.g., scene.background)
    let property_name = dotted_ident
        .clone()
        .map(|parts: Vec<String>| parts.join("."))
        .or(ident.clone());

    // Trailing line comment after a property value: `size: (100, 200) // half-extents`
    let trailing_comment = just("//")
        .ignore_then(none_of("\r\n").repeated().to_slice().map(String::from))
        .or_not();

    let property = property_name
        .then_ignore(just(':').padded())
        .then(expr.clone().map_with(|value, extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
            let span = extra.span();
            (value, ByteSpan { start: span.start, end: span.end })
        }))
        .then(trailing_comment)
        .map(|((name, (value, value_span)), comment)| Property {
            name,
            value,
            value_span: Some(value_span),
            trailing_comment: comment,
        })
        .labelled("property");

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
        Children(Vec<InlineItem>),
        SlotMarker,
        SlotFill(String, Vec<InlineItem>),
    }

    let inline_items = recursive(|inline_items| {
        let children_block = inline_items
            .clone()
            .delimited_by(just('{').padded(), just('}').padded())
            .or_not()
            .map(|c| c.unwrap_or_default());

        let flat_item = choice((
            // @slotname { items } in component instantiation blocks
            // MUST be tried BEFORE the @slot marker so that @slot { ... } is
            // parsed as a slot fill (not as a SlotMarker with a dropped block).
            just('@')
                .ignore_then(ident.clone())
                .then(
                    inline_items
                        .clone()
                        .delimited_by(just('{').padded(), just('}').padded()),
                )
                .map(|(name, items)| FlatItem::SlotFill(name, items)),
            // @slot marker in component definition blocks
            // Only matches when @slot appears WITHOUT a following { items } block
            // (because the SlotFill alternative above would have matched first).
            just("@slot").padded().to(FlatItem::SlotMarker),
            // Labeled inline item: label: Type [mods] [{ children }]
            ident
                .clone()
                .then_ignore(just(':').padded())
                .then(type_ident.clone())
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|(((label, ty), mods), children)| {
                    FlatItem::Labeled(label, ty, mods, children)
                }),
            // Anonymous inline item: Type [mods] [{ children }]
            type_ident
                .clone()
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|((ty, mods), children)| FlatItem::Anonymous(ty, mods, children)),
            property.clone().map(FlatItem::Prop),
            // Standalone children block: attaches to the preceding item
            inline_items
                .clone()
                .delimited_by(just('{').padded(), just('}').padded())
                .map(FlatItem::Children),
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
                                    _ => {}
                                }
                            }
                        }
                        FlatItem::Children(children) => {
                            if let Some(last) = result.last_mut() {
                                match last {
                                    InlineItem::Labeled { children: c, .. } => *c = children,
                                    InlineItem::Anonymous { children: c, .. } => *c = children,
                                    _ => {}
                                }
                            }
                        }
                        FlatItem::SlotMarker => {
                            result.push(InlineItem::SlotMarker);
                        }
                        FlatItem::SlotFill(name, items) => {
                            result.push(InlineItem::SlotFill {
                                slot_name: name,
                                items,
                            });
                        }
                    }
                }
                result
            })
    });

    let config_props = property
        .clone()
        .separated_by(just(',').padded())
        .allow_trailing()
        .collect::<Vec<_>>()
        .delimited_by(just('{').padded(), just('}').padded());

    let config_stmt = text::keyword("config")
        .ignore_then(config_props)
        .map(|settings| Stmt::Config { settings, span: None })
        .labelled("config")
        .padded();

    let stmt = recursive(|_stmt| {
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
            .padded();

        let assignment = dotted_ident
            .clone()
            .then_ignore(just('=').padded())
            .then(expr.clone().map_with(|value, extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                (value, ByteSpan { start: span.start, end: span.end })
            }))
            .then(modifiers.clone())
            .try_map(|((path, (value, value_span)), modifiers), span| {
                if path.is_empty() {
                    Err(Rich::custom(
                        span,
                        "assignment target must be a property name or actor.property path",
                    ))
                } else if path.len() == 1 {
                    // Single-segment assignment: e.g. `at = expr` inside a drive block
                    let property = path[0].clone();
                    Ok(Stmt::Assignment {
                        target: vec![],
                        property,
                        value,
                        modifiers,
                        value_span: Some(value_span),
                        span: None,
                    })
                } else {
                    let property = path.last().cloned().unwrap_or_default();
                    let target = path[..path.len() - 1].to_vec();
                    Ok(Stmt::Assignment {
                        target,
                        property,
                        value,
                        modifiers,
                        value_span: Some(value_span),
                        span: None,
                    })
                }
            })
            .labelled("assignment")
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

        let block_props = property
            .clone()
            .separated_by(just(',').padded())
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        let svg_stmt = ident
            .clone()
            .then_ignore(just(':').padded())
            .or_not()
            .then_ignore(text::keyword("Svg"))
            .then(block_props.clone())
            .map(|(label, props)| {
                let mut url = String::new();
                let mut at = None;
                let mut anchor = None;
                let mut offset = None;
                let mut scale = 1.0;
                for p in props {
                    match p.name.as_str() {
                        "url" => {
                            if let Expr::Str(s) = p.value {
                                url = s;
                            }
                        }
                        "at" => {
                            at = Some(p.value);
                        }
                        "anchor" => {
                            anchor = Some(p.value);
                        }
                        "offset" => {
                            offset = Some(p.value);
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
                    anchor,
                    offset,
                    scale,
                    span: None,
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
                let mut at = None;
                let mut anchor = None;
                let mut offset = None;
                let mut size = None;
                for p in props {
                    match p.name.as_str() {
                        "url" => {
                            if let Expr::Str(s) = p.value {
                                url = s;
                            }
                        }
                        "at" => {
                            at = Some(p.value);
                        }
                        "anchor" => {
                            anchor = Some(p.value);
                        }
                        "offset" => {
                            offset = Some(p.value);
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
                    anchor,
                    offset,
                    size,
                    span: None,
                }
            })
            .padded();

        // Shorthand: label: "string" → label: Text, text: "string"
        let text_shorthand = ident
            .clone()
            .then_ignore(just(':').padded())
            .then(str_val.clone())
            .then(modifiers.clone())
            .map(|((label, text), modifiers)| Stmt::ActorDecl {
                is_pub: false,
                label,
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
                    span: None,
                },
            )
            .labelled("actor declaration")
            .as_context()
            .padded();

        let action = ident
            .clone()
            .then(ident.clone().repeated().collect::<Vec<_>>()) // Simplified targets
            .then(modifiers.clone())
            .map_with(|((verb, targets), modifiers), extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                Stmt::Action(Action {
                    verb,
                    targets,
                    args: vec![],
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
        let always_body = _stmt
            .clone()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just('{').padded(), just('}').padded());

        let always_stmt = text::keyword("always")
            .ignore_then(always_body.clone())
            .map(|body| Stmt::Always { body, span: None })
            .padded();

        let drive_stmt = text::keyword("drive")
            .ignore_then(ident.clone())
            .then(always_body.clone())
            .map(|(label, body)| Stmt::Drive { label, body, span: None })
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
                span: None,
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

        let component_action_stmt = text::keyword("action")
            .ignore_then(ident.clone())
            .then(
                _stmt
                    .clone()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(name, body)| Stmt::ComponentAction {
                name,
                params: vec![],
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
                    .delimited_by(just('{').padded(), just('}').padded()),
            )
            .map(|(((is_pub, name), params), body)| {
                Stmt::ComponentDef(ComponentDef {
                    is_pub,
                    name,
                    params,
                    body,
                }, None)
            })
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
            drive_stmt,
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
    });

        let scene_ref = dotted_ident
            .clone()
            .map(|parts: Vec<String>| parts.join("."));

        // `play SceneName [modifier, ...]` — scene-level transition statement
        let play_stmt = text::keyword("play")
            .padded()
            .ignore_then(scene_ref)
            .then(modifiers.clone())
            .map(|(scene_name, mods)| {
                let transition = parse_transition_from_modifiers(&mods);
                Stmt::Play { scene_name, transition, span: None }
            })
            .labelled("play statement")
            .padded();

        // `# SceneName` — scene declaration (only at top level, not inside containers)
        let scene_decl = just('#')
            .ignore_then(ident.clone().padded())
            .map(|name| Stmt::Scene {
                name,
                config: vec![],
                body: vec![],
                span: None,
            })
            .labelled("scene declaration")
            .padded();

        let keyframe = just('#')
            .ignore_then(just('+').or_not())
            .then(time.clone())
            .then(stmt.clone().repeated().collect::<Vec<_>>())
            .map(|((is_relative, t), body)| {
                if is_relative.is_some() {
                    Stmt::RelativeKeyframe { offset: t, body, span: None }
                } else {
                    Stmt::Keyframe { time: t, body, span: None }
                }
            })
            .labelled("keyframe")
            .padded();

    // Top-level: scenes, keyframes, play, config, or standalone statements
    choice((
        keyframe,
        scene_decl,
        play_stmt,
        config_stmt,
        stmt.map(|s| Stmt::Keyframe {
            time: Time::Seconds(0.0), // default timeline wrapper
            body: vec![s],
            span: None,
        }),
    ))
    .repeated()
    .collect::<Vec<_>>()
    .map(group_scenes)
    .boxed()
}

/// After parsing, group flat statements into scenes.
///
/// If any `Stmt::Scene` markers exist in the parsed output:
///   - Everything before the first scene is the shared prelude.
///   - Each scene marker starts a new scene; its body accumulates
///     all subsequent statements until the next scene marker or EOF.
///   - A `config { ... }` immediately after a scene marker is absorbed
///     as that scene's config.
///   - `play` statements belong to the current scene's body.
///
/// If no scene markers exist, the output is returned unmodified
/// (single-scene file, backward compatible).
pub fn group_scenes(flat: Vec<Stmt>) -> Vec<Stmt> {
    let has_scenes = flat.iter().any(|s| matches!(s, Stmt::Scene { .. }));
    if !has_scenes {
        return flat;
    }

    let mut result: Vec<Stmt> = Vec::new();
    let mut current_scene: Option<Stmt> = None;

    for stmt in flat {
        match stmt {
            Stmt::Scene { name, config: _, body: _, span } => {
                // Finish previous scene if any
                if let Some(scene) = current_scene.take() {
                    result.push(scene);
                }
                current_scene = Some(Stmt::Scene { name, config: vec![], body: vec![], span });
            }
            Stmt::Config { .. } => {
                if let Some(Stmt::Scene { ref mut config, ref body, .. }) = current_scene {
                    // Absorb config into the scene only if both config and body
                    // are still empty (config must be the first thing after the scene name).
                    if config.is_empty() && body.is_empty() {
                        if let Stmt::Config { settings, .. } = stmt {
                            *config = settings;
                            continue;
                        }
                    }
                }
                // Otherwise treat as part of the body
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(stmt);
                } else {
                    // Prelude config — keep in result
                    result.push(stmt);
                }
            }
            other => {
                if let Some(Stmt::Scene { ref mut body, .. }) = current_scene {
                    body.push(other);
                } else {
                    // Prelude statements (imports, pub lets, etc.)
                    result.push(other);
                }
            }
        }
    }

    // Push the last scene
    if let Some(scene) = current_scene {
        result.push(scene);
    }

    result
}

/// Convert play statement modifiers into a `Transition` descriptor.
///
/// Modifiers format: `[fade, 300ms]` or `[wipe-left, 200ms]`.
/// The first bare identifier (not a time) is the transition type.
/// The first time literal is the duration.
fn parse_transition_from_modifiers(modifiers: &[Modifier]) -> Option<crate::ast::Transition> {
    let mut transition_id: Option<String> = None;
    let mut duration_ms: u64 = 0;

    for m in modifiers {
        match (&m.name, &m.value) {
            (None, Expr::Ident(name)) if transition_id.is_none() => {
                if crate::transition_registry::find(name).is_some() {
                    transition_id = Some(name.clone());
                }
            }
            (None, Expr::Ident(name)) if name.ends_with("ms") => {
                if let Ok(ms) = name.trim_end_matches("ms").parse::<u64>() {
                    if duration_ms == 0 {
                        duration_ms = ms;
                    }
                }
            }
            (None, Expr::Ident(name)) if name.ends_with('s') && !name.starts_with(|c: char| c.is_alphabetic()) => {
                if let Ok(s) = name.trim_end_matches('s').parse::<f64>() {
                    if duration_ms == 0 {
                        duration_ms = (s * 1000.0) as u64;
                    }
                }
            }
            _ => {}
        }
    }

    transition_id.map(|id| crate::ast::Transition {
        id,
        duration_ms,
        easing: crate::easing::Easing::Linear,
    })
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
            if let Stmt::LetDecl { is_pub, name, value, .. } = &body[0] {
                assert_eq!(*is_pub, false);
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

    #[test]
    fn test_text_shorthand_parser() {
        let input = r#"a: "hello world""#;
        let res = parser().parse(input).unwrap();

        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ActorDecl {
                is_pub,
                label,
                ty,
                props,
                modifiers,
                children,
                ..
            } = &body[0]
            {
                assert_eq!(*is_pub, false);
                assert_eq!(label, "a");
                assert_eq!(ty, "Text");
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].name, "text");
                assert_eq!(props[0].value, Expr::Str("hello world".to_string()));
                assert!(modifiers.is_empty());
                assert!(children.is_empty());
            } else {
                panic!("Expected ActorDecl, got {:?}", body[0]);
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_text_shorthand_with_modifiers() {
        let input = r#"title: "Slide 1" [2s, ease: ease-in-out]"#;
        let res = parser().parse(input).unwrap();

        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ActorDecl {
                label,
                ty,
                props,
                modifiers,
                ..
            } = &body[0]
            {
                assert_eq!(label, "title");
                assert_eq!(ty, "Text");
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].name, "text");
                assert_eq!(props[0].value, Expr::Str("Slide 1".to_string()));
                assert_eq!(modifiers.len(), 2);
            } else {
                panic!("Expected ActorDecl, got {:?}", body[0]);
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_vec2_value_span_accuracy() {
        // Reproduce the bug: size: (2494.552, 1377.7778) should have correct span
        let input = r#"backdrop: Rect, size: (2494.552, 1377.7778), color: scene.background"#;
        let res = parser().parse(input).unwrap();

        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                let span = size_prop.value_span.unwrap();

                // The value in source is "(2494.552, 1377.7778)"
                // Find its actual position in the input
                let value_start = input.find("(2494.552").unwrap();
                let value_end = input.find("1377.7778)").unwrap() + "1377.7778)".len();

                assert_eq!(span.start, value_start, "span start mismatch");
                assert_eq!(span.end, value_end, "span end mismatch");

                // Verify the span extracts the correct text
                let extracted = &input[span.start..span.end];
                assert_eq!(extracted, "(2494.552, 1377.7778)", "span extracts wrong text");
            } else {
                panic!("Expected ActorDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_vec2_value_span_with_trailing_comma() {
        // Test with trailing comma in tuple: (2494.552, 1377.7778,)
        let input = r#"backdrop: Rect, size: (2494.552, 1377.7778,), color: scene.background"#;
        let res = parser().parse(input).unwrap();

        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                let span = size_prop.value_span.unwrap();

                // The value in source is "(2494.552, 1377.7778,)"
                let value_start = input.find("(2494.552").unwrap();
                let value_end = input.find("1377.7778,)").unwrap() + "1377.7778,)".len();

                assert_eq!(span.start, value_start, "span start mismatch");
                assert_eq!(span.end, value_end, "span end mismatch");

                let extracted = &input[span.start..span.end];
                assert_eq!(extracted, "(2494.552, 1377.7778,)", "span extracts wrong text");
            } else {
                panic!("Expected ActorDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_multiple_properties_span_independence() {
        // Test that spans for multiple properties don't overlap
        let input = r#"backdrop: Rect, size: (100, 200), color: red, anchor: center"#;
        let res = parser().parse(input).unwrap();

        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                let color_prop = props.iter().find(|p| p.name == "color").unwrap();
                let anchor_prop = props.iter().find(|p| p.name == "anchor").unwrap();

                let size_span = size_prop.value_span.unwrap();
                let color_span = color_prop.value_span.unwrap();
                let anchor_span = anchor_prop.value_span.unwrap();

                // Verify spans don't overlap
                assert!(size_span.end <= color_span.start, "size span overlaps color span");
                assert!(color_span.end <= anchor_span.start, "color span overlaps anchor span");

                // Verify extracted text
                let size_text = &input[size_span.start..size_span.end];
                let color_text = &input[color_span.start..color_span.end];
                let anchor_text = &input[anchor_span.start..anchor_span.end];

                assert_eq!(size_text, "(100, 200)");
                assert_eq!(color_text, "red");
                assert_eq!(anchor_text, "center");
            } else {
                panic!("Expected ActorDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn test_drive_block_parser() {
        let input = r#"drive tracker {
    at = (640 + 100 * cos(t), 360 + 100 * sin(t))
}"#;
        let res = parser().parse(input).unwrap();
        assert_eq!(res.len(), 1);
        // Top-level statements are wrapped in a default keyframe
        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::Drive { label, body: drive_body, .. } = &body[0] {
                assert_eq!(label, "tracker");
                assert_eq!(drive_body.len(), 1);
                if let Stmt::Assignment {
                    target,
                    property,
                    ..
                } = &drive_body[0]
                {
                    assert!(target.is_empty(), "Expected empty target for single-segment assignment inside drive");
                    assert_eq!(property, "at");
                } else {
                    panic!("Expected Assignment");
                }
            } else {
                panic!("Expected Drive");
            }
        } else {
            panic!("Expected Keyframe wrapper");
        }
    }

    #[test]
    fn test_single_segment_assignment_outside_drive_is_parsed() {
        // Single-segment assignments parse successfully but are rejected at build time
        let input = r#"at = (100, 200)"#;
        let res = parser().parse(input).unwrap();
        assert_eq!(res.len(), 1);
        // Top-level statements are wrapped in a default keyframe
        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::Assignment { target, property, .. } = &body[0] {
                assert!(target.is_empty());
                assert_eq!(property, "at");
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected Keyframe wrapper");
        }
    }

    #[test]
    fn test_reactive_binding_parser() {
        let input = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
        let res = parser().parse(input).unwrap();
        assert_eq!(res.len(), 1);
        // Top-level statements are wrapped in a default keyframe
        if let Stmt::Keyframe { body, .. } = &res[0] {
            if let Stmt::ReactiveBinding { target, property, value, .. } = &body[0] {
                assert_eq!(target, &["orbiter"]);
                assert_eq!(property, "at");
                // Verify it's a binary expression (tracker.at + (...))
                if let Expr::Binary(left, BinaryOp::Add, _right) = value {
                    if let Expr::Path(parts) = left.as_ref() {
                        assert_eq!(parts, &["tracker", "at"]);
                    } else {
                        panic!("Expected Path for left side");
                    }
                } else {
                    panic!("Expected Binary Add expression");
                }
            } else {
                panic!("Expected ReactiveBinding");
            }
        } else {
            panic!("Expected Keyframe wrapper");
        }
    }

    #[test]
    fn test_reactive_binding_rejects_single_segment() {
        let input = r#"at := (100, 200)"#;
        let res = parser().parse(input);
        assert!(res.has_errors(), "Expected parse error for single-segment reactive binding");
    }
}
