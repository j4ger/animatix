use animatix::timeline::{EntityId, ResizeMode, ShapeKind, Timeline};

/// Interactive affordances exposed by an actor on the preview canvas.
#[derive(Clone, Debug, PartialEq)]
pub enum GizmoAffordance {
    /// Bounding box with sizing or scaling handles.
    BoundingBox { resize_mode: ResizeMode },
    /// Direct vertex manipulation (Polygon, Line, Arrow, Path).
    ControlPoints,
    /// Targeted annotation handle (Callout tip, label anchor, standoff).
    Callout,
    /// Inline text editing surface.
    InlineText { property: &'static str },
}

/// Determine all gizmo affordances for a given entity.
pub fn actor_gizmo_affordances(id: EntityId, timeline: &Timeline) -> Vec<GizmoAffordance> {
    let mut affordances = Vec::new();
    let caps = timeline.world().caps(id);

    // 1. Resize mode & bounding box
    let registry = timeline.primitive_registry_snapshot();
    let actor_type = timeline.world().actor_type(id);
    let resize_mode = if let Some(primitive) = registry.find(actor_type) {
        primitive.resize_mode(&caps)
    } else {
        ResizeMode::Size
    };
    affordances.push(GizmoAffordance::BoundingBox { resize_mode });

    // 2. Control points for shapes with editable vertices
    if timeline.world().shape(id).points.is_some() || matches!(caps.shape, Some(ShapeKind::Polygon))
    {
        affordances.push(GizmoAffordance::ControlPoints);
    }

    // 3. Callout annotation handles
    if caps.callout {
        affordances.push(GizmoAffordance::Callout);
    }

    // 4. Inline text editing
    if let Some(property) = get_track_text_property(actor_type, caps, &registry) {
        affordances.push(GizmoAffordance::InlineText { property });
    }

    affordances
}

/// Get the text property name for inline editing, if supported by the track.
pub fn get_track_text_property(
    actor_type: &str,
    caps: animatix::timeline::ActorCaps,
    registry: &animatix::primitives::PrimitiveRegistry,
) -> Option<&'static str> {
    match caps.text {
        Some(animatix::timeline::TextKind::Code) => Some("code"),
        Some(_) => Some("text"),
        None => registry
            .info_of(actor_type)
            .filter(|info| info.capabilities.text_paths)
            .map(|_| "text"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_affordances_include_bounding_box() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let affordances = actor_gizmo_affordances(id, &timeline);
        assert!(affordances.contains(&GizmoAffordance::BoundingBox {
            resize_mode: ResizeMode::Size
        }));
        assert!(!affordances.contains(&GizmoAffordance::Callout));
    }

    #[test]
    fn callout_affordances_include_callout_and_text() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("c", "Callout");
        // An annotation callout does not claim a text engine by default; the
        // fixture overrides caps to exercise the inline editing affordance.
        let mut caps = timeline.world().caps(id);
        caps.callout = true;
        caps.text = Some(animatix::timeline::TextKind::Text);
        timeline.world_mut().set_caps(id, caps);
        let affordances = actor_gizmo_affordances(id, &timeline);
        assert!(affordances.contains(&GizmoAffordance::Callout));
        assert!(affordances.contains(&GizmoAffordance::InlineText { property: "text" }));
    }
}
