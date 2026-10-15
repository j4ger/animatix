use animatix::timeline::{AnimationTrack, ResizeMode, ShapeKind, Timeline};

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

/// Determine all gizmo affordances for a given track.
pub fn actor_gizmo_affordances(
    track: &AnimationTrack,
    timeline: &Timeline,
) -> Vec<GizmoAffordance> {
    let mut affordances = Vec::new();

    // 1. Resize mode & bounding box
    let registry = timeline.primitive_registry_snapshot();
    let resize_mode = if let Some(primitive) = registry.find(&track.actor_type) {
        primitive.resize_mode(&track.caps)
    } else {
        ResizeMode::Size
    };
    affordances.push(GizmoAffordance::BoundingBox { resize_mode });

    // 2. Control points for shapes with editable vertices
    if track.shape.points.is_some() || matches!(track.caps.shape, Some(ShapeKind::Polygon)) {
        affordances.push(GizmoAffordance::ControlPoints);
    }

    // 3. Callout annotation handles
    if track.caps.callout {
        affordances.push(GizmoAffordance::Callout);
    }

    // 4. Inline text editing
    if let Some(property) = get_track_text_property(track, &registry) {
        affordances.push(GizmoAffordance::InlineText { property });
    }

    affordances
}

/// Get the text property name for inline editing, if supported by the track.
pub fn get_track_text_property(
    track: &AnimationTrack,
    registry: &animatix::primitives::PrimitiveRegistry,
) -> Option<&'static str> {
    match track.caps.text {
        Some(animatix::timeline::TextKind::Code) => Some("code"),
        Some(_) => Some("text"),
        None => registry
            .info_of(&track.actor_type)
            .filter(|info| info.capabilities.text_paths)
            .map(|_| "text"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_affordances_include_bounding_box() {
        let timeline = Timeline::new();
        let track = AnimationTrack::new("box".into(), "Rect");
        let affordances = actor_gizmo_affordances(&track, &timeline);
        assert!(affordances.contains(&GizmoAffordance::BoundingBox {
            resize_mode: ResizeMode::Size
        }));
        assert!(!affordances.contains(&GizmoAffordance::Callout));
    }

    #[test]
    fn callout_affordances_include_callout_and_text() {
        let timeline = Timeline::new();
        let mut track = AnimationTrack::new("c".into(), "Callout");
        track.caps.callout = true;
        track.caps.text = Some(animatix::timeline::TextKind::Text);
        let affordances = actor_gizmo_affordances(&track, &timeline);
        assert!(affordances.contains(&GizmoAffordance::Callout));
        assert!(affordances.contains(&GizmoAffordance::InlineText { property: "text" }));
    }
}
