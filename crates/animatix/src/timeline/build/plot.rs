//! This module implements `Timeline::build()`, the one-time lowering pass from
//! expanded AST to compiled timeline.
//!
//! It handles: colorscheme resolution, config processing, actor declarations,
//! property assignments, actions, container layout, component expansion,
//! text/math/code path compilation, and asset loading.

use std::collections::HashMap;
use super::*;
use crate::ast::{InlineItem, Property};
use crate::timeline::plot::{PlotCurveKind, ProceduralPlot};
use crate::timeline::vello_path::VelloPath;

/// Data for tick labels: screen positions and math values.
#[derive(Clone, Debug, Default)]
pub(crate) struct TickLabelData {
    /// (screen_x, screen_y, math_value) for each x-axis tick
    pub x_labels: Vec<(f64, f64, f64)>,
    /// (screen_x, screen_y, math_value) for each y-axis tick
    pub y_labels: Vec<(f64, f64, f64)>,
}

/// Parse the `tick_labels` string property into a flags-like value indicating
/// which axes should have labels.
/// Accepts: "auto", "true", "false", "x", "y", "both"
fn tick_labels_has_axis(value: &str, axis: char) -> bool {
    match value {
        "auto" | "true" | "both" => true,
        "x" => axis == 'x',
        "y" => axis == 'y',
        _ => false, // "false" or unrecognised
    }
}

/// Parameters for building plot curve paths.
pub(crate) struct PlotCurveParams<'a> {
    pub(super) kind: PlotCurveKind,
    pub(super) func: &'a Option<(Vec<String>, Box<Expr>)>,
    pub(super) p_x_domain: [f64; 2],
    pub(super) p_y_domain: [f64; 2],
    pub(super) p_size: [f64; 2],
    pub(super) t_domain: [f64; 2],
    pub(super) tolerance: f64,
    pub(super) max_depth: f64,
    pub(super) resolution: f64,
    pub(super) stroke_width: f32,
    pub(super) stroke_color: [f32; 4],
    pub(super) eval_env: &'a Environment,
}

/// Build plot curve VelloPaths from the given parameters.
/// This is the shared implementation used by both `process_plot_actor` and the
/// `process_body` ActorDecl fallback path.
pub(crate) fn build_plot_curve_paths(params: &PlotCurveParams<'_>) -> Vec<VelloPath> {
    let mut vello_paths = vec![];

    if let Some((args, body)) = params.func {
        let env_copy = params.eval_env.clone();
        let arg_name = if !args.is_empty() {
            args[0].clone()
        } else {
            "x".to_string()
        };

        let (min_t, max_t) = if params.kind == PlotCurveKind::Cartesian {
            (params.p_x_domain[0], params.p_x_domain[1])
        } else if params.kind == PlotCurveKind::Implicit {
            (0.0, 0.0)
        } else {
            (params.t_domain[0], params.t_domain[1])
        };

        if params.kind == PlotCurveKind::Implicit {
            let path = build_implicit_plot_path(
                &env_copy,
                args,
                body,
                &params.p_x_domain,
                &params.p_y_domain,
                &params.p_size,
                params.resolution.round().max(8.0) as usize,
            );
            vello_paths.push(VelloPath {
                path,
                fill: None,
                stroke: if params.stroke_width > 0.0 {
                    Some((
                        vello::peniko::Color::from_rgba8(
                            (params.stroke_color[0] * 255.0) as u8,
                            (params.stroke_color[1] * 255.0) as u8,
                            (params.stroke_color[2] * 255.0) as u8,
                            (params.stroke_color[3] * 255.0) as u8,
                        ),
                        params.stroke_width,
                    ))
                } else {
                    None
                },
            });
        } else {
            let start_eval = evaluate_with_binding(&env_copy, &arg_name, min_t, body)
                .unwrap_or(Value::Num(f64::NAN));
            let (start_math_x, start_math_y) = if params.kind == PlotCurveKind::Cartesian {
                (min_t, start_eval.as_num())
            } else if params.kind == PlotCurveKind::Parametric {
                match start_eval {
                    Value::Vec2([x, y]) => (x, y),
                    _ => (f64::NAN, f64::NAN),
                }
            } else {
                let start_val = start_eval.as_num();
                (start_val * min_t.cos(), start_val * min_t.sin())
            };
            let start_screen_x = -(params.p_size[0] / 2.0)
                + params.p_size[0]
                    * ((start_math_x - params.p_x_domain[0])
                        / (params.p_x_domain[1] - params.p_x_domain[0]));
            let start_screen_y = (params.p_size[1] / 2.0)
                - params.p_size[1]
                    * ((start_math_y - params.p_y_domain[0])
                        / (params.p_y_domain[1] - params.p_y_domain[0]));

            let end_eval = evaluate_with_binding(&env_copy, &arg_name, max_t, body)
                .unwrap_or(Value::Num(f64::NAN));
            let (end_math_x, end_math_y) = if params.kind == PlotCurveKind::Cartesian {
                (max_t, end_eval.as_num())
            } else if params.kind == PlotCurveKind::Parametric {
                match end_eval {
                    Value::Vec2([x, y]) => (x, y),
                    _ => (f64::NAN, f64::NAN),
                }
            } else {
                let end_val = end_eval.as_num();
                (end_val * max_t.cos(), end_val * max_t.sin())
            };
            let end_screen_x = -(params.p_size[0] / 2.0)
                + params.p_size[0]
                    * ((end_math_x - params.p_x_domain[0])
                        / (params.p_x_domain[1] - params.p_x_domain[0]));
            let end_screen_y = (params.p_size[1] / 2.0)
                - params.p_size[1]
                    * ((end_math_y - params.p_y_domain[0])
                        / (params.p_y_domain[1] - params.p_y_domain[0]));

            let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
            let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

            let mut pts = vec![p0];
            let mut cache = HashMap::<u64, Value>::new();

            if params.kind == PlotCurveKind::Cartesian {
                sample_recursive_cartesian(
                    min_t,
                    max_t,
                    p0,
                    p1,
                    0,
                    params.max_depth as usize,
                    params.tolerance,
                    &env_copy,
                    &arg_name,
                    body,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &mut cache,
                    &mut pts,
                );
            } else if params.kind == PlotCurveKind::Polar {
                sample_recursive_polar(
                    min_t,
                    max_t,
                    p0,
                    p1,
                    0,
                    params.max_depth as usize,
                    params.tolerance,
                    &env_copy,
                    &arg_name,
                    body,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &mut cache,
                    &mut pts,
                );
            } else {
                sample_recursive_parametric(
                    min_t,
                    max_t,
                    p0,
                    p1,
                    0,
                    params.max_depth as usize,
                    params.tolerance,
                    &env_copy,
                    &arg_name,
                    body,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &mut cache,
                    &mut pts,
                );
            }

            let mut path = kurbo::BezPath::new();
            let mut first = true;
            for pt in pts {
                if pt.x.is_nan() || pt.y.is_nan() {
                    first = true;
                } else if first {
                    path.move_to((pt.x, pt.y));
                    first = false;
                } else {
                    path.line_to((pt.x, pt.y));
                }
            }
            vello_paths.push(VelloPath {
                path,
                fill: None,
                stroke: if params.stroke_width > 0.0 {
                    Some((
                        vello::peniko::Color::from_rgba8(
                            (params.stroke_color[0] * 255.0) as u8,
                            (params.stroke_color[1] * 255.0) as u8,
                            (params.stroke_color[2] * 255.0) as u8,
                            (params.stroke_color[3] * 255.0) as u8,
                        ),
                        params.stroke_width,
                    ))
                } else {
                    None
                },
            });
        }
    }

    vello_paths
}

/// Build graph axis VelloPaths (X and Y axes, optional grid and ticks).
/// Omits an axis entirely when zero is not in its domain.
pub(crate) fn build_graph_axis_paths(
    size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    axis_color: [f32; 4],
    grid: bool,
    ticks: bool,
    _tick_labels: bool,
) -> Vec<VelloPath> {
    let mut paths = Vec::new();
    let mut axis_path = kurbo::BezPath::new();

    // X-axis: drawn only when y=0 is inside the y_domain
    let x_axis_y = if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let y = size[1] as f64 * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]));
        axis_path.move_to((-(size[0] as f64), y));
        axis_path.line_to((size[0] as f64, y));
        Some(y)
    } else {
        None
    };

    // Y-axis: drawn only when x=0 is inside the x_domain
    let y_axis_x = if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let x = size[0] as f64 * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]));
        axis_path.move_to((x, -(size[1] as f64)));
        axis_path.line_to((x, size[1] as f64));
        Some(x)
    } else {
        None
    };

    if !axis_path.elements().is_empty() {
        paths.push(VelloPath {
            path: axis_path,
            fill: None,
            stroke: Some((vello::peniko::Color::from_rgba8(
                (axis_color[0] * 255.0) as u8,
                (axis_color[1] * 255.0) as u8,
                (axis_color[2] * 255.0) as u8,
                (axis_color[3] * 255.0) as u8,
            ), 2.0)),
        });
    }

    // Grid lines
    if grid {
        let mut grid_path = kurbo::BezPath::new();
        let x_step = ((x_domain[1] - x_domain[0]).abs() / 10.0).max(0.5);
        let y_step = ((y_domain[1] - y_domain[0]).abs() / 10.0).max(0.5);

        // Vertical grid lines
        let mut x = (x_domain[0] / x_step).ceil() * x_step;
        while x <= x_domain[1] {
            if x != 0.0 {
                let screen_x = size[0] as f64 * (-1.0 + 2.0 * (x - x_domain[0]) / (x_domain[1] - x_domain[0]));
                grid_path.move_to((screen_x, -(size[1] as f64)));
                grid_path.line_to((screen_x, size[1] as f64));
            }
            x += x_step;
        }

        // Horizontal grid lines
        let mut y = (y_domain[0] / y_step).ceil() * y_step;
        while y <= y_domain[1] {
            if y != 0.0 {
                let screen_y = size[1] as f64 * (1.0 - 2.0 * (y - y_domain[0]) / (y_domain[1] - y_domain[0]));
                grid_path.move_to((-(size[0] as f64), screen_y));
                grid_path.line_to((size[0] as f64, screen_y));
            }
            y += y_step;
        }

        if !grid_path.elements().is_empty() {
            paths.push(VelloPath {
                path: grid_path,
                fill: None,
                stroke: Some((vello::peniko::Color::from_rgba8(
                    (axis_color[0] * 255.0) as u8,
                    (axis_color[1] * 255.0) as u8,
                    (axis_color[2] * 255.0) as u8,
                    (axis_color[3] * 255.0) as u8 / 4,
                ), 1.0)),
            });
        }
    }

    // Tick marks
    if ticks {
        let mut tick_path = kurbo::BezPath::new();
        let x_step = ((x_domain[1] - x_domain[0]).abs() / 10.0).max(0.5);
        let y_step = ((y_domain[1] - y_domain[0]).abs() / 10.0).max(0.5);
        let tick_len = 4.0;

        if let Some(y) = x_axis_y {
            let mut x = (x_domain[0] / x_step).ceil() * x_step;
            while x <= x_domain[1] {
                if x != 0.0 {
                    let screen_x = size[0] as f64 * (-1.0 + 2.0 * (x - x_domain[0]) / (x_domain[1] - x_domain[0]));
                    tick_path.move_to((screen_x, y - tick_len));
                    tick_path.line_to((screen_x, y + tick_len));
                }
                x += x_step;
            }
        }

        if let Some(x) = y_axis_x {
            let mut y = (y_domain[0] / y_step).ceil() * y_step;
            while y <= y_domain[1] {
                if y != 0.0 {
                    let screen_y = size[1] as f64 * (1.0 - 2.0 * (y - y_domain[0]) / (y_domain[1] - y_domain[0]));
                    tick_path.move_to((x - tick_len, screen_y));
                    tick_path.line_to((x + tick_len, screen_y));
                }
                y += y_step;
            }
        }

        if !tick_path.elements().is_empty() {
            paths.push(VelloPath {
                path: tick_path,
                fill: None,
                stroke: Some((vello::peniko::Color::from_rgba8(
                    (axis_color[0] * 255.0) as u8,
                    (axis_color[1] * 255.0) as u8,
                    (axis_color[2] * 255.0) as u8,
                    (axis_color[3] * 255.0) as u8,
                ), 1.5)),
            });
        }
    }

    paths
}

impl Timeline {
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub(super) fn process_plot_actor(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        parent_label: Option<&str>,
        children: &[InlineItem],
        diagnostics: &mut Vec<Diagnostic>,
        existing_track: &AnimationTrack,
    ) -> Option<(
        [f32; 2],
        [f32; 2],
        [f32; 2],
        [f32; 2],
        [f32; 4],
        f32,
        [f32; 4],
        f32,
        f32,
        ShapeType,
        Vec<VelloPath>,
        Option<ProceduralPlot>,
        Option<TickLabelData>,
    )> {
        let primitive = PrimitiveDescriptor::for_actor_type(ty);
        if !primitive.is_graph_host() && !primitive.is_plot() {
            return None;
        }

        let mut x_domain = [-10.0, 10.0];
        let mut y_domain = [-10.0, 10.0];
        let mut t_domain = [0.0, std::f64::consts::TAU];
        let mut func = None;
        let mut initial_size = DEFAULT_LAYOUT_HALF_SIZE;
        let mut tolerance = 0.5;
        let mut max_depth = 10.0;
        let mut resolution = 96.0;
        let mut kind = PlotCurveKind::Cartesian;
        let mut density = 16.0;
        let mut levels: Vec<f64> = vec![];
        let mut grid = false;
        let mut ticks = false;
        let mut tick_labels = String::from("auto");
        let mut x_range = [-10.0, 10.0, 2.0];
        let mut y_range = [-10.0, 10.0, 2.0];
        let is_number_plane = ty == "NumberPlane";
        let initial_eval_env = self.build_eval_env(time_ms as u64);

        // Start with track defaults, override from props.
        let mut color = existing_track.color.last(DEFAULT_WHITE);
        let mut stroke_width = existing_track.stroke_width.last(2.0);
        let mut stroke_color = existing_track.stroke_color.last(DEFAULT_WHITE);

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "size" => {
                    let size_val = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([w, h]) = size_val {
                        initial_size[0] = w as f32 / 2.0;
                        initial_size[1] = h as f32 / 2.0;
                    }
                }
                "radius" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    let r = v.as_num() as f32;
                    initial_size = [r, r];
                }
                "x_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        x_domain = [min, max];
                    }
                }
                "y_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        y_domain = [min, max];
                    }
                }
                "t_domain" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([min, max]) = v {
                        t_domain = [min, max];
                    }
                }
                "func" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Closure(args, body) = v {
                        func = Some((args, body));
                    }
                }
                "color" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Color(c) = v {
                        color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                    }
                }
                "stroke" | "stroke_color" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Color(c) = v {
                        stroke_color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
                    }
                }
                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_width = v.as_num() as f32;
                }
                "tolerance" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    tolerance = v.as_num();
                }
                "max_depth" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    max_depth = v.as_num();
                }
                "resolution" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(96.0));
                    resolution = v.as_num();
                }
                "density" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(16.0));
                    density = v.as_num().max(2.0).round();
                }
                "levels" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    match v {
                        Value::List(items) => {
                            levels = items.iter().map(|item| item.as_num()).collect();
                        }
                        Value::Num(n) => {
                            levels.push(n);
                        }
                        _ => {}
                    }
                }
                "grid" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    grid = v.as_bool();
                }
                "ticks" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    ticks = v.as_bool();
                }
                "tick_labels" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Str("auto".to_string()));
                    tick_labels = v.as_str().to_lowercase();
                }
                "kind" => {
                    if let Expr::Str(s) = &prop.value {
                        if let Some(k) = PlotCurveKind::from_str(s) {
                            kind = k;
                        }
                    } else if let Expr::Ident(s) = &prop.value {
                        if let Some(k) = PlotCurveKind::from_str(s) {
                            kind = k;
                        }
                    }
                }
                "x_range" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec3([min, max, step]) = v {
                        x_range = [min, max, step];
                    }
                }
                "y_range" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec3([min, max, step]) = v {
                        y_range = [min, max, step];
                    }
                }
                _ => {}
            }
        }

        // Validate plot func signature if present (skip for multi-arg plot types).
        let is_vector_field = ty == "VectorField";
        let is_heatmap = ty == "Heatmap";
        let is_contour_set = ty == "ContourSet";
        if let Some((ref args, ref body)) = func {
            if !is_vector_field && !is_heatmap && !is_contour_set {
            let (expected_arity, expected_ty) = match kind {
                PlotCurveKind::Cartesian | PlotCurveKind::Polar => (1, "number"),
                PlotCurveKind::Parametric => (1, "vec2"),
                PlotCurveKind::Implicit => (2, "number"),
            };
            if args.len() != expected_arity {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPlotFunc,
                        DiagnosticPhase::Build,
                        format!(
                            "{} expects a func with {} argument(s), got {}",
                            ty, expected_arity, args.len()
                        ),
                    )
                    .with_subject(label),
                );
            }
            // Type-check by evaluating with a test input.
            let mut test_env = initial_eval_env.clone();
            for arg in args.iter() {
                test_env.set(arg, Value::Num(0.0));
            }
            if let Ok(result) = evaluate_expr(body, &test_env) {
                let ok = matches!(
                    (expected_ty, &result),
                    ("number", Value::Num(_)) | ("vec2", Value::Vec2(_))
                );
                if !ok {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidPlotFunc,
                            DiagnosticPhase::Build,
                            format!(
                                "{} func should return {}, got {}",
                                ty,
                                expected_ty,
                                match result {
                                    Value::Num(_) => "number".to_string(),
                                    Value::Vec2(_) => "vec2".to_string(),
                                    Value::Vec3(_) => "vec3".to_string(),
                                    Value::Vec4(_) => "vec4".to_string(),
                                    Value::Color(_) => "color".to_string(),
                                    Value::Str(_) => "string".to_string(),
                                    Value::List(_) => "list".to_string(),
                                    Value::NativeFn(_) => "function".to_string(),
                                    Value::Closure(_, _) => "closure".to_string(),
                                    Value::Object(name, _) => name.clone(),
                                    Value::Bool(_) => "bool".to_string(),
                                }
                            ),
                        )
                        .with_subject(label),
                    );
                }
            }
            } // end inner if (graph host/plot_curve only validation)
        } // end func validation guard for plot_curve types only

        if primitive.is_graph_host() {
            self.env
                .set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
            self.env
                .set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
            self.env.set(
                &format!("{}_size", label),
                Value::Vec2([
                    initial_size[0] as f64,
                    initial_size[1] as f64,
                ]),
            );
        }

        self.process_inline_items(time_ms, children, label, diagnostics);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, std::f32::consts::PI];
        let size = existing_track.size.last(default_size);
        let line_from = existing_track.line_from.last([-50.0, 0.0]);
        let line_to = existing_track.line_to.last([50.0, 0.0]);
        let arc_angles = existing_track.arc_angles.last(default_arc);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let stroke_progress = existing_track.stroke_progress.last(1.0);
        let fill_opacity = 0.0f32;

        let mut vello_paths = vec![];
        let mut procedural_plot = None;
        let mut tick_label_data = TickLabelData::default();

        if primitive.is_graph_host() {
            let label_x = tick_labels_has_axis(&tick_labels, 'x');
            let label_y = tick_labels_has_axis(&tick_labels, 'y');

            // Use initial_size for axis paths so they match the parsed size
            let axis_size = if size != default_size { size } else { initial_size };
            vello_paths = build_graph_axis_paths(axis_size, x_domain, y_domain, stroke_color, grid, ticks, label_x || label_y);

            // Compute tick label positions (same logic as build_graph_axis_paths ticks section)
            let x_step = ((x_domain[1] - x_domain[0]).abs() / 10.0).max(0.5);
            let y_step = ((y_domain[1] - y_domain[0]).abs() / 10.0).max(0.5);
            let tick_label_offset = 14.0;

            // X-axis at y=0 screen position
            if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                let axis_y = axis_size[1] as f64 * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]));
                if label_x {
                    let mut x = (x_domain[0] / x_step).ceil() * x_step;
                    while x <= x_domain[1] {
                        if x != 0.0 {
                            let screen_x = axis_size[0] as f64 * (-1.0 + 2.0 * (x - x_domain[0]) / (x_domain[1] - x_domain[0]));
                            tick_label_data.x_labels.push((screen_x, axis_y + tick_label_offset, x));
                        }
                        x += x_step;
                    }
                }
            }

            // Y-axis at x=0 screen position
            if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
                let axis_x = axis_size[0] as f64 * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]));
                if label_y {
                    let mut y = (y_domain[0] / y_step).ceil() * y_step;
                    while y <= y_domain[1] {
                        if y != 0.0 {
                            let screen_y = axis_size[1] as f64 * (1.0 - 2.0 * (y - y_domain[0]) / (y_domain[1] - y_domain[0]));
                            tick_label_data.y_labels.push((axis_x - tick_label_offset, screen_y, y));
                        }
                        y += y_step;
                    }
                }
            }
        } else if is_vector_field {
            let eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                vello_paths = build_vector_field_paths(
                    &eval_env,
                    args,
                    body,
                    x_domain,
                    y_domain,
                    full_size,
                    density as usize,
                    stroke_color,
                    stroke_width,
                );
            }
        } else if is_heatmap {
            let eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                vello_paths = build_heatmap_paths(
                    &eval_env,
                    args,
                    body,
                    x_domain,
                    y_domain,
                    full_size,
                    resolution.max(2.0).round() as usize,
                    color,
                );
            }
        } else if is_contour_set {
            let eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                vello_paths = build_contour_set_paths(
                    &eval_env,
                    args,
                    body,
                    &levels,
                    x_domain,
                    y_domain,
                    full_size,
                    resolution.max(8.0) as usize,
                    stroke_color,
                    stroke_width,
                );
            }
        } else if is_number_plane {
            vello_paths = build_number_plane_paths(
                size, x_domain, y_domain, x_range, y_range, stroke_color,
            );
        } else if primitive.is_plot_curve() {
            let p_label = parent_label.unwrap_or("").to_string();
            let mut p_x_domain = [-10.0, 10.0];
            let mut p_y_domain = [-10.0, 10.0];
            let mut p_size = [500.0, 500.0];

            if let Some(Value::Vec2(xd)) = self.env.get(&format!("{}_x_domain", p_label)) {
                p_x_domain = xd;
            }
            if let Some(Value::Vec2(yd)) = self.env.get(&format!("{}_y_domain", p_label)) {
                p_y_domain = yd;
            }
            if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                p_size = sz;
            }

            let eval_env = self.build_eval_env(time_ms as u64);
            let curve_params = PlotCurveParams {
                kind,
                func: &func,
                p_x_domain,
                p_y_domain,
                p_size,
                t_domain,
                tolerance,
                max_depth,
                resolution,
                stroke_width,
                stroke_color,
                eval_env: &eval_env,
            };
            vello_paths = build_plot_curve_paths(&curve_params);

            // Only create a procedural_plot for dynamic plots (funcs that reference `t`).
            // Static plots use the build-time sampled paths directly, avoiding
            // redundant per-frame re-sampling.
            if let Some((args, body)) = func.as_ref() {
                if body.references_ident("t") {
                    procedural_plot = Some(ProceduralPlot {
                        kind,
                        func_args: args.clone(),
                        func_body: (**body).clone(),
                        p_x_domain,
                        p_y_domain,
                        p_size,
                        t_domain,
                        tolerance,
                        max_depth: max_depth as usize,
                        resolution: resolution as usize,
                        stroke_width,
                        stroke_color,
                    });
                }
            }
        }

        Some((
            initial_size,
            line_from,
            line_to,
            arc_angles,
            color,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            shape_type,
            vello_paths,
            procedural_plot,
            if primitive.is_graph_host() && (!tick_label_data.x_labels.is_empty() || !tick_label_data.y_labels.is_empty()) {
                Some(tick_label_data)
            } else {
                None
            },
        ))
    }
}

/// Evaluate `body` with `arg_name` bound to `arg_value`, without mutating `env`.
fn evaluate_with_binding(
    env: &Environment,
    arg_name: &str,
    arg_value: f64,
    body: &Expr,
) -> Result<Value, EvalError> {
    let mut local_env = env.clone();
    local_env.set(arg_name, Value::Num(arg_value));
    evaluate_expr(body, &local_env)
}

// ── Helpers for VectorField, Heatmap, ContourSet ────────────────────────

/// Build NumberPlane VelloPaths: axes, grid lines, tick marks.
///
/// Uses `x_range` and `y_range` (min, max, step) for grid/ticks placement,
/// mapped to screen coordinates via `x_domain`/`y_domain` and `size`.
pub(crate) fn build_number_plane_paths(
    size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    x_range: [f64; 3],
    y_range: [f64; 3],
    axis_color: [f32; 4],
) -> Vec<VelloPath> {
    let mut paths = Vec::new();
    let full_w = size[0] as f64 * 2.0;
    let full_h = size[1] as f64 * 2.0;

    // Helper: math coords → screen coords (local to plot center)
    let math_to_screen = |mx: f64, my: f64| -> (f64, f64) {
        let sx = -(full_w / 2.0) + full_w * ((mx - x_domain[0]) / (x_domain[1] - x_domain[0]));
        let sy = (full_h / 2.0) - full_h * ((my - y_domain[0]) / (y_domain[1] - y_domain[0]));
        (sx, sy)
    };

    let [x_min, x_max, x_step] = x_range;
    let [y_min, y_max, y_step] = y_range;
    let step_x = x_step.max(0.001);
    let step_y = y_step.max(0.001);

    let axis_c = vello::peniko::Color::from_rgba8(
        (axis_color[0] * 255.0) as u8,
        (axis_color[1] * 255.0) as u8,
        (axis_color[2] * 255.0) as u8,
        (axis_color[3] * 255.0) as u8,
    );
    let grid_c = vello::peniko::Color::from_rgba8(
        (axis_color[0] * 255.0) as u8,
        (axis_color[1] * 255.0) as u8,
        (axis_color[2] * 255.0) as u8,
        (axis_color[3] * 255.0) as u8 / 4,
    );

    // ── Grid lines ──
    let mut grid_path = kurbo::BezPath::new();

    // Vertical grid lines at each x step
    let mut x = (x_min / step_x).ceil() * step_x;
    while x <= x_max {
        let (sx, _) = math_to_screen(x, 0.0);
        grid_path.move_to((sx, -(full_h / 2.0)));
        grid_path.line_to((sx, full_h / 2.0));
        x += step_x;
    }

    // Horizontal grid lines at each y step
    let mut y = (y_min / step_y).ceil() * step_y;
    while y <= y_max {
        let (_, sy) = math_to_screen(0.0, y);
        grid_path.move_to((-(full_w / 2.0), sy));
        grid_path.line_to((full_w / 2.0, sy));
        y += step_y;
    }

    if !grid_path.elements().is_empty() {
        paths.push(VelloPath {
            path: grid_path,
            fill: None,
            stroke: Some((grid_c, 1.0)),
        });
    }

    // ── Axes (X-axis at y=0, Y-axis at x=0) ──
    let mut axis_path = kurbo::BezPath::new();
    let mut has_axis = false;

    // X-axis: y=0 in math coords
    if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let (sx0, sy) = math_to_screen(x_domain[0], 0.0);
        let (sx1, _) = math_to_screen(x_domain[1], 0.0);
        axis_path.move_to((sx0, sy));
        axis_path.line_to((sx1, sy));
        has_axis = true;
    }

    // Y-axis: x=0 in math coords
    if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let (sx, sy0) = math_to_screen(0.0, y_domain[0]);
        let (_, sy1) = math_to_screen(0.0, y_domain[1]);
        axis_path.move_to((sx, sy0));
        axis_path.line_to((sx, sy1));
        has_axis = true;
    }

    if has_axis {
        paths.push(VelloPath {
            path: axis_path,
            fill: None,
            stroke: Some((axis_c, 2.0)),
        });
    }

    // ── Tick marks (4px perpendicular lines at each step on axes) ──
    let mut tick_path = kurbo::BezPath::new();
    let tick_len = 4.0;

    // Ticks on X-axis (at each x step, if axis is visible)
    if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let (_, axis_y) = math_to_screen(0.0, 0.0);
        let mut tx = (x_min / step_x).ceil() * step_x;
        while tx <= x_max {
            if tx != 0.0 {
                let (sx, _) = math_to_screen(tx, 0.0);
                tick_path.move_to((sx, axis_y - tick_len));
                tick_path.line_to((sx, axis_y + tick_len));
            }
            tx += step_x;
        }
    }

    // Ticks on Y-axis (at each y step, if axis is visible)
    if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let (axis_x, _) = math_to_screen(0.0, 0.0);
        let mut ty = (y_min / step_y).ceil() * step_y;
        while ty <= y_max {
            if ty != 0.0 {
                let (_, sy) = math_to_screen(0.0, ty);
                tick_path.move_to((axis_x - tick_len, sy));
                tick_path.line_to((axis_x + tick_len, sy));
            }
            ty += step_y;
        }
    }

    if !tick_path.elements().is_empty() {
        paths.push(VelloPath {
            path: tick_path,
            fill: None,
            stroke: Some((axis_c, 1.5)),
        });
    }

    paths
}
fn math_to_screen(
    x: f64,
    y: f64,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    full_size: [f64; 2],
) -> (f64, f64) {
    let sx = -(full_size[0] / 2.0)
        + full_size[0] * ((x - x_domain[0]) / (x_domain[1] - x_domain[0]));
    let sy = (full_size[1] / 2.0)
        - full_size[1] * ((y - y_domain[0]) / (y_domain[1] - y_domain[0]));
    (sx, sy)
}

/// Evaluate a scalar field func at (x,y).
fn evaluate_scalar_field(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> f64 {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    let mut local_env = env.clone();
    local_env.set(x_name, Value::Num(x));
    local_env.set(y_name, Value::Num(y));
    evaluate_expr(body, &local_env)
        .unwrap_or(Value::Num(f64::NAN))
        .as_num()
}

/// Evaluate a vector field func at (x,y), returning (dx, dy).
fn evaluate_vec2_field(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> [f64; 2] {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    let mut local_env = env.clone();
    local_env.set(x_name, Value::Num(x));
    local_env.set(y_name, Value::Num(y));
    match evaluate_expr(body, &local_env).unwrap_or(Value::Vec2([0.0, 0.0])) {
        Value::Vec2(v) => v,
        Value::Num(n) => [n, 0.0],
        _ => [0.0, 0.0],
    }
}

/// Build VelloPaths for a VectorField.
///
/// Samples `func` on a `density × density` grid within x_domain/y_domain,
/// evaluates each sample to get (dx, dy), and draws arrows with a scale
/// that prevents overlap.
fn build_vector_field_paths(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
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

            let [dx, dy] = evaluate_vec2_field(env, arg_names, body, math_x, math_y);

            let (sx, sy) = math_to_screen(math_x, math_y, x_domain, y_domain, full_size);

            // Scale so the arrow fits within a cell (clamp denominator >= 1)
            let denom = dx.abs().max(dy.abs()).max(1.0);
            let scale = base_scale / denom;

            // dy is inverted because screen Y points down
            let ex = sx + dx * scale;
            let ey = sy - dy * scale;

            path.move_to((sx, sy));
            path.line_to((ex, ey));
        }
    }

    let c = vello::peniko::Color::from_rgba8(
        (stroke_color[0] * 255.0) as u8,
        (stroke_color[1] * 255.0) as u8,
        (stroke_color[2] * 255.0) as u8,
        (stroke_color[3] * 255.0) as u8,
    );

    vec![VelloPath {
        path,
        fill: None,
        stroke: if stroke_width > 0.0 {
            Some((c, stroke_width))
        } else {
            None
        },
    }]
}

/// Build VelloPaths for a Heatmap.
///
/// Samples `func` on a `resolution × resolution` grid, normalizes each
/// sample to [0,1] across the min/max range, and draws filled rectangles
/// at varying alpha using the actor's `color`.
fn build_heatmap_paths(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
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

    for (yi, row) in values.iter_mut().enumerate() {
        for (xi, val) in row.iter_mut().enumerate() {
            let math_x = x_domain[0] + (xi as f64 + 0.5) * x_step;
            let math_y = y_domain[0] + (yi as f64 + 0.5) * y_step;

            *val = evaluate_scalar_field(env, arg_names, body, math_x, math_y);
            if val.is_finite() {
                min_val = min_val.min(*val);
                max_val = max_val.max(*val);
            }
        }
    }

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

            // Compute cell screen coordinates
            let mut bp = kurbo::BezPath::new();
            bp.move_to(kurbo::Point::new(sx0, sy0));
            bp.line_to(kurbo::Point::new(sx1, sy0));
            bp.line_to(kurbo::Point::new(sx1, sy1));
            bp.line_to(kurbo::Point::new(sx0, sy1));
            bp.close_path();

            vello_paths.push(VelloPath {
                path: bp,
                fill: Some(vello::peniko::Color::from_rgba8(cr, cg, cb, alpha)),
                stroke: None,
            });
        }
    }

    vello_paths
}

/// Build VelloPaths for a ContourSet.
///
/// For each level value, constructs `func(x,y) - level` and delegates to
/// `build_implicit_plot_path` to trace the zero-contour.  Each level
/// produces one stroked path.
fn build_contour_set_paths(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
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
        // Build `func(x,y) - level` expression for the implicit solver
        let modified_body = Expr::Binary(
            Box::new((*body).clone()),
            crate::ast::BinaryOp::Sub,
            Box::new(crate::ast::Expr::Num(level)),
        );

        let bez_path = build_implicit_plot_path(
            env,
            arg_names,
            &modified_body,
            &x_domain,
            &y_domain,
            &full_size,
            resolution,
        );

        if !bez_path.elements().is_empty() {
            paths.push(VelloPath {
                path: bez_path,
                fill: None,
                stroke: if stroke_width > 0.0 {
                    Some((c, stroke_width))
                } else {
                    None
                },
            });
        }
    }

    paths
}
