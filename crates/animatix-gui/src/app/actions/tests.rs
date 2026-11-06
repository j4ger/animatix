use animatix::timeline::{ActorField, PropertyValue};

use crate::app::panels::PropertyValue as PV;

use super::property_edit::apply_property_edit_to_entity;

/// A GUI edit of a registry-backed tagged property must land in the store
/// the engine reads (`property_plan`), not only in `tagged_tracks`.
#[test]
fn gui_tagged_edit_is_visible_through_property_plan() {
    let mut timeline = animatix::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("box", "Rect");
    timeline.world_mut().rebuild_property_plan(id);
    let mut legacy_tagged = Vec::new();
    let mut field_writes = Vec::new();
    apply_property_edit_to_entity(
        timeline.world_mut(),
        id,
        "solo",
        &PV::Bool(true),
        1000,
        &mut legacy_tagged,
        &mut field_writes,
    );
    for (field, value) in field_writes {
        timeline.world_mut().write_property_field(
            id,
            field,
            value,
            1000,
            1000,
            animatix_syntax::easing::Easing::Linear,
            &mut Vec::new(),
        );
    }

    assert_eq!(
        timeline.world().read_property_value(id, ActorField::Tagged("solo"), 1000),
        Some(PropertyValue::Bool(true)),
        "solo edit should be readable at the edited time"
    );
    // An instant write is fenced, so it holds for every later time too.
    assert_eq!(
        timeline.world().read_property_value(id, ActorField::Tagged("solo"), 5000),
        Some(PropertyValue::Bool(true)),
        "solo edit should still be visible after the edited time"
    );
}

/// A GUI edit of a shape-tier property must reach the shape component
/// store: the track itself does not carry a `shape` field.
#[test]
fn gui_shape_edit_reaches_the_shape_tier() {
    let mut timeline = animatix::timeline::Timeline::new();
    let id = timeline.world_mut().spawn("box", "Rect");
    let mut legacy_tagged = Vec::new();
    let mut field_writes = Vec::new();
    apply_property_edit_to_entity(
        timeline.world_mut(),
        id,
        "corner_radius",
        &PV::F32(12.0),
        500,
        &mut legacy_tagged,
        &mut field_writes,
    );
    assert!(
        field_writes.iter().any(|(field, _)| *field == ActorField::CornerRadius),
        "shape-owned property edits must route through the world field bundle"
    );
    for (field, value) in field_writes {
        timeline.world_mut().write_property_field(
            id,
            field,
            value,
            500,
            500,
            animatix_syntax::easing::Easing::Linear,
            &mut Vec::new(),
        );
    }

    assert_eq!(
        timeline.world().read_property_value(id, ActorField::CornerRadius, 500),
        Some(PropertyValue::F32(12.0)),
        "corner_radius edit should be readable from the shape tier"
    );
}
