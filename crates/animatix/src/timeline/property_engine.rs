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
use crate::timeline::property_registry::{ActorField, ValueType};
use crate::timeline::{
    AnimationTrack, PropertyTrack, ShapeType, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE,
};

// Sibling module imports (accessible via super:: because we're a child of timeline)
use super::{
    evaluate_expr_with_lookup_diagnostic, parse_color_in_env_with_lookup_diagnostic,
    preserve_instant_delayed_value,
};

// ─────────────────────────────────────────────────────────────
// Parsed property values
// ─────────────────────────────────────────────────────────────

/// A parsed property value, typed by the ValueType that produced it.
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    F32(f32),
    U32(u32),
    Vec2([f32; 2]),
    Vec4([f32; 4]),
    PointList(Vec<[f32; 2]>),
    CommandList(String),
    Color([f32; 4]),
    String(String),
    PlacementMode(super::PlacementMode),
    MorphOptions(super::MorphOptions),
}

// ─────────────────────────────────────────────────────────────
// Parse: Expr → PropertyValue
// ─────────────────────────────────────────────────────────────

/// Parse an `Expr` into a `PropertyValue` based on the expected `ValueType`.
/// Returns `None` when parsing fails (the caller should skip the property).
pub(crate) fn parse_property_value(
    value_type: ValueType,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    match value_type {
        ValueType::F32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            Some(PropertyValue::F32(v.as_num() as f32))
        }
        ValueType::U32 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            let n = v.as_num();
            Some(PropertyValue::U32(n.max(0.0) as u32))
        }
        ValueType::Vec2 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec2([x, y]) => Some(PropertyValue::Vec2([x as f32, y as f32])),
                _ => None,
            }
        }
        ValueType::Vec4 => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            match v {
                Value::Vec4([a, b, c, d]) => Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32])),
                Value::Color([a, b, c, d]) => Some(PropertyValue::Vec4([a as f32, b as f32, c as f32, d as f32])),
                _ => None,
            }
        }
        ValueType::Color => {
            parse_color_in_env_with_lookup_diagnostic("", "color", expr, env, diagnostics, subject)
                .map(PropertyValue::Color)
        }
        ValueType::String => {
            let v = evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
            Some(PropertyValue::String(v.as_str()))
        }
        ValueType::PointList => {
            // Expect an Expr::Tuple of Expr::Tuple[Expr::Num, Expr::Num]
            if let Expr::Tuple(items) = expr {
                let mut points = Vec::with_capacity(items.len());
                for item in items {
                    if let Expr::Tuple(pair) = item {
                        if pair.len() == 2 {
                            if let (Expr::Num(x), Expr::Num(y)) = (&pair[0], &pair[1]) {
                                points.push([*x as f32, *y as f32]);
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                Some(PropertyValue::PointList(points))
            } else {
                None
            }
        }
        ValueType::CommandList => {
            crate::timeline::parse_path_commands_expr(expr, env)
                .map(|path| PropertyValue::CommandList(path.to_svg()))
        }
        // These types require context-specific handling (group resolution)
        ValueType::ShapeType
        | ValueType::PlacementMode
        | ValueType::SceneAnchor
        | ValueType::PositionBinding
        | ValueType::MorphOptions
        | ValueType::BuildTimeOnly => None,
    }
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
        TrackFieldRef::F32(opt) => opt.as_ref().map(|pt| PropertyValue::F32(pt.evaluate(time_ms))),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map(|pt| PropertyValue::Vec2(pt.evaluate(time_ms))),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map(|pt| PropertyValue::Color(pt.evaluate(time_ms))),
        TrackFieldRef::String(opt) => opt.as_ref().map(|pt| PropertyValue::String(pt.evaluate(time_ms))),
        TrackFieldRef::U32(opt) => opt.as_ref().map(|pt| PropertyValue::U32(pt.evaluate(time_ms))),
        TrackFieldRef::PointList(opt) => opt.as_ref().map(|pt| PropertyValue::PointList(pt.evaluate(time_ms))),
        TrackFieldRef::CommandList(opt) => opt.as_ref().map(|pt| PropertyValue::CommandList(pt.evaluate(time_ms))),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().map(|pt| PropertyValue::U32(shape_type_to_u32(pt.evaluate(time_ms)))),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().map(|pt| PropertyValue::PlacementMode(pt.evaluate(time_ms))),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().map(|pt| PropertyValue::MorphOptions(pt.evaluate(time_ms))),
    })
}

/// Read a property value, falling back to the schema default if the track
/// has no value for this property.
pub fn read_property_value_or_default(
    track: &AnimationTrack,
    field: ActorField,
    time_ms: u64,
    kind: crate::timeline::ActorKindId,
) -> PropertyValue {
    read_property_value(track, field, time_ms)
        .unwrap_or_else(|| {
            // Look up the schema for this field and return its default
            use crate::timeline::property_registry::PROPERTY_REGISTRY;
            for schema in PROPERTY_REGISTRY.iter() {
                if schema.field == field {
                    return (schema.default_value)(kind);
                }
            }
            // Fallback for unknown fields
            PropertyValue::F32(0.0)
        })
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
    track.field_ref(field).map_or(false, |f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::String(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::U32(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::PointList(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::CommandList(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::ShapeType(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::PlacementMode(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
        TrackFieldRef::MorphOptions(opt) => opt.as_ref().map_or(false, |pt| pt.keyframes.contains_key(&time_ms)),
    })
}

/// Returns the number of keyframes for a property on the given track.
pub fn property_keyframe_count(track: &AnimationTrack, field: ActorField) -> usize {
    use crate::timeline::track::TrackFieldRef;
    track.field_ref(field).map_or(0, |f| match f {
        TrackFieldRef::F32(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::Vec2(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        TrackFieldRef::Vec4(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
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
/// This is called from `inject_runtime_lookup_values()` for every
/// INJECTABLE property on every actor, every frame.
pub(crate) fn inject_property_into_env(
    env: &mut Environment,
    label: &str,
    track: &AnimationTrack,
    time_ms: u64,
) {
    // Geometry
    inject_vec2_env(env, label, "at",        &track.position, time_ms, [0.0, 0.0]);
    inject_vec2_env(env, label, "position",  &track.position, time_ms, [0.0, 0.0]);
    inject_vec2_env(env, label, "shift",     &track.motion_offset, time_ms, [0.0, 0.0]);
    let size_val = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
    let full_size = [size_val[0] * 2.0, size_val[1] * 2.0];
    inject_vec2_env(env, label, "size",      &track.size, time_ms, DEFAULT_LAYOUT_HALF_SIZE);
    env.set(&format!("{label}.width"), Value::Num(full_size[0] as f64));
    env.set(&format!("{label}.height"), Value::Num(full_size[1] as f64));
    inject_scalar_env(env, label, "rotation", &track.rotation, time_ms, 0.0);
    inject_scalar_env(env, label, "scale",   &track.scale, time_ms, 1.0);

    // Style
    inject_color_env(env, label, "color",           &track.color, time_ms, DEFAULT_WHITE);
    inject_scalar_env(env, label, "opacity",        &track.opacity, time_ms, 1.0);
    inject_color_env(env, label, "stroke_color",    &track.stroke_color, time_ms, DEFAULT_WHITE);
    inject_scalar_env(env, label, "stroke_width",   &track.stroke_width, time_ms, 2.0);
    inject_scalar_env(env, label, "stroke_progress", &track.stroke_progress, time_ms, 1.0);
    inject_scalar_env(env, label, "fill_opacity",   &track.fill_opacity, time_ms, 1.0);

    // Shape-specific derived fields
    let size = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);
    env.set(&format!("{label}.radius_x"), Value::Num(size[0] as f64));
    env.set(&format!("{label}.radius_y"), Value::Num(size[1] as f64));

    // Line from/to
    inject_vec2_env(env, label, "from", &track.line_from, time_ms, [-50.0, 0.0]);
    inject_vec2_env(env, label, "to",   &track.line_to, time_ms, [50.0, 0.0]);

    // Effects
    inject_vec2_env(env, label, "shadow_offset",   &track.shadow_offset, time_ms, [0.0, 0.0]);
    inject_scalar_env(env, label, "shadow_blur",    &track.shadow_blur, time_ms, 0.0);
    inject_color_env(env, label, "shadow_color",    &track.shadow_color, time_ms, [0.0, 0.0, 0.0, 0.0]);
    inject_scalar_env(env, label, "glow_radius",    &track.glow_radius, time_ms, 0.0);
    inject_color_env(env, label, "glow_color",      &track.glow_color, time_ms, [0.0, 0.0, 0.0, 0.0]);
    inject_scalar_env(env, label, "backdrop_blur",  &track.backdrop_blur, time_ms, 0.0);

    // Animation-state flags: inject `_animating_{property}` booleans so `always`
    // blocks can detect when a keyframe track exists for a property.
    inject_scalar_animating(env, label, "at",         &track.position);
    inject_scalar_animating(env, label, "position",   &track.position);
    inject_scalar_animating(env, label, "shift",      &track.motion_offset);
    inject_scalar_animating(env, label, "size",       &track.size);
    inject_scalar_animating(env, label, "rotation",   &track.rotation);
    inject_scalar_animating(env, label, "scale",      &track.scale);
    inject_scalar_animating(env, label, "color",      &track.color);
    inject_scalar_animating(env, label, "opacity",    &track.opacity);
    inject_scalar_animating(env, label, "stroke_color", &track.stroke_color);
    inject_scalar_animating(env, label, "stroke_width", &track.stroke_width);
    inject_scalar_animating(env, label, "stroke_progress", &track.stroke_progress);
    inject_scalar_animating(env, label, "fill_opacity", &track.fill_opacity);
    inject_scalar_animating(env, label, "from",       &track.line_from);
    inject_scalar_animating(env, label, "to",         &track.line_to);
    inject_scalar_animating(env, label, "shadow_offset", &track.shadow_offset);
    inject_scalar_animating(env, label, "shadow_blur",   &track.shadow_blur);
    inject_scalar_animating(env, label, "shadow_color",  &track.shadow_color);
    inject_scalar_animating(env, label, "glow_radius",   &track.glow_radius);
    inject_scalar_animating(env, label, "glow_color",    &track.glow_color);
    inject_scalar_animating(env, label, "backdrop_blur", &track.backdrop_blur);
}

fn inject_scalar_env(
    env: &mut Environment,
    label: &str,
    key: &str,
    field: &Option<PropertyTrack<f32>>,
    time_ms: u64,
    default: f32,
) {
    let val = field.get(time_ms, default) as f64;
    env.set(&format!("{label}.{key}"), Value::Num(val));
}

fn inject_vec2_env(
    env: &mut Environment,
    label: &str,
    key: &str,
    field: &Option<PropertyTrack<[f32; 2]>>,
    time_ms: u64,
    default: [f32; 2],
) {
    let val = field.get(time_ms, default);
    let f = |x: f32| x as f64;
    env.set(&format!("{label}.{key}"), Value::Vec2([f(val[0]), f(val[1])]));
    env.set(&format!("{label}.{key}.x"), Value::Num(f(val[0])));
    env.set(&format!("{label}.{key}.y"), Value::Num(f(val[1])));
}

fn inject_color_env(
    env: &mut Environment,
    label: &str,
    key: &str,
    field: &Option<PropertyTrack<[f32; 4]>>,
    time_ms: u64,
    default: [f32; 4],
) {
    let val = field.get(time_ms, default);
    let f = |x: f32| x as f64;
    env.set(&format!("{label}.{key}"), Value::Color([f(val[0]), f(val[1]), f(val[2]), f(val[3])]));
    env.set(&format!("{label}.{key}.r"), Value::Num(f(val[0])));
    env.set(&format!("{label}.{key}.g"), Value::Num(f(val[1])));
    env.set(&format!("{label}.{key}.b"), Value::Num(f(val[2])));
    env.set(&format!("{label}.{key}.a"), Value::Num(f(val[3])));
}

/// Inject an `_animating_{key}` boolean flag (1.0 = has keyframes, 0.0 = none).
fn inject_scalar_animating<T: Clone>(
    env: &mut Environment,
    label: &str,
    key: &str,
    field: &Option<PropertyTrack<T>>,
) {
    let has_keyframes = field.as_ref().map(|t| !t.keyframes.is_empty()).unwrap_or(false);
    env.set(
        &format!("{label}._animating_{key}"),
        Value::Num(if has_keyframes { 1.0 } else { 0.0 }),
    );
}
