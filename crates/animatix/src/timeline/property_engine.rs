//! # Property Engine
//!
//! Generic functions that replace the N×M match-block explosion.
//!
//! Instead of 7 separate `match prop.name.as_str()` blocks spread across
//! build.rs, assignments.rs, declarations_text.rs, media.rs, and runtime.rs,
//! all property dispatch goes through three functions:
//!
//! 1. `write_property_field()` — write a parsed value to the correct track field
//! 2. `parse_property_value()` — convert an Expr to a PropertyValue by ValueType
//! 3. `inject_property_into_env()` — inject a property's value into the runtime env
//!
//! ## Usage
//!
//! ```ignore
//! let schema = lookup_property("color").unwrap();
//! let value = parse_property_value(schema.value_type, &expr, env, diag, subject);
//! if let Some(value) = value {
//!     write_property_field(track, schema.field, value, t_start_ms, t_end_ms, easing, diag);
//! }
//! ```
//!
//! This replaces what was previously:
//! ```ignore
//! match prop.name.as_str() {
//!     "color" => {
//!         // parse color
//!         // snapshot start if duration > 0
//!         // preserve if delay > 0
//!         // write end keyframe
//!     }
//!     // repeat for every property
//! }
//! ```

use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_registry::{ActorField, PropertySchema, ValueType};
use crate::timeline::{
    AnimationTrack, PropertyTrack, ShapeType, TrackAccessor,
};

// Sibling module imports (accessible via super:: because we're a child of timeline)
use super::{
    preserve_instant_delayed_value,
};

// ─────────────────────────────────────────────────────────────
// Parsed property values
// ─────────────────────────────────────────────────────────────

/// A parsed property value, typed by the ValueType that produced it.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    /// Single 32-bit float.
    F32(f32),
    /// Single 32-bit unsigned integer.
    U32(u32),
    /// 2-component vector.
    Vec2([f32; 2]),
    /// 4-component vector.
    Vec4([f32; 4]),
    /// List of 2D points.
    PointList(Vec<[f32; 2]>),
    /// SVG path command string.
    CommandList(String),
    /// RGBA color.
    Color([f32; 4]),
    /// Arbitrary string.
    String(String),
    /// Placement mode for layout.
    PlacementMode(super::PlacementMode),
    /// Options controlling path morphing.
    MorphOptions(super::MorphOptions),
    /// 2D affine transform matrix (6 components).
    Transform([f32; 6]),
}

// ─────────────────────────────────────────────────────────────
// Parse: Expr → PropertyValue
// ─────────────────────────────────────────────────────────────

/// Parse an `Expr` into a `PropertyValue` based on the expected `ValueType`.
/// Returns `None` when parsing fails (the caller should skip the property).
///
/// This function delegates to `value_parser::parse_value()` which contains
/// the full per-type parsing logic.
pub(crate) fn parse_property_value(
    value_type: ValueType,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    super::value_parser::parse_value(value_type, expr, env, diagnostics, subject)
}

// ─────────────────────────────────────────────────────────────
// Write: ActorField + PropertyValue + timing → keyframes
// ─────────────────────────────────────────────────────────────

/// Write a parsed property value to the correct track field,
/// handling keyframe timing (snapshot start, preserve delayed, write end).
///
/// This is the central dispatch that replaces the repetitive per-property
/// pattern in build.rs, assignments.rs, and declarations_text.rs.
///
/// The dispatch works in two tiers:
/// 1. Special cases that need custom extraction/conversion (ShapeType, no-ops, group diagnostics)
/// 2. Uniform dispatch via `ActorField::default_value()` + `TrackFieldMut` for standard fields
pub(crate) fn write_property_field(
    track: &mut AnimationTrack,
    field: ActorField,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let has_duration = t_end_ms > t_start_ms;
    let has_delay = t_start_ms > 0 && !has_duration;

    // ── Tier 1: Special cases ──
    match field {
        // ShapeType needs U32 -> ShapeType conversion
        ActorField::ShapeType => {
            if let PropertyValue::U32(v) = value {
                let st = ShapeType::from(v);
                write_shape_type(
                    &mut track.shape_type, st, t_start_ms, t_end_ms, easing,
                    ShapeType::Rect, has_duration, has_delay,
                );
            }
            return;
        }
        // No-ops: set via other means, not keyframed directly
        ActorField::PlacementMode
        | ActorField::PositionBinding
        | ActorField::MorphOptions
        | ActorField::VectorPaths
        | ActorField::TextPaths
        | ActorField::ImageData
        | ActorField::SvgPaths
        | ActorField::AudioSource
        | ActorField::AudioVolume => return,
        // Group fields: produce a diagnostic
        ActorField::PositionBindingGroup
        | ActorField::VectorShapeGroup
        | ActorField::PlotDomainGroup
        | ActorField::ContainerLayoutGroup => {
            let field_name = match field {
                ActorField::PositionBindingGroup => "position",
                ActorField::VectorShapeGroup => "shape",
                ActorField::PlotDomainGroup => "plot domain",
                ActorField::ContainerLayoutGroup => "layout",
                _ => unreachable!(),
            };
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Cannot set '{}' directly; use its individual properties instead.",
                        field_name
                    ),
                )
            );
            return;
        }
        _ => {} // Fall through to tier 2
    }

    // ── Tier 2: Uniform dispatch via TrackFieldMut ──
    use crate::timeline::track::TrackFieldMut;
    let pv_default = ActorField::default_value(field);
    if let Some(tf) = track.field_mut(field) {
        match tf {
            TrackFieldMut::F32(f) => {
                let default = match pv_default {
                    Some(PropertyValue::F32(d)) => d,
                    _ => 0.0,
                };
                write_f32(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::Vec2(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Vec2(d)) => d,
                    _ => [0.0, 0.0],
                };
                write_vec2(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::Vec4(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Vec4(d)) => d,
                    _ => [1.0, 1.0, 1.0, 1.0],
                };
                write_vec4(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::Transform(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Transform(d)) => d,
                    _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                };
                write_transform(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::String(f) => {
                let default = match pv_default {
                    Some(PropertyValue::String(d)) => d,
                    _ => String::new(),
                };
                write_string(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::U32(f) => {
                let default = match pv_default {
                    Some(PropertyValue::U32(d)) => d,
                    _ => 0,
                };
                // U32 uses write_f32 under the hood (PropertyValue uses F32 for numeric keyframes)
                // Handle by extracting and writing via the f32 track
                if let PropertyValue::F32(v) = value {
                    let v_u32 = v.max(0.0) as u32;
                    if has_duration {
                        let start_val = f.get(t_start_ms, default);
                        f.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if has_delay {
                        preserve_instant_delayed_value(f, t_start_ms);
                    }
                    f.ensure(default).add_keyframe(t_end_ms, v_u32, easing);
                } else if let PropertyValue::U32(v) = value {
                    if has_duration {
                        let start_val = f.get(t_start_ms, default);
                        f.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if has_delay {
                        preserve_instant_delayed_value(f, t_start_ms);
                    }
                    f.ensure(default).add_keyframe(t_end_ms, v, easing);
                }
            }
            TrackFieldMut::PointList(f) => {
                let default = match pv_default {
                    Some(PropertyValue::PointList(d)) => d,
                    _ => Vec::new(),
                };
                write_point_list(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            TrackFieldMut::CommandList(f) => {
                let default = match pv_default {
                    Some(PropertyValue::CommandList(d)) => d,
                    _ => String::new(),
                };
                write_command_list(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            }
            // PlacementMode and MorphOptions are handled in tier 1 (no-ops),
            // but field_mut returns them, so this arm is here for exhaustiveness.
            TrackFieldMut::PlacementMode(_) | TrackFieldMut::MorphOptions(_) => {}
            // ShapeType is handled in tier 1 above
            TrackFieldMut::ShapeType(_) => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Internal write helpers
// ─────────────────────────────────────────────────────────────

pub(crate) fn write_f32(
    field: &mut Option<PropertyTrack<f32>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: f32,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::F32(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default);
        field.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_vec2(
    field: &mut Option<PropertyTrack<[f32; 2]>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: [f32; 2],
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::Vec2(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default);
        field.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_vec4(
    field: &mut Option<PropertyTrack<[f32; 4]>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: [f32; 4],
    has_duration: bool,
    has_delay: bool,
) {
    let v = match value {
        PropertyValue::Vec4(v) => v,
        PropertyValue::Color(v) => v,
        _ => return,
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default);
        field.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_transform(
    field: &mut Option<PropertyTrack<[f32; 6]>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: [f32; 6],
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::Transform(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default);
        field.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_point_list(
    field: &mut Option<PropertyTrack<Vec<[f32; 2]>>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: Vec<[f32; 2]>,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::PointList(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field.ensure(default.clone()).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_command_list(
    field: &mut Option<PropertyTrack<String>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: String,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::CommandList(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field.ensure(default.clone()).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_shape_type(
    field: &mut Option<PropertyTrack<ShapeType>>,
    value: ShapeType,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: ShapeType,
    has_duration: bool,
    has_delay: bool,
) {
    if has_duration {
        let start_val = field.get(t_start_ms, default);
        field.ensure(default).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, value, easing);
}

pub(crate) fn write_string(
    field: &mut Option<PropertyTrack<String>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: String,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::String(v) = value else { return };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field.ensure(default.clone()).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

// ─────────────────────────────────────────────────────────────
// Read: ActorField + time_ms → PropertyValue
// ─────────────────────────────────────────────────────────────

/// Read the current value of a property from a track at the given time.
/// Returns `None` if the property has no track (not set on this actor).
pub fn read_property_value(track: &AnimationTrack, field: ActorField, time_ms: u64) -> Option<PropertyValue> {
    read_property_value_inner(track, field, time_ms)
}

fn read_property_value_inner(track: &AnimationTrack, field: ActorField, time_ms: u64) -> Option<PropertyValue> {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).and_then(|f| match f {
        // Copy types — use evaluate_copy to avoid cloning
        TrackFieldRef::F32(opt) => opt.as_ref().map(|pt| PropertyValue::F32(pt.evaluate_copy(time_ms))),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map(|pt| PropertyValue::Vec2(pt.evaluate_copy(time_ms))),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map(|pt| PropertyValue::Color(pt.evaluate_copy(time_ms))),
        TrackFieldRef::Transform(opt) => opt.as_ref().map(|pt| PropertyValue::Transform(pt.evaluate_copy(time_ms))),
        TrackFieldRef::U32(opt) => opt.as_ref().map(|pt| PropertyValue::U32(pt.evaluate_copy(time_ms))),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().map(|pt| PropertyValue::U32(shape_type_to_u32(pt.evaluate_copy(time_ms)))),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().map(|pt| PropertyValue::PlacementMode(pt.evaluate_copy(time_ms))),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().map(|pt| PropertyValue::MorphOptions(pt.evaluate_copy(time_ms))),
        // Clone types — keep using evaluate
        TrackFieldRef::String(opt) => opt.as_ref().map(|pt| PropertyValue::String(pt.evaluate(time_ms))),
        TrackFieldRef::PointList(opt) => opt.as_ref().map(|pt| PropertyValue::PointList(pt.evaluate(time_ms))),
        TrackFieldRef::CommandList(opt) => opt.as_ref().map(|pt| PropertyValue::CommandList(pt.evaluate(time_ms))),
    })
}

/// Read a property value, falling back to the schema default if the track
/// has no value for this property.
pub fn read_property_value_or_default(
    track: &AnimationTrack,
    schema: &PropertySchema,
    time_ms: u64,
) -> PropertyValue {
    read_property_value(track, schema.field, time_ms)
        .unwrap_or_else(|| (schema.default_value)(track.kind))
}

fn shape_type_to_u32(st: ShapeType) -> u32 {
    match st {
        ShapeType::Rect => 0,
        ShapeType::Ellipse => 1,
        ShapeType::Line => 2,
        ShapeType::Polygon => 3,
        ShapeType::Path => 4,
        ShapeType::Graph => 5,
        ShapeType::Plot => 6,
        ShapeType::Arrow => 7,
    }
}

// ─────────────────────────────────────────────────────────────
// Keyframe introspection
// ─────────────────────────────────────────────────────────────

/// Returns whether a property has any keyframes on the given track.
pub fn property_has_keyframes(track: &AnimationTrack, field: ActorField) -> bool {
    property_keyframe_count(track, field) > 0
}

/// Returns whether a property has a keyframe at exactly the given time.
pub fn property_has_keyframe_at(track: &AnimationTrack, field: ActorField, time_ms: u64) -> bool {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).is_some_and(|f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::Vec2(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::Vec4(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::Transform(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::String(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::U32(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::PointList(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::CommandList(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
    })
}

/// Returns the number of keyframes for a property on the given track.
pub fn property_keyframe_count(track: &AnimationTrack, field: ActorField) -> usize {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).map_or(0, |f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::Transform(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::String(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::U32(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::PointList(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::CommandList(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
    })
}

/// Returns all keyframe times (in ms) for a property, sorted.
pub fn property_keyframe_times(track: &AnimationTrack, field: ActorField) -> Vec<u64> {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).map_or(Vec::new(), |f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::Transform(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::String(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::U32(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::PointList(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::CommandList(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
    })
}

/// Returns the easing at a specific keyframe time for a property.
pub fn property_keyframe_easing(track: &AnimationTrack, field: ActorField, time_ms: u64) -> Option<Easing> {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).and_then(|f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::Vec2(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::Vec4(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::Transform(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::String(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::U32(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::PointList(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::CommandList(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
    })
}

// ─────────────────────────────────────────────────────────────
// Environment injection
// ─────────────────────────────────────────────────────────────

/// Inject a property's value into the runtime Environment.
///
/// Iterates over the property registry and injects every INJECTABLE property
/// that has a direct track storage field.  Derived values (width, height,
/// radius, at alias, shift, line_cap, line_join) are handled afterwards.
///
/// This is called from `inject_runtime_lookup_values()` for every actor,
/// every frame.
pub(crate) fn inject_property_into_env(
    env: &mut Environment,
    label: &str,
    track: &AnimationTrack,
    time_ms: u64,
) {
    use crate::timeline::property_registry::{PROPERTY_REGISTRY, PropertyFlags, ReadSource};

    // Reusable key buffer to avoid repeated small String allocations.
    let mut key = String::with_capacity(label.len() + 24);
    key.push_str(label);
    key.push('.');
    let prefix_len = key.len();

    // ── Inject every registered INJECTABLE property via its read_source ──
    for schema in PROPERTY_REGISTRY.iter() {
        if !schema.flags.contains(PropertyFlags::INJECTABLE) {
            continue;
        }

        // Read via the schema's read_source (handles Field, Alias, Component).
        let pv = match schema.read_source.read(track, time_ms) {
            Some(pv) => pv,
            None => {
                // No track value — fall back to schema default.
                // For Component sources, extract the indexed component.
                let write_default = (schema.default_value)(track.kind);
                match schema.read_source {
                    ReadSource::Field(_) | ReadSource::Alias(_) => write_default,
                    ReadSource::Component { index, scale, .. } => {
                        if let PropertyValue::Vec2(v) = write_default {
                            PropertyValue::F32(v[index] * scale as f32)
                        } else {
                            write_default
                        }
                    }
                    ReadSource::None_ => continue,
                }
            }
        };

        // Inject the value and typed sub-keys (x, y, r, g, b, a).
        key.truncate(prefix_len);
        key.push_str(schema.name);
        inject_value(env, &mut key, prefix_len, schema.name, &pv);

        // Inject the _animating_* flag from the read_source's storage field.
        if let Some(storage) = schema.read_source.storage_field() {
            key.truncate(prefix_len);
            key.push_str("_animating_");
            key.push_str(schema.name);
            let animating = track.is_field_currently_animating(storage, time_ms);
            env.set(&key, Value::Num(if animating { 1.0 } else { 0.0 }));
        }
    }
}

/// Inject a `PropertyValue` into the environment, with typed sub-keys
/// (`.x`, `.y` for Vec2; `.r`, `.g`, `.b`, `.a` for Color/Vec4).
///
/// `key` must be positioned at the end of the property name (the full
/// `{label}.{property}` prefix).  It is restored to `prefix_len + name.len()`
/// after injection so that callers can continue using the buffer.
fn inject_value(env: &mut Environment, key: &mut String, prefix_len: usize, name: &str, value: &PropertyValue) {
    let restore = prefix_len + name.len();
    match value {
        PropertyValue::F32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        }
        PropertyValue::U32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        }
        PropertyValue::Vec2(v) => {
            env.set(&*key, Value::Vec2([v[0] as f64, v[1] as f64]));
            key.push_str(".x");
            env.set(&*key, Value::Num(v[0] as f64));
            key.truncate(restore);
            key.push_str(".y");
            env.set(&*key, Value::Num(v[1] as f64));
        }
        PropertyValue::Vec4(v) | PropertyValue::Color(v) => {
            env.set(&*key, Value::Color([v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64]));
            key.push_str(".r");
            env.set(&*key, Value::Num(v[0] as f64));
            key.truncate(restore);
            key.push_str(".g");
            env.set(&*key, Value::Num(v[1] as f64));
            key.truncate(restore);
            key.push_str(".b");
            env.set(&*key, Value::Num(v[2] as f64));
            key.truncate(restore);
            key.push_str(".a");
            env.set(&*key, Value::Num(v[3] as f64));
        }
        PropertyValue::Transform(v) => {
            env.set(&*key, Value::List(vec![
                Value::Num(v[0] as f64),
                Value::Num(v[1] as f64),
                Value::Num(v[2] as f64),
                Value::Num(v[3] as f64),
                Value::Num(v[4] as f64),
                Value::Num(v[5] as f64),
            ]));
        }
        PropertyValue::String(v) => {
            env.set(&*key, Value::Str(v.clone()));
        }
        _ => {}
    }
    key.truncate(restore);
}

// ─────────────────────────────────────────────────────────────
// Override-aware property reads
// ─────────────────────────────────────────────────────────────

/// Read an effective f32 property value, preferring modifier overrides
/// over track keyframes. Uses the property registry to map `name` to its
/// storage field for the track fallback.
pub(crate) fn effective_f32(
    track: &AnimationTrack,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    name: &str,
    default: f32,
) -> f32 {
    if let Some(Value::Num(v)) = overrides.and_then(|ov| ov.get(name)) {
        return *v as f32;
    }
    if let Some(schema) = crate::timeline::property_registry::lookup_property(name)
        && let Some(pv) = read_property_value(track, schema.field, time_ms)
        && let PropertyValue::F32(v) = pv
    {
        return v;
    }
    default
}

/// Read an effective Vec2 property value, preferring modifier overrides
/// over track keyframes.
pub(crate) fn effective_vec2(
    track: &AnimationTrack,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    name: &str,
    default: [f32; 2],
) -> [f32; 2] {
    if let Some(Value::Vec2(v)) = overrides.and_then(|ov| ov.get(name)) {
        return [v[0] as f32, v[1] as f32];
    }
    if let Some(schema) = crate::timeline::property_registry::lookup_property(name)
        && let Some(pv) = read_property_value(track, schema.field, time_ms)
        && let PropertyValue::Vec2(v) = pv
    {
        return v;
    }
    default
}

/// Read an effective Transform property value ([f32; 6]), preferring modifier
/// overrides over track keyframes.
pub(crate) fn effective_transform(
    track: &AnimationTrack,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    name: &str,
    default: [f32; 6],
) -> [f32; 6] {
    if let Some(Value::List(items)) = overrides.and_then(|ov| ov.get(name)) {
        if items.len() == 6 {
            let mut t = default;
            for (i, item) in items.iter().enumerate() {
                t[i] = item.as_num() as f32;
            }
            return t;
        }
    }
    if let Some(schema) = crate::timeline::property_registry::lookup_property(name)
        && let Some(pv) = read_property_value(track, schema.field, time_ms)
        && let PropertyValue::Transform(v) = pv
    {
        return v;
    }
    default
}
