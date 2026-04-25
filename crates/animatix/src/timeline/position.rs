use super::property_lookup::parse_numeric_vec2_with_lookup_diagnostic;
use super::{
    AnimationTrack, Environment, Interpolate, PlacementMode, PositionBinding, PropertyTrack,
    SceneAnchor, SceneDimensions,
};
use crate::ast::Expr;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;

pub(crate) fn parse_scene_anchor(expr: &Expr) -> Option<SceneAnchor> {
    match expr {
        Expr::Path(parts) if parts.len() == 2 && parts[0] == "scene" => match parts[1].as_str() {
            "top_left" => Some(SceneAnchor::TopLeft),
            "top" => Some(SceneAnchor::Top),
            "top_right" => Some(SceneAnchor::TopRight),
            "left" => Some(SceneAnchor::Left),
            "center" => Some(SceneAnchor::Center),
            "right" => Some(SceneAnchor::Right),
            "bottom_left" => Some(SceneAnchor::BottomLeft),
            "bottom" => Some(SceneAnchor::Bottom),
            "bottom_right" => Some(SceneAnchor::BottomRight),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn parse_percent_vec2(expr: &Expr) -> Option<[f32; 2]> {
    match expr {
        Expr::Tuple(items) if items.len() == 2 => match (&items[0], &items[1]) {
            (Expr::Percent(x), Expr::Percent(y)) => {
                Some([(*x as f32) / 100.0, (*y as f32) / 100.0])
            }
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn resolve_position_binding_with_lookup_diagnostic(
    at_expr: Option<&Expr>,
    anchor_expr: Option<&Expr>,
    offset_expr: Option<&Expr>,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<(PositionBinding, Option<[f32; 2]>)> {
    let offset = offset_expr
        .and_then(|expr| parse_numeric_vec2_with_lookup_diagnostic(expr, env, diagnostics, subject))
        .unwrap_or([0.0, 0.0]);

    if let Some(anchor_expr) = anchor_expr {
        if let Some(anchor) = parse_scene_anchor(anchor_expr) {
            return Some((PositionBinding::SceneAnchor { anchor, offset }, None));
        }
    }

    if let Some(at_expr) = at_expr {
        if let Some(anchor) = parse_scene_anchor(at_expr) {
            return Some((PositionBinding::SceneAnchor { anchor, offset }, None));
        }

        if let Some([x, y]) = parse_percent_vec2(at_expr) {
            return Some((PositionBinding::ScenePercent { x, y, offset }, None));
        }

        if let Some(position) =
            parse_numeric_vec2_with_lookup_diagnostic(at_expr, env, diagnostics, subject)
        {
            return Some((PositionBinding::Absolute, Some(position)));
        }
    }

    None
}

pub(crate) fn scene_anchor_point(
    anchor: SceneAnchor,
    scene_dimensions: SceneDimensions,
) -> kurbo::Point {
    let width = scene_dimensions.width as f64;
    let height = scene_dimensions.height as f64;
    match anchor {
        SceneAnchor::TopLeft => kurbo::Point::new(0.0, 0.0),
        SceneAnchor::Top => kurbo::Point::new(width / 2.0, 0.0),
        SceneAnchor::TopRight => kurbo::Point::new(width, 0.0),
        SceneAnchor::Left => kurbo::Point::new(0.0, height / 2.0),
        SceneAnchor::Center => kurbo::Point::new(width / 2.0, height / 2.0),
        SceneAnchor::Right => kurbo::Point::new(width, height / 2.0),
        SceneAnchor::BottomLeft => kurbo::Point::new(0.0, height),
        SceneAnchor::Bottom => kurbo::Point::new(width / 2.0, height),
        SceneAnchor::BottomRight => kurbo::Point::new(width, height),
    }
}

pub(crate) fn resolve_bound_position(
    binding: PositionBinding,
    base_position: [f32; 2],
    parent_transform: kurbo::Affine,
    scene_dimensions: SceneDimensions,
) -> [f32; 2] {
    let scene_point = match binding {
        PositionBinding::Absolute => return base_position,
        PositionBinding::SceneAnchor { anchor, offset } => {
            let point = scene_anchor_point(anchor, scene_dimensions);
            kurbo::Point::new(point.x + offset[0] as f64, point.y + offset[1] as f64)
        }
        PositionBinding::ScenePercent { x, y, offset } => kurbo::Point::new(
            scene_dimensions.width as f64 * x as f64 + offset[0] as f64,
            scene_dimensions.height as f64 * y as f64 + offset[1] as f64,
        ),
        PositionBinding::ContainerDefault { anchor } => {
            scene_anchor_point(anchor, scene_dimensions)
        }
    };

    let local_point = parent_transform.inverse() * scene_point;
    [local_point.x as f32, local_point.y as f32]
}

pub(crate) fn mark_track_manual_position(track: &mut AnimationTrack, time_ms: u64) {
    track
        .placement_mode
        .ensure(PlacementMode::LayoutManaged)
        .add_keyframe(time_ms, PlacementMode::Manual, Easing::Linear);
}

pub(crate) fn preserve_discrete_position_state_before(track: &mut AnimationTrack, time_ms: u64) {
    if time_ms == 0 {
        return;
    }

    let previous_time = time_ms - 1;

    if !track.placement_mode.as_ref().map(|t| t.keyframes.contains_key(&previous_time)).unwrap_or(false) {
        let previous_mode = track.placement_mode.get(previous_time, PlacementMode::LayoutManaged);
        track
            .placement_mode
            .ensure(PlacementMode::LayoutManaged)
            .add_keyframe(previous_time, previous_mode, Easing::Linear);
    }

    if !track
        .position_binding
        .as_ref()
        .map(|t| t.keyframes.contains_key(&previous_time))
        .unwrap_or(false)
    {
        let previous_binding = track.position_binding.get(previous_time, PositionBinding::Absolute);
        track
            .position_binding
            .ensure(PositionBinding::Absolute)
            .add_keyframe(previous_time, previous_binding, Easing::Linear);
    }
}

pub(crate) fn preserve_instant_delayed_value<T: Interpolate + Clone>(
    track: &mut Option<PropertyTrack<T>>,
    t_start_ms: u64,
) where
    T: Default,
{
    if t_start_ms == 0 {
        return;
    }

    let previous_time = t_start_ms.saturating_sub(1);

    // Ensure the track exists (creating with default value if needed)
    let inner = track.ensure(T::default());

    if inner.keyframes.contains_key(&previous_time) {
        return;
    }

    let previous_value = inner.evaluate(previous_time);
    inner.add_keyframe(previous_time, previous_value, Easing::Linear);
}

pub(crate) fn set_track_position_binding(
    track: &mut AnimationTrack,
    time_ms: u64,
    binding: PositionBinding,
) {
    track
        .position_binding
        .ensure(PositionBinding::Absolute)
        .add_keyframe(time_ms, binding, Easing::Linear);
}

pub(crate) fn apply_explicit_position_binding(
    track: &mut AnimationTrack,
    time_ms: u64,
    binding: PositionBinding,
    position: Option<[f32; 2]>,
) {
    mark_track_manual_position(track, time_ms);
    set_track_position_binding(track, time_ms, binding);
    if let Some(position) = position {
        track
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(time_ms, position, Easing::Linear);
    }
}
