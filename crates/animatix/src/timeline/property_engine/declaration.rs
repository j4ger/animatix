//! Declaration- and assignment-time property writes: the central
//! [`write_property_field`] dispatch and the typed track writers it uses.

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::renderer::types::GradientSpec;
use crate::timeline::preserve_instant_delayed_value;
use crate::timeline::property_registry::ActorField;
use crate::timeline::{AnimationTrack, PropertyTrack, ShapeType, TrackAccessor};

use super::plan::write_property_plan_slot_on;
use super::value::{EnumPropertyValue, PropertyValue};

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
use crate::timeline::world::TrackFieldsMut;

/// Write a parsed property value into a bundle of tier fields.
pub fn write_property_fields(
    fields: &mut TrackFieldsMut<'_>,
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
                // The shape tier is a required component on a spawned entity;
                // a bundle built without one (track-only test helpers) has no
                // destination for the keyframe.
                let Some(shape) = fields.shape.as_deref_mut() else {
                    tracing::debug!(
                        "shape_type write for '{}' dropped: no shape tier in this field bundle",
                        fields.label
                    );
                    return;
                };
                write_shape_type(
                    &mut shape.shape_type,
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
        | ActorField::SvgPaths => return,
        // Audio source cannot be animated; volume can be animated via the volume fields.
        ActorField::AudioSource => {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::UnimplementedProperty,
                DiagnosticPhase::Build,
                format!(
                    "`{}.source` cannot be animated yet: audio `source` is snapshotted \
                     from the declaration, so this write is dropped. Set `source: …` on the Audio \
                     declaration instead.",
                    fields.label
                ),
            ));
            return;
        },
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
            .map(|schema| (schema.default_value)(&fields.caps)),
        _ => None,
    };

    if field == ActorField::Tagged("callout_place")
        && let PropertyValue::Enum(choice) = &value
        && let Some(place) = crate::timeline::animation_track::CalloutPlace::from_str(choice)
    {
        // The geometry tier is a required component on a spawned entity; a
        // bundle built without one (track-only test helpers) has no
        // destination for the keyframe.
        let Some(geometry) = fields.geometry.as_deref_mut() else {
            tracing::debug!(
                "callout_place write for '{}' dropped: no geometry tier in this field bundle",
                fields.label
            );
            return;
        };
        let callout_track = &mut geometry.callout_place;
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
        if let Some(plan) = fields.property_plan.as_mut() {
            let kind = plan
                .get(id)
                .map(|slot| slot.kind)
                .unwrap_or(crate::timeline::PropertyKind::Generic);
            if write_property_plan_slot_on(
                plan,
                id,
                kind,
                value.clone(),
                t_start_ms,
                t_end_ms,
                easing,
            ) {
                return;
            }
        }
    }

    if let Some(tf) = fields.field_mut(field) {
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
                // Handle by extracting and writing via the f32 fields
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
            TrackFieldMut::F32List(f) => {
                let default = match pv_default {
                    Some(PropertyValue::F32List(d)) => d,
                    _ => Vec::new(),
                };
                write_f32_list(
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
            TrackFieldMut::Gradient(f) => {
                let default = match pv_default {
                    Some(PropertyValue::Gradient(d)) => d,
                    _ => GradientSpec::default(),
                };
                write_gradient(
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
                    && let Some(place) =
                        crate::timeline::animation_track::CalloutPlace::from_str(choice)
                {
                    f.ensure(crate::timeline::animation_track::CalloutPlace::Right)
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
            TrackFieldMut::Image(_) => {},
        }
    }
}

/// Write a parsed property value to the correct track field.
pub fn write_property_field(
    track: &mut AnimationTrack,
    field: ActorField,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut fields = TrackFieldsMut::from_track(track);
    write_property_fields(&mut fields, field, value, t_start_ms, t_end_ms, easing, diagnostics);
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
        // A color written as text (`stroke = "#ff2d55"`) is as valid as one
        // written as a tuple: hex and named colors resolve everywhere a color is
        // accepted, including frame-time overrides.
        PropertyValue::String(text) => match crate::timeline::utils::color_from_text(&text) {
            Some(color) => color,
            None => {
                tracing::warn!("write_vec4: '{text}' is not a color; the assignment is dropped");
                return;
            },
        },
        other => {
            tracing::warn!(
                "write_vec4: expected a color, got {other:?}; the assignment is dropped"
            );
            return;
        },
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

/// Write a float-list property with the same keyframe semantics as
/// [`write_point_list`] (step when undated, lerp when dated).
pub(crate) fn write_f32_list(
    field: &mut Option<PropertyTrack<Vec<f32>>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: Vec<f32>,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::F32List(v) = value else {
        return;
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    } else {
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    }
}

/// Write a gradient paint with the same keyframe semantics as
/// [`write_f32_list`] (step when undated, interpolated when dated).
pub(crate) fn write_gradient(
    field: &mut Option<PropertyTrack<GradientSpec>>,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: GradientSpec,
    has_duration: bool,
    has_delay: bool,
) {
    let PropertyValue::Gradient(v) = value else {
        return;
    };
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    } else {
        field.ensure(default).add_keyframe(t_end_ms, v, easing);
    }
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
