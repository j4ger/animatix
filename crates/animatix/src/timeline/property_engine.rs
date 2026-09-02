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

// Sibling module imports (accessible via super:: because we're a child of timeline)
use super::preserve_instant_delayed_value;
use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::extension_context::ExtensionContext;
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_registry::{ActorField, ValueType};
use crate::timeline::{AnimationTrack, Interpolate, PropertyTrack, ShapeType, TrackAccessor};
use animatix_syntax::schema::PropertyValueKind;

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

impl EnumPropertyValue for super::PlacementMode {
    fn name(&self) -> &'static str {
        self.as_str()
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::from_str(name)
    }
}

impl EnumPropertyValue for super::animation_track::CalloutPlace {
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
        // ShapeType needs U32/Enum -> ShapeType conversion
        ActorField::ShapeType => {
            let st = match value {
                PropertyValue::U32(v) => Some(ShapeType::from(v)),
                PropertyValue::Enum(name) => ShapeType::from_name(&name),
                _ => None,
            };
            if let Some(st) = st {
                write_shape_type(
                    &mut track.shape.shape_type,
                    st,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    ShapeType::Rect,
                    has_duration,
                    has_delay,
                );
            }
            return;
        },
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
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                format!(
                    "Cannot set '{}' directly; use its individual properties instead.",
                    field_name
                ),
            ));
            return;
        },
        _ => {}, // Fall through to tier 2
    }

    // ── Tier 2: Uniform dispatch via TrackFieldMut ──
    use crate::timeline::dispatch::TrackFieldMut;
    let pv_default = ActorField::default_value(field);
    let tagged_default = match field {
        ActorField::Tagged(name) => crate::timeline::property_registry::lookup_property(name)
            .map(|schema| (schema.default_value)(track.kind)),
        _ => None,
    };
    if field == ActorField::Tagged("legend") {
        track.legend.legend_declared = true;
    }

    if field == ActorField::Tagged("callout_place")
        && let PropertyValue::Enum(choice) = &value
        && let Some(place) = crate::timeline::animation_track::CalloutPlace::from_str(choice)
    {
        let callout_track = &mut track.geometry.callout_place;
        if has_duration {
            let start = callout_track
                .get(t_start_ms, crate::timeline::animation_track::CalloutPlace::Right);
            callout_track
                .ensure(crate::timeline::animation_track::CalloutPlace::Right)
                .add_keyframe(t_start_ms, start, Easing::Linear);
        } else if has_delay {
            preserve_instant_delayed_value(callout_track, t_start_ms);
        }
        callout_track
            .ensure(crate::timeline::animation_track::CalloutPlace::Right)
            .add_keyframe(t_end_ms, place, easing);
    }

    // Route registry-backed tagged properties through the actor plan before
    // falling back to the legacy tagged_tracks map. Special callout/legend
    // fields remain on their existing typed paths for now.
    if let ActorField::Tagged(name) = field
        && name != "legend"
        && name != "callout_place"
        && let Some(id) = crate::timeline::property_registry::property_id(name)
    {
        let kind = track
            .property_plan
            .get(id)
            .map(|slot| slot.kind)
            .unwrap_or(crate::timeline::PropertyKind::Generic);
        if write_property_plan_slot(track, id, kind, value.clone(), t_start_ms, t_end_ms, easing) {
            return;
        }
    }

    if let Some(tf) = track.field_mut(field) {
        match tf {
            TrackFieldMut::F32(f) => {
                let default = match pv_default {
                    Some(PropertyValue::F32(d)) => d,
                    _ => 0.0,
                };
                write_f32(f, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
            },
            TrackFieldMut::Vec2(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Vec2(d)) => d,
                    _ => [0.0, 0.0],
                };
                write_vec2(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            TrackFieldMut::Vec4(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Vec4(d)) => d,
                    _ => [1.0, 1.0, 1.0, 1.0],
                };
                write_vec4(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            TrackFieldMut::Transform(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Transform(d)) => d,
                    _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                };
                write_transform(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            TrackFieldMut::String(f) => {
                let default = match pv_default {
                    Some(PropertyValue::String(d)) => d,
                    _ => String::new(),
                };
                write_string(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
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
            },
            TrackFieldMut::PointList(f) => {
                let default = match pv_default {
                    Some(PropertyValue::PointList(d)) => d,
                    _ => Vec::new(),
                };
                write_point_list(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            TrackFieldMut::Tagged(_name, f) => {
                let default = tagged_default.clone().unwrap_or(PropertyValue::Bool(true));
                write_tagged(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            TrackFieldMut::CommandList(f) => {
                let default = match pv_default {
                    Some(PropertyValue::CommandList(d)) => d,
                    _ => String::new(),
                };
                write_command_list(
                    f,
                    value,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    default,
                    has_duration,
                    has_delay,
                );
            },
            // PlacementMode and MorphOptions are handled in tier 1 (no-ops),
            // but field_mut returns them, so this arm is here for exhaustiveness.
            TrackFieldMut::PlacementMode(_) | TrackFieldMut::MorphOptions(_) => {},
            // CalloutPlace is written via write_callout_place below.
            TrackFieldMut::CalloutPlace(f) => {
                if let PropertyValue::Enum(choice) = &value
                    && let Some(place) = super::animation_track::CalloutPlace::from_str(choice)
                {
                    f.ensure(super::animation_track::CalloutPlace::Right)
                        .add_keyframe(t_end_ms, place, easing);
                }
            },
            // ShapeType is handled in tier 1 above
            TrackFieldMut::ShapeType(_) => {},
            // VectorPaths, TextPaths, PositionBinding — generated/cached at build time, no
            // keyframing.
            TrackFieldMut::VectorPaths(_)
            | TrackFieldMut::TextPaths(_)
            | TrackFieldMut::PositionBinding(_) => {},
            // Image — generated/cached at build time, no keyframing.
            #[cfg(feature = "render")]
            TrackFieldMut::Image(_) => {},
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Plan-backed property access
// ─────────────────────────────────────────────────────────────

/// Write a property value through the actor's registry-driven `PropertyPlan`.
///
/// Returns `true` when the value was stored in a plan slot. The slot is
/// created lazily for extension properties not present in the built-in
/// registry.
pub fn write_property_plan_slot(
    track: &mut AnimationTrack,
    id: animatix_syntax::schema::PropertyId,
    kind: crate::timeline::PropertyKind,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) -> bool {
    let has_duration = t_end_ms > t_start_ms;
    let slot = track.property_plan.ensure_slot(id, kind);
    if has_duration {
        if let Some(start) = slot.track.sample(t_start_ms) {
            slot.track.add_keyframe(t_start_ms, start);
        }
    }
    slot.track.add_keyframe_eased(t_end_ms, value, easing).is_some()
}

/// Read a property value through the actor's registry-driven `PropertyPlan`.
pub fn read_property_plan_slot(
    track: &AnimationTrack,
    id: animatix_syntax::schema::PropertyId,
    time_ms: u64,
) -> Option<PropertyValue> {
    track.property_plan.get(id).and_then(|slot| slot.track.sample(time_ms))
}

/// Parse an expression into the finite value kind declared by an extension.
///
/// When the extension property declares a precise `Enum` tooling type, bare
/// variant identifiers (`mode: ring`) and string literals (`mode: "ring"`) are
/// both accepted and validated against the declared variants, mirroring the
/// built-in `ValueType::Enum` path. Without this, a bare `mode: ring` would be
/// mis-read as a variable lookup, fail, and silently drop the property.
pub(crate) fn parse_extension_property_value(
    kind: PropertyValueKind,
    ty: Option<&animatix_syntax::typing::Type>,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    if let Some(animatix_syntax::typing::Type::Enum(variants)) = ty {
        let text = match expr {
            Expr::Ident(name) | Expr::Str(name) => name.clone(),
            _ => super::evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?
                .as_str(),
        };
        if variants.iter().any(|variant| variant == &text) {
            return Some(PropertyValue::Enum(text));
        }
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("'{}' expects one of {}, got '{}'", subject, variants.join(" | "), text),
            )
            .with_subject(subject),
        );
        return None;
    }
    let value = super::evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
    let parsed = match kind {
        PropertyValueKind::F32 => match value {
            Value::Num(n) => Some(PropertyValue::F32(n as f32)),
            _ => None,
        },
        PropertyValueKind::U32 => match value {
            Value::Num(n) if n >= 0.0 && n <= u32::MAX as f64 => Some(PropertyValue::U32(n as u32)),
            _ => None,
        },
        PropertyValueKind::Bool => match value {
            Value::Bool(b) => Some(PropertyValue::Bool(b)),
            _ => None,
        },
        PropertyValueKind::Vec2 => match value {
            Value::Vec2(v) => Some(PropertyValue::Vec2([v[0] as f32, v[1] as f32])),
            _ => None,
        },
        PropertyValueKind::Vec4 => match value {
            Value::Vec4(v) | Value::Color(v) => {
                Some(PropertyValue::Vec4([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32]))
            },
            _ => None,
        },
        PropertyValueKind::String => match value {
            Value::Str(s) => Some(PropertyValue::String(s)),
            _ => None,
        },
        PropertyValueKind::PointList => match value {
            Value::List(items) => {
                let mut points = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        Value::Vec2(v) => points.push([v[0] as f32, v[1] as f32]),
                        _ => return None,
                    }
                }
                Some(PropertyValue::PointList(points))
            },
            _ => None,
        },
        PropertyValueKind::Generic => property_value_from_value(value),
    };
    if parsed.is_none() {
        tracing::warn!(
            "{subject}: extension property expects {:?}, got a value of another kind",
            kind
        );
    }
    parsed
}

fn property_value_from_value(value: Value) -> Option<PropertyValue> {
    Some(match value {
        Value::Num(n) => PropertyValue::F32(n as f32),
        Value::Str(s) => PropertyValue::String(s),
        Value::Bool(b) => PropertyValue::Bool(b),
        Value::Vec2(v) => PropertyValue::Vec2([v[0] as f32, v[1] as f32]),
        Value::Vec3(v) => PropertyValue::Vec4([v[0] as f32, v[1] as f32, v[2] as f32, 1.0]),
        Value::Vec4(v) | Value::Color(v) => {
            PropertyValue::Color([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32])
        },
        Value::List(items) => {
            if items.iter().all(|item| matches!(item, Value::Vec2(_))) {
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

/// Write an extension property into the actor plan.
pub(crate) fn write_extension_property_slot(
    track: &mut AnimationTrack,
    ctx: &ExtensionContext,
    actor_type: &str,
    property: &str,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) -> bool {
    let Some(spec) = ctx.property_spec(actor_type, property) else {
        return false;
    };
    let value = match (spec.kind, &value) {
        (PropertyValueKind::Vec4, PropertyValue::Color(v)) => PropertyValue::Vec4(*v),
        (PropertyValueKind::F32, PropertyValue::U32(v)) => PropertyValue::F32(*v as f32),
        (PropertyValueKind::U32, PropertyValue::F32(v)) => PropertyValue::U32(v.max(0.0) as u32),
        _ => value,
    };
    write_property_plan_slot(track, spec.id, spec.kind.into(), value, t_start_ms, t_end_ms, easing)
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
    let PropertyValue::Vec2(v) = value else {
        return;
    };
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
    let PropertyValue::Transform(v) = value else {
        return;
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
    let PropertyValue::PointList(v) = value else {
        return;
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
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
    let PropertyValue::CommandList(v) = value else {
        return;
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

pub(crate) fn write_tagged(
    field: &mut Option<PropertyTrack<PropertyValue>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: PropertyValue,
    has_duration: bool,
    has_delay: bool,
) {
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay && t_start_ms > 0 {
        if field.is_none() {
            *field = Some(PropertyTrack::new(default.clone()));
        }
        if let Some(inner) = field.as_mut() {
            let previous_time = t_start_ms.saturating_sub(1);
            if !inner.keyframes.contains_key(&previous_time) {
                let previous_value = inner.evaluate(previous_time);
                inner.add_keyframe(previous_time, previous_value, Easing::Linear);
            }
        }
    }
    field.ensure(default).add_keyframe(t_end_ms, value, easing);
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
    let PropertyValue::String(v) = value else {
        return;
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, v, easing);
}

// ─────────────────────────────────────────────────────────────
// Read: ActorField + time_ms → PropertyValue
// ─────────────────────────────────────────────────────────────

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
                    },
                    ReadSource::None_ => continue,
                }
            },
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

/// Inject external property values into a frame environment.
pub(crate) fn inject_extension_properties_into_env(
    env: &mut Environment,
    label: &str,
    track: &AnimationTrack,
    time_ms: u64,
    ctx: Option<&ExtensionContext>,
) {
    let Some(ctx) = ctx else {
        return;
    };
    let Some(actor_type) = track.actor_type.as_deref() else {
        return;
    };

    let mut key = String::with_capacity(label.len() + 24);
    key.push_str(label);
    key.push('.');
    let prefix_len = key.len();

    for spec in ctx.property_specs() {
        if !spec.injectable || spec.actor_type != actor_type {
            continue;
        }
        let Some(pv) = read_property_plan_slot(track, spec.id, time_ms) else {
            continue;
        };
        key.truncate(prefix_len);
        key.push_str(&spec.name);
        inject_value(env, &mut key, prefix_len, &spec.name, &pv);

        let animating = track
            .property_plan
            .get(spec.id)
            .is_some_and(|slot| slot.track.has_any_keyframes());
        key.truncate(prefix_len);
        key.push_str("_animating_");
        key.push_str(&spec.name);
        env.set(&key, Value::Num(if animating { 1.0 } else { 0.0 }));
    }
}

/// Inject a `PropertyValue` into the environment, with typed sub-keys
/// (`.x`, `.y` for Vec2; `.r`, `.g`, `.b`, `.a` for Color/Vec4).
///
/// `key` must be positioned at the end of the property name (the full
/// `{label}.{property}` prefix).  It is restored to `prefix_len + name.len()`
/// after injection so that callers can continue using the buffer.
fn inject_value(
    env: &mut Environment,
    key: &mut String,
    prefix_len: usize,
    name: &str,
    value: &PropertyValue,
) {
    let restore = prefix_len + name.len();
    match value {
        PropertyValue::F32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        },
        PropertyValue::U32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        },
        PropertyValue::Vec2(v) => {
            env.set(&*key, Value::Vec2([v[0] as f64, v[1] as f64]));
            key.push_str(".x");
            env.set(&*key, Value::Num(v[0] as f64));
            key.truncate(restore);
            key.push_str(".y");
            env.set(&*key, Value::Num(v[1] as f64));
        },
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
        },
        PropertyValue::Transform(v) => {
            env.set(
                &*key,
                Value::List(vec![
                    Value::Num(v[0] as f64),
                    Value::Num(v[1] as f64),
                    Value::Num(v[2] as f64),
                    Value::Num(v[3] as f64),
                    Value::Num(v[4] as f64),
                    Value::Num(v[5] as f64),
                ]),
            );
        },
        PropertyValue::String(v) => {
            env.set(&*key, Value::Str(v.clone()));
        },
        PropertyValue::Enum(v) => {
            env.set(&*key, Value::Str(v.clone()));
        },
        PropertyValue::Bool(v) => {
            env.set(&*key, Value::Bool(*v));
        },
        PropertyValue::Variant { value, .. } => {
            inject_value(env, key, prefix_len, name, value);
            return;
        },
        _ => {},
    }
    key.truncate(restore);
}

// ─────────────────────────────────────────────────────────────
// Override-aware property reads
// ─────────────────────────────────────────────────────────────

/// Read a property value using a pre-resolved schema / plan-slot pair.
///
/// Same semantics as `dispatch::read_property_value`, including the fallback to
/// the track's own field, but skips the name → id lookup that
/// `property_registry::resolve_property` has already done for this call.
pub(crate) fn read_property_value_resolved(
    track: &AnimationTrack,
    schema: &crate::timeline::property_registry::PropertySchema,
    slot: Option<animatix_syntax::schema::PropertyId>,
    time_ms: u64,
) -> Option<PropertyValue> {
    if let Some(id) = slot
        && let Some(value) = read_property_plan_slot(track, id, time_ms)
    {
        return Some(value);
    }
    track.field_ref(schema.field).and_then(|f| f.evaluate_value(time_ms))
}

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
    if let Some((schema, slot)) = crate::timeline::property_registry::resolve_property(name)
        && let Some(pv) = read_property_value_resolved(track, schema, slot, time_ms)
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
    if let Some((schema, slot)) = crate::timeline::property_registry::resolve_property(name)
        && let Some(pv) = read_property_value_resolved(track, schema, slot, time_ms)
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
    if let Some((schema, slot)) = crate::timeline::property_registry::resolve_property(name)
        && let Some(pv) = read_property_value_resolved(track, schema, slot, time_ms)
        && let PropertyValue::Transform(v) = pv
    {
        return v;
    }
    default
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::PlacementMode;
    use crate::timeline::animation_track::CalloutPlace;
    use crate::timeline::dispatch::{
        property_has_keyframe_at, property_has_keyframes, property_keyframe_count,
        property_keyframe_easing, property_keyframe_times, read_property_value,
        read_property_value_or_default,
    };

    #[test]
    fn enum_boundary_shape_type_roundtrips() {
        let value = ShapeType::Arrow;
        assert_eq!(value.name(), "Arrow");
        assert_eq!(ShapeType::from_name(value.name()), Some(value));
        assert_eq!(value.to_property_value(), PropertyValue::Enum("Arrow".to_string()));
    }

    #[test]
    fn enum_boundary_placement_mode_roundtrips() {
        let value = PlacementMode::Manual;
        assert_eq!(value.name(), "manual");
        assert_eq!(PlacementMode::from_name(value.name()), Some(value));
        assert_eq!(value.to_property_value(), PropertyValue::Enum("manual".to_string()));
    }

    #[test]
    fn enum_boundary_callout_place_roundtrips() {
        let value = CalloutPlace::Left;
        assert_eq!(value.name(), "left");
        assert_eq!(CalloutPlace::from_name(value.name()), Some(value));
        assert_eq!(value.to_property_value(), PropertyValue::Enum("left".to_string()));
    }

    #[test]
    fn tagged_property_routes_through_actor_plan() {
        let mut track = AnimationTrack::new("test".to_string());
        track.rebuild_property_plan();
        let field = ActorField::Tagged("legend_title");
        write_property_field(
            &mut track,
            field,
            PropertyValue::String("Revenue".to_string()),
            0,
            0,
            Easing::Linear,
            &mut vec![],
        );
        assert_eq!(
            read_property_value(&track, field, 0),
            Some(PropertyValue::String("Revenue".to_string()))
        );

        write_property_field(
            &mut track,
            field,
            PropertyValue::String("Costs".to_string()),
            0,
            1000,
            Easing::EaseInOut,
            &mut vec![],
        );
        assert!(property_has_keyframes(&track, field));
        assert!(property_has_keyframe_at(&track, field, 1000));
        assert_eq!(property_keyframe_times(&track, field), vec![0, 1000]);
        assert_eq!(property_keyframe_easing(&track, field, 1000), Some(Easing::EaseInOut));
    }

    #[test]
    fn property_plan_slot_writes_and_reads_without_string_lookup() {
        let mut track = AnimationTrack::new("test".to_string());
        track.rebuild_property_plan();
        let position = crate::timeline::property_id("position").expect("position is registered");

        assert!(write_property_plan_slot(
            &mut track,
            position,
            crate::timeline::PropertyKind::Vec2,
            PropertyValue::Vec2([10.0, 20.0]),
            0,
            0,
            Easing::Linear,
        ));
        assert_eq!(
            read_property_plan_slot(&track, position, 0),
            Some(PropertyValue::Vec2([10.0, 20.0]))
        );

        assert!(write_property_plan_slot(
            &mut track,
            position,
            crate::timeline::PropertyKind::Vec2,
            PropertyValue::Vec2([110.0, 20.0]),
            0,
            1000,
            Easing::Linear,
        ));
        let slot = track.property_plan.get(position).expect("position slot");
        assert_eq!(slot.track.sample(500), Some(PropertyValue::Vec2([60.0, 20.0])));
    }

    // Helper: write a keyframe and read it back
    fn write_read_roundtrip(
        track: &mut AnimationTrack,
        field: ActorField,
        value: PropertyValue,
        time_ms: u64,
    ) -> Option<PropertyValue> {
        write_property_field(track, field, value, time_ms, time_ms, Easing::Linear, &mut vec![]);
        read_property_value(track, field, time_ms)
    }

    // ────────────────────────────────────────────────
    // 4.2: write/read round-trip tests
    // ────────────────────────────────────────────────

    #[test]
    fn test_write_read_roundtrip_f32() {
        let mut track = AnimationTrack::new("test".to_string());
        let result =
            write_read_roundtrip(&mut track, ActorField::Opacity, PropertyValue::F32(0.75), 500);
        assert_eq!(result, Some(PropertyValue::F32(0.75)));
    }

    #[test]
    fn test_write_read_roundtrip_vec2() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::Position,
            PropertyValue::Vec2([100.0, 200.0]),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Vec2([100.0, 200.0])));
    }

    #[test]
    fn test_write_read_roundtrip_color() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::Color,
            PropertyValue::Color([1.0, 0.0, 0.0, 1.0]),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Color([1.0, 0.0, 0.0, 1.0])));
    }

    #[test]
    fn test_write_read_roundtrip_vec4() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::StrokeColor,
            PropertyValue::Vec4([0.0, 1.0, 0.0, 0.5]),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Color([0.0, 1.0, 0.0, 0.5])));
    }

    #[test]
    fn test_write_read_roundtrip_transform() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::Transform,
            PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0]),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Transform([2.0, 0.0, 0.0, 2.0, 50.0, 100.0])));
    }

    #[test]
    fn test_write_read_roundtrip_string() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::FontFamily,
            PropertyValue::String("Arial".to_string()),
            500,
        );
        assert_eq!(result, Some(PropertyValue::String("Arial".to_string())));
    }

    #[test]
    fn test_write_read_roundtrip_tagged_bool() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::Tagged("legend"),
            PropertyValue::Bool(false),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Bool(false)));
    }

    #[test]
    fn test_property_value_interpolation_rules() {
        let a = PropertyValue::F32(0.0);
        let b = PropertyValue::F32(100.0);
        assert_eq!(a.interpolate(&b, 0.5), PropertyValue::F32(50.0));

        let hidden = PropertyValue::Bool(false);
        let shown = PropertyValue::Bool(true);
        assert_eq!(hidden.interpolate(&shown, 0.25), PropertyValue::Bool(false));
        assert_eq!(hidden.interpolate(&shown, 0.75), PropertyValue::Bool(true));

        let left = PropertyValue::String("left".to_string());
        let right = PropertyValue::String("right".to_string());
        assert_eq!(left.interpolate(&right, 0.25), left);
        assert_eq!(left.interpolate(&right, 0.75), right);

        // Cross-variant transitions snap at the midpoint.
        assert_eq!(hidden.interpolate(&right, 0.25), hidden);
        assert_eq!(hidden.interpolate(&right, 0.75), right);
    }

    #[test]
    fn test_tagged_auto_transition_snaps_cross_variant() {
        let mut track = AnimationTrack::new("test".to_string());
        let mut diag = Vec::new();
        write_property_field(
            &mut track,
            ActorField::Tagged("legend"),
            PropertyValue::Bool(false),
            0,
            1000,
            Easing::Linear,
            &mut diag,
        );
        write_property_field(
            &mut track,
            ActorField::Tagged("legend"),
            PropertyValue::String("Revenue".to_string()),
            1000,
            2000,
            Easing::Linear,
            &mut diag,
        );

        assert_eq!(
            read_property_value(&track, ActorField::Tagged("legend"), 1250),
            Some(PropertyValue::Bool(false))
        );
        assert_eq!(
            read_property_value(&track, ActorField::Tagged("legend"), 1750),
            Some(PropertyValue::String("Revenue".to_string()))
        );
    }

    #[test]
    fn test_write_read_roundtrip_min_width() {
        let mut track = AnimationTrack::new("test".to_string());
        let result =
            write_read_roundtrip(&mut track, ActorField::MinWidth, PropertyValue::F32(200.0), 500);
        assert_eq!(result, Some(PropertyValue::F32(200.0)));
    }

    #[test]
    fn test_write_read_roundtrip_max_height() {
        let mut track = AnimationTrack::new("test".to_string());
        let result =
            write_read_roundtrip(&mut track, ActorField::MaxHeight, PropertyValue::F32(800.0), 500);
        assert_eq!(result, Some(PropertyValue::F32(800.0)));
    }

    #[test]
    fn test_write_read_roundtrip_filter_brightness() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::FilterBrightness,
            PropertyValue::F32(1.5),
            500,
        );
        assert_eq!(result, Some(PropertyValue::F32(1.5)));
    }

    #[test]
    fn test_write_read_roundtrip_highlight_color() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::HighlightColor,
            PropertyValue::Vec4([1.0, 0.0, 0.0, 1.0]),
            500,
        );
        assert_eq!(result, Some(PropertyValue::Color([1.0, 0.0, 0.0, 1.0])));
    }

    #[test]
    fn test_write_read_roundtrip_highlight_opacity() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::HighlightOpacity,
            PropertyValue::F32(0.5),
            500,
        );
        assert_eq!(result, Some(PropertyValue::F32(0.5)));
    }

    #[test]
    fn test_write_read_roundtrip_min_height() {
        let mut track = AnimationTrack::new("test".to_string());
        let result =
            write_read_roundtrip(&mut track, ActorField::MinHeight, PropertyValue::F32(300.0), 500);
        assert_eq!(result, Some(PropertyValue::F32(300.0)));
    }

    #[test]
    fn test_write_read_roundtrip_letter_spacing() {
        let mut track = AnimationTrack::new("test".to_string());
        let result = write_read_roundtrip(
            &mut track,
            ActorField::LetterSpacing,
            PropertyValue::F32(2.5),
            500,
        );
        assert_eq!(result, Some(PropertyValue::F32(2.5)));
    }

    #[test]
    fn test_write_read_roundtrip_with_duration_uses_linear_easing_at_start() {
        let mut track = AnimationTrack::new("test".to_string());
        let mut diag = vec![];
        write_property_field(
            &mut track,
            ActorField::Opacity,
            PropertyValue::F32(0.5),
            0,    // t_start
            1000, // t_end
            Easing::EaseOut,
            &mut diag,
        );
        // Should have keyframes at 0 (start snapshot) and 1000 (end value)
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert_eq!(rf.keyframe_count(), 2);
        assert_eq!(rf.keyframe_easing(0), Some(Easing::Linear));
        assert_eq!(rf.keyframe_easing(1000), Some(Easing::EaseOut));
    }

    #[test]
    fn test_read_property_value_or_default_falls_back() {
        let track = AnimationTrack::new("test".to_string());
        let schema = crate::timeline::property_registry::lookup_property("opacity").unwrap();
        let val = read_property_value_or_default(&track, schema, 0);
        assert_eq!(val, PropertyValue::F32(1.0));
    }

    #[test]
    fn test_property_has_keyframes() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        assert!(property_has_keyframes(&track, ActorField::Opacity));
        assert!(!property_has_keyframes(&track, ActorField::Rotation));
    }

    #[test]
    fn test_property_has_keyframe_at() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        assert!(property_has_keyframe_at(&track, ActorField::Opacity, 500));
        assert!(!property_has_keyframe_at(&track, ActorField::Opacity, 0));
    }

    #[test]
    fn test_property_keyframe_times_sorted() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        let times = property_keyframe_times(&track, ActorField::Opacity);
        assert_eq!(times, vec![0, 1000]);
    }

    #[test]
    fn test_property_keyframe_count() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        assert_eq!(property_keyframe_count(&track, ActorField::Opacity), 2);
        assert_eq!(property_keyframe_count(&track, ActorField::Rotation), 0);
    }

    #[test]
    fn test_property_keyframe_easing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::EaseInOut);
        let easing = property_keyframe_easing(&track, ActorField::Opacity, 0);
        assert_eq!(easing, Some(Easing::EaseInOut));
    }
}
