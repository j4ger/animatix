//!
//! Inline item helpers for the Animatix parser.
//!
//! This module provides types and helper functions used when parsing
//! inline items inside actor declaration blocks (`{ ... }`).
//! The actual combinator construction lives in [`super::parser()`].

use crate::ast::*;

/// Internal flat representation of items parsed inside an actor block.
///
/// During parsing, inline items are first collected as a flat sequence
/// of `FlatItem` variants, then assembled into `InlineItem` trees.
#[derive(Clone)]
pub(crate) enum FlatItem {
    /// `label: Type [modifiers] [{ children }]`
    Labeled(String, String, Vec<Modifier>, Vec<InlineItem>),
    /// `Type [modifiers] [{ children }]`
    Anonymous(String, Vec<Modifier>, Vec<InlineItem>),
    /// `name: value`
    Prop(Property),
    /// `{ children }` — attaches to preceding item
    Children(Vec<InlineItem>),
    /// `@slot` in component definition blocks
    SlotMarker,
    /// `@slotname { items }` in component instantiation blocks
    SlotFill(String, Vec<InlineItem>),
}