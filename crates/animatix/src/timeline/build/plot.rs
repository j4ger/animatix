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

/// Processed plot actor data returned by [`Timeline::process_plot_actor`].
#[derive(Debug, Clone)]
pub(crate) struct ProcessedPlotActor {
    pub initial_size: [f32; 2],
    pub line_from: [f32; 2],
    pub line_to: [f32; 2],
    pub arc_angles: [f32; 2],
    pub color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    pub stroke_progress: f32,
    pub fill_opacity: f32,
    pub shape_type: ShapeType,
    pub vello_paths: Vec<VelloPath>,
    pub procedural_plot: Option<ProceduralPlot>,
    pub tick_label_data: Option<TickLabelData>,
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
    pub(super) build_quality: crate::timeline::BuildQuality,
}

/// Build plot curve VelloPaths from the given parameters.
/// This is the shared implementation used by both `process_plot_actor` and the
/// `process_body` ActorDecl fallback path.
pub(crate) fn build_plot_curve_paths(params: &PlotCurveParams<'_>) -> Vec<VelloPath> {
    let mut vello_paths = vec![];

    // Apply build-quality scaling to plot sampling parameters (Phase 6.3)
    let mut tolerance = params.tolerance;
    let mut max_depth = params.max_depth as usize;
    let mut resolution = params.resolution as usize;
    params.build_quality.scale_plot_params(&mut tolerance,
        &mut max_depth,
        &mut resolution,
    );

    if let Some((args, body)) = params.func {
        let mut env_copy = params.eval_env.clone();
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
                &mut env_copy,
                args,
                body,
                &params.p_x_domain,
                &params.p_y_domain,
                &params.p_size,
                resolution.max(8),
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
                line_cap: 0,
                line_join: 0,
            });
        } else {
            env_copy.set_binding(&arg_name, Value::Num(min_t));
            let start_eval = evaluate_expr(body, &env_copy)
                .unwrap_or(Value::Num(f64::NAN));
            env_copy.clear_bindings();
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

            env_copy.set_binding(&arg_name, Value::Num(max_t));
            let end_eval = evaluate_expr(body, &env_copy)
                .unwrap_or(Value::Num(f64::NAN));
            env_copy.clear_bindings();
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
                    max_depth,
                    tolerance,
                    &mut env_copy,
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
                    max_depth,
                    tolerance,
                    &mut env_copy,
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
                    max_depth,
                    tolerance,
                    &mut env_copy,
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
                line_cap: 0,
                line_join: 0,
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
            line_cap: 0,
            line_join: 0,
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
                line_cap: 0,
                line_join: 0,
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
                line_cap: 0,
                line_join: 0,
            });
        }
    }

    paths
}

impl Timeline {
    #[allow(clippy::too_many_arguments)]
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
    ) -> Option<ProcessedPlotActor> {
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
        let mut stroke_progress = existing_track.stroke_progress.last(1.0);

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
                "stroke_progress" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0));
                    if let Value::Num(n) = v {
                        stroke_progress = n.clamp(0.0, 1.0) as f32;
                    }
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
        let is_bar_chart = ty == "BarChart";
        if let Some((ref args, ref body)) = func {
            if !is_vector_field && !is_heatmap && !is_contour_set && !is_bar_chart {
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
            // Store graph axis settings so they survive size-assignment rebuilds
            self.env.set(&format!("{}_grid", label), Value::Bool(grid));
            self.env.set(&format!("{}_ticks", label), Value::Bool(ticks));
            self.env.set(&format!("{}_tick_labels", label), Value::Str(tick_labels.clone()));
        }

        self.process_inline_items(time_ms, children, label, diagnostics);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, std::f32::consts::PI];
        let size = existing_track.size.last(default_size);
        let line_from = existing_track.shape.line_from.last([-50.0, 0.0]);
        let line_to = existing_track.shape.line_to.last([50.0, 0.0]);
        let arc_angles = existing_track.shape.arc_angles.last(default_arc);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
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
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_density = density as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(&mut _tolerance, &mut _max_depth, &mut scaled_density);
                vello_paths = build_vector_field_paths(
                    &mut eval_env,
                    args,
                    body,
                    x_domain,
                    y_domain,
                    full_size,
                    scaled_density.max(4),
                    stroke_color,
                    stroke_width,
                );
            }
        } else if is_heatmap {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = resolution.max(2.0).round() as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(&mut _tolerance, &mut _max_depth, &mut scaled_res);
                vello_paths = build_heatmap_paths(
                    &mut eval_env,
                    args,
                    body,
                    x_domain,
                    y_domain,
                    full_size,
                    scaled_res.max(4),
                    color,
                );
            }
        } else if is_contour_set {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = resolution.max(8.0) as usize;
                let mut _max_depth = 0usize;
                let mut _tolerance = 0.0f64;
                self.build_quality.scale_plot_params(&mut _tolerance, &mut _max_depth, &mut scaled_res);
                vello_paths = build_contour_set_paths(
                    &mut eval_env,
                    args,
                    body,
                    &levels,
                    x_domain,
                    y_domain,
                    full_size,
                    scaled_res.max(8),
                    stroke_color,
                    stroke_width,
                );
            }
        } else if is_number_plane {
            vello_paths = build_number_plane_paths(
                size, x_domain, y_domain, x_range, y_range, stroke_color,
            );
        } else if is_bar_chart {
            // Determine parent graph context (if any)
            let p_label = parent_label.unwrap_or("").to_string();
            let parent_size = if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                Some(sz)
            } else {
                None
            };
            let p_x_domain = if let Some(Value::Vec2(xd)) = self.env.get(&format!("{}_x_domain", p_label)) {
                xd
            } else {
                x_domain
            };
            let p_y_domain = if let Some(Value::Vec2(yd)) = self.env.get(&format!("{}_y_domain", p_label)) {
                yd
            } else {
                y_domain
            };

            vello_paths = build_bar_chart_paths(
                props,
                size,
                color,
                stroke_color,
                stroke_width,
                p_x_domain,
                p_y_domain,
                parent_size,
                diagnostics,
                label,
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

            // Phase 6.4: Check cache for static plot paths before rebuilding.
            let is_static = func.as_ref().is_none_or(|(_, body)| !body.references_ident("t"));
            let cache_key = if is_static {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                format!("{:?}", kind).hash(&mut hasher);
                format!("{:?}", func).hash(&mut hasher);
                p_x_domain.map(|v| v.to_bits()).hash(&mut hasher);
                p_y_domain.map(|v| v.to_bits()).hash(&mut hasher);
                p_size.map(|v| v.to_bits()).hash(&mut hasher);
                t_domain.map(|v| v.to_bits()).hash(&mut hasher);
                tolerance.to_bits().hash(&mut hasher);
                max_depth.to_bits().hash(&mut hasher);
                resolution.to_bits().hash(&mut hasher);
                stroke_width.to_bits().hash(&mut hasher);
                stroke_color.map(|v| v.to_bits()).hash(&mut hasher);
                self.build_quality.hash(&mut hasher);
                Some(hasher.finish())
            } else {
                None
            };

            if let Some(key) = cache_key {
                if let Some(cached) = self.plot_path_cache.get(&key) {
                    vello_paths = cached.clone();
                } else {
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
                        build_quality: self.build_quality,
                    };
                    vello_paths = build_plot_curve_paths(&curve_params);
                    self.plot_path_cache.insert(key, vello_paths.clone());
                }
            } else {
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
                    build_quality: self.build_quality,
                };
                vello_paths = build_plot_curve_paths(&curve_params);
            }

            // Collect custom numeric params from declaration props.
            // Done unconditionally so that plots with keyframeable params
            // (even without `t` in the func body) get a procedural_plot.
            let mut plot_params: Vec<(String, f64)> = Vec::new();
            if let Some((_, _)) = func.as_ref() {
                for prop in props {
                    // Skip known plot properties
                    match prop.name.as_str() {
                        "kind" | "func" | "x_domain" | "y_domain" | "t_domain"
                        | "tolerance" | "max_depth" | "resolution" | "density"
                        | "levels" | "grid" | "ticks" | "tick_labels"
                        | "x_range" | "y_range" | "size" | "at" | "position"
                        | "color" | "opacity" | "stroke" | "stroke_color"
                        | "stroke_width" | "stroke_progress" | "fill_opacity"
                        | "radius" | "radius_x" | "radius_y"
                        | "from" | "to" | "head_size"
                        | "text" | "content" | "code" | "font_size" | "font_family"
                        | "url" | "source" | "volume"
                        | "anchor" | "offset" | "rotation" | "scale" | "transform"
                        | "blur" | "brightness" | "contrast" | "saturate"
                        | "hue_rotate" | "sepia" | "gap" | "padding" | "align"
                        | "cols" | "data" | "bar_width" | "bar_colors"
                        | "direction" | "max_value" | "show_axis" | "show_labels" => {}
                        _ => {
                            // Treat unknown numeric props as plot parameters
                            let eval_env = self.build_eval_env(time_ms as u64);
                            if let Ok(Value::Num(n)) = evaluate_expr(&prop.value, &eval_env) {
                                plot_params.push((prop.name.clone(), n));
                            }
                        }
                    }
                }
            }

            // Create a procedural_plot for dynamic plots (funcs that reference `t`)
            // OR plots with custom numeric params that can be keyframed.
            // Pure-static plots (no `t`, no params) use the build-time sampled
            // paths directly, avoiding redundant per-frame re-sampling.
            if let Some((args, body)) = func.as_ref() {
                if body.references_ident("t") || !plot_params.is_empty() {
                    let param_names: Vec<String> = plot_params.iter().map(|(n, _)| n.clone()).collect();

                    procedural_plot = Some(ProceduralPlot {
                        kind,
                        func_args: args.clone(),
                        func_body: (**body).clone(),
                        actor_label: label.to_string(),
                        param_names,
                        p_x_domain,
                        p_y_domain,
                        p_size,
                        t_domain,
                        tolerance,
                        max_depth: max_depth as usize,
                        resolution: resolution as usize,
                        stroke_width,
                        stroke_color,
                        params: plot_params,
                    });
                }
            }
        }

        Some(ProcessedPlotActor {
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
            tick_label_data: if primitive.is_graph_host() && (!tick_label_data.x_labels.is_empty() || !tick_label_data.y_labels.is_empty()) {
                Some(tick_label_data)
            } else {
                None
            },
        })
    }
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
                    line_cap: 0,
                    line_join: 0,
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
                    line_cap: 0,
                    line_join: 0,
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
                    line_cap: 0,
                    line_join: 0,
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
    env: &mut Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> f64 {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    env.set_binding(x_name, Value::Num(x));
    env.set_binding(y_name, Value::Num(y));
    evaluate_expr(body, env)
        .unwrap_or(Value::Num(f64::NAN))
        .as_num()
}

/// Evaluate a vector field func at (x,y), returning (dx, dy).
fn evaluate_vec2_field(
    env: &mut Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> [f64; 2] {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    env.set_binding(x_name, Value::Num(x));
    env.set_binding(y_name, Value::Num(y));
    match evaluate_expr(body, env).unwrap_or(Value::Vec2([0.0, 0.0])) {
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
    env: &mut Environment,
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
        line_cap: 0,
        line_join: 0,
    }]
}

/// Build VelloPaths for a Heatmap.
///
/// Samples `func` on a `resolution × resolution` grid, normalizes each
/// sample to [0,1] across the min/max range, and draws filled rectangles
/// at varying alpha using the actor's `color`.
fn build_heatmap_paths(
    env: &mut Environment,
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

    crate::timeline::utils::disable_eval_cache();
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
                line_cap: 0,
                line_join: 0,
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
    env: &mut Environment,
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
                line_cap: 0,
                line_join: 0,
            });
        }
    }

    paths
}

/// Parse bar chart data from a `data` property value.
///
/// Supports:
/// - `((2, 1.0), (5, 0.55), (9, 0.3))` — numeric keys (auto-labeled as "2", "5", "9")
/// - `(("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3))` — string-labeled tuples
/// - `()` — empty data
pub(crate) fn parse_bar_chart_data(
    props: &[Property],
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> Vec<(String, f32)> {
    let mut data: Vec<(String, f32)> = Vec::new();

    for prop in props {
        if prop.name != "data" {
            continue;
        }
        let expr = &prop.value;
        // Expect a list (outer list of bars)
        if let Expr::List(items) = expr {
            for item in items {
                match item {
                    Expr::Tuple(bar) if bar.len() == 2 => {
                        let key_str = match &bar[0] {
                            Expr::Num(n) => format_float(*n),
                            Expr::Str(s) => s.clone(),
                            _ => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("BarChart '{}' data key must be a number or string", label),
                                ));
                                continue;
                            }
                        };
                        let val = match &bar[1] {
                            Expr::Num(n) => *n as f32,
                            _ => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("BarChart '{}' data value must be a number", label),
                                ));
                                continue;
                            }
                        };
                        data.push((key_str, val));
                    }
                    _ => {
                        diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!("BarChart '{}' data entry must be a (key, value) tuple", label),
                        ));
                    }
                }
            }
        }
        break; // Only process the first `data` property
    }

    data
}

/// Format a number for use as a bar label (strip trailing zeros).
fn format_float(n: f64) -> String {
    if n == n.floor() && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{:.2}", n)
    }
}

/// Build VelloPaths for a BarChart.
///
/// Produces one filled-rectangle path per bar (in declaration order),
/// optionally preceded by an axis line path.
///
/// # Standalone mode
/// Bars are positioned within the chart's `size` bounds, centered on the actor.
/// Gap between bars is auto-calculated from `(plot_width - bar_count * bar_width) / (bar_count + 1)`.
///
/// # Graph child mode
/// Bars use math-coordinate mapping via `p_x_domain`, `p_y_domain`, `p_size`.
/// Labels are in math-space but rendered as child Text tracks (handled by the caller).
pub(crate) fn build_bar_chart_paths(
    props: &[Property],
    size: [f32; 2],        // half-size (same convention as other plot builders)
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    parent_size: Option<[f64; 2]>,  // Some() when inside a Graph
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> Vec<VelloPath> {
    let data = parse_bar_chart_data(props, diagnostics, label);
    if data.is_empty() {
        return vec![];
    }

    // Parse properties
    let mut bar_width_auto = true;
    let mut bar_width_val = 20.0f32;
    let mut gap_auto = true;
    let mut gap_val = 4.0f32;
    let mut bar_colors_auto = true;
    let mut bar_colors: Vec<[f32; 4]> = vec![];
    let mut show_axis = true;
    let _direction = "vertical";  // Reserved for horizontal bar support
    let mut max_value_auto = true;
    let mut max_value_val = 0.0f32;

    for prop in props {
        let _subject = format!("{}.{}", label, prop.name);
        match prop.name.as_str() {
            "bar_width" => {
                if let Expr::Num(n) = &prop.value {
                    bar_width_val = *n as f32;
                    bar_width_auto = false;
                }
            }
            "gap" => {
                if let Expr::Num(n) = &prop.value {
                    gap_val = *n as f32;
                    gap_auto = false;
                }
            }
            "bar_colors" => {
                if let Expr::List(colors) = &prop.value {
                    let mut parsed = Vec::new();
                    for c in colors {
                        // Try to parse as RGBA tuple
                        if let Expr::Tuple(rgba) = c {
                            if rgba.len() == 4 {
                                let mut comps = [0.0f32; 4];
                                let mut ok = true;
                                for (i, v) in rgba.iter().enumerate() {
                                    if let Expr::Num(n) = v {
                                        comps[i] = *n as f32;
                                    } else {
                                        ok = false;
                                        break;
                                    }
                                }
                                if ok {
                                    parsed.push(comps);
                                }
                            }
                        }
                    }
                    if !parsed.is_empty() {
                        bar_colors = parsed;
                        bar_colors_auto = false;
                    }
                }
            }
            "show_axis" => {
                if let Expr::Str(s) = &prop.value {
                    show_axis = s == "true" || s == "1";
                }
            }
            "direction" => {
                // Parsed but not yet used; reserved for horizontal bar support
            }
            "max_value" => {
                if let Expr::Num(n) = &prop.value {
                    max_value_val = *n as f32;
                    if max_value_val > 0.0 {
                        max_value_auto = false;
                    }
                }
            }
            _ => {}
        }
    }

    let n = data.len() as f32;
    let full_w = (size[0] * 2.0) as f64;
    let full_h = (size[1] * 2.0) as f64;

    // Graph child mode: use parent domain/size for math→screen mapping
    let (use_math_coords, plot_w, plot_h, math_x0, math_x1, _math_y0, math_y1, baseline_y) =
        if let Some(p_size) = parent_size {
            // Inside a Graph — map math coordinates to pixels
            let pw = p_size[0];
            let ph = p_size[1];
            let baseline = if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                // Baseline at y=0 in math coords → screen
                ph * (1.0 - (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]))
            } else {
                // Baseline at min y
                ph * (1.0 - (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]))
            };
            (
                true, pw, ph,
                x_domain[0], x_domain[1],
                y_domain[0], y_domain[1],
                baseline,
            )
        } else {
            // Standalone — pixel coordinates within size bounds
            let plot_left = -(full_w / 2.0) + 40.0;   // margin for labels
            let plot_right = full_w / 2.0 - 20.0;
            let plot_top = -(full_h / 2.0) + 20.0;
            let plot_bottom = full_h / 2.0 - 40.0;    // margin for labels
            (
                false,
                plot_right - plot_left,
                plot_bottom - plot_top,
                plot_left, plot_right,
                plot_top, plot_bottom,
                plot_bottom,  // baseline at bottom
            )
        };

    // Auto bar width
    let n_f64 = n as f64;
    let bw = if bar_width_auto {
        let total_gap = gap_auto as usize as f64 * n_f64 * 4.0;
        ((plot_w - total_gap) / n_f64).max(4.0)
    } else if use_math_coords {
        // In graph mode, bar_width is in math x-units; convert to pixels
        let x_range_pixels = plot_w;
        let x_range_math = math_x1 - math_x0;
        if x_range_math > 0.0 {
            bar_width_val as f64 * x_range_pixels / x_range_math
        } else {
            bar_width_val as f64
        }
    } else {
        bar_width_val as f64
    };

    // Gap
    let n_f64 = n as f64;
    let gap = if gap_auto {
        if n_f64 <= 1.0 { 0.0 } else { (plot_w - bw * n_f64) / (n_f64 + 1.0) }
    } else if use_math_coords {
        let x_range_pixels = plot_w;
        let x_range_math = math_x1 - math_x0;
        if x_range_math > 0.0 {
            gap_val as f64 * x_range_pixels / x_range_math
        } else {
            gap_val as f64
        }
    } else {
        gap_val as f64
    };
    let gap = gap.max(1.0);

    // Determine max value for scaling (if using math coords, domain is authoritative)
    let max_val = if !max_value_auto {
        max_value_val as f64
    } else if use_math_coords {
        math_y1
    } else {
        data.iter().map(|(_, v)| *v as f64).fold(0.0f64, f64::max).max(0.001)
    };

    let mut paths: Vec<VelloPath> = Vec::with_capacity(data.len() + 1);

    // Optional axis line
    if show_axis {
        let mut axis = kurbo::BezPath::new();
        if use_math_coords {
            let ax0 = -(plot_w / 2.0);
            let ax1 = plot_w / 2.0;
            axis.move_to((ax0, baseline_y - plot_h / 2.0));
            axis.line_to((ax1, baseline_y - plot_h / 2.0));
        } else {
            axis.move_to((plot_w / 2.0, baseline_y));
            axis.line_to((-plot_w / 2.0, baseline_y));
        };
        let c = vello::peniko::Color::from_rgba8(
            (stroke_color[0] * 255.0) as u8,
            (stroke_color[1] * 255.0) as u8,
            (stroke_color[2] * 255.0) as u8,
            (stroke_color[3] * 255.0) as u8,
        );
        paths.push(VelloPath {
            path: axis,
            fill: None,
            stroke: Some((c, stroke_width.max(1.0))),
            line_cap: 0,
            line_join: 0,
        });
    }

    // Per-bar paths
    for (i, (_label_text, value)) in data.iter().enumerate() {
        let i_f = i as f64;

        // Bar position: evenly spaced from left to right
        let bar_x_start = -(plot_w / 2.0) + gap + i_f * (bw + gap);
        let bar_x_end = bar_x_start + bw;

        // Bar height in screen coords
        let val_norm = if max_val > 0.0 { *value as f64 / max_val } else { 0.0 };
        let bar_screen_height = val_norm * plot_h;
        let bar_top_y = baseline_y - bar_screen_height;

        // Build rectangle path
        let mut bp = kurbo::BezPath::new();
        bp.move_to(kurbo::Point::new(bar_x_start, baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, bar_top_y));
        bp.line_to(kurbo::Point::new(bar_x_start, bar_top_y));
        bp.close_path();

        // Per-bar color (cycle from list or use default)
        let bar_c = if !bar_colors_auto && i < bar_colors.len() {
            bar_colors[i]
        } else {
            color
        };
        let fill_c = vello::peniko::Color::from_rgba8(
            (bar_c[0] * 255.0) as u8,
            (bar_c[1] * 255.0) as u8,
            (bar_c[2] * 255.0) as u8,
            (bar_c[3] * 255.0) as u8,
        );

        paths.push(VelloPath {
            path: bp,
            fill: Some(fill_c),
            stroke: if stroke_width > 0.0 {
                let sc = vello::peniko::Color::from_rgba8(
                    (stroke_color[0] * 255.0) as u8,
                    (stroke_color[1] * 255.0) as u8,
                    (stroke_color[2] * 255.0) as u8,
                    (stroke_color[3] * 255.0) as u8,
                );
                Some((sc, stroke_width))
            } else {
                None
            },
            line_cap: 0,
            line_join: 0,
        });
    }

    paths
}
