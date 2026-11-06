//! Procedural plot data model and per-frame sampling.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::adaptive::{
    CurvePathBuilder, PlotScreenTransform, PointSink, base_subdivision_depth,
    math_to_screen_padded, sample_recursive_cartesian, sample_recursive_cartesian_impl,
    sample_recursive_parametric, sample_recursive_parametric_impl, sample_recursive_polar,
    sample_recursive_polar_impl,
};
use super::{
    CapturedEnv, Environment, FuncBlendMode, FuncSource, FuncTransition, PlotCurveKind,
    PlotFuncRef, blend_depth, eval_scalar, eval_vec2,
};
use crate::renderer::types::VelloPath;
use crate::timeline::Value;
use crate::timeline::modifier_runtime::ir::{
    CompiledExpr, FastScalarExpr, compile_fast_scalar, compile_fast_vec2, resolve_slot_values,
};

/// Pre-compiled fast plot expression kind.
#[derive(Clone, Debug)]
pub enum FastExprKind {
    Scalar(FastScalarExpr),
    Vec2(FastScalarExpr, FastScalarExpr),
}

/// A cached fast plot program, holding the fast expression tree and the slot names
/// for environment variable lookups.
#[derive(Clone, Debug)]
pub struct FastPlotProgram {
    pub expr: FastExprKind,
    pub slot_names: Vec<String>,
}

pub fn compile_fast_plot_program(
    body: &CompiledExpr,
    leaf_arg: &str,
    kind: PlotCurveKind,
) -> Option<FastPlotProgram> {
    let mut slot_names_raw = Vec::with_capacity(8);
    let expr = if kind == PlotCurveKind::Parametric {
        compile_fast_vec2(body, leaf_arg, &mut slot_names_raw)
            .map(|(x, y)| FastExprKind::Vec2(x, y))?
    } else {
        compile_fast_scalar(body, leaf_arg, &mut slot_names_raw).map(FastExprKind::Scalar)?
    };
    Some(FastPlotProgram {
        expr,
        slot_names: slot_names_raw.into_iter().map(str::to_string).collect(),
    })
}

/// Discriminant for the procedural plot generator represented by
/// [`ProceduralPlot`]. Existing `PlotCurve` kinds remain on the curve variant
/// so sampling can reuse the adaptive curve code unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ProceduralPlotKind {
    /// `PlotCurve` with its existing sampling strategy.
    Curve(PlotCurveKind),
    /// Grid-sampled vector field.
    VectorField,
    /// Scalar field rendered as colored cells.
    Heatmap,
    /// Multiple level-set curves for a scalar field.
    ContourSet,
}

impl Default for ProceduralPlotKind {
    fn default() -> Self {
        Self::Curve(PlotCurveKind::Cartesian)
    }
}

/// All parameters needed to re-sample a plot curve at frame time.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProceduralPlot {
    /// Which procedural renderer this plot uses.
    #[cfg_attr(feature = "serde", serde(default))]
    pub plot_type: ProceduralPlotKind,
    pub kind: PlotCurveKind,
    pub func_args: Vec<String>,
    pub func_body: CompiledExpr,
    /// Label of the actor that owns this procedural plot.
    pub actor_label: String,
    /// Declared parameter names (e.g. ["freq", "amp"]) for actor-local injection.
    pub param_names: Vec<String>,
    pub p_x_domain: [f64; 2],
    pub p_y_domain: [f64; 2],
    pub p_size: [f64; 2],
    /// Parent graph padding `[left, right, top, bottom]` in pixels.
    pub padding: [f64; 4],
    pub t_domain: [f64; 2],
    pub tolerance: f64,
    pub max_depth: usize,
    pub resolution: usize,
    #[cfg_attr(feature = "serde", serde(default))]
    pub density: usize,
    #[cfg_attr(feature = "serde", serde(default))]
    pub levels: Vec<f64>,
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    /// Fill/heat color for plots that render filled cells.
    #[cfg_attr(feature = "serde", serde(default))]
    pub fill_color: [f32; 4],
    /// Custom numeric parameters that can be referenced by the func closure.
    /// Populated from declaration props like `freq: 2`, `amplitude: 1.5`.
    pub params: Vec<(String, f64)>,
    /// Captured environment variables (e.g., loop variables) needed at render time.
    /// Populated during build when the func closure is created inside a for loop.
    pub extra_captures: CapturedEnv,
}

impl ProceduralPlot {
    /// Returns `true` if the function references the timeline variable `t`, or
    /// if the plot has custom parameters that require per-frame environment
    /// injection, or if the closure captures a variable that an `always` block
    /// writes (the spec §14 "runtime parameters" pattern — the frame-env value
    /// shadows the build-time capture only when per-frame sampling actually
    /// runs). Used to decide whether to resample on every frame.
    ///
    /// `frame_written_vars` is the set of bare names any `always` block in the
    /// scene assigns (`freq = ...`); computed once per build from
    /// `Timeline::modifiers`.
    pub fn is_dynamic(&self, frame_written_vars: &std::collections::HashSet<String>) -> bool {
        self.func_body.references_ident("t")
            || !self.param_names.is_empty()
            || self.extra_captures.0.keys().any(|name| frame_written_vars.contains(name))
    }
}

/// Re-sample a procedural plot at frame time, blending between function sources
/// driven by `transitions` at `time_ms`.
///
/// - If a transition is active at `time_ms`, evaluates both endpoints and combines them by the
///   transition's [`FuncBlendMode`].
/// - If all transitions are complete, uses the last completed transition's `to`.
/// - Otherwise uses the declaration function from `plot.func_body`.
#[allow(dead_code)] // Public plot sampling API without cache, exercised by tests
pub fn sample_procedural_plot_at(
    plot: &ProceduralPlot,
    env: &Environment,
    time_ms: u64,
    transitions: &[FuncTransition],
) -> Vec<VelloPath> {
    sample_procedural_plot_at_cached(plot, env, time_ms, transitions, None)
}

/// Re-sample a procedural plot at frame time, reusing compiled fast expressions from
/// `cache` across frames when available.
pub fn sample_procedural_plot_at_cached(
    plot: &ProceduralPlot,
    env: &Environment,
    time_ms: u64,
    transitions: &[FuncTransition],
    cache: Option<&crate::timeline::EvalCaches>,
) -> Vec<VelloPath> {
    // Inject custom plot parameters as fallback defaults: only set the build-time
    // static value when the frame environment does not already carry an override
    // for the same name (e.g. from `always { freq = ... }` or a keyframe `let`).
    let mut param_scope = Vec::new();
    for (name, val) in &plot.params {
        if env.get(name).is_none() {
            param_scope.push((name.clone(), crate::timeline::Value::Num(*val)));
        }
    }
    let has_param_scope = !param_scope.is_empty();
    if has_param_scope {
        env.push_let_scope(param_scope);
    }
    struct LetScopeGuard<'a>(&'a Environment, bool);
    impl<'a> Drop for LetScopeGuard<'a> {
        fn drop(&mut self) {
            if self.1 {
                self.0.pop_let_scope();
            }
        }
    }
    let _guard = LetScopeGuard(env, has_param_scope);

    let active_transition = transitions.iter().find(|t| t.active_at(time_ms).is_some());
    let active = active_transition.and_then(|t| t.active_at(time_ms));

    if let Some((progress, from, to, _)) = active {
        let transition = active_transition.expect("active transition must exist");
        if transition.blend_mode == FuncBlendMode::Opacity {
            let quality_factor = opacity_quality_factor(from, to);
            let (actual_max_depth, actual_tolerance, actual_resolution) =
                scaled_plot_quality(plot, quality_factor);
            let from_paths = sample_plot_source_cached(
                plot,
                env,
                PlotFuncRef::Single(from),
                actual_max_depth,
                actual_tolerance,
                actual_resolution,
                cache,
            );
            let to_paths = sample_plot_source_cached(
                plot,
                env,
                PlotFuncRef::Single(to),
                actual_max_depth,
                actual_tolerance,
                actual_resolution,
                cache,
            );
            return crate::timeline::morph::interpolate_vello_paths(
                &from_paths,
                &to_paths,
                progress as f32,
                crate::timeline::morph::MorphOptions {
                    strategy: crate::timeline::morph::MorphStrategy::Fade,
                    ..Default::default()
                },
            );
        }
    }

    // Resolve the active function reference for this frame without cloning.
    let func_ref: PlotFuncRef<'_> = if let Some((progress, from, to, _)) = active {
        PlotFuncRef::Blended { from, to, progress }
    } else {
        // Use the last completed transition's target, or the declaration func.
        let last_complete = transitions.iter().rev().find(|t| t.is_complete_at(time_ms));
        match last_complete {
            Some(t) => PlotFuncRef::Single(&t.to),
            None => {
                PlotFuncRef::Declaration(&plot.func_args, &plot.func_body, &plot.extra_captures)
            },
        }
    };

    // Compute quality factor for adaptive sampling during function transitions.
    // Reduces sampling quality exponentially with nested blend depth to maintain
    // frame rate during cascading transitions.
    let quality_factor = if let PlotFuncRef::Blended { from, to, .. } = &func_ref {
        opacity_quality_factor(from, to)
    } else {
        1.0
    };
    let (actual_max_depth, actual_tolerance, actual_resolution) =
        scaled_plot_quality(plot, quality_factor);

    sample_plot_source_cached(
        plot,
        env,
        func_ref,
        actual_max_depth,
        actual_tolerance,
        actual_resolution,
        cache,
    )
}

fn opacity_quality_factor(from: &FuncSource, to: &FuncSource) -> f64 {
    let depth = blend_depth(from).max(blend_depth(to)) + 1;
    // depth 1: 1.0, depth 2: 0.75, depth 3: 0.5, etc.
    0.75_f64.powi(depth as i32 - 1)
}

fn scaled_plot_quality(plot: &ProceduralPlot, quality_factor: f64) -> (usize, f64, usize) {
    (
        (plot.max_depth as f64 * quality_factor).max(2.0) as usize,
        plot.tolerance / quality_factor,
        (plot.resolution as f64 * quality_factor).max(8.0) as usize,
    )
}

/// Re-sample one concrete `FuncSource` with optional cache for compiled fast programs.
pub fn sample_plot_source_cached(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: PlotFuncRef<'_>,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
    cache: Option<&crate::timeline::EvalCaches>,
) -> Vec<VelloPath> {
    match plot.plot_type {
        ProceduralPlotKind::Curve(_) => sample_curve_plot_source(
            plot,
            env,
            func_ref,
            actual_max_depth,
            actual_tolerance,
            actual_resolution,
            cache,
        ),
        ProceduralPlotKind::VectorField => {
            let full_size = [plot.p_size[0] * 2.0, plot.p_size[1] * 2.0];
            let mut local_env = env.clone();
            let source = func_ref.to_func_source();
            super::super::build::plot::build_vector_field_paths(
                &mut local_env,
                &source,
                plot.p_x_domain,
                plot.p_y_domain,
                full_size,
                plot.density.max(4),
                plot.stroke_color,
                plot.stroke_width,
            )
        },
        ProceduralPlotKind::Heatmap => {
            let full_size = [plot.p_size[0] * 2.0, plot.p_size[1] * 2.0];
            let mut local_env = env.clone();
            let source = func_ref.to_func_source();
            super::super::build::plot::build_heatmap_paths(
                &mut local_env,
                &source,
                plot.p_x_domain,
                plot.p_y_domain,
                full_size,
                actual_resolution.max(4),
                plot.fill_color,
            )
        },
        ProceduralPlotKind::ContourSet => {
            let full_size = [plot.p_size[0] * 2.0, plot.p_size[1] * 2.0];
            let mut local_env = env.clone();
            let source = func_ref.to_func_source();
            super::super::build::plot::build_contour_set_paths(
                &mut local_env,
                &source,
                &plot.levels,
                plot.p_x_domain,
                plot.p_y_domain,
                full_size,
                actual_resolution.max(8),
                plot.stroke_color,
                plot.stroke_width,
            )
        },
    }
}

fn setup_curve_domain(plot: &ProceduralPlot) -> (String, f64, f64) {
    let arg_name = if !plot.func_args.is_empty() {
        plot.func_args[0].clone()
    } else {
        "x".to_string()
    };

    let (min_t, max_t) = if plot.kind == PlotCurveKind::Cartesian {
        (plot.p_x_domain[0], plot.p_x_domain[1])
    } else if plot.kind == PlotCurveKind::Implicit {
        (0.0, 0.0)
    } else {
        (plot.t_domain[0], plot.t_domain[1])
    };

    (arg_name, min_t, max_t)
}

fn make_curve_vello_path(
    path: std::sync::Arc<kurbo::BezPath>,
    stroke_width: f32,
    stroke_color: [f32; 4],
) -> VelloPath {
    VelloPath {
        path,
        fill: None,
        stroke: if stroke_width > 0.0 {
            Some((
                vello::peniko::Color::from_rgba8(
                    (stroke_color[0] * 255.0) as u8,
                    (stroke_color[1] * 255.0) as u8,
                    (stroke_color[2] * 255.0) as u8,
                    (stroke_color[3] * 255.0) as u8,
                ),
                stroke_width,
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
    }
}

fn sample_implicit_curve(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: &PlotFuncRef<'_>,
    actual_resolution: usize,
    cache: Option<&crate::timeline::EvalCaches>,
) -> Vec<VelloPath> {
    let mut local_env = env.clone();
    let source = func_ref.to_func_source();
    let path = super::implicit::build_implicit_plot_path_from_source_cached(
        &mut local_env,
        &source,
        &plot.p_x_domain,
        &plot.p_y_domain,
        &plot.p_size,
        actual_resolution,
        &plot.padding,
        cache,
    );
    vec![make_curve_vello_path(
        std::sync::Arc::new(path),
        plot.stroke_width,
        plot.stroke_color,
    )]
}

pub(crate) struct FuncLeaf<'a> {
    pub(crate) weight: f64,
    pub(crate) args: &'a [String],
    pub(crate) body: &'a CompiledExpr,
    pub(crate) captures: &'a CapturedEnv,
}

fn collect_src_leaves<'a>(weight: f64, src: &'a FuncSource, out: &mut Vec<FuncLeaf<'a>>) {
    match src {
        FuncSource::Compiled(args, body, captures) => {
            out.push(FuncLeaf {
                weight,
                args,
                body,
                captures,
            });
        },
        FuncSource::Blend {
            from,
            to,
            frozen_progress,
        } => {
            collect_src_leaves(weight * (1.0 - frozen_progress), from, out);
            collect_src_leaves(weight * frozen_progress, to, out);
        },
    }
}

fn collect_func_leaves<'a>(func_ref: &PlotFuncRef<'a>) -> Vec<FuncLeaf<'a>> {
    let mut func_leaves = Vec::with_capacity(2);
    match func_ref {
        PlotFuncRef::Declaration(args, body, captures) => {
            func_leaves.push(FuncLeaf {
                weight: 1.0,
                args,
                body,
                captures,
            });
        },
        PlotFuncRef::Single(src) => {
            collect_src_leaves(1.0, src, &mut func_leaves);
        },
        PlotFuncRef::Blended { from, to, progress } => {
            collect_src_leaves(1.0 - progress, from, &mut func_leaves);
            collect_src_leaves(*progress, to, &mut func_leaves);
        },
    }
    func_leaves
}

struct FastLeaf {
    weight: f64,
    expr: FastExprKind,
    slots: Vec<f64>,
}

fn resolve_fast_leaves(
    func_leaves: &[FuncLeaf<'_>],
    plot_kind: PlotCurveKind,
    arg_name: &str,
    env: &Environment,
    cache: Option<&crate::timeline::EvalCaches>,
) -> Option<Vec<FastLeaf>> {
    if func_leaves.is_empty() {
        return None;
    }
    let kind_id = if plot_kind == PlotCurveKind::Parametric {
        1
    } else {
        0
    };
    let mut fast_leaves = Vec::with_capacity(func_leaves.len());

    for leaf in func_leaves {
        let leaf_arg = leaf.args.first().map(String::as_str).unwrap_or(arg_name);
        let body_ptr = leaf.body as *const CompiledExpr as usize;
        let key = (body_ptr, kind_id);

        let program = if let Some(cache) = cache {
            cache.get_or_compile_fast_plot(key, || {
                compile_fast_plot_program(leaf.body, leaf_arg, plot_kind)
            })?
        } else {
            std::sync::Arc::new(compile_fast_plot_program(leaf.body, leaf_arg, plot_kind)?)
        };

        let slot_names_refs: Vec<&str> = program.slot_names.iter().map(String::as_str).collect();
        let mut slots = Vec::with_capacity(program.slot_names.len());
        if !resolve_slot_values(&slot_names_refs, env, leaf.captures, &mut slots) {
            return None;
        }

        let folded_expr = match program.expr {
            FastExprKind::Scalar(ref s) => FastExprKind::Scalar(s.fold_slots(&slots)),
            FastExprKind::Vec2(ref x, ref y) => {
                FastExprKind::Vec2(x.fold_slots(&slots), y.fold_slots(&slots))
            },
        };

        fast_leaves.push(FastLeaf {
            weight: leaf.weight,
            expr: folded_expr,
            slots,
        });
    }

    Some(fast_leaves)
}

fn eval_fast_leaves_endpoint(kind: PlotCurveKind, fast_leaves: &[FastLeaf], t: f64) -> (f64, f64) {
    match kind {
        PlotCurveKind::Cartesian => {
            let mut y = 0.0;
            for leaf in fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    y += leaf.weight * expr.eval(t, &leaf.slots);
                }
            }
            (t, y)
        },
        PlotCurveKind::Parametric => {
            let mut x_sum = 0.0;
            let mut y_sum = 0.0;
            for leaf in fast_leaves {
                if let FastExprKind::Vec2(ref x_expr, ref y_expr) = leaf.expr {
                    x_sum += leaf.weight * x_expr.eval(t, &leaf.slots);
                    y_sum += leaf.weight * y_expr.eval(t, &leaf.slots);
                }
            }
            (x_sum, y_sum)
        },
        _ => {
            let mut r = 0.0;
            for leaf in fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    r += leaf.weight * expr.eval(t, &leaf.slots);
                }
            }
            (r * t.cos(), r * t.sin())
        },
    }
}

fn sample_curve_fast_path(
    plot: &ProceduralPlot,
    fast_leaves: &[FastLeaf],
    min_t: f64,
    max_t: f64,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
) -> Vec<VelloPath> {
    let (start_math_x, start_math_y) = eval_fast_leaves_endpoint(plot.kind, fast_leaves, min_t);
    let (end_math_x, end_math_y) = eval_fast_leaves_endpoint(plot.kind, fast_leaves, max_t);

    let transform =
        PlotScreenTransform::new(&plot.p_x_domain, &plot.p_y_domain, &plot.p_size, &plot.padding);

    let (start_screen_x, start_screen_y) = transform.map(start_math_x, start_math_y);
    let (end_screen_x, end_screen_y) = transform.map(end_math_x, end_math_y);

    let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
    let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

    let mut builder = CurvePathBuilder::new();
    builder.push(p0);

    let subdivision_depth = base_subdivision_depth(actual_resolution, actual_max_depth);

    match plot.kind {
        PlotCurveKind::Cartesian => {
            let mut eval = |t: f64| {
                let mut y = 0.0;
                for leaf in fast_leaves {
                    if let FastExprKind::Scalar(ref expr) = leaf.expr {
                        y += leaf.weight * expr.eval(t, &leaf.slots);
                    }
                }
                y
            };
            sample_recursive_cartesian_impl(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        },
        PlotCurveKind::Polar => {
            let mut eval = |t: f64| {
                let mut r = 0.0;
                for leaf in fast_leaves {
                    if let FastExprKind::Scalar(ref expr) = leaf.expr {
                        r += leaf.weight * expr.eval(t, &leaf.slots);
                    }
                }
                r
            };
            sample_recursive_polar_impl(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        },
        PlotCurveKind::Parametric => {
            let mut eval = |t: f64| {
                let mut x_sum = 0.0;
                let mut y_sum = 0.0;
                for leaf in fast_leaves {
                    if let FastExprKind::Vec2(ref x_expr, ref y_expr) = leaf.expr {
                        x_sum += leaf.weight * x_expr.eval(t, &leaf.slots);
                        y_sum += leaf.weight * y_expr.eval(t, &leaf.slots);
                    }
                }
                [x_sum, y_sum]
            };
            sample_recursive_parametric_impl(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        },
        PlotCurveKind::Implicit => unreachable!(),
    }

    vec![make_curve_vello_path(
        std::sync::Arc::new(builder.path),
        plot.stroke_width,
        plot.stroke_color,
    )]
}

fn eval_slow_endpoint(
    kind: PlotCurveKind,
    func_ref: &PlotFuncRef<'_>,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
) -> (f64, f64) {
    match kind {
        PlotCurveKind::Cartesian => {
            let y = eval_scalar(func_ref, env, arg_name, t, from_cache, to_cache);
            (t, y)
        },
        PlotCurveKind::Parametric => {
            let [x, y] = eval_vec2(func_ref, env, arg_name, t, from_cache, to_cache);
            (x, y)
        },
        _ => {
            let r = eval_scalar(func_ref, env, arg_name, t, from_cache, to_cache);
            (r * t.cos(), r * t.sin())
        },
    }
}

fn sample_curve_slow_path(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: &PlotFuncRef<'_>,
    arg_name: &str,
    min_t: f64,
    max_t: f64,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
) -> Vec<VelloPath> {
    let mut local_env = env.clone();
    let mut from_cache = HashMap::<u64, Value>::with_capacity(256);
    let mut to_cache = HashMap::<u64, Value>::with_capacity(256);

    let (start_math_x, start_math_y) = eval_slow_endpoint(
        plot.kind,
        func_ref,
        &mut local_env,
        arg_name,
        min_t,
        &mut from_cache,
        &mut to_cache,
    );
    let (end_math_x, end_math_y) = eval_slow_endpoint(
        plot.kind,
        func_ref,
        &mut local_env,
        arg_name,
        max_t,
        &mut from_cache,
        &mut to_cache,
    );

    let (start_screen_x, start_screen_y) = math_to_screen_padded(
        start_math_x,
        start_math_y,
        &plot.p_x_domain,
        &plot.p_y_domain,
        &plot.p_size,
        &plot.padding,
    );
    let (end_screen_x, end_screen_y) = math_to_screen_padded(
        end_math_x,
        end_math_y,
        &plot.p_x_domain,
        &plot.p_y_domain,
        &plot.p_size,
        &plot.padding,
    );

    let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
    let p1 = kurbo::Point::new(end_screen_x, end_screen_y);
    let mut pts = vec![p0];
    let subdivision_depth = base_subdivision_depth(actual_resolution, actual_max_depth);

    match plot.kind {
        PlotCurveKind::Cartesian => {
            sample_recursive_cartesian(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &mut local_env,
                arg_name,
                func_ref,
                &plot.p_x_domain,
                &plot.p_y_domain,
                &plot.p_size,
                &plot.padding,
                &mut from_cache,
                &mut to_cache,
                &mut pts,
            );
        },
        PlotCurveKind::Polar => {
            sample_recursive_polar(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &mut local_env,
                arg_name,
                func_ref,
                &plot.p_x_domain,
                &plot.p_y_domain,
                &plot.p_size,
                &plot.padding,
                &mut from_cache,
                &mut to_cache,
                &mut pts,
            );
        },
        PlotCurveKind::Parametric => {
            sample_recursive_parametric(
                min_t,
                max_t,
                p0,
                p1,
                0,
                actual_max_depth,
                subdivision_depth,
                actual_tolerance,
                &mut local_env,
                arg_name,
                func_ref,
                &plot.p_x_domain,
                &plot.p_y_domain,
                &plot.p_size,
                &plot.padding,
                &mut from_cache,
                &mut to_cache,
                &mut pts,
            );
        },
        PlotCurveKind::Implicit => unreachable!(),
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
    vec![make_curve_vello_path(
        std::sync::Arc::new(path),
        plot.stroke_width,
        plot.stroke_color,
    )]
}

fn sample_curve_plot_source(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: PlotFuncRef<'_>,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
    cache: Option<&crate::timeline::EvalCaches>,
) -> Vec<VelloPath> {
    if plot.kind == PlotCurveKind::Implicit {
        return sample_implicit_curve(plot, env, &func_ref, actual_resolution, cache);
    }

    let (arg_name, min_t, max_t) = setup_curve_domain(plot);
    let func_leaves = collect_func_leaves(&func_ref);

    if let Some(fast_leaves) = resolve_fast_leaves(&func_leaves, plot.kind, &arg_name, env, cache) {
        return sample_curve_fast_path(
            plot,
            &fast_leaves,
            min_t,
            max_t,
            actual_max_depth,
            actual_tolerance,
            actual_resolution,
        );
    }

    sample_curve_slow_path(
        plot,
        env,
        &func_ref,
        &arg_name,
        min_t,
        max_t,
        actual_max_depth,
        actual_tolerance,
        actual_resolution,
    )
}
