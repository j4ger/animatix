//! Property-edit application for the timeline (legend, registry, effects).

use animatix::timeline::{
    ActorField, GeometryTracks, Interpolate, LegendTracks, PlacementMode, PropertyTrack,
    PropertyValue, ShapeType, TrackAccessor, TrackFieldMut, ValueType,
    animation_track::CalloutPlace, lookup_property, world::TrackFieldsMut,
};
use animatix_syntax::easing::Easing;

use super::*;
use crate::app::panels::PropertyValue as PV;

/// Apply a property edit to one entity in the timeline `World`.
///
/// This helper centralises the per-property dispatch so both keyframe-mode
/// and overwrite-mode edits stay consistent. It always adds a keyframe at
/// `time_ms` (in addition to updating `default_value`) so the renderer's
/// `evaluate()` immediately sees the new value.
///
/// The geometry tier is a `World` component (`World::geometry`), so this
/// takes the world and the entity id; the world-aware field bundle resolves
/// geometry, style, shape, and text writes.
pub(super) fn apply_property_edit_to_entity(
    world: &mut animatix::timeline::World,
    id: animatix::timeline::EntityId,
    property: &str,
    value: &panels::PropertyValue,
    time_ms: u64,
    legacy_tagged: &mut Vec<(String, PropertyValue)>,
    field_writes: &mut Vec<(ActorField, PropertyValue)>,
) {
    let linear = Easing::Linear;

    // `scope.stage.param` edits address the scope's effect chain.
    if let Some((stage, param)) = property.split_once('.')
        && let Some(effects) = world.effects_mut(id)
        && effects.write_param(stage, param, value.clone(), time_ms, time_ms, linear)
    {
        return;
    }

    // `place` and `legend` are tagged properties with a bespoke payload; the
    // rest of the tagged/scalar surface is table-driven below.
    if property == "place" {
        apply_place_edit(world, id, value, time_ms, linear, legacy_tagged);
        return;
    }

    if property == "legend" {
        apply_legend_mode_edit(value, legacy_tagged);
        return;
    }

    // Legend scalar fields share one shape: assign the struct field and
    // mirror it into the tagged-property plan.
    let caps = world.caps(id);
    if caps.legend_host {
        let legend = world.ensure_legend(id);
        if apply_legend_fields(
            legend,
            property,
            value,
            time_ms,
            linear,
            legacy_tagged,
            field_writes,
        ) {
            return;
        }
    }

    // ── Special / compound properties (not covered by generic registry dispatch) ──
    // The geometry tier lives in the world component store, so borrow it for
    // the whole block and then fall through to the world-aware field bundle
    // for the registry-driven dispatch below.
    let Some(mut fields) = world.fields_mut(id) else {
        return;
    };
    if apply_geometry_edit(fields.geometry.as_deref_mut(), property, value, time_ms, linear) {
        return;
    }

    apply_registry_edit(&mut fields, property, value, time_ms, linear, legacy_tagged, field_writes);
}

/// Normalise an inspector value to the kind its plan slot accepts.
///
/// The inspector emits `F32` for every numeric field, including `U32`
/// properties (`decimals`, `max_lines`), and colours as `Color` even though
/// the plan stores every `Color`/`Vec4` descriptor in a `Vec4` slot. A plan
/// slot rejects a value of another kind, so an uncoerced write would fall
/// back to tagged tracks and be shadowed by the plan the engine reads.
fn coerce_for_value_type(pv: PropertyValue, value_type: ValueType) -> PropertyValue {
    match (value_type, pv) {
        (ValueType::U32, PropertyValue::F32(v)) => PropertyValue::U32(v.max(0.0).round() as u32),
        (ValueType::U32, PropertyValue::U32(v)) => PropertyValue::U32(v),
        (ValueType::Color | ValueType::Vec4, PropertyValue::Color(c)) => PropertyValue::Vec4(c),
        (ValueType::Color | ValueType::Vec4, PropertyValue::Vec4(c)) => PropertyValue::Vec4(c),
        (_, pv) => pv,
    }
}

/// Write one tagged property value.
///
/// Registry-backed tagged properties go through the engine's
/// `write_property_field`, which stores them in the actor's `PropertyPlan`
/// — the same store `read_property_value` and the renderer consult first.
/// Writing only `tagged_tracks` would leave the edit invisible behind the
/// plan (the reported `solo`/`shadow` drift). Keys with no descriptor
/// (`legend_title`, `legend_gap`, ...) keep tagged tracks, which is where
/// their readers look.
fn write_tagged_value(
    key: &str,
    pv: PropertyValue,
    _time_ms: u64,
    _linear: Easing,
    legacy_tagged: &mut Vec<(String, PropertyValue)>,
    field_writes: &mut Vec<(ActorField, PropertyValue)>,
) {
    if let Some(schema) = lookup_property(key)
        && let ActorField::Tagged(tag) = schema.field
    {
        let pv = coerce_for_value_type(pv, schema.value_type);
        field_writes.push((ActorField::Tagged(tag), pv));
        return;
    }

    legacy_tagged.push((key.to_string(), pv));
}

/// `place` writes both the typed `CalloutPlace` track and tagged tracks.
fn apply_place_edit(
    world: &mut animatix::timeline::World,
    id: animatix::timeline::EntityId,
    value: &PV,
    time_ms: u64,
    linear: Easing,
    legacy_tagged: &mut Vec<(String, PropertyValue)>,
) {
    if let PV::String(choice) = value
        && let Some(place) = CalloutPlace::from_str(choice)
    {
        world
            .geometry_mut(id)
            .callout_place
            .ensure(CalloutPlace::Right)
            .add_keyframe(time_ms, place, linear);
        legacy_tagged.push(("callout_place".to_string(), PropertyValue::Enum(choice.clone())));
    }
}

/// The `legend` tagged payload is a mode (`auto`/`hidden`) or a `label`.
fn apply_legend_mode_edit(value: &PV, legacy_tagged: &mut Vec<(String, PropertyValue)>) {
    let pv = match value {
        PV::Bool(true) => PropertyValue::Variant {
            name: "auto".to_string(),
            value: Box::new(PropertyValue::Bool(true)),
        },
        PV::Bool(false) => PropertyValue::Variant {
            name: "hidden".to_string(),
            value: Box::new(PropertyValue::Bool(false)),
        },
        PV::String(label) => PropertyValue::Variant {
            name: "label".to_string(),
            value: Box::new(PropertyValue::String(label.clone())),
        },
        _ => return,
    };
    legacy_tagged.push(("legend".to_string(), pv));
}

/// Legend scalar fields share one shape: assign the struct field and mirror
/// it into the tagged-property plan. Kept as one table so a new legend field
/// cannot drift between the two writes. Returns whether `property` was one of
/// the table entries (or `label_color`).
fn apply_legend_fields(
    legend: &mut LegendTracks,
    property: &str,
    value: &PV,
    time_ms: u64,
    linear: Easing,
    legacy_tagged: &mut Vec<(String, PropertyValue)>,
    field_writes: &mut Vec<(ActorField, PropertyValue)>,
) -> bool {
    macro_rules! legend_scalar_fields {
        ($(($name:literal, $field:ident, $tag:literal, $variant:ident)),* $(,)?) => {
            match property {
                $(
                    $name => {
                        if let PV::$variant(v) = value {
                            legend.$field = v.clone();
                            write_tagged_value(
                                $tag,
                                PropertyValue::$variant(v.clone()),
                                time_ms,
                                linear,
                                legacy_tagged,
                                field_writes,
                            );
                        }
                        return true;
                    }
                )*
                _ => {},
            }
        };
    }

    // World-aware capabilities: the projection carries any per-entity
    // overrides the catalog derivation alone would miss.
    legend_scalar_fields!(
        ("title", title, "legend_title", String),
        ("font_size", font_size, "legend_font_size", F32),
        ("swatch_size", swatch_size, "legend_swatch_size", F32),
        ("gap", gap, "legend_gap", F32),
        ("text_max_width", text_max_width, "legend_text_max_width", F32),
    );

    if property == "label_color" {
        if let PV::String(v) = value {
            let parsed = parse_gui_color(v);
            legend.label_color = parsed;
            if let Some(color) = parsed {
                write_tagged_value(
                    "legend_label_color",
                    PropertyValue::Color(color),
                    time_ms,
                    linear,
                    legacy_tagged,
                    field_writes,
                );
            }
        }
        return true;
    }

    false
}

/// Geometry-tier properties (`size`/`radius*`/`tip*`/`at`/`offset`).
///
/// These are intentionally an explicit `match`: each branch reads a different
/// pair of geometry tracks and some preserve the partner axis, so folding them
/// into a table would hide the per-property default/merge rules. Returns
/// whether `property` was handled.
fn apply_geometry_edit(
    geometry: Option<&mut GeometryTracks>,
    property: &str,
    value: &PV,
    time_ms: u64,
    linear: Easing,
) -> bool {
    let Some(geometry) = geometry else {
        return false;
    };

    match property {
        "size" => {
            if let PV::Vec2(v) = value {
                let half = [v[0] / 2.0, v[1] / 2.0];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(half));
                pt.set_default_value(half);
                pt.add_keyframe(time_ms, half, linear);
            }
            true
        },
        "offset" => {
            if let PV::Vec2(v) = value
                && let Some(ref mut pb_track) = geometry.position_binding
            {
                let current = pb_track.evaluate(time_ms);
                if let animatix::timeline::PositionBinding::SceneAnchor { anchor, .. } = current {
                    let new_binding =
                        animatix::timeline::PositionBinding::SceneAnchor { anchor, offset: *v };
                    pb_track.set_default_value(new_binding);
                    pb_track.add_keyframe(time_ms, new_binding, linear);
                }
            }
            true
        },
        "at" => {
            if let PV::Vec2(v) = value {
                let binding = geometry.position_binding.as_ref().map(|pb| pb.evaluate(time_ms));
                match binding {
                    Some(animatix::timeline::PositionBinding::ScenePercent { .. }) => {
                        if let Some(ref mut pb_track) = geometry.position_binding {
                            let new_binding = animatix::timeline::PositionBinding::ScenePercent {
                                x: v[0],
                                y: v[1],
                                offset: [0.0, 0.0],
                            };
                            pb_track.set_default_value(new_binding);
                            pb_track.add_keyframe(time_ms, new_binding, linear);
                        }
                    },
                    _ => {
                        let pt = geometry.position.get_or_insert_with(|| PropertyTrack::new(*v));
                        pt.set_default_value(*v);
                        pt.add_keyframe(time_ms, *v, linear);
                    },
                }
            }
            true
        },
        "radius" => {
            if let PV::F32(v) = value {
                let size = [*v, *v];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            true
        },
        "radius_x" => {
            if let PV::F32(v) = value {
                let current =
                    geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            true
        },
        "radius_y" => {
            if let PV::F32(v) = value {
                let current =
                    geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            true
        },
        "tip_length" => {
            if let PV::F32(v) = value {
                let current =
                    geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            true
        },
        "tip_width" => {
            if let PV::F32(v) = value {
                let current =
                    geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            true
        },
        _ => false,
    }
}

/// Registry-driven dispatch for standard properties.
fn apply_registry_edit(
    fields: &mut TrackFieldsMut<'_>,
    property: &str,
    value: &PV,
    time_ms: u64,
    linear: Easing,
    legacy_tagged: &mut Vec<(String, PropertyValue)>,
    field_writes: &mut Vec<(ActorField, PropertyValue)>,
) {
    let Some(schema) = lookup_property(property) else {
        return;
    };

    // Tagged properties have no `TrackFieldMut` arm below — `field_mut`
    // hands back the tagged tracks entry, which this path never wrote. Route
    // them through the one tagged writer instead.
    if let ActorField::Tagged(tag) = schema.field {
        write_tagged_value(tag, value.clone(), time_ms, linear, legacy_tagged, field_writes);
        return;
    }

    // Shape-owned fields must keep routing through the plan writer rather
    // than the tier store, matching the pre-migration behaviour.
    let shape_owned = is_shape_field(schema.field);
    if !shape_owned && let Some(field_mut) = fields.field_mut(schema.field) {
        write_track_field(field_mut, value, time_ms, linear);
    } else if is_shape_field(schema.field) {
        // Shape-owned fields moved to the shape component store, so the
        // track-level accessor has no destination for them. Route the write
        // through the world field bundle, which is shape-aware.
        let pv = match (schema.field, value) {
            // The inspector emits the enum choice as a string; the engine
            // writer only converts an Enum payload into a ShapeType.
            (ActorField::ShapeType, PV::String(name)) => PV::Enum(name.clone()),
            _ => coerce_for_value_type(value.clone(), schema.value_type),
        };
        field_writes.push((schema.field, pv));
    }
}

/// Write an inspector value into the matching `TrackFieldMut` slot.
///
/// The five plain value arms share `write_track`; enum-like fields parse the
/// inspector's string first and keep their silent ignore on an unknown choice.
fn write_track_field(field_mut: TrackFieldMut<'_>, value: &PV, time_ms: u64, linear: Easing) {
    use TrackFieldMut as TF;

    match (field_mut, value) {
        (TF::Vec2(f), PV::Vec2(v)) => write_track(f, *v, time_ms, linear),
        (TF::F32(f), PV::F32(v)) => write_track(f, *v, time_ms, linear),
        (TF::Vec4(f), PV::Color(v)) => write_track(f, *v, time_ms, linear),
        (TF::String(f), PV::String(v)) => write_track(f, v.clone(), time_ms, linear),
        (TF::PointList(f), PV::PointList(v)) => write_track(f, v.clone(), time_ms, linear),
        (TF::ShapeType(f), PV::String(v)) => {
            if let Ok(shape) = v.parse::<ShapeType>() {
                write_track(f, shape, time_ms, linear);
            }
        },
        (TF::PlacementMode(f), PV::String(v)) => {
            let mode = match v.as_str() {
                "manual" => Some(PlacementMode::Manual),
                "layout" => Some(PlacementMode::LayoutManaged),
                _ => None,
            };
            if let Some(mode) = mode {
                write_track(f, mode, time_ms, linear);
            }
        },
        (TF::CalloutPlace(f), PV::String(v)) => {
            if let Some(place) = CalloutPlace::from_str(v.as_str()) {
                write_track(f, place, time_ms, linear);
            }
        },
        _ => {},
    }
}

/// Set the default and add a keyframe in one step (the common write pattern).
fn write_track<T: Interpolate>(
    field: &mut Option<PropertyTrack<T>>,
    value: T,
    time_ms: u64,
    linear: Easing,
) {
    let pt = field.get_or_insert_with(|| PropertyTrack::new(value.clone()));
    pt.set_default_value(value.clone());
    pt.add_keyframe(time_ms, value, linear);
}

/// Whether a storage field lives in the shape component store.
fn is_shape_field(field: ActorField) -> bool {
    matches!(
        field,
        ActorField::ShapeType
            | ActorField::LineFrom
            | ActorField::LineTo
            | ActorField::HeadSize
            | ActorField::ArcAngles
            | ActorField::CornerRadius
            | ActorField::Points
            | ActorField::Commands
            | ActorField::VectorPaths
    )
}

/// Apply an edit to a shape-tier angle property (`start_angle`/`sweep_angle`).
///
/// The shape tier is a `World` component, so the caller passes the entity's
/// shape tier in explicitly. Returns `true` when the
/// edit was a shape-angle edit and has been applied.
pub(super) fn apply_shape_angle_edit(
    shape: &mut animatix::timeline::animation_track::ShapeTracks,
    property: &str,
    value: &panels::PropertyValue,
    time_ms: u64,
) -> bool {
    let linear = Easing::Linear;
    match property {
        "start_angle" => {
            let PV::F32(v) = value else {
                return false;
            };
            let current = shape.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
            let angles = [*v, current[1]];
            let pt = shape.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
            pt.set_default_value(angles);
            pt.add_keyframe(time_ms, angles, linear);
            true
        },
        "sweep_angle" => {
            let PV::F32(v) = value else {
                return false;
            };
            let current = shape.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
            let angles = [current[0], *v];
            let pt = shape.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
            pt.set_default_value(angles);
            pt.add_keyframe(time_ms, angles, linear);
            true
        },
        _ => false,
    }
}
