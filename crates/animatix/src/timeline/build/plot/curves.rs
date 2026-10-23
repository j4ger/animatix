//! Plot curve sampling and the curve plot-actor builder.

use std::collections::HashMap;

use super::PlotActorProps;
use super::*;
use crate::timeline::modifier_runtime::ir::evaluate_compiled_expr;
use crate::timeline::plot::{
    FuncSource, PlotCurveKind, PlotFuncRef, ProceduralPlot, build_implicit_plot_path_from_source,
};
use crate::timeline::vello_path::VelloPath;

/// Parameters for building plot curve paths.
pub(crate) struct PlotCurveParams<'a> {
    pub(crate) kind: PlotCurveKind,
    /// Actor label, used in evaluation-failure diagnostics.
    pub(crate) label: &'a str,
    /// Declared plot parameter names (`freq: 2`). These are injected at frame
    /// time from `plot_param_tracks`, so their absence from the build-time
    /// probe environment is the designed handoff to per-frame resampling, not
    /// an evaluation error.
    pub(crate) param_names: &'a [String],
    pub(crate) func: &'a Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    pub(crate) p_x_domain: [f64; 2],
    pub(crate) p_y_domain: [f64; 2],
    pub(crate) p_size: [f64; 2],
    /// Parent graph padding [left, right, top, bottom] in pixels.
    pub(crate) p_padding: [f64; 4],
    pub(crate) t_domain: [f64; 2],
    pub(crate) tolerance: f64,
    pub(crate) max_depth: f64,
    pub(crate) resolution: f64,
    pub(crate) stroke_width: f32,
    pub(crate) stroke_color: [f32; 4],
    pub(crate) eval_env: &'a Environment,
    pub(crate) build_quality: crate::timeline::BuildQuality,
}

/// Build plot curve VelloPaths from the given parameters.
/// This is the shared implementation used by both `process_plot_actor` and the
/// `process_body` ActorDecl fallback path.
pub(crate) fn build_plot_curve_paths(
    params: &PlotCurveParams<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<VelloPath> {
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
                path: std::sync::Arc::new(path),
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
                dash_pattern: None,
                dash_offset: 0.0,
                fill_gradient: None,
                stroke_gradient: None,
            });
        } else {
            env_copy.set_binding(&arg_name, Value::Num(min_t));
            let start_eval = match evaluate_compiled_expr(body, &env_copy) {
                Ok(v) => v,
                // `t`, the plot argument, and declared plot params are
                // frame-time names; their absence at build time is the designed
                // handoff to per-frame resampling, not an error.
                Err(EvalError::UndefinedVariable(name))
                    if name == "t"
                        || name == arg_name
                        || params.param_names.iter().any(|param| param == &name) =>
                {
                    Value::Num(f64::NAN)
                },
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "plot '{}': func evaluation failed at x={min_t}: {e}",
                                params.label
                            ),
                        )
                        .with_subject(params.label),
                    );
                    Value::Num(f64::NAN)
                },
            };
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
            let end_eval = match evaluate_compiled_expr(body, &env_copy) {
                Ok(v) => v,
                Err(EvalError::UndefinedVariable(name))
                    if name == "t"
                        || name == arg_name
                        || params.param_names.iter().any(|param| param == &name) =>
                {
                    Value::Num(f64::NAN)
                },
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "plot '{}': func evaluation failed at x={max_t}: {e}",
                                params.label
                            ),
                        )
                        .with_subject(params.label),
                    );
                    Value::Num(f64::NAN)
                },
            };
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
                    crate::timeline::plot::base_subdivision_depth(resolution, max_depth),
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
                    crate::timeline::plot::base_subdivision_depth(resolution, max_depth),
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
                    crate::timeline::plot::base_subdivision_depth(resolution, max_depth),
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
                path: std::sync::Arc::new(path),
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
                dash_pattern: None,
                dash_offset: 0.0,
                fill_gradient: None,
                stroke_gradient: None,
            });
        }
    }

    vello_paths
}

/// Validate the plot func's arity and return type for this actor kind.
pub(super) fn validate_plot_func(
    primitive: &PrimitiveFamilyDescriptor,
    ty: &str,
    label: &str,
    func: &Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    kind: PlotCurveKind,
    initial_eval_env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use animatix_core::caps::PlotKind;
    let is_vector_field = primitive.plot == Some(PlotKind::VectorField);
    let is_heatmap = primitive.plot == Some(PlotKind::Heatmap);
    let is_contour_set = primitive.plot == Some(PlotKind::ContourSet);
    let is_bar_chart = primitive.plot == Some(PlotKind::BarChart);
    if let Some((args, body, _)) = func {
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
                                    Value::PropRef { label, prop } => {
                                        format!("property reference &{label}.{prop}")
                                    },
                                    Value::Num(_) => "number".to_string(),
                                    Value::Vec2(_) => "vec2".to_string(),
                                    Value::Vec3(_) => "vec3".to_string(),
                                    Value::Vec4(_) => "vec4".to_string(),
                                    Value::Color(_) => "color".to_string(),
                                    Value::Str(_) => "string".to_string(),
                                    Value::List(_) => "list".to_string(),
                                    Value::NativeFn(_) => "function".to_string(),
                                    Value::Closure(_, _, _) => "closure".to_string(),
                                    Value::UserFn { name, .. } => format!("user function {name}"),
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
}

impl Timeline {
    /// Build a plot curve's paths and its frame-time `ProceduralPlot`.
    pub(super) fn build_curve_plot_paths(
        &mut self,
        label: &str,
        parent_label: Option<&str>,
        parsed: &PlotActorProps,
        plot_params: &[(String, f64)],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> (Vec<VelloPath>, Option<ProceduralPlot>) {
        let p_label = parent_label.unwrap_or("").to_string();
        let mut p_x_domain = [-10.0, 10.0];
        let mut p_y_domain = [-10.0, 10.0];
        let mut p_size = [500.0, 500.0];
        let mut p_padding = [0.0; 4];

        // Resolve coordinate context from the parent host's frame component.
        if let Some(frame) = self.coordinate_frame_for(&p_label) {
            p_x_domain = frame.x_domain;
            p_y_domain = frame.y_domain;
            p_padding = frame.padding;
        }

        let size_key = crate::timeline::env_keys::side_channel(&p_label, "size");
        if let Some(Value::Vec2(sz)) = self.env.get(size_key.as_str()) {
            p_size = sz;
        }

        let eval_env = self.build_eval_env(time_ms as u64);

        // Phase 6.4: Check cache for static plot paths before rebuilding.
        let is_static = parsed.func.as_ref().is_none_or(|(_, body, _)| !body.references_ident("t"));
        let cache_key = if is_static {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            format!("{:?}", parsed.kind).hash(&mut hasher);
            format!("{:?}", parsed.func).hash(&mut hasher);
            p_x_domain.map(|v| v.to_bits()).hash(&mut hasher);
            p_y_domain.map(|v| v.to_bits()).hash(&mut hasher);
            p_size.map(|v| v.to_bits()).hash(&mut hasher);
            parsed.t_domain.map(|v| v.to_bits()).hash(&mut hasher);
            parsed.tolerance.to_bits().hash(&mut hasher);
            parsed.max_depth.to_bits().hash(&mut hasher);
            parsed.resolution.to_bits().hash(&mut hasher);
            parsed.stroke_width.to_bits().hash(&mut hasher);
            parsed.stroke_color.map(|v| v.to_bits()).hash(&mut hasher);
            self.build_quality.hash(&mut hasher);
            Some(hasher.finish())
        } else {
            None
        };

        let param_names: Vec<String> = plot_params.iter().map(|(n, _)| n.clone()).collect();
        let mut vello_paths = vec![];

        if let Some(key) = cache_key {
            if let Some(cached) = self.plot_path_cache.get(&key) {
                vello_paths = cached.clone();
            } else {
                let curve_params = PlotCurveParams {
                    kind: parsed.kind,
                    func: &parsed.func,
                    p_x_domain,
                    p_y_domain,
                    p_size,
                    p_padding,
                    t_domain: parsed.t_domain,
                    tolerance: parsed.tolerance,
                    max_depth: parsed.max_depth,
                    resolution: parsed.resolution,
                    stroke_width: parsed.stroke_width,
                    stroke_color: parsed.stroke_color,
                    eval_env: &eval_env,
                    build_quality: self.build_quality,
                    label,
                    param_names: &param_names,
                };
                vello_paths = build_plot_curve_paths(&curve_params, diagnostics);
                self.plot_path_cache.insert(key, vello_paths.clone());
            }
        } else {
            let curve_params = PlotCurveParams {
                kind: parsed.kind,
                func: &parsed.func,
                p_x_domain,
                p_y_domain,
                p_size,
                p_padding,
                t_domain: parsed.t_domain,
                tolerance: parsed.tolerance,
                max_depth: parsed.max_depth,
                resolution: parsed.resolution,
                stroke_width: parsed.stroke_width,
                stroke_color: parsed.stroke_color,
                eval_env: &eval_env,
                build_quality: self.build_quality,
                label,
                param_names: &param_names,
            };
            vello_paths = build_plot_curve_paths(&curve_params, diagnostics);
        }

        // Always create a procedural_plot for PlotCurve actors so that
        // func transitions can be added later via assignment. Static plots
        // (no `t`, no params, no transitions) are still guarded at frame
        // time by `is_dynamic()` in scene_eval.rs, so they keep using the
        // cached build-time paths with zero per-frame overhead.
        let mut procedural_plot = None;
        if let Some((args, body, captures)) = parsed.func.as_ref() {
            procedural_plot = Some(ProceduralPlot {
                plot_type: crate::timeline::plot::ProceduralPlotKind::Curve(parsed.kind),
                kind: parsed.kind,
                func_args: args.clone(),
                func_body: (**body).clone(),
                actor_label: label.to_string(),
                param_names,
                p_x_domain,
                p_y_domain,
                p_size,
                padding: p_padding,
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

        (vello_paths, procedural_plot)
    }
}
