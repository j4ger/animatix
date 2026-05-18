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

/// Parameters for building plot curve paths.
pub(super) struct PlotCurveParams<'a> {
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
pub(super) fn build_plot_curve_paths(params: &PlotCurveParams<'_>) -> Vec<VelloPath> {
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

/// Build graph axis VelloPaths (X and Y axes).
/// Omits an axis entirely when zero is not in its domain.
pub(super) fn build_graph_axis_paths(
    size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    axis_color: [f32; 4],
) -> Vec<VelloPath> {
    let mut path = kurbo::BezPath::new();

    // X-axis: drawn only when y=0 is inside the y_domain
    if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        let x_axis_y = size[1] as f64 * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]));
        path.move_to((-(size[0] as f64), x_axis_y));
        path.line_to((size[0] as f64, x_axis_y));
    }

    // Y-axis: drawn only when x=0 is inside the x_domain
    if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        let y_axis_x = size[0] as f64 * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]));
        path.move_to((y_axis_x, -(size[1] as f64)));
        path.line_to((y_axis_x, size[1] as f64));
    }

    if path.elements().is_empty() {
        return Vec::new();
    }

    vec![VelloPath {
        path,
        fill: None,
        stroke: Some((vello::peniko::Color::from_rgba8(
            (axis_color[0] * 255.0) as u8,
            (axis_color[1] * 255.0) as u8,
            (axis_color[2] * 255.0) as u8,
            (axis_color[3] * 255.0) as u8,
        ), 2.0)),
    }]
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
                _ => {}
            }
        }

        // Validate plot func signature if present.
        if let Some((ref args, ref body)) = func {
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
                let ok = match (expected_ty, &result) {
                    ("number", Value::Num(_)) => true,
                    ("vec2", Value::Vec2(_)) => true,
                    _ => false,
                };
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
        }

        if primitive.is_graph_host() {
            self.env
                .set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
            self.env
                .set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
            self.env.set(
                &format!("{}_size", label),
                Value::Vec2([
                    initial_size[0] as f64 * 2.0,
                    initial_size[1] as f64 * 2.0,
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

        if primitive.is_graph_host() {
            vello_paths = build_graph_axis_paths(size, x_domain, y_domain, stroke_color);
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
