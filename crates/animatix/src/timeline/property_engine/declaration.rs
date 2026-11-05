//! Declaration- and assignment-time property writes: the central
//! [`write_property_field`] dispatch and the typed track writers it uses.

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::renderer::types::GradientSpec;
use crate::timeline::preserve_instant_delayed_value;
use crate::timeline::property_registry::ActorField;
use crate::timeline::world::TrackFieldsMut;
use crate::timeline::{EntityId, PropertyTrack, ShapeType, TrackAccessor, World};

use super::plan::write_property_plan_slot_on;
use super::value::{EnumPropertyValue, PropertyValue};

// ─────────────────────────────────────────────────────────────
// Write: ActorField + PropertyValue + timing → keyframes
// ─────────────────────────────────────────────────────────────

/// Extract the schema default for a typed track and hand the value to its
/// writer.
///
/// Every typed arm of the tier-2 dispatch is the same default match plus the
/// same writer call; the macro keeps that pairing out of each arm. The context
/// is passed explicitly because `macro_rules!` bodies cannot capture locals.
macro_rules! write_typed_field {
    (
        $writer:ident, $field:expr, $variant:ident, $fallback:expr,
        pv = $pv_default:expr, value = $value:expr, start = $start:expr,
        end = $end:expr, easing = $easing:expr,
        duration = $duration:expr, delay = $delay:expr
    ) => {{
        let default = match $pv_default {
            Some(PropertyValue::$variant(value)) => value,
            _ => $fallback,
        };
        $writer($field, $value, $start, $end, $easing, default, $duration, $delay);
    }};
}

/// Write a parsed property value into a bundle of tier fields, handling
/// keyframe timing (snapshot start, preserve delayed, write end).
///
/// This is the central dispatch for declaration- and assignment-time property
/// writes, so every property gets the same timing and drop behavior.
///
/// The dispatch works in two tiers:
/// 1. Special cases that need custom extraction/conversion (ShapeType, no-ops, group diagnostics)
/// 2. Uniform dispatch via `ActorField::default_value()` + `TrackFieldMut` for standard fields
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
                // Shape tier may be absent in track-only test helpers.
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

    // A max-width write changes the wrap layout of already-compiled static
    // text. Drop the build-time glyph caches so `evaluate_text_paths` compiles
    // against the new wrap width instead of replaying the old static paths.
    // Animated text (no `static_paths`) keeps its keyframed path track.
    if field == ActorField::TextMaxWidth
        && let Some(text) = fields.text.as_deref_mut()
        && text.static_paths.is_some()
    {
        text.static_paths = None;
        text.hardware_layout = None;
        text.text_paths = None;
    }

    // ── Tier 2: Uniform dispatch via TrackFieldMut ──
    use crate::timeline::dispatch::TrackFieldMut;
    let pv_default = ActorField::default_value(field);
    let tagged_default = match field {
        ActorField::Tagged(name) => crate::timeline::property_registry::lookup_property(name)
            .map(|schema| (schema.default_value)(&fields.caps)),
        _ => None,
    };

    if field == ActorField::Tagged("callout_place") {
        // Geometry tier may be absent in track-only test helpers.
        let Some(geometry) = fields.geometry.as_deref_mut() else {
            tracing::debug!(
                "callout_place write for '{}' dropped: no geometry tier in this field bundle",
                fields.label
            );
            return;
        };
        let Some(place) = (match &value {
            PropertyValue::Enum(choice) => {
                crate::timeline::animation_track::CalloutPlace::from_str(choice)
            },
            _ => None,
        }) else {
            tracing::warn!(
                "callout_place write for '{}' dropped: expected a placement name, got {value:?}",
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
        // Typed track is canonical; tagged is a legacy read-only fallback.
        return;
    }

    // Route tagged properties through the actor plan; `legend` stays tagged.
    if let ActorField::Tagged(name) = field
        && name != "legend"
        && name != "callout_place"
        && let Some(id) = crate::timeline::property_registry::property_id(name)
        && let Some(plan) = fields.property_plan.as_mut()
    {
        let kind = plan
            .get(id)
            .map(|slot| slot.kind)
            .unwrap_or(crate::timeline::PropertyKind::Generic);
        if write_property_plan_slot_on(plan, id, kind, value.clone(), t_start_ms, t_end_ms, easing)
        {
            return;
        }
    }

    if let Some(tf) = fields.field_mut(field) {
        match tf {
            TrackFieldMut::F32(f) => write_typed_field!(
                write_f32,
                f,
                F32,
                0.0,
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::Vec2(f) => write_typed_field!(
                write_vec2,
                f,
                Vec2,
                [0.0, 0.0],
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::Vec4(f) => write_typed_field!(
                write_vec4,
                f,
                Vec4,
                [1.0, 1.0, 1.0, 1.0],
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::Transform(f) => write_typed_field!(
                write_transform,
                f,
                Transform,
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::String(f) => write_typed_field!(
                write_string,
                f,
                String,
                String::new(),
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::U32(f) => {
                let default = match pv_default {
                    Some(PropertyValue::U32(d)) => d,
                    _ => 0,
                };
                // U32 tracks accept F32 keyframes; clamp negatives.
                let value = match value {
                    PropertyValue::F32(v) => Some(v.max(0.0) as u32),
                    PropertyValue::U32(v) => Some(v),
                    _ => None,
                };
                if let Some(value) = value {
                    write_track(
                        f,
                        value,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        default,
                        has_duration,
                        has_delay,
                    );
                }
            },
            TrackFieldMut::PointList(f) => write_typed_field!(
                write_point_list,
                f,
                PointList,
                Vec::new(),
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::F32List(f) => write_typed_field!(
                write_f32_list,
                f,
                F32List,
                Vec::new(),
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            TrackFieldMut::Gradient(f) => write_typed_field!(
                write_gradient,
                f,
                Gradient,
                GradientSpec::default(),
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
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
            TrackFieldMut::CommandList(f) => write_typed_field!(
                write_command_list,
                f,
                CommandList,
                String::new(),
                pv = pv_default,
                value = value,
                start = t_start_ms,
                end = t_end_ms,
                easing = easing,
                duration = has_duration,
                delay = has_delay
            ),
            // Handled in tier 1; arm keeps the match exhaustive.
            TrackFieldMut::PlacementMode(_) | TrackFieldMut::MorphOptions(_) => {},
            // Typed `callout_place`; the tagged name is handled above.
            TrackFieldMut::CalloutPlace(f) => {
                if let PropertyValue::Enum(choice) = &value
                    && let Some(place) =
                        crate::timeline::animation_track::CalloutPlace::from_str(choice)
                {
                    f.ensure(crate::timeline::animation_track::CalloutPlace::Right)
                        .add_keyframe(t_end_ms, place, easing);
                }
            },
            // ShapeType is handled in tier 1 above.
            TrackFieldMut::ShapeType(_) => {},
            // Generated/cached at build time; not keyframed.
            TrackFieldMut::VectorPaths(_)
            | TrackFieldMut::TextPaths(_)
            | TrackFieldMut::PositionBinding(_) => {},
            TrackFieldMut::Image(_) => {},
        }
    }
}
/// Write a parsed property value to the correct track field for `id`,
/// routing through the entity's tier bundle.
pub fn write_property_field(
    world: &mut World,
    id: EntityId,
    field: ActorField,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(mut fields) = world.fields_mut(id) {
        write_property_fields(&mut fields, field, value, t_start_ms, t_end_ms, easing, diagnostics);
    }
}

// ─────────────────────────────────────────────────────────────
// Internal write helpers
// ─────────────────────────────────────────────────────────────

/// Write one typed track with the keyframe timing shared by every property
/// writer: snapshot the value at `t_start_ms` when the write spans a duration,
/// fence an instant delayed write, then write the target at `t_end_ms`.
fn write_track<T: Clone + crate::timeline::property_track::Interpolate + Default>(
    field: &mut Option<PropertyTrack<T>>,
    value: T,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    default: T,
    has_duration: bool,
    has_delay: bool,
) {
    if has_duration {
        let start_val = field.get(t_start_ms, default.clone());
        field
            .ensure(default.clone())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if has_delay {
        preserve_instant_delayed_value(field, t_start_ms);
    }
    field.ensure(default).add_keyframe(t_end_ms, value, easing);
}

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
    let PropertyValue::F32(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::Vec2(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
}

/// Convert a property value to a color, warning and dropping when it is not one.
fn color_value(value: PropertyValue) -> Option<[f32; 4]> {
    match value {
        PropertyValue::Vec4(v) => Some(v),
        PropertyValue::Color(v) => Some(v),
        // A color written as text (`stroke = "#ff2d55"`) is as valid as one
        // written as a tuple: hex and named colors resolve everywhere a color is
        // accepted, including frame-time overrides.
        PropertyValue::String(text) => match crate::timeline::utils::color_from_text(&text) {
            Some(color) => Some(color),
            None => {
                tracing::warn!("write_vec4: '{text}' is not a color; the assignment is dropped");
                None
            },
        },
        other => {
            tracing::warn!(
                "write_vec4: expected a color, got {other:?}; the assignment is dropped"
            );
            None
        },
    }
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
    let Some(value) = color_value(value) else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::Transform(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::PointList(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::F32List(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::Gradient(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::CommandList(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
}

/// Tagged values keep their own delayed-write fence: unlike the typed tracks,
/// an instant write has to materialize a keyframe one millisecond earlier so
/// the previous value holds rather than easing into the new one.
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
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
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
    let PropertyValue::String(value) = value else {
        return;
    };
    write_track(field, value, t_start_ms, t_end_ms, easing, default, has_duration, has_delay);
}
