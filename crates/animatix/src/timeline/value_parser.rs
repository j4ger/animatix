//! # Value Parser
//!
//! Generic `Expr → PropertyValue` dispatch driven by `ValueType`.
//!
//! This module is currently a placeholder — it will be activated in Phase 3
//! (generic engine switch-over) when the old match blocks are migrated.
//!
//! For now, parsing is handled by `property_engine.rs::parse_property_value()`.

#![allow(dead_code)]

use crate::timeline::property_registry::ValueType;

pub enum PropertyValue {
    F32(f32),
    U32(u32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    PointList(Vec<[f32; 2]>),
    Color([f32; 4]),
    String(String),
}

pub(crate) fn parse_value(
    _value_type: ValueType,
) -> Option<PropertyValue> {
    None
}
