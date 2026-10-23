//! Parsed property values: the generic [`PropertyValue`] representation, the
//! enum bridge, interpolation, and expression-to-value conversion.

use crate::ast::Expr;
use crate::diagnostics::Diagnostic;
use crate::renderer::types::GradientSpec;
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_registry::ValueType;
use crate::timeline::{Interpolate, ShapeType};

// ─────────────────────────────────────────────────────────────
// Parsed property values
// ─────────────────────────────────────────────────────────────

/// A parsed property value, typed by the ValueType that produced it.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PropertyValue {
    /// Single 32-bit float.
    F32(f32),
    /// Boolean flag.
    Bool(bool),
    /// Single 32-bit unsigned integer.
    U32(u32),
    /// 2-component vector.
    Vec2([f32; 2]),
    /// 4-component vector.
    Vec4([f32; 4]),
    /// List of 2D points.
    PointList(Vec<[f32; 2]>),
    /// List of floats (stroke dash pattern, in scene pixels).
    F32List(Vec<f32>),
    /// A color ramp painted by `fill_gradient:` / `stroke_gradient:`.
    Gradient(GradientSpec),
    /// SVG path command string.
    CommandList(String),
    /// RGBA color.
    Color([f32; 4]),
    /// Arbitrary string.
    String(String),
    /// List of strings used for reordering/selection data.
    StringList(Vec<String>),
    /// 2D affine transform matrix (6 components).
    Transform([f32; 6]),
    /// A fixed choice selected from `ValueType::Enum`.
    Enum(String),
    /// A named variant with a payload, produced by `ValueType::Sum`.
    Variant {
        /// Canonical variant name.
        name: String,
        /// Parsed payload value.
        value: Box<PropertyValue>,
    },
}

/// Tolerance for the loop-seam comparison: below this, a jump between the last
/// and first frame of a replayed scene is not visible.
const LOOP_SEAM_EPSILON: f32 = 1e-3;

/// True when two sampled values are close enough that replaying a scene
/// end-to-start shows no jump.
pub(crate) fn values_wrap(a: &PropertyValue, b: &PropertyValue) -> bool {
    let close = |x: f32, y: f32| (x - y).abs() <= LOOP_SEAM_EPSILON;
    fn close_list(l: &[f32], r: &[f32]) -> bool {
        l.len() == r.len() && l.iter().zip(r).all(|(a, b)| (a - b).abs() <= LOOP_SEAM_EPSILON)
    }
    match (a, b) {
        (PropertyValue::F32(x), PropertyValue::F32(y)) => close(*x, *y),
        (PropertyValue::U32(x), PropertyValue::U32(y)) => x == y,
        (PropertyValue::Bool(x), PropertyValue::Bool(y)) => x == y,
        (PropertyValue::Vec2(x), PropertyValue::Vec2(y)) => close(x[0], y[0]) && close(x[1], y[1]),
        (PropertyValue::Vec4(x), PropertyValue::Vec4(y))
        | (PropertyValue::Color(x), PropertyValue::Color(y)) => {
            x.iter().zip(y).all(|(l, r)| close(*l, *r))
        },
        (PropertyValue::PointList(x), PropertyValue::PointList(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|(l, r)| l.len() == r.len() && close_list(&[l[0], l[1]], &[r[0], r[1]]))
        },
        (PropertyValue::F32List(x), PropertyValue::F32List(y)) => {
            x.len() == y.len() && close_list(x, y)
        },
        (PropertyValue::Gradient(x), PropertyValue::Gradient(y)) => {
            x.stops.len() == y.stops.len()
                && x.stops
                    .iter()
                    .zip(&y.stops)
                    .all(|(l, r)| close(l.offset, r.offset) && l.color == r.color)
        },
        (PropertyValue::String(x), PropertyValue::String(y))
        | (PropertyValue::CommandList(x), PropertyValue::CommandList(y))
        | (PropertyValue::Enum(x), PropertyValue::Enum(y)) => x == y,
        (PropertyValue::StringList(x), PropertyValue::StringList(y)) => x == y,
        (PropertyValue::Transform(x), PropertyValue::Transform(y)) => {
            x.iter().zip(y).all(|(l, r)| close(*l, *r))
        },
        (
            PropertyValue::Variant {
                name: n1,
                value: v1,
            },
            PropertyValue::Variant {
                name: n2,
                value: v2,
            },
        ) => n1 == n2 && values_wrap(v1, v2),
        // Different kinds at the two ends of a looping scene is itself a jump.
        _ => false,
    }
}

/// Stable boundary between typed internal enums and generic property values.
///
/// Internal enums stay typed in their tracks, but any code that needs a
/// schema-driven representation can cross through `to_property_value()` without
/// matching on the concrete Rust enum.
pub trait EnumPropertyValue: Copy + PartialEq + std::fmt::Debug {
    /// Stable name of this value.
    fn name(&self) -> &'static str;

    /// Parse a stable name back into the typed value.
    fn from_name(name: &str) -> Option<Self>;

    /// Convert to the generic property-value representation.
    fn to_property_value(&self) -> PropertyValue {
        PropertyValue::Enum(self.name().to_string())
    }
}

impl EnumPropertyValue for ShapeType {
    fn name(&self) -> &'static str {
        self.as_str()
    }

    fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }
}

impl EnumPropertyValue for crate::timeline::PlacementMode {
    fn name(&self) -> &'static str {
        self.as_str()
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::from_str(name)
    }
}

impl EnumPropertyValue for crate::timeline::animation_track::CalloutPlace {
    fn name(&self) -> &'static str {
        self.as_str()
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::from_str(name)
    }
}

impl Interpolate for PropertyValue {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        match (self, other) {
            (PropertyValue::F32(a), PropertyValue::F32(b)) => {
                PropertyValue::F32(a.interpolate(b, t))
            },
            (PropertyValue::U32(a), PropertyValue::U32(b)) => {
                PropertyValue::U32(a.interpolate(b, t))
            },
            (PropertyValue::Vec2(a), PropertyValue::Vec2(b)) => {
                PropertyValue::Vec2(a.interpolate(b, t))
            },
            (PropertyValue::Vec4(a), PropertyValue::Vec4(b)) => {
                PropertyValue::Vec4(a.interpolate(b, t))
            },
            (PropertyValue::Color(a), PropertyValue::Color(b)) => {
                PropertyValue::Color(a.interpolate(b, t))
            },
            (PropertyValue::Transform(a), PropertyValue::Transform(b)) => {
                PropertyValue::Transform(a.interpolate(b, t))
            },
            (PropertyValue::PointList(a), PropertyValue::PointList(b)) => {
                PropertyValue::PointList(a.interpolate(b, t))
            },
            (PropertyValue::F32List(a), PropertyValue::F32List(b)) => {
                PropertyValue::F32List(a.interpolate(b, t))
            },
            (PropertyValue::Gradient(a), PropertyValue::Gradient(b)) => {
                PropertyValue::Gradient(a.interpolate(b, t))
            },
            (PropertyValue::String(a), PropertyValue::String(b)) => {
                PropertyValue::String(a.interpolate(b, t))
            },
            (PropertyValue::StringList(a), PropertyValue::StringList(b)) => {
                if t < 0.5 {
                    PropertyValue::StringList(a.clone())
                } else {
                    PropertyValue::StringList(b.clone())
                }
            },
            (PropertyValue::CommandList(a), PropertyValue::CommandList(b)) => {
                PropertyValue::CommandList(a.interpolate(b, t))
            },
            (
                PropertyValue::Variant {
                    name: a_name,
                    value: a_value,
                },
                PropertyValue::Variant {
                    name: b_name,
                    value: b_value,
                },
            ) if a_name == b_name => PropertyValue::Variant {
                name: a_name.clone(),
                value: Box::new(a_value.as_ref().interpolate(b_value.as_ref(), t)),
            },
            _ => {
                if t < 0.5 {
                    self.clone()
                } else {
                    other.clone()
                }
            },
        }
    }
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
    crate::timeline::value_parser::parse_value(value_type, expr, env, diagnostics, subject)
}

pub(super) fn property_value_from_value(value: Value) -> Option<PropertyValue> {
    if matches!(value, Value::PropRef { .. }) {
        // A property reference is not assignable as a value.
        return None;
    }
    Some(match value {
        Value::PropRef { .. } => unreachable!("filtered above"),
        Value::Num(n) => PropertyValue::F32(n as f32),
        Value::Str(s) => PropertyValue::String(s),
        Value::Bool(b) => PropertyValue::Bool(b),
        Value::Vec2(v) => PropertyValue::Vec2([v[0] as f32, v[1] as f32]),
        Value::Vec3(v) => PropertyValue::Vec4([v[0] as f32, v[1] as f32, v[2] as f32, 1.0]),
        Value::Vec4(v) | Value::Color(v) => {
            PropertyValue::Color([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
        },
        Value::List(items) => {
            if items.iter().all(|item| matches!(item, Value::Num(_))) {
                PropertyValue::F32List(
                    items
                        .iter()
                        .map(|item| {
                            let Value::Num(n) = item else {
                                unreachable!("filtered above");
                            };
                            *n as f32
                        })
                        .collect(),
                )
            } else if items.iter().all(|item| matches!(item, Value::Vec2(_))) {
                PropertyValue::PointList(
                    items
                        .iter()
                        .map(|item| {
                            let Value::Vec2(v) = item else {
                                unreachable!("filtered above");
                            };
                            [v[0] as f32, v[1] as f32]
                        })
                        .collect(),
                )
            } else if items.iter().all(|item| matches!(item, Value::Str(_))) {
                PropertyValue::StringList(
                    items
                        .iter()
                        .map(|item| {
                            let Value::Str(s) = item else {
                                unreachable!("filtered above");
                            };
                            s.clone()
                        })
                        .collect(),
                )
            } else {
                return None;
            }
        },
        Value::Object(_, _)
        | Value::NativeFn(_)
        | Value::Closure(_, _, _)
        | Value::UserFn { .. } => return None,
    })
}
