//! Scalar/vector field sampling and the VectorField, Heatmap and ContourSet
//! path builders.

use super::PlotActorProps;
use super::*;
use crate::timeline::modifier_runtime::ir::evaluate_compiled_expr;
use crate::timeline::plot::{
    FuncSource, ProceduralPlot, build_implicit_plot_path_from_source, flatten_blend,
};

fn math_to_screen(
    x: f64,
    y: f64,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    full_size: [f64; 2],
) -> (f64, f64) {
    let sx =
        -(full_size[0] / 2.0) + full_size[0] * ((x - x_domain[0]) / (x_domain[1] - x_domain[0]));
    let sy =
        (full_size[1] / 2.0) - full_size[1] * ((y - y_domain[0]) / (y_domain[1] - y_domain[0]));
    (sx, sy)
}

/// Evaluate a scalar field `FuncSource` at (x,y).
fn eval_scalar_field_source(source: &FuncSource, env: &mut Environment, x: f64, y: f64) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let x_name = args.first().map(String::as_str).unwrap_or("x");
            let y_name = args.get(1).map(String::as_str).unwrap_or("y");
            let inserted = captures.merge_missing_into(env);
            env.set_binding(x_name, Value::Num(x));
            env.set_binding(y_name, Value::Num(y));
            let result = evaluate_compiled_expr(body, env).unwrap_or(Value::Num(f64::NAN)).as_num();
            env.clear_bindings();
            for key in inserted {
                env.overrides.remove(&key);
                env.mark_mutated();
            }
            result
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                sum += weight * eval_scalar_field_source(src, env, x, y);
            }
            sum
        },
    }
}

/// Evaluate a vector field `FuncSource` at (x,y), returning (dx, dy).
fn eval_vec2_field_source(source: &FuncSource, env: &mut Environment, x: f64, y: f64) -> [f64; 2] {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let x_name = args.first().map(String::as_str).unwrap_or("x");
            let y_name = args.get(1).map(String::as_str).unwrap_or("y");
            let inserted = captures.merge_missing_into(env);
            env.set_binding(x_name, Value::Num(x));
            env.set_binding(y_name, Value::Num(y));
            let result = match evaluate_compiled_expr(body, env).unwrap_or(Value::Vec2([0.0, 0.0]))
            {
                Value::Vec2(v) => v,
                Value::Num(n) => [n, 0.0],
                _ => [0.0, 0.0],
            };
            env.clear_bindings();
            for key in inserted {
                env.overrides.remove(&key);
                env.mark_mutated();
            }
            result
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            for (weight, src) in flat {
                let [vx, vy] = eval_vec2_field_source(src, env, x, y);
                sum_x += weight * vx;
                sum_y += weight * vy;
            }
            [sum_x, sum_y]
        },
    }
}

/// Shift a scalar field source by `-level` so an implicit zero-contour solver
/// traces the requested level set. Nested blends are shifted recursively so
/// output blending stays equivalent to evaluating the blended field minus the
/// level.
fn shift_contour_source(source: &FuncSource, level: f64) -> FuncSource {
    match source {
        FuncSource::Compiled(args, body, captures) => FuncSource::Compiled(
            args.clone(),
            Box::new(crate::timeline::modifier_runtime::ir::CompiledExpr::Binary(
                Box::new((**body).clone()),
                crate::ast::BinaryOp::Sub,
                Box::new(crate::timeline::modifier_runtime::ir::CompiledExpr::Const(
                    crate::timeline::Value::Num(level),
                )),
            )),
            captures.clone(),
        ),
        FuncSource::Blend {
            from,
            to,
            frozen_progress,
        } => FuncSource::Blend {
            from: Box::new(shift_contour_source(from, level)),
            to: Box::new(shift_contour_source(to, level)),
            frozen_progress: *frozen_progress,
        },
    }
}

/// Build VelloPaths for a VectorField.
///
/// Samples `source` on a `density × density` grid within x_domain/y_domain,
/// evaluates each sample to get (dx, dy), and draws arrows with a scale
/// that prevents overlap.
pub(crate) fn build_vector_field_paths(
    env: &mut Environment,
    source: &FuncSource,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    full_size: [f64; 2],
    density: usize,
    stroke_color: [f32; 4],
    stroke_width: f32,
) -> Vec<VelloPath> {
    let dx_domain = x_domain[1] - x_domain[0];
    let dy_domain = y_domain[1] - y_domain[0];
    let cell_w = full_size[0] / density as f64;
    let cell_h = full_size[1] / density as f64;
    let base_scale = cell_w.min(cell_h) * 0.4;

    let mut path = kurbo::BezPath::new();

    for yi in 0..density {
        for xi in 0..density {
            let x_frac = (xi as f64 + 0.5) / density as f64;
            let y_frac = (yi as f64 + 0.5) / density as f64;
            let math_x = x_domain[0] + x_frac * dx_domain;
            let math_y = y_domain[0] + y_frac * dy_domain;

            let [dx, dy] = eval_vec2_field_source(source, env, math_x, math_y);

            let (sx, sy) = math_to_screen(math_x, math_y, x_domain, y_domain, full_size);

            let mag = (dx * dx + dy * dy).sqrt();

            // Scale so the vector fits within a cell
            let denom = dx.abs().max(dy.abs()).max(1.0);
            let scale = base_scale / denom;

            // Screen direction: dy is inverted because screen Y points down
            let vx = dx * scale;
            let vy = -dy * scale;
            let len = (vx * vx + vy * vy).sqrt();

            // Shaft
            let end_x = sx + vx;
            let end_y = sy + vy;
            path.move_to((sx, sy));
            path.line_to((end_x, end_y));

            // Arrowhead tips (drawn when vector has noticeable magnitude)
            if len > 2.0 && mag > 1e-4 {
                let arrow_len = (len * 0.35).min(base_scale * 0.35).max(2.5);
                let u_x = vx / len;
                let u_y = vy / len;
                let perp_x = -u_y;
                let perp_y = u_x;

                let arrow_w = arrow_len * 0.5;
                let left_x = end_x - u_x * arrow_len + perp_x * arrow_w;
                let left_y = end_y - u_y * arrow_len + perp_y * arrow_w;
                let right_x = end_x - u_x * arrow_len - perp_x * arrow_w;
                let right_y = end_y - u_y * arrow_len - perp_y * arrow_w;

                path.move_to((end_x, end_y));
                path.line_to((left_x, left_y));
                path.move_to((end_x, end_y));
                path.line_to((right_x, right_y));
            }
        }
    }

    let c = vello::peniko::Color::from_rgba8(
        (stroke_color[0] * 255.0) as u8,
        (stroke_color[1] * 255.0) as u8,
        (stroke_color[2] * 255.0) as u8,
        (stroke_color[3] * 255.0) as u8,
    );

    vec![VelloPath {
        path: std::sync::Arc::new(path),
        fill: None,
        stroke: if stroke_width > 0.0 {
            Some((c, stroke_width))
        } else {
            None
        },
        line_cap: 0,
        line_join: 0,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    }]
}

/// Build VelloPaths for a Heatmap.
///
/// Samples `source` on a `resolution × resolution` grid, normalizes each
/// sample to [0,1] across the min/max range, and draws filled rectangles
/// at varying alpha using the actor's `color`.
pub(crate) fn build_heatmap_paths(
    env: &mut Environment,
    source: &FuncSource,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    full_size: [f64; 2],
    resolution: usize,
    color: [f32; 4],
) -> Vec<VelloPath> {
    let res = resolution.max(2);
    let x_step = (x_domain[1] - x_domain[0]) / res as f64;
    let y_step = (y_domain[1] - y_domain[0]) / res as f64;

    // Sample the function on a grid
    let mut values = vec![vec![0.0f64; res]; res];
    let mut min_val = f64::MAX;
    let mut max_val = f64::MIN;

    crate::timeline::utils::disable_eval_cache();
    for (yi, row) in values.iter_mut().enumerate() {
        for (xi, val) in row.iter_mut().enumerate() {
            let math_x = x_domain[0] + (xi as f64 + 0.5) * x_step;
            let math_y = y_domain[0] + (yi as f64 + 0.5) * y_step;

            *val = eval_scalar_field_source(source, env, math_x, math_y);
            if val.is_finite() {
                min_val = min_val.min(*val);
                max_val = max_val.max(*val);
            }
        }
    }

    crate::timeline::utils::enable_eval_cache();

    let range = (max_val - min_val).max(1e-10);
    let mut vello_paths = Vec::with_capacity(res * res);

    let cr = (color[0] * 255.0) as u8;
    let cg = (color[1] * 255.0) as u8;
    let cb = (color[2] * 255.0) as u8;

    for (yi, row) in values.iter().enumerate() {
        for (xi, val) in row.iter().enumerate() {
            let normalized = ((*val - min_val) / range).clamp(0.0, 1.0);
            let alpha = (normalized * 255.0) as u8;

            // Compute cell screen coordinates
            let sx0 = -(full_size[0] / 2.0) + full_size[0] * xi as f64 / res as f64;
            let sx1 = -(full_size[0] / 2.0) + full_size[0] * (xi as f64 + 1.0) / res as f64;
            let sy0 = (full_size[1] / 2.0) - full_size[1] * (yi as f64 + 1.0) / res as f64;
            let sy1 = (full_size[1] / 2.0) - full_size[1] * yi as f64 / res as f64;

            let mut bp = kurbo::BezPath::new();
            bp.move_to(kurbo::Point::new(sx0, sy0));
            bp.line_to(kurbo::Point::new(sx1, sy0));
            bp.line_to(kurbo::Point::new(sx1, sy1));
            bp.line_to(kurbo::Point::new(sx0, sy1));
            bp.close_path();

            vello_paths.push(VelloPath {
                path: std::sync::Arc::new(bp),
                fill: Some(vello::peniko::Color::from_rgba8(cr, cg, cb, alpha)),
                stroke: None,
                line_cap: 0,
                line_join: 0,
                dash_pattern: None,
                dash_offset: 0.0,
                fill_gradient: None,
                stroke_gradient: None,
            });
        }
    }

    vello_paths
}

/// Build VelloPaths for a ContourSet.
///
/// For each level value, shifts `source` so `func(x,y) - level` is traced by
/// the implicit solver. Each level produces one stroked path.
pub(crate) fn build_contour_set_paths(
    env: &mut Environment,
    source: &FuncSource,
    levels: &[f64],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    full_size: [f64; 2],
    resolution: usize,
    stroke_color: [f32; 4],
    stroke_width: f32,
) -> Vec<VelloPath> {
    let c = vello::peniko::Color::from_rgba8(
        (stroke_color[0] * 255.0) as u8,
        (stroke_color[1] * 255.0) as u8,
        (stroke_color[2] * 255.0) as u8,
        (stroke_color[3] * 255.0) as u8,
    );
    let mut paths = Vec::new();

    for &level in levels {
        let level_source = shift_contour_source(source, level);
        let bez_path = build_implicit_plot_path_from_source(
            env,
            &level_source,
            &x_domain,
            &y_domain,
            &full_size,
            resolution,
            &[0.0; 4],
        );

        if !bez_path.elements().is_empty() {
            paths.push(VelloPath {
                path: std::sync::Arc::new(bez_path),
                fill: None,
                stroke: if stroke_width > 0.0 {
                    Some((c, stroke_width))
                } else {
                    None
                },
                line_cap: 0,
                line_join: 0,
                dash_pattern: None,
                dash_offset: 0.0,
                fill_gradient: None,
                stroke_gradient: None,
            });
        }
    }

    paths
}

impl Timeline {
    /// Build the sampled paths for a VectorField/Heatmap/ContourSet and their
    /// frame-time `ProceduralPlot` when the declaration carries a func.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_field_plot_paths(
        &self,
        is_vector_field: bool,
        is_heatmap: bool,
        is_contour_set: bool,
        label: &str,
        parsed: &PlotActorProps,
        size: [f32; 2],
        time_ms: f64,
        plot_params: &[(String, f64)],
    ) -> (Vec<VelloPath>, Option<ProceduralPlot>) {
        let func = &parsed.func;
        let mut vello_paths = vec![];
        if is_vector_field {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_density = parsed.density as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(
                    &mut _tolerance,
                    &mut _max_depth,
                    &mut scaled_density,
                );
                let source = FuncSource::Compiled(
                    args.clone(),
                    Box::new((**body).clone()),
                    captures.clone(),
                );
                vello_paths = build_vector_field_paths(
                    &mut eval_env,
                    &source,
                    parsed.x_domain,
                    parsed.y_domain,
                    full_size,
                    scaled_density.max(4),
                    parsed.stroke_color,
                    parsed.stroke_width,
                );
            }
        } else if is_heatmap {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = parsed.resolution.max(2.0).round() as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(
                    &mut _tolerance,
                    &mut _max_depth,
                    &mut scaled_res,
                );
                let source = FuncSource::Compiled(
                    args.clone(),
                    Box::new((**body).clone()),
                    captures.clone(),
                );
                vello_paths = build_heatmap_paths(
                    &mut eval_env,
                    &source,
                    parsed.x_domain,
                    parsed.y_domain,
                    full_size,
                    scaled_res.max(4),
                    parsed.color,
                );
            }
        } else if is_contour_set {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = parsed.resolution.max(8.0) as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(
                    &mut _tolerance,
                    &mut _max_depth,
                    &mut scaled_res,
                );
                let source = FuncSource::Compiled(
                    args.clone(),
                    Box::new((**body).clone()),
                    captures.clone(),
                );
                vello_paths = build_contour_set_paths(
                    &mut eval_env,
                    &source,
                    &parsed.levels,
                    parsed.x_domain,
                    parsed.y_domain,
                    full_size,
                    scaled_res.max(8),
                    parsed.stroke_color,
                    parsed.stroke_width,
                );
            }
        }

        let mut procedural_plot = None;
        if is_vector_field || is_heatmap || is_contour_set {
            if let Some((args, body, captures)) = func.as_ref() {
                let plot_type = if is_vector_field {
                    crate::timeline::plot::ProceduralPlotKind::VectorField
                } else if is_heatmap {
                    crate::timeline::plot::ProceduralPlotKind::Heatmap
                } else {
                    crate::timeline::plot::ProceduralPlotKind::ContourSet
                };
                let param_names: Vec<String> = plot_params.iter().map(|(n, _)| n.clone()).collect();

                procedural_plot = Some(ProceduralPlot {
                    plot_type,
                    kind: parsed.kind,
                    func_args: args.clone(),
                    func_body: (**body).clone(),
                    actor_label: label.to_string(),
                    param_names,
                    p_x_domain: parsed.x_domain,
                    p_y_domain: parsed.y_domain,
                    p_size: [size[0] as f64, size[1] as f64],
                    padding: [0.0; 4],
                    t_domain: parsed.t_domain,
                    tolerance: parsed.tolerance,
                    max_depth: parsed.max_depth as usize,
                    resolution: parsed.resolution as usize,
                    density: parsed.density as usize,
                    levels: parsed.levels.clone(),
                    stroke_width: parsed.stroke_width,
                    stroke_color: parsed.stroke_color,
                    fill_color: parsed.color,
                    params: plot_params.to_vec(),
                    extra_captures: captures.clone(),
                });
            }
        }

        (vello_paths, procedural_plot)
    }
}
