use super::property_lookup::parse_numeric_vec2_with_lookup_diagnostic;
use super::{
    AnimationTrack, Environment, Interpolate, PlacementMode, PositionBinding, PropertyTrack,
    SceneAnchor, SceneDimensions,
};
use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
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

    // ── Conflict detection ──
    // Warn when both `at` and `anchor` specify a position; `anchor` takes precedence.
    let at_resolves = at_expr.map(|expr| {
        parse_scene_anchor(expr).is_some()
            || parse_percent_vec2(expr).is_some()
            || parse_numeric_vec2_with_lookup_diagnostic(expr, env, diagnostics, subject).is_some()
    }).unwrap_or(false);

    if anchor_expr.is_some() && at_resolves {
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::ConflictingPositionBinding,
                DiagnosticPhase::Build,
                format!(
                    "`at` and `anchor` both specify position for '{subject}'; `anchor` takes precedence."
                ),
            )
            .with_subject(subject),
        );
    }

    if let Some(anchor_expr) = anchor_expr {
        if let Some(anchor) = parse_scene_anchor(anchor_expr) {
            return Some((PositionBinding::SceneAnchor { anchor, offset }, None));
        }
        if let Some([x, y]) = parse_percent_vec2(anchor_expr) {
            return Some((PositionBinding::ScenePercent { x, y, offset }, None));
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
            if offset != [0.0, 0.0] {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::IgnoredOffset,
                        DiagnosticPhase::Build,
                        format!(
                            "`offset` has no effect with absolute `at` on '{subject}'; the value is ignored."
                        ),
                    )
                    .with_subject(subject),
                );
            }
            return Some((PositionBinding::Absolute, Some(position)));
        }
    }

    None
}

/// Compute the scene-space anchor point for the given anchor and scene dimensions.
pub fn scene_anchor_point(
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
        PositionBinding::ContainerPercent { x, y } => {
            // Container-managed position is in base_position.
            // Apply percentage offset and return directly (already in local coords).
            return [
                base_position[0] + scene_dimensions.width as f32 * x,
                base_position[1] + scene_dimensions.height as f32 * y,
            ];
        }
    };

    let local_point = parent_transform.inverse() * scene_point;
    [local_point.x as f32, local_point.y as f32]
}

pub(crate) fn mark_track_manual_position(track: &mut AnimationTrack, time_ms: u64) {
    track
        .geometry
        .placement_mode
        .ensure(PlacementMode::LayoutManaged)
        .add_keyframe(time_ms, PlacementMode::Manual, Easing::Linear);
}

pub(crate) fn preserve_discrete_position_state_before(track: &mut AnimationTrack, time_ms: u64) {
    if time_ms == 0 {
        return;
    }

    let previous_time = time_ms - 1;

    if !track.geometry.placement_mode.as_ref().map(|t| t.keyframes.contains_key(&previous_time)).unwrap_or(false) {
        let previous_mode = track.geometry.placement_mode.get(previous_time, PlacementMode::LayoutManaged);
        track
            .geometry
            .placement_mode
            .ensure(PlacementMode::LayoutManaged)
            .add_keyframe(previous_time, previous_mode, Easing::Linear);
    }

    if !track
        .geometry
        .position_binding
        .as_ref()
        .map(|t| t.keyframes.contains_key(&previous_time))
        .unwrap_or(false)
    {
        let previous_binding = track.geometry.position_binding.get(previous_time, PositionBinding::Absolute);
        track
            .geometry
            .position_binding
            .ensure(PositionBinding::Absolute)
            .add_keyframe(previous_time, previous_binding, Easing::Linear);
    }
}

pub(crate) fn preserve_instant_delayed_value<T: Interpolate + Default>(
    track: &mut Option<PropertyTrack<T>>,
    t_start_ms: u64,
) {
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
        .geometry
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
            .geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(time_ms, position, Easing::Linear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expr;
    use crate::diagnostics::{DiagnosticCode, DiagnosticPhase, DiagnosticSeverity};
    use crate::timeline::Environment;

    fn has_warning_with_code(diagnostics: &[Diagnostic], code: DiagnosticCode) -> bool {
        diagnostics.iter().any(|d| {
            d.code == code
                && matches!(d.severity, DiagnosticSeverity::Warning)
                && matches!(d.phase, DiagnosticPhase::Build)
        })
    }

    #[test]
    fn conflicting_at_and_anchor_emits_warning() {
        let at = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        let anchor = Expr::Path(vec!["scene".to_string(), "center".to_string()]);
        let mut diagnostics = Vec::new();
        let env = Environment::new();

        let result = resolve_position_binding_with_lookup_diagnostic(
            Some(&at),
            Some(&anchor),
            None,
            &env,
            &mut diagnostics,
            "test_actor",
        );

        assert!(result.is_some(), "Should resolve a binding");
        assert!(
            has_warning_with_code(&diagnostics, DiagnosticCode::ConflictingPositionBinding),
            "Should emit ConflictingPositionBinding warning"
        );
        // anchor takes precedence
        assert!(
            matches!(result.unwrap().0, PositionBinding::SceneAnchor { .. }),
            "Anchor should take precedence"
        );
    }

    #[test]
    fn ignored_offset_with_absolute_at_emits_warning() {
        let at = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        let offset = Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)]);
        let mut diagnostics = Vec::new();
        let env = Environment::new();

        let result = resolve_position_binding_with_lookup_diagnostic(
            Some(&at),
            None,
            Some(&offset),
            &env,
            &mut diagnostics,
            "test_actor",
        );

        assert!(result.is_some());
        assert!(
            has_warning_with_code(&diagnostics, DiagnosticCode::IgnoredOffset),
            "Should emit IgnoredOffset warning when offset is used with absolute at"
        );
        // offset should be ignored, position should be (100, 200)
        assert_eq!(result.unwrap().1, Some([100.0, 200.0]));
    }

    #[test]
    fn absolute_at_without_offset_no_warning() {
        let at = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        let mut diagnostics = Vec::new();
        let env = Environment::new();

        let result = resolve_position_binding_with_lookup_diagnostic(
            Some(&at),
            None,
            None,
            &env,
            &mut diagnostics,
            "test_actor",
        );

        assert!(result.is_some());
        assert!(
            diagnostics.is_empty(),
            "Should not emit any warnings for absolute at without offset"
        );
    }

    #[test]
    fn anchor_with_percent_no_warning() {
        let anchor = Expr::Tuple(vec![Expr::Percent(50.0), Expr::Percent(60.0)]);
        let mut diagnostics = Vec::new();
        let env = Environment::new();

        let result = resolve_position_binding_with_lookup_diagnostic(
            None,
            Some(&anchor),
            None,
            &env,
            &mut diagnostics,
            "test_actor",
        );

        assert!(result.is_some());
        assert!(
            diagnostics.is_empty(),
            "Should not emit any warnings for anchor with percentages"
        );
        assert!(
            matches!(result.unwrap().0, PositionBinding::ScenePercent { x: 0.5, y: 0.6, offset: [0.0, 0.0] }),
            "Anchor should accept percentage tuples"
        );
    }

    #[test]
    fn anchor_with_offset_no_warning() {
        let anchor = Expr::Path(vec!["scene".to_string(), "center".to_string()]);
        let offset = Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)]);
        let mut diagnostics = Vec::new();
        let env = Environment::new();

        let result = resolve_position_binding_with_lookup_diagnostic(
            None,
            Some(&anchor),
            Some(&offset),
            &env,
            &mut diagnostics,
            "test_actor",
        );

        assert!(result.is_some());
        assert!(
            diagnostics.is_empty(),
            "Should not emit any warnings for anchor with offset"
        );
        assert!(
            matches!(result.unwrap().0, PositionBinding::SceneAnchor { anchor: SceneAnchor::Center, offset: [10.0, 20.0] }),
            "Offset should be applied to anchor binding"
        );
    }

}
