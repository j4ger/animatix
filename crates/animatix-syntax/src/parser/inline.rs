//!
//! Inline item parser for the Animatix parser.
//!
//! This module provides the [`FlatItem`] type (internal flat representation
//! used during parsing) and the [`parser()`] function that builds the inline
//! items combinator.  The combinator is invoked from [`super::parser()`] to
//! parse the children of actor declarations.

use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use chumsky::input::MapExtra;
use chumsky::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use super::common::{self, ExprParser, InlineItemsParser, ParserExtra, StrInput};

/// Internal flat representation of items parsed inside an actor block.
///
/// During parsing, inline items are first collected as a flat sequence
/// of `FlatItem` variants, then assembled into `InlineItem` trees.
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
    /// `for item, i in list { ... }` inside container children
    ForLoop(String, Option<String>, Expr, Vec<InlineItem>),
}

/// Build the inline items parser.
///
/// Takes the outer expression, property, and modifiers parsers and returns
/// a boxed parser that recognizes a comma-separated sequence of inline items
/// (labeled actors, anonymous actors, properties, slots, for-loops, etc.)
/// inside an actor declaration block `{ ... }`.
pub(crate) fn parser<'src>(
    expr: ExprParser<'src>,
    property: impl Parser<'src, StrInput<'src>, Property, ParserExtra<'src>> + Clone + 'src,
    modifiers: impl Parser<'src, StrInput<'src>, Vec<Modifier>, ParserExtra<'src>> + Clone + 'src,
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
) -> InlineItemsParser<'src> {
    let type_ident = common::type_ident();
    let label_expr = common::label_expr(expr.clone());

    let inline_items = recursive(|inline_items| {
        let children_block = inline_items
            .clone()
            .delimited_by(just('{').padded(), just('}').padded())
            .or_not()
            .map(|c| c.unwrap_or_default());

        let for_inline_index = just(',')
            .padded()
            .ignore_then(common::ident())
            .or_not();

        let flat_item = choice((
            // @slotname { items } in component instantiation blocks
            just('@')
                .ignore_then(common::ident())
                .then(
                    inline_items
                        .clone()
                        .delimited_by(just('{').padded(), just('}').padded()),
                )
                .map(|(name, items)| FlatItem::SlotFill(name, items)),
            // @slot marker in component definition blocks
            just("@slot").padded().to(FlatItem::SlotMarker),
            // For loop inside container children: `for item in list { ... }` or `for item, i in list { ... }`
            text::keyword("for")
                .ignore_then(common::ident())
                .then(for_inline_index)
                .then_ignore(text::keyword("in").padded())
                .then(expr.clone())
                .then(
                    inline_items
                        .clone()
                        .delimited_by(just('{').padded(), just('}').padded()),
                )
                .map(|(((var, index_var), iterable), body)| {
                    FlatItem::ForLoop(var, index_var, iterable, body)
                }),
            // Labeled inline item: label: Type [mods] [{ children }] or label[idx]: Type
            label_expr
                .clone()
                .then_ignore(just(':').padded())
                .then(type_ident.clone())
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|((((label, array_index), ty), mods), children)| {
                    FlatItem::Labeled(label, array_index, ty, mods, children)
                }),
            // Anonymous inline item: Type [mods] [{ children }]
            type_ident.clone()
                .then(modifiers.clone())
                .then(children_block.clone())
                .map(|((ty, mods), children)| FlatItem::Anonymous(ty, mods, children)),
            property.clone().map_with(|p, extra: &mut MapExtra<'src, '_, &'src str, extra::Err<Rich<'src, char>>>| {
                let span = extra.span();
                FlatItem::Prop(p, ByteSpan { start: span.start, end: span.end })
            }),
            // Standalone children block: attaches to the preceding item
            inline_items
                .clone()
                .delimited_by(just('{').padded(), just('}').padded())
                .map(FlatItem::Children),
        ))
        .padded();

        let w = warnings.clone();
        flat_item
            .separated_by(just(',').padded().or_not())
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
                        }
                        FlatItem::Anonymous(ty, mods, children) => {
                            result.push(InlineItem::Anonymous {
                                ty,
                                props: Vec::new(),
                                modifiers: mods,
                                children,
                            });
                        }
                        FlatItem::Prop(p, span) => {
                            if let Some(last) = result.last_mut() {
                                match last {
                                    InlineItem::Labeled { props, .. } => props.push(p),
                                    InlineItem::Anonymous { props, .. } => props.push(p),
                                    _ => {
                                        // Property dropped: attached to SlotMarker, ForLoop, or SlotFill
                                        let prop_name = p.name.clone();
                                        let location_span = p.value_span.unwrap_or(span);
                                        w.borrow_mut().push(
                                            Diagnostic::warning(
                                                DiagnosticCode::BracedPropertySilentDrop,
                                                DiagnosticPhase::Parse,
                                                format!(
                                                    "property '{prop_name}' inside braces has no \
                                                     preceding actor to attach to; properties must \
                                                     be declared before the children block, e.g. \
                                                     'Type, {prop_name}: value'"
                                                ),
                                            )
                                            .with_location(0, 0, location_span.start..location_span.end),
                                        );
                                    }
                                }
                            } else {
                                // Property dropped: no preceding actor at all
                                let prop_name = p.name.clone();
                                let location_span = p.value_span.unwrap_or(span);
                                w.borrow_mut().push(
                                    Diagnostic::warning(
                                        DiagnosticCode::BracedPropertySilentDrop,
                                        DiagnosticPhase::Parse,
                                        format!(
                                            "property '{prop_name}' inside braces has no \
                                             preceding actor to attach to; properties must \
                                             be declared before the children block, e.g. \
                                             'Type, {prop_name}: value'"
                                        ),
                                    )
                                    .with_location(0, 0, location_span.start..location_span.end),
                                );
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
                        FlatItem::ForLoop(var, index_var, iterable, body) => {
                            result.push(InlineItem::ForLoop {
                                var,
                                index_var,
                                iterable,
                                body,
                            });
                        }
                        FlatItem::SlotFill(name, items) => {
                            result.push(InlineItem::SlotFill {
                                slot: name,
                                items,
                            });
                        }
                    }
                }
                result
            })
    });

    inline_items.boxed()
}
