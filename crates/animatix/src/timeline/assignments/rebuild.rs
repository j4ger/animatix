use super::*;
use crate::ast::Expr;

// ─────────────────────────────────────────────────────────────
// Helper: size-like assignments (also write to layout_size)
// ─────────────────────────────────────────────────────────────

pub(super) fn handle_size_assignment(
    track: &mut AnimationTrack,
    property: &str,
    value: &Expr,
    env: &Environment,
    subject: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> [f32; 2] {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let has_duration = t_end_ms > t_start_ms;

    let target_size = match property {
        "size" => {
            if let Some(Value::Vec2([w, h])) =
                evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            {
                [w as f32 / 2.0, h as f32 / 2.0]
            } else {
                track.geometry.size.last(default_size)
            }
        },
        "radius_x" => {
            let mut s = track.geometry.size.last(default_size);
            s[0] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[0]);
            s
        },
        "radius_y" => {
            let mut s = track.geometry.size.last(default_size);
            s[1] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[1]);
            s
        },
        _ => track.geometry.size.last(default_size),
    };

    if has_duration {
        let start_val = track.geometry.size.get(t_start_ms, default_size);
        track.geometry.size.ensure(default_size).add_keyframe(
            t_start_ms,
            start_val,
            Easing::Linear,
        );
        if let Some(layout_start) = track.layout_size_get(t_start_ms) {
            track.ensure_layout_size(default_size).add_keyframe(
                t_start_ms,
                layout_start,
                Easing::Linear,
            );
        }
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.geometry.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.geometry.layout_size, t_start_ms);
    }
    track
        .geometry
        .size
        .ensure(default_size)
        .add_keyframe(t_end_ms, target_size, easing);
    track
        .ensure_layout_size(default_size)
        .add_keyframe(t_end_ms, target_size, easing);

    // Rebuild vector paths after size change
    rebuild_vector_paths(track, t_start_ms, t_end_ms, easing, diagnostics, Some(env));

    target_size
}

pub(super) fn rebuild_vector_paths(
    track: &mut AnimationTrack,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    _diagnostics: &mut Vec<Diagnostic>,
    env: Option<&Environment>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, 0.0];
    let has_duration = t_end_ms > t_start_ms;
    let size = track.geometry.size.last(default_size);
    let shape_type = track.shape.shape_type.last(ShapeType::Rect);

    // ── Special case: Graph actors rebuild axis/grid/tick paths ──
    if shape_type == ShapeType::Graph {
        if let Some(env) = env {
            let label = &track.label;
            let x_domain = env
                .get(&format!("{}_x_domain", label))
                .and_then(|v| {
                    if let Value::Vec2(d) = v {
                        Some(d)
                    } else {
                        None
                    }
                })
                .unwrap_or([-10.0, 10.0]);
            let y_domain = env
                .get(&format!("{}_y_domain", label))
                .and_then(|v| {
                    if let Value::Vec2(d) = v {
                        Some(d)
                    } else {
                        None
                    }
                })
                .unwrap_or([-10.0, 10.0]);
            let stroke_color = track.style.stroke_color.last(DEFAULT_WHITE);

            // Read graph axis settings from env (stored during build)
            let grid = env
                .get(&format!("{}_grid", label))
                .and_then(|v| {
                    if let Value::Bool(b) = v {
                        Some(b)
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let ticks = env
                .get(&format!("{}_ticks", label))
                .and_then(|v| {
                    if let Value::Bool(b) = v {
                        Some(b)
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let tick_labels_str = env
                .get(&format!("{}_tick_labels", label))
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "auto".to_string());
            let has_labels = matches!(tick_labels_str.as_str(), "auto" | "true" | "both");

            let padding = env
                .get(&format!("{}_padding", label))
                .and_then(|v| {
                    if let Value::Vec4(p) = v {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap_or([0.0; 4]);
            let x_scale = env
                .get(&format!("{}_x_scale", label))
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(crate::timeline::build::utils::ScaleType::from_str(&s))
                    } else {
                        None
                    }
                })
                .unwrap_or(crate::timeline::build::utils::ScaleType::Linear);
            let y_scale = env
                .get(&format!("{}_y_scale", label))
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(crate::timeline::build::utils::ScaleType::from_str(&s))
                    } else {
                        None
                    }
                })
                .unwrap_or(crate::timeline::build::utils::ScaleType::Linear);

            let new_paths = build_graph_axis_paths(
                size,
                x_domain,
                y_domain,
                stroke_color,
                grid,
                ticks,
                has_labels,
                padding,
                x_scale,
                y_scale,
            );

            if has_duration {
                let start_paths = track.evaluate_vector_paths_value(t_start_ms);
                track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
                    t_start_ms,
                    start_paths,
                    Easing::Linear,
                );
            } else if t_end_ms > 0 {
                preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
            }
            track
                .shape
                .vector_paths
                .ensure(Vec::new())
                .add_keyframe(t_end_ms, new_paths, easing);
            return;
        }
    }

    let line_from = track.shape.line_from.last([-50.0, 0.0]);
    let line_to = track.shape.line_to.last([50.0, 0.0]);
    let arc_angles = track.shape.arc_angles.last(default_arc);
    let color = track.style.color.last(DEFAULT_WHITE);
    let stroke_width = track.style.stroke_width.last(default_stroke_width(track.kind));
    let stroke_color = track.style.stroke_color.last(DEFAULT_WHITE);
    let fill_opacity = track.style.fill_opacity.last(1.0);

    // Build vector shape state and compute paths
    let mut vector_shape_state = VectorShapeState::new(shape_type, size);
    // Restore shape-specific fields from track data
    match &mut vector_shape_state {
        VectorShapeState::Line(line) => {
            line.line_from = track.shape.line_from.last([-50.0, 0.0]);
            line.line_to = track.shape.line_to.last([50.0, 0.0]);
        },
        VectorShapeState::Arrow(arrow) => {
            arrow.from = track.shape.line_from.last([-50.0, 0.0]);
            arrow.to = track.shape.line_to.last([50.0, 0.0]);
            arrow.head_size = track.shape.head_size.last(10.0);
        },
        VectorShapeState::Callout(callout) => {
            callout.from = track.shape.line_from.last([-100.0, 0.0]);
            callout.to = track.shape.line_to.last([100.0, 0.0]);
            callout.head_size = track.shape.head_size.last(10.0);
        },
        VectorShapeState::Polygon(poly) => {
            // Restore points for Polygon actors
            poly.points = track.shape.points.last(Vec::new());
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
            let commands_svg = track.shape.commands.last(String::new());
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
        let start_paths = track.evaluate_vector_paths_value(t_start_ms);
        track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
            t_start_ms,
            start_paths,
            Easing::Linear,
        );
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
    }
    track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
        t_end_ms,
        vec![target_vello_path],
        easing,
    );
}

// ─────────────────────────────────────────────────────────────
// Helper: scale PlotCurve paths when parent Graph resizes
// ─────────────────────────────────────────────────────────────

pub(super) fn scale_plot_curve_paths(
    track: &mut AnimationTrack,
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
    let current_paths = track.evaluate_vector_paths_value(t_end_ms);
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
        let start_paths = track.evaluate_vector_paths_value(t_start_ms);
        track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
            t_start_ms,
            start_paths,
            Easing::Linear,
        );
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
    }
    track
        .shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, scaled_paths, easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: does this property affect shape geometry?
// ─────────────────────────────────────────────────────────────

pub(super) fn affects_shape_geometry(property: &str) -> bool {
    matches!(
        property,
        "from" | "to" | "radius_x" | "radius_y" | "size" | "points" | "commands" | "shape_type"
    )
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
