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
use super::implicit::build_implicit_plot_path_from_source;
use super::{
    CapturedEnv, Environment, FuncBlendMode, FuncSource, FuncTransition, PlotCurveKind,
    PlotFuncRef, blend_depth, eval_scalar, eval_vec2,
};
use crate::renderer::types::VelloPath;
use crate::timeline::Value;
use crate::timeline::modifier_runtime::ir::{
    CompiledExpr, FastScalarExpr, compile_fast_scalar, compile_fast_vec2, resolve_slot_values,
};

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

/// Re-sample a procedural plot at frame time using the given environment.
/// Compatibility shim for `scene_eval.rs` (pre-Task-4). Delegates to
/// `sample_procedural_plot_at` with no active transitions.
#[deprecated(since = "0.5.0", note = "use sample_procedural_plot_at instead")]
#[allow(dead_code)] // Legacy public API shim; kept for backward compatibility.
pub fn sample_procedural_plot(plot: &ProceduralPlot, env: &mut Environment) -> Vec<VelloPath> {
    sample_procedural_plot_at(plot, env, 0, &[])
}

/// Re-sample a procedural plot at frame time, blending between function sources
/// driven by `transitions` at `time_ms`.
///
/// - If a transition is active at `time_ms`, evaluates both endpoints and combines them by the
///   transition's [`FuncBlendMode`].
/// - If all transitions are complete, uses the last completed transition's `to`.
/// - Otherwise uses the declaration function from `plot.func_body`.
pub fn sample_procedural_plot_at(
    plot: &ProceduralPlot,
    env: &Environment,
    time_ms: u64,
    transitions: &[FuncTransition],
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
            let from_paths = sample_plot_source(
                plot,
                env,
                PlotFuncRef::Single(from),
                actual_max_depth,
                actual_tolerance,
                actual_resolution,
            );
            let to_paths = sample_plot_source(
                plot,
                env,
                PlotFuncRef::Single(to),
                actual_max_depth,
                actual_tolerance,
                actual_resolution,
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

    sample_plot_source(plot, env, func_ref, actual_max_depth, actual_tolerance, actual_resolution)
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

/// Re-sample one concrete `FuncSource` with the procedural renderer described
/// by `plot`. Output-mode blending passes a [`FuncSource::Blend`] here;
/// opacity-mode blending calls this separately for each endpoint.
pub fn sample_plot_source(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: PlotFuncRef<'_>,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
) -> Vec<VelloPath> {
    match plot.plot_type {
        ProceduralPlotKind::Curve(_) => sample_curve_plot_source(
            plot,
            env,
            func_ref,
            actual_max_depth,
            actual_tolerance,
            actual_resolution,
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

fn sample_curve_plot_source(
    plot: &ProceduralPlot,
    env: &Environment,
    func_ref: PlotFuncRef<'_>,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
) -> Vec<VelloPath> {
    let mut vello_paths = vec![];
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

    if plot.kind == PlotCurveKind::Implicit {
        let mut local_env = env.clone();
        let source = func_ref.to_func_source();
        let path = build_implicit_plot_path_from_source(
            &mut local_env,
            &source,
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            actual_resolution,
            &plot.padding,
        );
        vello_paths.push(VelloPath {
            path: std::sync::Arc::new(path),
            fill: None,
            stroke: if plot.stroke_width > 0.0 {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (plot.stroke_color[0] * 255.0) as u8,
                        (plot.stroke_color[1] * 255.0) as u8,
                        (plot.stroke_color[2] * 255.0) as u8,
                        (plot.stroke_color[3] * 255.0) as u8,
                    ),
                    plot.stroke_width,
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
        return vello_paths;
    }

    // Fast path: try unboxed direct math evaluation without Environment mutation
    struct FuncLeaf<'a> {
        weight: f64,
        args: &'a [String],
        body: &'a CompiledExpr,
        captures: &'a CapturedEnv,
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

    let mut func_leaves = Vec::with_capacity(2);
    match &func_ref {
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

    enum FastExprKind {
        Scalar(FastScalarExpr),
        Vec2(FastScalarExpr, FastScalarExpr),
    }

    struct FastLeaf {
        weight: f64,
        expr: FastExprKind,
        slots: Vec<f64>,
    }

    let mut can_fast = !func_leaves.is_empty();
    let mut fast_leaves = Vec::with_capacity(func_leaves.len());
    for leaf in &func_leaves {
        let leaf_arg = leaf.args.first().map(String::as_str).unwrap_or(&arg_name);
        let mut slot_names = Vec::with_capacity(8);
        let expr = if plot.kind == PlotCurveKind::Parametric {
            compile_fast_vec2(leaf.body, leaf_arg, &mut slot_names)
                .map(|(x, y)| FastExprKind::Vec2(x, y))
        } else {
            compile_fast_scalar(leaf.body, leaf_arg, &mut slot_names).map(FastExprKind::Scalar)
        };
        if let Some(expr) = expr {
            let mut slots = Vec::with_capacity(slot_names.len());
            if resolve_slot_values(&slot_names, env, leaf.captures, &mut slots) {
                fast_leaves.push(FastLeaf {
                    weight: leaf.weight,
                    expr,
                    slots,
                });
            } else {
                can_fast = false;
                break;
            }
        } else {
            can_fast = false;
            break;
        }
    }

    if can_fast {
        let (start_math_x, start_math_y) = if plot.kind == PlotCurveKind::Cartesian {
            let mut y = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    y += leaf.weight * expr.eval(min_t, &leaf.slots);
                }
            }
            (min_t, y)
        } else if plot.kind == PlotCurveKind::Parametric {
            let mut x_sum = 0.0;
            let mut y_sum = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Vec2(ref x_expr, ref y_expr) = leaf.expr {
                    x_sum += leaf.weight * x_expr.eval(min_t, &leaf.slots);
                    y_sum += leaf.weight * y_expr.eval(min_t, &leaf.slots);
                }
            }
            (x_sum, y_sum)
        } else {
            let mut r = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    r += leaf.weight * expr.eval(min_t, &leaf.slots);
                }
            }
            (r * min_t.cos(), r * min_t.sin())
        };

        let (end_math_x, end_math_y) = if plot.kind == PlotCurveKind::Cartesian {
            let mut y = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    y += leaf.weight * expr.eval(max_t, &leaf.slots);
                }
            }
            (max_t, y)
        } else if plot.kind == PlotCurveKind::Parametric {
            let mut x_sum = 0.0;
            let mut y_sum = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Vec2(ref x_expr, ref y_expr) = leaf.expr {
                    x_sum += leaf.weight * x_expr.eval(max_t, &leaf.slots);
                    y_sum += leaf.weight * y_expr.eval(max_t, &leaf.slots);
                }
            }
            (x_sum, y_sum)
        } else {
            let mut r = 0.0;
            for leaf in &fast_leaves {
                if let FastExprKind::Scalar(ref expr) = leaf.expr {
                    r += leaf.weight * expr.eval(max_t, &leaf.slots);
                }
            }
            (r * max_t.cos(), r * max_t.sin())
        };

        let transform = PlotScreenTransform::new(
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            &plot.padding,
        );

        let (start_screen_x, start_screen_y) = transform.map(start_math_x, start_math_y);
        let (end_screen_x, end_screen_y) = transform.map(end_math_x, end_math_y);

        let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
        let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

        let mut builder = CurvePathBuilder::new();
        builder.push(p0);

        if plot.kind == PlotCurveKind::Cartesian {
            let mut eval = |t: f64| {
                let mut y = 0.0;
                for leaf in &fast_leaves {
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
                base_subdivision_depth(actual_resolution, actual_max_depth),
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        } else if plot.kind == PlotCurveKind::Polar {
            let mut eval = |t: f64| {
                let mut r = 0.0;
                for leaf in &fast_leaves {
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
                base_subdivision_depth(actual_resolution, actual_max_depth),
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        } else {
            let mut eval = |t: f64| {
                let mut x_sum = 0.0;
                let mut y_sum = 0.0;
                for leaf in &fast_leaves {
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
                base_subdivision_depth(actual_resolution, actual_max_depth),
                actual_tolerance,
                &transform,
                &mut builder,
                &mut eval,
            );
        }

        vello_paths.push(VelloPath {
            path: std::sync::Arc::new(builder.path),
            fill: None,
            stroke: if plot.stroke_width > 0.0 {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (plot.stroke_color[0] * 255.0) as u8,
                        (plot.stroke_color[1] * 255.0) as u8,
                        (plot.stroke_color[2] * 255.0) as u8,
                        (plot.stroke_color[3] * 255.0) as u8,
                    ),
                    plot.stroke_width,
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

        return vello_paths;
    }

    // Slow-path fallback: evaluate closures via environment mutation with localized clone
    let mut local_env = env.clone();
    let mut from_cache = HashMap::<u64, Value>::with_capacity(256);
    let mut to_cache = HashMap::<u64, Value>::with_capacity(256);

    let (start_math_x, start_math_y) = if plot.kind == PlotCurveKind::Cartesian {
        let y = eval_scalar(
            &func_ref,
            &mut local_env,
            &arg_name,
            min_t,
            &mut from_cache,
            &mut to_cache,
        );
        (min_t, y)
    } else if plot.kind == PlotCurveKind::Parametric {
        let [x, y] =
            eval_vec2(&func_ref, &mut local_env, &arg_name, min_t, &mut from_cache, &mut to_cache);
        (x, y)
    } else {
        let r = eval_scalar(
            &func_ref,
            &mut local_env,
            &arg_name,
            min_t,
            &mut from_cache,
            &mut to_cache,
        );
        (r * min_t.cos(), r * min_t.sin())
    };
    let (start_screen_x, start_screen_y) = math_to_screen_padded(
        start_math_x,
        start_math_y,
        &plot.p_x_domain,
        &plot.p_y_domain,
        &plot.p_size,
        &plot.padding,
    );

    let (end_math_x, end_math_y) = if plot.kind == PlotCurveKind::Cartesian {
        let y = eval_scalar(
            &func_ref,
            &mut local_env,
            &arg_name,
            max_t,
            &mut from_cache,
            &mut to_cache,
        );
        (max_t, y)
    } else if plot.kind == PlotCurveKind::Parametric {
        let [x, y] =
            eval_vec2(&func_ref, &mut local_env, &arg_name, max_t, &mut from_cache, &mut to_cache);
        (x, y)
    } else {
        let r = eval_scalar(
            &func_ref,
            &mut local_env,
            &arg_name,
            max_t,
            &mut from_cache,
            &mut to_cache,
        );
        (r * max_t.cos(), r * max_t.sin())
    };
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

    if plot.kind == PlotCurveKind::Cartesian {
        sample_recursive_cartesian(
            min_t,
            max_t,
            p0,
            p1,
            0,
            actual_max_depth,
            base_subdivision_depth(actual_resolution, actual_max_depth),
            actual_tolerance,
            &mut local_env,
            &arg_name,
            &func_ref,
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            &plot.padding,
            &mut from_cache,
            &mut to_cache,
            &mut pts,
        );
    } else if plot.kind == PlotCurveKind::Polar {
        sample_recursive_polar(
            min_t,
            max_t,
            p0,
            p1,
            0,
            actual_max_depth,
            base_subdivision_depth(actual_resolution, actual_max_depth),
            actual_tolerance,
            &mut local_env,
            &arg_name,
            &func_ref,
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            &plot.padding,
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
            actual_max_depth,
            base_subdivision_depth(actual_resolution, actual_max_depth),
            actual_tolerance,
            &mut local_env,
            &arg_name,
            &func_ref,
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            &plot.padding,
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
        stroke: if plot.stroke_width > 0.0 {
            Some((
                vello::peniko::Color::from_rgba8(
                    (plot.stroke_color[0] * 255.0) as u8,
                    (plot.stroke_color[1] * 255.0) as u8,
                    (plot.stroke_color[2] * 255.0) as u8,
                    (plot.stroke_color[3] * 255.0) as u8,
                ),
                plot.stroke_width,
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

    vello_paths
}
