//! This module implements `Timeline::build()`, the one-time lowering pass from
//! expanded AST to compiled timeline.
//!
//! It handles: colorscheme resolution, config processing, actor declarations,
//! property assignments, actions, container layout, component expansion,
//! text/math/code path compilation, and asset loading.

use std::collections::HashMap;

use super::*;
use crate::ast::{InlineItem, Property};
use crate::timeline::modifier_runtime::ir::evaluate_compiled_expr;
use crate::timeline::plot::{
    FuncSource, PlotCurveKind, PlotFuncRef, ProceduralPlot, build_implicit_plot_path_from_source,
    flatten_blend,
};
use crate::timeline::vello_path::VelloPath;

/// Data for tick/bar labels: positions and display text.
#[derive(Clone, Debug, Default)]
pub(crate) struct TickLabelData {
    /// (screen_x, screen_y, math_value) for each x-axis tick
    pub x_labels: Vec<(f64, f64, f64)>,
    /// (screen_x, screen_y, math_value) for each y-axis tick
    pub y_labels: Vec<(f64, f64, f64)>,
    /// (local_x, local_y, label text) for each BarChart bar
    pub bar_labels: Vec<(f64, f64, String)>,
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
    pub(super) func: &'a Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    pub(super) p_x_domain: [f64; 2],
    pub(super) p_y_domain: [f64; 2],
    pub(super) p_size: [f64; 2],
    /// Parent graph padding [left, right, top, bottom] in pixels.
    pub(super) p_padding: [f64; 4],
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
    params
        .build_quality
        .scale_plot_params(&mut tolerance, &mut max_depth, &mut resolution);

    if let Some((args, body, _captures)) = params.func {
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
            let source = FuncSource::Compiled(
                args.to_vec(),
                Box::new((**body).clone()),
                CapturedEnv::default(),
            );
            let path = build_implicit_plot_path_from_source(
                &mut env_copy,
                &source,
                &params.p_x_domain,
                &params.p_y_domain,
                &params.p_size,
                resolution.max(8),
                &params.p_padding,
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
            let start_eval =
                evaluate_compiled_expr(body, &env_copy).unwrap_or(Value::Num(f64::NAN));
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
            let (start_screen_x, start_screen_y) = crate::timeline::plot::math_to_screen_padded(
                start_math_x,
                start_math_y,
                &params.p_x_domain,
                &params.p_y_domain,
                &params.p_size,
                &params.p_padding,
            );

            env_copy.set_binding(&arg_name, Value::Num(max_t));
            let end_eval = evaluate_compiled_expr(body, &env_copy).unwrap_or(Value::Num(f64::NAN));
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
            let (end_screen_x, end_screen_y) = crate::timeline::plot::math_to_screen_padded(
                end_math_x,
                end_math_y,
                &params.p_x_domain,
                &params.p_y_domain,
                &params.p_size,
                &params.p_padding,
            );

            let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
            let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

            let mut pts = vec![p0];
            // Wrap the declaration body in a PlotFuncRef::Single for the
            // refactored sampling functions. Build time always uses Single.
            let func_source = FuncSource::Compiled(
                args.clone(),
                Box::new((**body).clone()),
                CapturedEnv::default(),
            );
            let plot_func = PlotFuncRef::Single(&func_source);
            let mut from_cache = HashMap::<u64, Value>::new();
            let mut to_cache = HashMap::<u64, Value>::new();

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
                    &plot_func,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &params.p_padding,
                    &mut from_cache,
                    &mut to_cache,
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
                    &plot_func,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &params.p_padding,
                    &mut from_cache,
                    &mut to_cache,
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
                    &plot_func,
                    &params.p_x_domain,
                    &params.p_y_domain,
                    &params.p_size,
                    &params.p_padding,
                    &mut from_cache,
                    &mut to_cache,
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

/// Normalize `v` in `[min, max]` to `[0, 1]` using the given scale.
/// Returns `0.5` for invalid log domains (any input ≤ 0).
#[inline]
fn normalize_axis(v: f64, min: f64, max: f64, scale: super::utils::ScaleType) -> f64 {
    if scale.is_log() {
        if v <= 0.0 || min <= 0.0 || max <= 0.0 {
            return 0.5;
        }
        (v.ln() - min.ln()) / (max.ln() - min.ln())
    } else {
        let range = max - min;
        if range != 0.0 { (v - min) / range } else { 0.5 }
    }
}

/// Generate tick positions for an axis.
///
/// For `Log` scale, returns powers-of-10 and their multiples (1–9).
/// For linear, returns ~10 evenly spaced values.
fn generate_axis_ticks(min: f64, max: f64, scale: super::utils::ScaleType) -> Vec<f64> {
    if scale.is_log() {
        if min <= 0.0 || max <= 0.0 {
            return vec![];
        }
        let mut ticks = vec![];
        let min_exp = min.log10().floor() as i32;
        let max_exp = max.log10().ceil() as i32;
        for exp in min_exp..=max_exp {
            let base = 10.0_f64.powi(exp);
            for i in 1..10u32 {
                let tick = base * i as f64;
                if tick >= min && tick <= max {
                    ticks.push(tick);
                }
            }
        }
        ticks
    } else {
        let step = ((max - min).abs() / 10.0).max(0.5);
        let mut ticks = vec![];
        let mut v = (min / step).ceil() * step;
        while v <= max {
            ticks.push(v);
            v += step;
        }
        ticks
    }
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
    padding: [f64; 4],
    x_scale: super::utils::ScaleType,
    y_scale: super::utils::ScaleType,
) -> Vec<VelloPath> {
    let mut paths = Vec::new();
    let mut axis_path = kurbo::BezPath::new();

    // Padding components and derived plot-area extents.
    let (pad_left, pad_right, pad_top, pad_bottom) =
        (padding[0], padding[1], padding[2], padding[3]);
    let hw = size[0] as f64; // graph half-width
    let hh = size[1] as f64; // graph half-height
    // Full-width/height of the padded plot area.
    let plot_fw = 2.0 * hw - pad_left - pad_right;
    let plot_fh = 2.0 * hh - pad_top - pad_bottom;
    // Screen-space shift of the padded plot center from the graph center.
    let shift_x = (pad_left - pad_right) / 2.0;
    let shift_y = (pad_top - pad_bottom) / 2.0;
    // Padded edge positions in local screen space.
    let left_edge = -hw + pad_left;
    let right_edge = hw - pad_right;
    let top_edge = -hh + pad_top;
    let bot_edge = hh - pad_bottom;

    // Helper: map a normalised coordinate to a padded screen position.
    let norm_to_screen_x = |norm: f64| shift_x + (norm - 0.5) * plot_fw;
    let norm_to_screen_y = |norm: f64| shift_y + (0.5 - norm) * plot_fh;
    let math_x_to_screen =
        |mx: f64| norm_to_screen_x(normalize_axis(mx, x_domain[0], x_domain[1], x_scale));
    let math_y_to_screen =
        |my: f64| norm_to_screen_y(normalize_axis(my, y_domain[0], y_domain[1], y_scale));

    // X-axis: drawn only when y=0 is inside the y_domain (only valid for linear scale)
    let x_axis_y = if !y_scale.is_log() && y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let y = math_y_to_screen(0.0);
        axis_path.move_to((left_edge, y));
        axis_path.line_to((right_edge, y));
        Some(y)
    } else {
        None
    };

    // Y-axis: drawn only when x=0 is inside the x_domain (only valid for linear scale)
    let y_axis_x = if !x_scale.is_log() && x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let x = math_x_to_screen(0.0);
        axis_path.move_to((x, top_edge));
        axis_path.line_to((x, bot_edge));
        Some(x)
    } else {
        None
    };

    if !axis_path.elements().is_empty() {
        paths.push(VelloPath {
            path: axis_path,
            fill: None,
            stroke: Some((
                vello::peniko::Color::from_rgba8(
                    (axis_color[0] * 255.0) as u8,
                    (axis_color[1] * 255.0) as u8,
                    (axis_color[2] * 255.0) as u8,
                    (axis_color[3] * 255.0) as u8,
                ),
                2.0,
            )),
            line_cap: 0,
            line_join: 0,
        });
    }

    // Grid lines
    if grid {
        let mut grid_path = kurbo::BezPath::new();
        let x_grid_ticks = generate_axis_ticks(x_domain[0], x_domain[1], x_scale);
        let y_grid_ticks = generate_axis_ticks(y_domain[0], y_domain[1], y_scale);

        // Vertical grid lines (constant x)
        for x in &x_grid_ticks {
            if *x != 0.0 {
                let screen_x = math_x_to_screen(*x);
                grid_path.move_to((screen_x, top_edge));
                grid_path.line_to((screen_x, bot_edge));
            }
        }

        // Horizontal grid lines (constant y)
        for y in &y_grid_ticks {
            if *y != 0.0 {
                let screen_y = math_y_to_screen(*y);
                grid_path.move_to((left_edge, screen_y));
                grid_path.line_to((right_edge, screen_y));
            }
        }

        if !grid_path.elements().is_empty() {
            paths.push(VelloPath {
                path: grid_path,
                fill: None,
                stroke: Some((
                    vello::peniko::Color::from_rgba8(
                        (axis_color[0] * 255.0) as u8,
                        (axis_color[1] * 255.0) as u8,
                        (axis_color[2] * 255.0) as u8,
                        (axis_color[3] * 255.0) as u8 / 4,
                    ),
                    1.0,
                )),
                line_cap: 0,
                line_join: 0,
            });
        }
    }

    // Tick marks
    if ticks {
        let mut tick_path = kurbo::BezPath::new();
        let tick_len = 4.0;
        let x_tick_vals = generate_axis_ticks(x_domain[0], x_domain[1], x_scale);
        let y_tick_vals = generate_axis_ticks(y_domain[0], y_domain[1], y_scale);

        if let Some(y) = x_axis_y {
            for x in &x_tick_vals {
                if *x != 0.0 {
                    let screen_x = math_x_to_screen(*x);
                    tick_path.move_to((screen_x, y - tick_len));
                    tick_path.line_to((screen_x, y + tick_len));
                }
            }
        }

        if let Some(x) = y_axis_x {
            for y in &y_tick_vals {
                if *y != 0.0 {
                    let screen_y = math_y_to_screen(*y);
                    tick_path.move_to((x - tick_len, screen_y));
                    tick_path.line_to((x + tick_len, screen_y));
                }
            }
        }

        if !tick_path.elements().is_empty() {
            paths.push(VelloPath {
                path: tick_path,
                fill: None,
                stroke: Some((
                    vello::peniko::Color::from_rgba8(
                        (axis_color[0] * 255.0) as u8,
                        (axis_color[1] * 255.0) as u8,
                        (axis_color[2] * 255.0) as u8,
                        (axis_color[3] * 255.0) as u8,
                    ),
                    1.5,
                )),
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
        let primitive = self
            .primitive_registry
            .find(ty)
            .map(PrimitiveFamilyDescriptor::from_primitive)
            .unwrap_or_default();
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
        let mut x_scale = super::utils::ScaleType::Linear;
        let mut y_scale = super::utils::ScaleType::Linear;
        let mut x_range = [-10.0, 10.0, 2.0];
        let mut y_range = [-10.0, 10.0, 2.0];
        let is_number_plane = ty == "NumberPlane";
        let initial_eval_env = self.build_eval_env(time_ms as u64);

        // Graph plot-area padding [left, right, top, bottom] in pixels.
        // Pre-scan props for padding so it's available before the main match loop.
        let graph_padding: [f64; 4] = if primitive.is_graph_host() {
            props
                .iter()
                .find(|p| p.name == "padding")
                .and_then(|p| {
                    match crate::timeline::utils::evaluate_expr(&p.value, &initial_eval_env) {
                        Ok(Value::Vec4([l, r, t, b])) => Some([l, r, t, b]),
                        Ok(Value::Num(n)) => Some([n, n, n, n]),
                        _ => None,
                    }
                })
                .unwrap_or([0.0; 4])
        } else {
            [0.0; 4]
        };

        // Start with track defaults, override from props.
        let mut color = existing_track.style.color.last(DEFAULT_WHITE);
        let default_stroke =
            ActorKindId::from_type_name(ty).map(default_stroke_width).unwrap_or(0.0);
        let mut stroke_width = existing_track.style.stroke_width.last(default_stroke);
        let mut stroke_color = existing_track.style.stroke_color.last(DEFAULT_WHITE);
        let mut stroke_progress = existing_track.style.stroke_progress.last(1.0);

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
                },
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
                },
                "x_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => x_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 'x_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "y_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => y_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 'y_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "t_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => t_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 't_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "func" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Closure(args, body, captures)) => {
                            func = Some((args, body, captures))
                        },
                        Some(v) => tracing::warn!(
                            "{}: 'func' expects a closure, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
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
                },
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
                },
                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_width = v.as_num() as f32;
                },
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
                },
                "tolerance" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    tolerance = v.as_num();
                },
                "max_depth" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    max_depth = v.as_num();
                },
                "resolution" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(96.0));
                    resolution = v.as_num();
                },
                "density" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(16.0));
                    density = v.as_num().max(2.0).round();
                },
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
                        },
                        Value::Num(n) => {
                            levels.push(n);
                        },
                        _ => {
                            tracing::warn!(
                                "{}: 'levels' expects a number or list of numbers, got {:?}",
                                prop_subject,
                                v
                            );
                        },
                    }
                },
                "grid" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    grid = v.as_bool();
                },
                "ticks" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    ticks = v.as_bool();
                },
                "tick_labels" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Str("auto".to_string()));
                    tick_labels = v.as_str().to_lowercase();
                },
                "kind" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        if let Some(k) = PlotCurveKind::from_str(&v.as_str().to_lowercase()) {
                            kind = k;
                        } else {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("Invalid plot kind: '{}'", v.as_str()),
                                )
                                .with_subject(&prop_subject),
                            );
                        }
                    }
                },
                "x_range" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec3([min, max, step])) => x_range = [min, max, step],
                        Some(v) => tracing::warn!(
                            "{}: 'x_range' expects a (min, max, step) triple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "y_range" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec3([min, max, step])) => y_range = [min, max, step],
                        Some(v) => tracing::warn!(
                            "{}: 'y_range' expects a (min, max, step) triple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "padding" if primitive.is_graph_host() => {
                    // Graph padding is pre-computed above; skip here to prevent
                    // falling through to the custom plot-param collector.
                },
                "x_scale" if primitive.is_graph_host() => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        x_scale = super::utils::ScaleType::from_str(&v.as_str());
                    }
                },
                "y_scale" if primitive.is_graph_host() => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        y_scale = super::utils::ScaleType::from_str(&v.as_str());
                    }
                },
                _ => {}, /* Non-plot properties (color, stroke, etc.) are handled by the general
                          * actor pipeline. */
            }
        }

        // Validate plot func signature if present (BarChart does not take func).
        let is_vector_field = ty == "VectorField";
        let is_heatmap = ty == "Heatmap";
        let is_contour_set = ty == "ContourSet";
        let is_bar_chart = ty == "BarChart";
        if let Some((ref args, ref body, _)) = func {
            if !is_bar_chart {
                let (expected_arity, expected_ty) = if is_vector_field {
                    (2, "vec2")
                } else if is_heatmap || is_contour_set {
                    (2, "number")
                } else {
                    match kind {
                        PlotCurveKind::Cartesian | PlotCurveKind::Polar => (1, "number"),
                        PlotCurveKind::Parametric => (1, "vec2"),
                        PlotCurveKind::Implicit => (2, "number"),
                    }
                };
                if args.len() != expected_arity {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidPlotFunc,
                            DiagnosticPhase::Build,
                            format!(
                                "{} expects a func with {} argument(s), got {}",
                                ty,
                                expected_arity,
                                args.len()
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
                if let Ok(result) = evaluate_compiled_expr(body, &test_env) {
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
                                        Value::Closure(_, _, _) => "closure".to_string(),
                                        Value::UserFn { name, .. } =>
                                            format!("user function {name}"),
                                        Value::Object(name, _) => name.clone(),
                                        Value::Bool(_) => "bool".to_string(),
                                    }
                                ),
                            )
                            .with_subject(label),
                        );
                    }
                }
            } // end func validation guard (BarChart only)
        } // end func validation guard

        if primitive.is_graph_host() {
            self.env.set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
            self.env.set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
            // Store the DECLARED (full-pixel) size. Every consumer of this env
            // key (math_to_screen_padded, hosted BarChart math mode,
            // .map()/.map_inverse via GraphGeometry) treats it as full size;
            // seeding the half-size here made hosted plots occupy only the
            // central half of the axis box.
            self.env.set(
                &format!("{}_size", label),
                Value::Vec2([
                    (initial_size[0] * 2.0) as f64,
                    (initial_size[1] * 2.0) as f64,
                ]),
            );
            // Store graph axis settings so they survive size-assignment rebuilds
            self.env.set(&format!("{}_grid", label), Value::Bool(grid));
            self.env.set(&format!("{}_ticks", label), Value::Bool(ticks));
            self.env.set(&format!("{}_tick_labels", label), Value::Str(tick_labels.clone()));
            self.env.set(&format!("{}_padding", label), Value::Vec4(graph_padding));
            // Store scale in env as strings so assignments.rs rebuild can read them.
            let x_scale_str = if x_scale.is_log() { "log" } else { "linear" };
            let y_scale_str = if y_scale.is_log() { "log" } else { "linear" };
            self.env.set(&format!("{}_x_scale", label), Value::Str(x_scale_str.to_string()));
            self.env.set(&format!("{}_y_scale", label), Value::Str(y_scale_str.to_string()));

            // Inject {label}.map as a NativeFn that converts math coords to screen coords.
            // Captures x_domain/y_domain/graph_padding/scales (static), reads size/at from runtime
            // env.
            let map_label = label.to_string();
            let nf = make_graph_map_fn(
                map_label.clone(),
                x_domain,
                y_domain,
                graph_padding,
                x_scale,
                y_scale,
            );
            self.env.set(&format!("{}.map", label), nf);

            // Inject {label}.map_inverse as a NativeFn that converts screen coords to math coords.
            let nf_inv = make_graph_map_inverse_fn(map_label, x_domain, y_domain, x_scale, y_scale);
            self.env.set(&format!("{}.map_inverse", label), nf_inv);
        }

        self.process_inline_items(time_ms, children, label, diagnostics);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, std::f32::consts::PI];
        // Prefer the size parsed from this declaration. On first declaration
        // the pre-declaration track snapshot below still holds the [50, 50]
        // default, which made standalone plot builders (BarChart, VectorField,
        // Heatmap, ContourSet, NumberPlane) lay out in a ~40x40 box regardless
        // of `size:`. Re-declarations keep inheriting the stored track size.
        let size = if existing_track.geometry.size.last(default_size) != default_size {
            existing_track.geometry.size.last(default_size)
        } else {
            initial_size
        };
        let line_from = existing_track.shape.line_from.last([-50.0, 0.0]);
        let line_to = existing_track.shape.line_to.last([50.0, 0.0]);
        let arc_angles = existing_track.shape.arc_angles.last(default_arc);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let fill_opacity = 0.0f32;

        let mut vello_paths = vec![];
        let mut procedural_plot = None;
        let mut tick_label_data = TickLabelData::default();

        // Collect custom numeric params from declaration props. Done for every
        // func-backed plot so keyframeable params get a procedural_plot even
        // when the body does not reference `t`.
        let mut plot_params: Vec<(String, f64)> = Vec::new();
        if func.is_some() {
            for prop in props {
                // Skip known plot properties
                match prop.name.as_str() {
                    "kind" | "func" | "x_domain" | "y_domain" | "t_domain" | "tolerance"
                    | "max_depth" | "resolution" | "density" | "levels" | "grid" | "ticks"
                    | "tick_labels" | "x_range" | "y_range" | "size" | "at" | "position"
                    | "color" | "opacity" | "stroke" | "stroke_color" | "stroke_width"
                    | "stroke_progress" | "fill_opacity" | "radius" | "radius_x" | "radius_y"
                    | "from" | "to" | "head_size" | "text" | "content" | "code" | "font_size"
                    | "font_family" | "url" | "source" | "volume" | "anchor" | "offset"
                    | "rotation" | "scale" | "transform" | "blur" | "brightness" | "contrast"
                    | "saturate" | "hue_rotate" | "sepia" | "gap" | "padding" | "align"
                    | "cols" | "data" | "bar_width" | "bar_colors" | "direction" | "max_value"
                    | "show_axis" | "show_labels" => {},
                    _ => {
                        // Treat unknown numeric props as plot parameters
                        let eval_env = self.build_eval_env(time_ms as u64);
                        if let Ok(Value::Num(n)) = evaluate_expr(&prop.value, &eval_env) {
                            plot_params.push((prop.name.clone(), n));
                        }
                    },
                }
            }
        }

        if primitive.is_graph_host() {
            let label_x = tick_labels_has_axis(&tick_labels, 'x');
            let label_y = tick_labels_has_axis(&tick_labels, 'y');

            vello_paths = build_graph_axis_paths(
                size,
                x_domain,
                y_domain,
                stroke_color,
                grid,
                ticks,
                label_x || label_y,
                graph_padding,
                x_scale,
                y_scale,
            );

            // Compute tick label positions (same logic as build_graph_axis_paths ticks section).
            // Uses padded coordinate mapping so labels align with padded axes.
            let tick_label_offset = 14.0;
            let hw = size[0] as f64;
            let hh = size[1] as f64;
            let plot_fw = 2.0 * hw - graph_padding[0] - graph_padding[1];
            let plot_fh = 2.0 * hh - graph_padding[2] - graph_padding[3];
            let shift_x = (graph_padding[0] - graph_padding[1]) / 2.0;
            let shift_y = (graph_padding[2] - graph_padding[3]) / 2.0;
            let tick_math_x_to_screen = |mx: f64| {
                let norm = normalize_axis(mx, x_domain[0], x_domain[1], x_scale);
                shift_x + (norm - 0.5) * plot_fw
            };
            let tick_math_y_to_screen = |my: f64| {
                let norm = normalize_axis(my, y_domain[0], y_domain[1], y_scale);
                shift_y + (0.5 - norm) * plot_fh
            };

            // X-axis at y=0 screen position
            let x_ticks = generate_axis_ticks(x_domain[0], x_domain[1], x_scale);
            if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                let axis_y = tick_math_y_to_screen(0.0);
                if label_x {
                    for &x in &x_ticks {
                        if x != 0.0 {
                            let screen_x = tick_math_x_to_screen(x);
                            tick_label_data.x_labels.push((
                                screen_x,
                                axis_y + tick_label_offset,
                                x,
                            ));
                        }
                    }
                }
            }

            // Y-axis at x=0 screen position
            let y_ticks = generate_axis_ticks(y_domain[0], y_domain[1], y_scale);
            if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
                let axis_x = tick_math_x_to_screen(0.0);
                if label_y {
                    for &y in &y_ticks {
                        if y != 0.0 {
                            let screen_y = tick_math_y_to_screen(y);
                            tick_label_data.y_labels.push((
                                axis_x - tick_label_offset,
                                screen_y,
                                y,
                            ));
                        }
                    }
                }
            }
        } else if is_vector_field {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_density = density as usize;
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
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = resolution.max(2.0).round() as usize;
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
                    x_domain,
                    y_domain,
                    full_size,
                    scaled_res.max(4),
                    color,
                );
            }
        } else if is_contour_set {
            let mut eval_env = self.build_eval_env(time_ms as u64);
            if let Some((args, body, captures)) = func.as_ref() {
                let full_size = [size[0] as f64 * 2.0, size[1] as f64 * 2.0];
                let mut scaled_res = resolution.max(8.0) as usize;
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
            vello_paths =
                build_number_plane_paths(size, x_domain, y_domain, x_range, y_range, stroke_color);
        } else if is_bar_chart {
            // Determine parent graph context (if any)
            let p_label = parent_label.unwrap_or("").to_string();
            let parent_size =
                if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                    Some(sz)
                } else {
                    None
                };
            let p_x_domain =
                if let Some(Value::Vec2(xd)) = self.env.get(&format!("{}_x_domain", p_label)) {
                    xd
                } else {
                    x_domain
                };
            let p_y_domain =
                if let Some(Value::Vec2(yd)) = self.env.get(&format!("{}_y_domain", p_label)) {
                    yd
                } else {
                    y_domain
                };

            let (paths, bar_labels) = build_bar_chart_paths(
                props,
                size,
                color,
                stroke_color,
                stroke_width,
                p_x_domain,
                p_y_domain,
                parent_size,
                &initial_eval_env,
                diagnostics,
                label,
            );
            vello_paths = paths;
            tick_label_data.bar_labels = bar_labels;
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
            let is_static = func.as_ref().is_none_or(|(_, body, _)| !body.references_ident("t"));
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

            let p_padding = self
                .env
                .get(&format!("{}_padding", p_label))
                .and_then(|v| {
                    if let Value::Vec4(p) = v {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap_or([0.0; 4]);

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
                        p_padding,
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
                    p_padding,
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

            // Always create a procedural_plot for PlotCurve actors so that
            // func transitions can be added later via assignment. Static plots
            // (no `t`, no params, no transitions) are still guarded at frame
            // time by `is_dynamic()` in scene_eval.rs, so they keep using the
            // cached build-time paths with zero per-frame overhead.
            if let Some((args, body, captures)) = func.as_ref() {
                let param_names: Vec<String> = plot_params.iter().map(|(n, _)| n.clone()).collect();

                procedural_plot = Some(ProceduralPlot {
                    plot_type: crate::timeline::plot::ProceduralPlotKind::Curve(kind),
                    kind,
                    func_args: args.clone(),
                    func_body: (**body).clone(),
                    actor_label: label.to_string(),
                    param_names,
                    p_x_domain,
                    p_y_domain,
                    p_size,
                    padding: p_padding,
                    t_domain,
                    tolerance,
                    max_depth: max_depth as usize,
                    resolution: resolution as usize,
                    density: density as usize,
                    levels: levels.clone(),
                    stroke_width,
                    stroke_color,
                    fill_color: color,
                    params: plot_params.clone(),
                    extra_captures: captures.clone(),
                });
            }
        }

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
                    kind,
                    func_args: args.clone(),
                    func_body: (**body).clone(),
                    actor_label: label.to_string(),
                    param_names,
                    p_x_domain: x_domain,
                    p_y_domain: y_domain,
                    p_size: [size[0] as f64, size[1] as f64],
                    padding: [0.0; 4],
                    t_domain,
                    tolerance,
                    max_depth: max_depth as usize,
                    resolution: resolution as usize,
                    density: density as usize,
                    levels: levels.clone(),
                    stroke_width,
                    stroke_color,
                    fill_color: color,
                    params: plot_params,
                    extra_captures: captures.clone(),
                });
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
            tick_label_data: if (primitive.is_graph_host() || ty == "BarChart")
                && (!tick_label_data.x_labels.is_empty()
                    || !tick_label_data.y_labels.is_empty()
                    || !tick_label_data.bar_labels.is_empty())
            {
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
            // Flat number list? E.g. {10, 20, 30} — auto-label with 1-based indices.
            if !items.is_empty() && items.iter().all(|item| matches!(item, Expr::Num(_))) {
                for (i, item) in items.iter().enumerate() {
                    if let Expr::Num(n) = item {
                        data.push(((i + 1).to_string(), *n as f32));
                    }
                }
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{}' data: flat number list detected — auto-labeling with 1-based indices. \
                         To specify custom labels, use (label, value) tuples like ('A', 10), ('B', 20)",
                        label
                    ),
                ));
            } else {
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
                                        format!(
                                            "BarChart '{}' data key must be a number or string",
                                            label
                                        ),
                                    ));
                                    continue;
                                },
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
                                },
                            };
                            data.push((key_str, val));
                        },
                        _ => {
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "BarChart '{}' data entry must be a (key, value) tuple",
                                    label
                                ),
                            ));
                        },
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
/// Gap between bars is auto-calculated from `(plot_width - bar_count * bar_width) / (bar_count +
/// 1)`.
///
/// # Graph child mode
/// Bars use math-coordinate mapping via `p_x_domain`, `p_y_domain`, `p_size`.
/// Labels are in math-space but rendered as child Text tracks (handled by the caller).
pub(crate) fn build_bar_chart_paths(
    props: &[Property],
    size: [f32; 2], // half-size (same convention as other plot builders)
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    parent_size: Option<[f64; 2]>, // Some() when inside a Graph
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> (Vec<VelloPath>, Vec<(f64, f64, String)>) {
    let data = parse_bar_chart_data(props, diagnostics, label);
    if data.is_empty() {
        return (vec![], vec![]);
    }

    // Parse properties
    let mut bar_width_auto = true;
    let mut bar_width_val = 20.0f32;
    let mut gap_auto = true;
    let mut gap_val = 4.0f32;
    let mut bar_colors_auto = true;
    let mut bar_colors: Vec<[f32; 4]> = vec![];
    let mut show_axis = true;
    let mut show_labels = true;
    let mut bar_labels = Vec::new();
    let _direction = "vertical"; // Reserved for horizontal bar support
    let mut max_value_auto = true;
    let mut max_value_val = 0.0f32;

    for prop in props {
        let subject = format!("{}.{}", label, prop.name);
        match prop.name.as_str() {
            "bar_width" => {
                let is_auto = matches!(&prop.value, Expr::Ident(s) | Expr::Str(s) if s == "auto");
                if is_auto {
                    // keep default auto distribution
                } else if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject)
                {
                    match v {
                        Value::Num(n) => {
                            bar_width_val = n as f32;
                            bar_width_auto = false;
                        },
                        other => diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "BarChart '{label}' bar_width expects a number or \"auto\", got {:?}",
                                    other
                                ),
                            )
                            .with_subject(&subject),
                        ),
                    }
                }
            },
            "gap" => {
                let is_auto = matches!(&prop.value, Expr::Ident(s) | Expr::Str(s) if s == "auto");
                if is_auto {
                    // keep default auto spacing
                } else if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject)
                {
                    match v {
                        Value::Num(n) => {
                            gap_val = n as f32;
                            gap_auto = false;
                        },
                        other => diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "BarChart '{label}' gap expects a number or \"auto\", got {:?}",
                                    other
                                ),
                            )
                            .with_subject(&subject),
                        ),
                    }
                }
            },
            "bar_colors" => {
                let is_auto = match &prop.value {
                    Expr::Ident(s) | Expr::Str(s) => s == "auto",
                    _ => false,
                };
                if is_auto {
                    // keep defaults (bar_colors_auto stays true)
                } else if let Expr::List(colors) = &prop.value {
                    let mut parsed = Vec::with_capacity(colors.len());
                    for c in colors {
                        if let Some(col) = parse_color_in_env_with_lookup_diagnostic(
                            label,
                            "bar_colors",
                            c,
                            env,
                            diagnostics,
                            &subject,
                        ) {
                            parsed.push(col);
                        }
                    }
                    if !parsed.is_empty() {
                        bar_colors = parsed;
                        bar_colors_auto = false;
                    }
                } else {
                    // Single color (no list) → uniform color
                    match resolve_color_in_env(&prop.value, env) {
                        Ok(Some(col)) => {
                            bar_colors = vec![col];
                            bar_colors_auto = false;
                        },
                        Ok(None) => {
                            // expression didn't resolve to a color — fall back to auto
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!("BarChart '{label}' bar_colors value is not a color; falling back to auto"),
                            ).with_subject(&subject));
                        },
                        Err(EvalError::UndefinedVariable(key)) => {
                            let candidate_keys = env.all_keys();
                            let suggestion = best_path_suggestion(
                                &key,
                                candidate_keys.iter().map(String::as_str),
                            );
                            let hint = suggestion
                                .map(|candidate| format!(" Did you mean '{candidate}'?"))
                                .unwrap_or_default();
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::UnknownColorReference,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Color value '{key}' on '{}.bar_colors' does not resolve to a known color; falling back to auto bar colors.{hint}",
                                        label
                                    ),
                                ).with_subject(&subject),
                            );
                            // fall back to auto (bar_colors_auto stays true, bar_colors stays
                            // empty)
                        },
                        Err(_) => {
                            // other eval error — fall back to auto
                        },
                    }
                }
            },
            "show_axis" => {
                if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject)
                {
                    show_axis = match v {
                        Value::Bool(b) => b,
                        Value::Str(s) => s == "true" || s == "1",
                        _ => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("BarChart '{label}' show_axis expects a boolean"),
                                )
                                .with_subject(&subject),
                            );
                            true
                        },
                    };
                }
            },
            "show_labels" => {
                if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject)
                {
                    show_labels = match v {
                        Value::Bool(b) => b,
                        Value::Str(s) => s == "true" || s == "1",
                        _ => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("BarChart '{label}' show_labels expects a boolean"),
                                )
                                .with_subject(&subject),
                            );
                            true
                        },
                    };
                }
            },
            "direction" => {
                // Parsed but not yet used; reserved for horizontal bar support
            },
            "max_value" => {
                let is_auto = matches!(&prop.value, Expr::Ident(s) | Expr::Str(s) if s == "auto");
                if is_auto {
                    // keep default auto scale
                } else if let Some(v) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, &subject)
                {
                    match v {
                        Value::Num(n) => {
                            let n = n as f32;
                            max_value_val = n;
                            if n > 0.0 {
                                max_value_auto = false;
                            }
                        },
                        other => diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "BarChart '{label}' max_value expects a positive number or \"auto\", got {:?}",
                                    other
                                ),
                            )
                            .with_subject(&subject),
                        ),
                    }
                }
            },
            _ => {}, // Non-bar-chart properties are handled by the general actor pipeline.
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
            (true, pw, ph, x_domain[0], x_domain[1], y_domain[0], y_domain[1], baseline)
        } else {
            // Standalone — pixel coordinates within size bounds
            let plot_left = -(full_w / 2.0) + 40.0; // margin for labels
            let plot_right = full_w / 2.0 - 20.0;
            let plot_top = -(full_h / 2.0) + 20.0;
            let plot_bottom = full_h / 2.0 - 40.0; // margin for labels
            (
                false,
                plot_right - plot_left,
                plot_bottom - plot_top,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
                plot_bottom, // baseline at bottom
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
        if n_f64 <= 1.0 {
            0.0
        } else {
            (plot_w - bw * n_f64) / (n_f64 + 1.0)
        }
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
    for (i, (label_text, value)) in data.iter().enumerate() {
        let i_f = i as f64;

        // Bar position: evenly spaced from left to right
        let bar_x_start = -(plot_w / 2.0) + gap + i_f * (bw + gap);
        let bar_x_end = bar_x_start + bw;
        let bar_center_x = bar_x_start + bw / 2.0;

        // Bar height in screen coords
        let val_norm = if max_val > 0.0 {
            *value as f64 / max_val
        } else {
            0.0
        };
        let bar_screen_height = val_norm * plot_h;
        let bar_top_y = baseline_y - bar_screen_height;

        // Build rectangle path
        let mut bp = kurbo::BezPath::new();
        bp.move_to(kurbo::Point::new(bar_x_start, baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, bar_top_y));
        bp.line_to(kurbo::Point::new(bar_x_start, bar_top_y));
        bp.close_path();

        // Per-bar color: a single `bar_colors` value is uniform across all
        // bars (documented at the parse site); a list assigns per bar, with
        // bars past the end of the list falling back to the actor color.
        let bar_c = if !bar_colors_auto && bar_colors.len() == 1 {
            bar_colors[0]
        } else if !bar_colors_auto && i < bar_colors.len() {
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

        if show_labels {
            bar_labels.push((bar_center_x, baseline_y + 16.0, label_text.clone()));
        }
    }

    (paths, bar_labels)
}

/// Create a `Value::NativeFn` for `{label}.map_inverse(screen_x, screen_y)` → math coords.
///
/// Captures `x_domain`/`y_domain`/`x_scale`/`y_scale` (static), reads `size`, `at`, and
/// `padding` from the runtime environment to support animation of graph size/position.
fn make_graph_map_inverse_fn(
    label: String,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    x_scale: super::utils::ScaleType,
    y_scale: super::utils::ScaleType,
) -> Value {
    Value::NativeFn(std::sync::Arc::new(
        move |args: &[Value],
              env: &crate::timeline::Environment|
              -> Result<Value, crate::timeline::EvalError> {
            if args.len() != 2 {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "graph.map_inverse expects 2 arguments: (screen_x, screen_y)".to_string(),
                ));
            }
            let sx = match &args[0] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map_inverse: first argument 'screen_x' must be a number".to_string(),
                    ));
                },
            };
            let sy = match &args[1] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map_inverse: second argument 'screen_y' must be a number"
                            .to_string(),
                    ));
                },
            };

            // Read size, at, padding from runtime env for animation support.
            // These are set during build with underscore-prefixed keys.
            let size = env
                .get(&format!("{}_size", label))
                .and_then(|v| match v {
                    Value::Vec2(s) => Some(s),
                    _ => None,
                })
                .unwrap_or([500.0, 500.0]);
            let at = env
                .get(&format!("{}_at", label))
                .and_then(|v| match v {
                    Value::Vec2(a) => Some(a),
                    _ => None,
                })
                .unwrap_or([0.0, 0.0]);
            let padding = env
                .get(&format!("{}_padding", label))
                .and_then(|v| match v {
                    Value::Vec4(p) => Some(p),
                    _ => None,
                })
                .unwrap_or([0.0; 4]);

            let scale = super::utils::GraphScaleConfig::new(x_domain, y_domain, x_scale, y_scale);
            let geo = super::utils::GraphGeometry::new(size, at, padding);
            let [mx, my] = super::utils::graph_screen_to_math(sx, sy, &scale, &geo);
            Ok(Value::Vec2([mx, my]))
        },
    ))
}

/// Create a `Value::NativeFn` for `{label}.map(math_x, math_y)` → screen coords.
///
/// Captures `x_domain`/`y_domain`/`padding`/`x_scale`/`y_scale` (static), reads `size` and
/// `at` from the runtime environment to support animation of graph size/position.
fn make_graph_map_fn(
    label: String,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    padding: [f64; 4],
    x_scale: super::utils::ScaleType,
    y_scale: super::utils::ScaleType,
) -> Value {
    Value::NativeFn(std::sync::Arc::new(
        move |args: &[Value],
              env: &crate::timeline::Environment|
              -> Result<Value, crate::timeline::EvalError> {
            if args.len() != 2 {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "graph.map expects 2 arguments: (math_x, math_y)".to_string(),
                ));
            }
            let mx = match &args[0] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map: first argument 'math_x' must be a number".to_string(),
                    ));
                },
            };
            let my = match &args[1] {
                Value::Num(n) => *n,
                _ => {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "graph.map: second argument 'math_y' must be a number".to_string(),
                    ));
                },
            };

            // Read size and at from runtime env for animation support.
            let size = env
                .get(&format!("{}.size", label))
                .and_then(|v| match v {
                    Value::Vec2(s) => Some(s),
                    _ => None,
                })
                .unwrap_or([500.0, 500.0]);
            let at = env
                .get(&format!("{}.at", label))
                .and_then(|v| match v {
                    Value::Vec2(a) => Some(a),
                    _ => None,
                })
                .unwrap_or([0.0, 0.0]);

            let scale = super::utils::GraphScaleConfig::new(x_domain, y_domain, x_scale, y_scale);
            let geo = super::utils::GraphGeometry::new(size, at, padding);
            let [sx, sy] = super::utils::graph_math_to_screen(mx, my, &scale, &geo, false);
            Ok(Value::Vec2([sx, sy]))
        },
    ))
}
