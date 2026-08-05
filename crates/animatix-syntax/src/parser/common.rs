//! Shared parser types and factory functions.
//!
//! This module provides reusable parser combinators used across parser
//! submodules. Each factory function produces a parser with the same
//! behavior as the original inline definition in `parser()`.

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use crate::ast::*;
use crate::easing::parse_easing_name;

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

pub(crate) type ParserExtra<'src> = extra::Err<Rich<'src, char>>;
pub(crate) type StrInput<'src> = &'src str;
pub(crate) type ExprParser<'src> = Boxed<'src, 'src, StrInput<'src>, Expr, ParserExtra<'src>>;
pub(crate) type TimeParser<'src> = Boxed<'src, 'src, StrInput<'src>, Time, ParserExtra<'src>>;
pub(crate) type IdentParser<'src> = Boxed<'src, 'src, StrInput<'src>, String, ParserExtra<'src>>;
pub(crate) type PropertyParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Property, ParserExtra<'src>>;
pub(crate) type ModifierParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Modifier, ParserExtra<'src>>;
pub(crate) type ModifiersParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Vec<Modifier>, ParserExtra<'src>>;
pub(crate) type InlineItemsParser<'src> =
    Boxed<'src, 'src, StrInput<'src>, Vec<InlineItem>, ParserExtra<'src>>;
pub(crate) type StmtParser<'src> = Boxed<'src, 'src, StrInput<'src>, Stmt, ParserExtra<'src>>;

// ---------------------------------------------------------------------------
// Identifier parser
// ---------------------------------------------------------------------------

/// Parse a single identifier, rejecting reserved keywords.
///
/// Supports hyphenated identifiers (e.g. `fade-in`, `move-to`).
pub(crate) fn ident<'src>() -> IdentParser<'src> {
    text::ident()
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
                Err(Rich::custom(span, format!("'{}' is a reserved keyword", ident)))
            } else {
                Ok(String::from(ident))
            }
        })
        .padded()
        .boxed()
}

// ---------------------------------------------------------------------------
// Dotted identifier parser
// ---------------------------------------------------------------------------

/// Parse a dotted path (e.g. `scene.background`, `container.child.prop`).
///
/// Returns a `Vec<String>` of path segments. Each segment is a regular
/// identifier (with reserved keyword rejection).
pub(crate) fn dotted_ident<'src>()
-> impl Parser<'src, StrInput<'src>, Vec<String>, ParserExtra<'src>> + Clone {
    ident().separated_by(just('.').padded()).at_least(1).collect::<Vec<_>>()
}

// ---------------------------------------------------------------------------
// Indexed dotted identifier parser (for targets/assignments)
// ---------------------------------------------------------------------------

/// Parse a dotted path where each segment may carry an integer array index or
/// a runtime index expression.
///
/// `dots[0].at` → `[Static("dots__0"), Static("at")]`
/// `actor.prop`  → `[Static("actor"), Static("prop")]`
/// `bars[i].color` → `[Indexed { base: "bars", index: i }, Static("color")]`
///
/// Integer-literal indices are rewritten to `Static("label__N")` (same as before).
/// Non-literal expressions produce `Indexed { base, index }` for frame-time resolution.
pub(crate) fn indexed_dotted_ident<'src>()
-> impl Parser<'src, StrInput<'src>, Vec<TargetSegment>, ParserExtra<'src>> + Clone {
    use crate::ast::TargetSegment;

    // Original integer-literal-only parser, now producing TargetSegment.
    // `label[n]` → `Static("label__n")` for integer n.
    // This is used for action targets (build-time only).
    let segment = ident()
        .then(
            just('[')
                .ignore_then(text::int::<_, ParserExtra<'src>>(10).to_slice().try_map(
                    |s: &str, span| {
                        s.parse::<usize>().map_err(|_| {
                            Rich::custom(span, "array index must be a non-negative integer literal")
                        })
                    },
                ))
                .then_ignore(just(']'))
                .or_not(),
        )
        .map(|(name, idx)| match idx {
            Some(n) => TargetSegment::Static(format!("{}__{}", name, n)),
            None => TargetSegment::Static(name),
        });

    segment.separated_by(just('.').padded()).at_least(1).collect::<Vec<_>>()
}

/// Version of `indexed_dotted_ident` that accepts an expression parser
/// for runtime-indexed targets (e.g., `bars[i].color`).
///
/// Integer-literal indices produce `Static("label__N")`.
/// Non-literal expressions produce `Indexed { base, index }`.
pub(crate) fn indexed_dotted_ident_with_expr<'src>(
    expr: ExprParser<'src>,
) -> impl Parser<'src, StrInput<'src>, Vec<TargetSegment>, ParserExtra<'src>> + Clone {
    use crate::ast::{Expr, TargetSegment};

    let segment = ident().then(just('[').ignore_then(expr).then_ignore(just(']')).or_not()).map(
        |(name, idx)| match idx {
            Some(Expr::Num(n)) if n.trunc() == n && n >= 0.0 => {
                // Integer literal: rewrite `label[n]` → `Static("label__n")`
                TargetSegment::Static(format!("{}__{}", name, n as usize))
            },
            Some(e) => {
                // Non-literal expression: runtime-indexed segment.
                TargetSegment::Indexed {
                    base: name,
                    index: Box::new(e),
                }
            },
            None => TargetSegment::Static(name),
        },
    );

    segment.separated_by(just('.').padded()).at_least(1).collect::<Vec<_>>()
}

// ---------------------------------------------------------------------------
// Type identifier parser
// ---------------------------------------------------------------------------

/// Parse a type identifier: a string starting with an uppercase letter.
///
/// Used for actor type names like `Rect`, `Text`, `Circle`, etc.
pub(crate) fn type_ident<'src>() -> IdentParser<'src> {
    ident()
        .filter(|s: &String| s.chars().next().is_some_and(|c| c.is_uppercase()))
        .boxed()
}

// ---------------------------------------------------------------------------
// Label expression parser
// ---------------------------------------------------------------------------

/// Parse a label expression: `name` or `name[index_expr]`.
///
/// Used for named actors (e.g. `btn: Rect`) and array actor generation
/// (e.g. `btn[count]: Rect`).
pub(crate) fn label_expr<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> Boxed<'src, 'src, StrInput<'src>, (String, Option<Expr>), ParserExtra<'src>> {
    ident()
        .then(expr.clone().delimited_by(just('[').padded(), just(']').padded()).or_not())
        .boxed()
}

// ---------------------------------------------------------------------------
// String literal parser
// ---------------------------------------------------------------------------

/// Parse a quoted string literal, returning `Expr::Str`.
pub(crate) fn string_literal<'src>()
-> impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone {
    just('"')
        .ignore_then(none_of('"').repeated().collect::<String>())
        .then_ignore(just('"'))
        .map(Expr::Str)
        .padded()
}

// ---------------------------------------------------------------------------
// Time literal parser
// ---------------------------------------------------------------------------

/// Parse a time literal: `2s`, `500ms`, or `1.5s`.
pub(crate) fn time<'src>() -> TimeParser<'src> {
    text::int(10)
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
        .padded()
        .boxed()
}

// ---------------------------------------------------------------------------
// Expression with span helper
// ---------------------------------------------------------------------------

/// Wrap an expression parser to capture the byte span of the parsed value.
pub(crate) fn expr_with_span<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> Boxed<'src, 'src, StrInput<'src>, (Expr, ByteSpan), ParserExtra<'src>> {
    expr.map_with(|value, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
        let span = extra.span();
        (
            value,
            ByteSpan {
                start: span.start,
                end: span.end,
            },
        )
    })
    .boxed()
}

// ---------------------------------------------------------------------------
// Property parser
// ---------------------------------------------------------------------------

/// Parse a property assignment: `name: value [// comment]`.
///
/// The property name may be a simple identifier or a dotted path. The value
/// span is captured for surgical source edits.
pub(crate) fn property<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
) -> PropertyParser<'src> {
    let property_name = dotted_ident().map(|parts: Vec<String>| parts.join(".")).or(ident());

    let trailing_comment = just("//")
        .ignore_then(none_of("\r\n").repeated().to_slice().map(String::from))
        .or_not();

    property_name
        .then_ignore(just(':').padded())
        .then(expr_with_span(expr))
        .then(trailing_comment)
        .map(|((name, (value, value_span)), comment)| Property {
            name,
            value,
            value_span: Some(value_span),
            trailing_comment: comment,
        })
        .labelled("property")
        .boxed()
}

// ---------------------------------------------------------------------------
// Modifier parser
// ---------------------------------------------------------------------------

/// Parse a single modifier: `name: value`, `2s`, or any expression.
pub(crate) fn modifier<'src>(
    expr: impl Parser<'src, StrInput<'src>, Expr, ParserExtra<'src>> + Clone + 'src,
    time: impl Parser<'src, StrInput<'src>, Time, ParserExtra<'src>> + Clone + 'src,
) -> ModifierParser<'src> {
    choice((
        // named modifier: ease: bounce
        ident()
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
    .boxed()
}

// ---------------------------------------------------------------------------
// Modifiers list parser
// ---------------------------------------------------------------------------

/// Parse a bracketed modifier list: `[2s, ease: bounce]`.
///
/// Returns an empty `Vec` if no modifiers are present.
pub(crate) fn modifiers<'src>(
    modifier: impl Parser<'src, StrInput<'src>, Modifier, ParserExtra<'src>> + Clone + 'src,
) -> ModifiersParser<'src> {
    modifier
        .separated_by(just(',').padded())
        .collect::<Vec<_>>()
        .delimited_by(just('[').padded(), just(']').padded())
        .or_not()
        .map(|m: Option<Vec<Modifier>>| m.unwrap_or_default())
        .labelled("modifier list")
        .as_context()
        .boxed()
}

// ---------------------------------------------------------------------------
// Easing extraction
// ---------------------------------------------------------------------------

/// Scan modifiers for `ease: ...` and extract the easing value.
/// Removes the ease modifier from the list so it doesn't get processed twice.
pub(crate) fn extract_easing(modifiers: &mut Vec<Modifier>) -> Option<crate::easing::Easing> {
    let mut easing = None;
    modifiers.retain(|m| {
        if m.name.as_deref() == Some("ease") {
            if let Expr::Ident(raw) = &m.value {
                easing = parse_easing_name(raw);
            }
            false // remove the modifier
        } else {
            true
        }
    });
    easing
}
