//!
//! Inline item parser for the Animatix parser.
//!
//! This module provides the [`FlatItem`] type (internal flat representation
//! used during parsing) and the [`parser()`] function that builds the inline
//! items combinator.

use std::cell::RefCell;
use std::rc::Rc;

use chumsky::input::MapExtra;
use chumsky::prelude::*;

use super::common::{self, ExprParser, InlineItemsParser, ParserExtra, StrInput};
use super::token_parser::*;
use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

/// Internal flat representation of items parsed inside an actor block.
#[derive(Clone)]
pub(crate) enum FlatItem {
    /// `label: Type [modifiers] [{ children }]`
    Labeled(String, Option<Expr>, String, Vec<Modifier>, Vec<InlineItem>),
    /// `Type [modifiers] [{ children }]`
    Anonymous(String, Vec<Modifier>, Vec<InlineItem>),
    /// `name: value`
    Prop(Property, ByteSpan),
    /// `{ children }` — attaches to preceding item
    Children(Vec<InlineItem>),
    /// `@slot` in component definition blocks
    SlotMarker,
    /// `@slotname { items }` in component instantiation blocks
    SlotFill(String, Vec<InlineItem>),
    /// `for item, i in list { ... }` or `for (a, b) in list { ... }` inside container children
    ForLoop(LoopPattern, Option<String>, Expr, Vec<InlineItem>),
}

/// Build the inline items parser.
pub(crate) fn parser<'src>(
    expr: ExprParser<'src>,
    property: impl Parser<'src, StrInput<'src>, Property, ParserExtra<'src>> + Clone + 'src,
    modifiers: impl Parser<'src, StrInput<'src>, Vec<Modifier>, ParserExtra<'src>> + Clone + 'src,
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
) -> InlineItemsParser<'src> {
    let type_ident = common::type_ident();
    let label_expr = common::label_expr(expr.clone());

    let inline_items =
        recursive(|inline_items| {
            let children_block = inline_items
                .clone()
                .delimited_by(lbrace(), rbrace())
                .or_not()
                .map(|c| c.unwrap_or_default());

            let for_inline_index = comma().ignore_then(common::ident()).or_not();

            let flat_item = choice((
                at().ignore_then(common::ident())
                    .then(inline_items.clone().delimited_by(lbrace(), rbrace()))
                    .map(|(name, items)| FlatItem::SlotFill(name, items)),
                at_slot().to(FlatItem::SlotMarker),
                {
                    let loop_var_pat = common::ident().map(LoopPattern::Single).or(common::ident()
                        .separated_by(comma())
                        .collect::<Vec<_>>()
                        .delimited_by(lparen(), rparen())
                        .map(LoopPattern::Tuple));
                    keyword("for")
                        .ignore_then(loop_var_pat)
                        .then(for_inline_index)
                        .then_ignore(keyword("in"))
                        .then(expr.clone())
                        .then(inline_items.clone().delimited_by(lbrace(), rbrace()))
                        .map(|(((var, index_var), iterable), body)| {
                            FlatItem::ForLoop(var, index_var, iterable, body)
                        })
                },
                label_expr
                    .clone()
                    .then_ignore(colon())
                    .then(type_ident.clone())
                    .then(modifiers.clone())
                    .then(children_block.clone())
                    .map(|((((label, array_index), ty), mods), children)| {
                        FlatItem::Labeled(label, array_index, ty, mods, children)
                    }),
                type_ident
                    .clone()
                    .then(modifiers.clone())
                    .then(children_block.clone())
                    .map(|((ty, mods), children)| FlatItem::Anonymous(ty, mods, children)),
                property.clone().map_with(
                    |p, extra: &mut MapExtra<'src, '_, StrInput<'src>, ParserExtra<'src>>| {
                        FlatItem::Prop(p, extra.span())
                    },
                ),
                inline_items.clone().delimited_by(lbrace(), rbrace()).map(FlatItem::Children),
            ));

            let w = warnings.clone();
            flat_item
                .separated_by(comma().or_not())
                .allow_trailing()
                .collect::<Vec<_>>()
                .map(move |items| {
                    let mut result = Vec::new();
                    for item in items {
                        match item {
                            FlatItem::Labeled(label, array_index, ty, mods, children) => {
                                result.push(InlineItem::Labeled {
                                    label,
                                    array_index,
                                    ty,
                                    props: Vec::new(),
                                    modifiers: mods,
                                    children,
                                });
                            },
                            FlatItem::Anonymous(ty, mods, children) => {
                                result.push(InlineItem::Anonymous {
                                    ty,
                                    props: Vec::new(),
                                    modifiers: mods,
                                    children,
                                });
                            },
                            FlatItem::Prop(p, span) => {
                                if let Some(last) = result.last_mut() {
                                    match last {
                                        InlineItem::Labeled { props, .. } => props.push(p),
                                        InlineItem::Anonymous { props, .. } => props.push(p),
                                        _ => {
                                            let prop_name = p.name.clone();
                                            let location_span = p.value_span.unwrap_or(span);
                                            w.borrow_mut().push(Diagnostic::warning(
                                            DiagnosticCode::BracedPropertySilentDrop,
                                            DiagnosticPhase::Parse,
                                            format!(
                                                "property '{prop_name}' inside braces has no \
                                                 preceding actor to attach to; properties must \
                                                 be declared before the children block, e.g. \
                                                 'Type, {prop_name}: value'"
                                            ),
                                        ).with_location(
                                            0,
                                            0,
                                            location_span.start..location_span.end,
                                        ));
                                        },
                                    }
                                } else {
                                    let prop_name = p.name.clone();
                                    let location_span = p.value_span.unwrap_or(span);
                                    w.borrow_mut().push(Diagnostic::warning(
                                    DiagnosticCode::BracedPropertySilentDrop,
                                    DiagnosticPhase::Parse,
                                    format!(
                                        "property '{prop_name}' inside braces has no preceding \
                                         actor to attach to; properties must be declared before \
                                         the children block, e.g. 'Type, {prop_name}: value'"
                                    ),
                                ).with_location(0, 0, location_span.start..location_span.end));
                                }
                            },
                            FlatItem::Children(children) => {
                                if let Some(last) = result.last_mut() {
                                    match last {
                                        InlineItem::Labeled { children: c, .. } => *c = children,
                                        InlineItem::Anonymous { children: c, .. } => *c = children,
                                        _ => {},
                                    }
                                }
                            },
                            FlatItem::SlotMarker => {
                                result.push(InlineItem::SlotMarker);
                            },
                            FlatItem::ForLoop(var, index_var, iterable, body) => {
                                result.push(InlineItem::ForLoop {
                                    var,
                                    index_var,
                                    iterable,
                                    body,
                                });
                            },
                            FlatItem::SlotFill(name, items) => {
                                result.push(InlineItem::SlotFill { slot: name, items });
                            },
                        }
                    }
                    result
                })
        });

    inline_items.boxed()
}
