use super::*;
use crate::ast::Expr;
use crate::timeline::animation_track::{GeometryTracks, ShapeTracks, StyleTracks};

// ─────────────────────────────────────────────────────────────
// Helper: size-like assignments (also write to layout_size)
// ─────────────────────────────────────────────────────────────

pub(super) fn handle_size_assignment(
    track: &mut AnimationTrack,
    actor_type: &str,
    style: &StyleTracks,
    geometry: &mut GeometryTracks,
    shape: &mut ShapeTracks,
    property: &str,
    value: &Expr,
    env: &Environment,
    subject: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    diagnostics: &mut Vec<Diagnostic>,
    coordinate_frame: Option<&crate::timeline::CoordinateFrameComponent>,
) -> [f32; 2] {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let has_duration = t_end_ms > t_start_ms;

    let target_size = match property {
        "size" => {
            // Accept a Vec2 or an all-numeric 2-element brace list
            // (`size = {40, 80}`); anything else keeps the previous size.
            match evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject) {
                Some(Value::Vec2([w, h])) => [w as f32 / 2.0, h as f32 / 2.0],
                Some(Value::List(items)) if items.len() == 2 => {
                    let dims: Option<Vec<f64>> = items
                        .iter()
                        .map(|item| match item {
                            Value::Num(n) => Some(*n),
                            _ => None,
                        })
                        .collect();
                    match dims {
                        Some(dims) => [dims[0] as f32 / 2.0, dims[1] as f32 / 2.0],
                        None => geometry.size.last(default_size),
                    }
                },
                _ => geometry.size.last(default_size),
            }
        },
        "radius_x" => {
            let mut s = geometry.size.last(default_size);
            s[0] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[0]);
            s
        },
        "radius_y" => {
            let mut s = geometry.size.last(default_size);
            s[1] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[1]);
            s
        },
        _ => geometry.size.last(default_size),
    };

    if has_duration {
        let start_val = geometry.size.get(t_start_ms, default_size);
        geometry
            .size
            .ensure(default_size)
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        if let Some(layout_start) = track.layout_size_get(geometry, t_start_ms) {
            track.ensure_layout_size(geometry, default_size).add_keyframe(
                t_start_ms,
                layout_start,
                Easing::Linear,
            );
        }
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut geometry.size, t_start_ms);
        preserve_instant_delayed_value(&mut geometry.layout_size, t_start_ms);
    }
    geometry.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
    track
        .ensure_layout_size(geometry, default_size)
        .add_keyframe(t_end_ms, target_size, easing);

    // Rebuild vector paths after size change
    rebuild_vector_paths(
        track,
        actor_type,
        style,
        geometry,
        shape,
        t_start_ms,
        t_end_ms,
        easing,
        diagnostics,
        coordinate_frame,
    );

    target_size
}

pub(super) fn rebuild_vector_paths(
    track: &mut AnimationTrack,
    actor_type: &str,
    style: &StyleTracks,
    geometry: &GeometryTracks,
    shape: &mut ShapeTracks,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    _diagnostics: &mut Vec<Diagnostic>,
    coordinate_frame: Option<&crate::timeline::CoordinateFrameComponent>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, 0.0];
    let has_duration = t_end_ms > t_start_ms;
    let size = geometry.size.last(default_size);
    let shape_type = shape.shape_type.last(ShapeType::Rect);

    // ── Special case: Graph actors rebuild axis/grid/tick paths ──
    if shape_type == ShapeType::Graph {
        // The host's coordinate frame component is the only source for the
        // rebuilt axes. An absent frame degrades to the component defaults
        // (linear scales, [-10, 10] domains), never to an env fallback.
        let frame = coordinate_frame.cloned().unwrap_or_default();
        let stroke_color = style.stroke_color.last(DEFAULT_WHITE);
        let new_paths = build_graph_axis_paths(
            size,
            frame.x_domain,
            frame.y_domain,
            stroke_color,
            frame.grid,
            frame.ticks,
            matches!(frame.tick_labels.as_str(), "auto" | "true" | "both"),
            frame.padding,
            frame.x_scale,
            frame.y_scale,
        );

        if has_duration {
            let start_paths = track.evaluate_vector_paths_value(shape, style, t_start_ms);
            shape.vector_paths.ensure(Vec::new()).add_keyframe(
                t_start_ms,
                start_paths,
                Easing::Linear,
            );
        } else if t_end_ms > 0 {
            preserve_instant_delayed_value(&mut shape.vector_paths, t_end_ms);
        }
        shape.vector_paths.ensure(Vec::new()).add_keyframe(t_end_ms, new_paths, easing);
        return;
    }

    let line_from = shape.line_from.last([-50.0, 0.0]);
    let line_to = shape.line_to.last([50.0, 0.0]);
    let arc_angles = shape.arc_angles.last(default_arc);
    let color = style.color.last(DEFAULT_WHITE);
    // Bare-track helper: derive the projection from the actor type (the same
    // derivation the catalog performs). `World`-aware callers apply overrides.
    let caps = animatix_std::caps_for_type(actor_type).unwrap_or_default();
    let stroke_width = style.stroke_width.last(caps.default_stroke_width());
    let stroke_color = style.stroke_color.last(DEFAULT_WHITE);
    let fill_opacity = style.fill_opacity.last(1.0);

    // Build vector shape state and compute paths
    let mut vector_shape_state = VectorShapeState::new(shape_type, size);
    if let VectorShapeState::Rect(rect) = &mut vector_shape_state {
        rect.corner_radius = shape.corner_radius.last(0.0);
    }
    // Restore shape-specific fields from track data
    match &mut vector_shape_state {
        VectorShapeState::Line(line) => {
            line.line_from = shape.line_from.last([-50.0, 0.0]);
            line.line_to = shape.line_to.last([50.0, 0.0]);
        },
        VectorShapeState::Arrow(arrow) => {
            arrow.from = shape.line_from.last([-50.0, 0.0]);
            arrow.to = shape.line_to.last([50.0, 0.0]);
            arrow.head_size = shape.head_size.last(10.0);
        },
        VectorShapeState::Callout(callout) => {
            callout.from = shape.line_from.last([-100.0, 0.0]);
            callout.to = shape.line_to.last([100.0, 0.0]);
            callout.head_size = shape.head_size.last(10.0);
        },
        VectorShapeState::Polygon(poly) => {
            // Restore points for Polygon actors
            poly.points = shape.points.last(Vec::new());
            if !poly.points.is_empty() {
                use crate::timeline::KurboShape;
                let pts: Vec<kurbo::Point> = poly
                    .points
                    .iter()
                    .map(|&[x, y]| kurbo::Point::new(x as f64, y as f64))
                    .collect();
                poly.custom_path = Some(KurboShape::Polygon { points: pts }.to_path_default());
            }
        },
        VectorShapeState::Path(path_state) => {
            // Restore commands for Path actors
            let commands_svg = shape.commands.last(String::new());
            if !commands_svg.is_empty() {
                if let Ok(path) = kurbo::BezPath::from_svg(&commands_svg) {
                    path_state.custom_path = Some(path);
                }
            }
        },
        _ => {},
    }
    let target_vello_path = build_vector_shape_vello_path(
        shape_type,
        &vector_shape_state,
        VectorShapeStyle {
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
            line_cap: 0,
            line_join: 0,
        },
    )
    .unwrap_or_else(|| {
        build_shape_vello_path(
            shape_type,
            size,
            line_from,
            line_to,
            arc_angles,
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
        )
    });

    if has_duration {
        let start_paths = track.evaluate_vector_paths_value(shape, style, t_start_ms);
        shape
            .vector_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_paths, Easing::Linear);
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut shape.vector_paths, t_end_ms);
    }
    shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, vec![target_vello_path], easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: scale PlotCurve paths when parent Graph resizes
// ─────────────────────────────────────────────────────────────

pub(super) fn scale_plot_curve_paths(
    track: &mut AnimationTrack,
    style: &StyleTracks,
    shape: &mut ShapeTracks,
    scale_x: f32,
    scale_y: f32,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    // Skip if scale is identity
    if (scale_x - 1.0).abs() < f32::EPSILON && (scale_y - 1.0).abs() < f32::EPSILON {
        return;
    }

    let has_duration = t_end_ms > t_start_ms;
    let current_paths = track.evaluate_vector_paths_value(shape, style, t_end_ms);
    if current_paths.is_empty() {
        return;
    }

    let scale_transform = kurbo::Affine::scale_non_uniform(scale_x as f64, scale_y as f64);
    let scaled_paths: Vec<VelloPath> = current_paths
        .into_iter()
        .map(|mut vp| {
            // Owned geometry (unique Arc here): apply the scale in place
            // instead of building a second BezPath per path per edit.
            std::sync::Arc::make_mut(&mut vp.path).apply_affine(scale_transform);
            vp
        })
        .collect();

    if has_duration {
        let start_paths = track.evaluate_vector_paths_value(shape, style, t_start_ms);
        shape
            .vector_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_paths, Easing::Linear);
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut shape.vector_paths, t_end_ms);
    }
    shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, scaled_paths, easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: does this property affect shape geometry?
// ─────────────────────────────────────────────────────────────

pub(super) fn affects_shape_geometry(property: &str) -> bool {
    lookup_property(property).is_some_and(|schema| schema.affects_vector_paths())
}

pub(super) fn push_unsupported_assignment_property_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    target_key: &str,
    property: &str,
) {
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnsupportedAssignmentProperty,
            DiagnosticPhase::Build,
            format!(
                "Assignment property '{property}' on '{target_key}' is not part of the current runtime assignment surface; ignoring this assignment."
            ),
        )
        .with_subject(subject),
    );
}
