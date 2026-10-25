//! Property-edit application for the timeline (legend, registry, effects).

use super::*;
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
    legacy_tagged: &mut Vec<(String, animatix::timeline::PropertyValue)>,
    field_writes: &mut Vec<(animatix::timeline::ActorField, animatix::timeline::PropertyValue)>,
) {
    use crate::app::panels::PropertyValue as PV;
    use animatix::timeline::{PropertyTrack, TrackAccessor, TrackFieldMut};

    let linear = animatix_syntax::easing::Easing::Linear;

    // `scope.stage.param` edits address the scope's effect chain.
    if let Some((stage, param)) = property.split_once('.') {
        if let Some(effects) = world.effects_mut(id) {
            if effects.write_param(stage, param, value.clone(), time_ms, time_ms, linear) {
                return;
            }
        }
    }

    /// Normalise an inspector value to the kind its plan slot accepts.
    ///
    /// The inspector emits `F32` for every numeric field, including `U32`
    /// properties (`decimals`, `max_lines`), and colours as `Color` even though
    /// the plan stores every `Color`/`Vec4` descriptor in a `Vec4` slot. A plan
    /// slot rejects a value of another kind, so an uncoerced write would fall
    /// back to the legacy map and be shadowed by the plan the engine reads.
    fn coerce_for_value_type(
        pv: animatix::timeline::PropertyValue,
        value_type: animatix::timeline::ValueType,
    ) -> animatix::timeline::PropertyValue {
        use animatix::timeline::{PropertyValue, ValueType};
        match (value_type, pv) {
            (ValueType::U32, PropertyValue::F32(v)) => {
                PropertyValue::U32(v.max(0.0).round() as u32)
            },
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
    /// (`legend_title`, `legend_gap`, ...) keep the legacy map, which is where
    /// their readers look.
    fn write_tagged_value(
        key: &str,
        pv: animatix::timeline::PropertyValue,
        _time_ms: u64,
        _linear: animatix_syntax::easing::Easing,
        legacy_tagged: &mut Vec<(String, animatix::timeline::PropertyValue)>,
        field_writes: &mut Vec<(animatix::timeline::ActorField, animatix::timeline::PropertyValue)>,
    ) {
        use animatix::timeline::{ActorField, lookup_property};

        if let Some(schema) = lookup_property(key)
            && let ActorField::Tagged(tag) = schema.field
        {
            let pv = coerce_for_value_type(pv, schema.value_type);
            field_writes.push((ActorField::Tagged(tag), pv));
            return;
        }

        legacy_tagged.push((key.to_string(), pv));
    }

    if property == "place" {
        if let PV::String(choice) = value
            && let Some(place) = animatix::timeline::animation_track::CalloutPlace::from_str(choice)
        {
            world
                .geometry_mut(id)
                .callout_place
                .ensure(animatix::timeline::animation_track::CalloutPlace::Right)
                .add_keyframe(time_ms, place, linear);
            legacy_tagged.push((
                "callout_place".to_string(),
                animatix::timeline::PropertyValue::Enum(choice.clone()),
            ));
        }
        return;
    }

    if property == "legend" {
        let pv = match value {
            PV::Bool(true) => animatix::timeline::PropertyValue::Variant {
                name: "auto".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::Bool(true)),
            },
            PV::Bool(false) => animatix::timeline::PropertyValue::Variant {
                name: "hidden".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::Bool(false)),
            },
            PV::String(label) => animatix::timeline::PropertyValue::Variant {
                name: "label".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::String(label.clone())),
            },
            _ => return,
        };
        legacy_tagged.push(("legend".to_string(), pv));
        return;
    }

    // World-aware capabilities: the projection carries any per-entity
    // overrides the catalog derivation alone would miss.
    let caps = world.caps(id);
    if caps.legend_host {
        let legend = world.ensure_legend(id);
        match property {
            "title" => {
                if let PV::String(v) = value {
                    legend.title = v.clone();
                    write_tagged_value(
                        "legend_title",
                        animatix::timeline::PropertyValue::String(v.clone()),
                        time_ms,
                        linear,
                        legacy_tagged,
                        field_writes,
                    );
                }
                return;
            },
            "font_size" => {
                if let PV::F32(v) = value {
                    legend.font_size = *v;
                    write_tagged_value(
                        "legend_font_size",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                        legacy_tagged,
                        field_writes,
                    );
                }
                return;
            },
            "swatch_size" => {
                if let PV::F32(v) = value {
                    legend.swatch_size = *v;
                    write_tagged_value(
                        "legend_swatch_size",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                        legacy_tagged,
                        field_writes,
                    );
                }
                return;
            },
            "gap" => {
                if let PV::F32(v) = value {
                    legend.gap = *v;
                    write_tagged_value(
                        "legend_gap",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                        legacy_tagged,
                        field_writes,
                    );
                }
                return;
            },
            "text_max_width" => {
                if let PV::F32(v) = value {
                    legend.text_max_width = *v;
                    write_tagged_value(
                        "legend_text_max_width",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                        legacy_tagged,
                        field_writes,
                    );
                }
                return;
            },
            "label_color" => {
                if let PV::String(v) = value {
                    let parsed = parse_gui_color(v);
                    legend.label_color = parsed;
                    if let Some(color) = parsed {
                        write_tagged_value(
                            "legend_label_color",
                            animatix::timeline::PropertyValue::Color(color),
                            time_ms,
                            linear,
                            legacy_tagged,
                            field_writes,
                        );
                    }
                }
                return;
            },
            _ => {},
        }
    }

    // ── Special / compound properties (not covered by generic registry dispatch) ──
    // The geometry tier lives in the world component store, so borrow it for
    // the whole block and then fall through to the world-aware field bundle
    // for the registry-driven dispatch below.
    let Some(mut fields) = world.fields_mut(id) else {
        return;
    };
    let special = if let Some(geometry) = fields.geometry.as_mut() {
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
                if let PV::Vec2(v) = value {
                    if let Some(ref mut pb_track) = geometry.position_binding {
                        let current = pb_track.evaluate(time_ms);
                        if let animatix::timeline::PositionBinding::SceneAnchor { anchor, .. } =
                            current
                        {
                            let new_binding = animatix::timeline::PositionBinding::SceneAnchor {
                                anchor,
                                offset: *v,
                            };
                            pb_track.set_default_value(new_binding);
                            pb_track.add_keyframe(time_ms, new_binding, linear);
                        }
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
                                let new_binding =
                                    animatix::timeline::PositionBinding::ScenePercent {
                                        x: v[0],
                                        y: v[1],
                                        offset: [0.0, 0.0],
                                    };
                                pb_track.set_default_value(new_binding);
                                pb_track.add_keyframe(time_ms, new_binding, linear);
                            }
                        },
                        _ => {
                            let pt =
                                geometry.position.get_or_insert_with(|| PropertyTrack::new(*v));
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
    } else {
        false
    };
    if special {
        return;
    }

    // ── Registry-driven dispatch for standard properties ──
    if let Some(schema) = animatix::timeline::property_registry::lookup_property(property) {
        // Tagged properties have no `TrackFieldMut` arm below — `field_mut`
        // hands back the legacy map entry, which nothing here used to write.
        // Route them through the one tagged writer instead.
        if let animatix::timeline::ActorField::Tagged(tag) = schema.field {
            write_tagged_value(tag, value.clone(), time_ms, linear, legacy_tagged, field_writes);
            return;
        }
        // Shape-owned fields must keep routing through the plan writer rather
        // than the tier store, matching the pre-migration behaviour.
        let shape_owned = is_shape_field(schema.field);
        if !shape_owned && let Some(field_mut) = fields.field_mut(schema.field) {
            match (field_mut, value) {
                (TrackFieldMut::Vec2(f), PV::Vec2(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::F32(f), PV::F32(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::Vec4(f), PV::Color(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::String(f), PV::String(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                    pt.set_default_value(v.clone());
                    pt.add_keyframe(time_ms, v.clone(), linear);
                },
                (TrackFieldMut::PointList(f), PV::PointList(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                    pt.set_default_value(v.clone());
                    pt.add_keyframe(time_ms, v.clone(), linear);
                },
                (TrackFieldMut::ShapeType(f), PV::String(v)) => {
                    if let Ok(shape) = v.parse::<animatix::timeline::ShapeType>() {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(shape));
                        pt.set_default_value(shape);
                        pt.add_keyframe(time_ms, shape, linear);
                    }
                },
                (TrackFieldMut::PlacementMode(f), PV::String(v)) => {
                    let mode = match v.as_str() {
                        "manual" => Some(animatix::timeline::PlacementMode::Manual),
                        "layout" => Some(animatix::timeline::PlacementMode::LayoutManaged),
                        _ => None,
                    };
                    if let Some(mode) = mode {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(mode));
                        pt.set_default_value(mode);
                        pt.add_keyframe(time_ms, mode, linear);
                    }
                },
                (TrackFieldMut::CalloutPlace(f), PV::String(v)) => {
                    if let Some(place) =
                        animatix::timeline::animation_track::CalloutPlace::from_str(v.as_str())
                    {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(place));
                        pt.set_default_value(place);
                        pt.add_keyframe(time_ms, place, linear);
                    }
                },
                _ => {},
            }
        } else if is_shape_field(schema.field) {
            // Shape-owned fields moved to the shape component store, so the
            // track-level accessor has no destination for them. Route the write
            // through the world field bundle, which is shape-aware.
            let pv = match (schema.field, value) {
                // The inspector emits the enum choice as a string; the engine
                // writer only converts an Enum payload into a ShapeType.
                (animatix::timeline::ActorField::ShapeType, PV::String(name)) => {
                    PV::Enum(name.clone())
                },
                _ => coerce_for_value_type(value.clone(), schema.value_type),
            };
            field_writes.push((schema.field, pv));
        }
    }
}

/// Whether a storage field lives in the shape component store.
fn is_shape_field(field: animatix::timeline::ActorField) -> bool {
    use animatix::timeline::ActorField;
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
    use animatix::timeline::{PropertyTrack, TrackAccessor};

    use crate::app::panels::PropertyValue as PV;

    let linear = animatix_syntax::easing::Easing::Linear;
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
