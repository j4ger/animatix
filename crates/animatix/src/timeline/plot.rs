//!
//! # Non-Interpolatable Property Transitions (Side-Channel Pattern)
//!
//! The `func` property on plot actors cannot use standard `PropertyTrack<T>`
//! animation because closures (function bodies like `(x) => sin(x * freq)`)
//! cannot implement the [`Interpolate`](crate::easing::Interpolate) trait.
//! There is no meaningful way to "lerp" two closure ASTs.
//!
//! Instead, `func` transitions use a **side-channel** pattern:
//!
//! - [`FuncTransition`] records a time range, easing, blend mode, and the
//!   `from`/`to` [`FuncSource`] closures in a parallel `Vec` on `AnimationTrack`.
//! - At frame time, [`sample_procedural_plot_at`] checks for active transitions.
//!   Output blending evaluates both sources and lerps their outputs at each
//!   sample point; opacity blending renders the two generated path sets at
//!   partial opacity.
//! - [`FuncSource::Blend`] captures a mid-flight snap when the transition is frozen mid-progress
//!   (used for combined transitions).
//!
//! This pattern is intentional. Future property types whose values cannot
//! implement `Interpolate` (e.g., AST nodes, resource handles) should use
//! the same pattern. See `AnimationTrack::func_transitions` in `dispatch.rs`
//! for the full implementation checklist.
//!
//! # Adaptive Sampling Algorithm
//!
//! The plotting functions in this module use recursive midpoint subdivision to
//! sample mathematical curves at adaptive resolution. The algorithm works as follows:
//!
//! 1. **Recursive Midpoint Subdivision**: Start with two endpoints of a segment. Compute the
//!    midpoint and compare it against a linear interpolation between endpoints. If the deviation
//!    exceeds `tolerance`, subdivide both halves recursively.
//!
//! 2. **Coarse-to-Fine Refinement**: Begin with coarse sampling and refine only where the curve
//!    deviates significantly from a straight line. This captures detail where needed while avoiding
//!    unnecessary computation in flat regions.
//!
//! 3. **Maximum Depth Cap**: Subdivision stops when reaching `max_depth` to prevent infinite
//!    recursion and control computational cost. The minimum segment size is thus `(total_range) /
//!    2^max_depth`.
//!
//! 4. **Discontinuity Handling**: When detecting steep jumps (asymptotes, discontinuities), inject
//!    a NaN point so Vello's path renderer breaks the stroke. This prevents erroneous straight-line
//!    connections across gaps.
//!
//! 5. **Visibility Culling**: Segments whose y-coordinates (and x-coordinates for parametric/polar)
//!    lie entirely outside the visible region with margin are culled with NaN separators, skipping
//!    unnecessary evaluation.
//!
//! 6. **Tolerance-Accuracy Tradeoff**: The `tolerance` parameter (squared distance threshold)
//!    controls how much deviation is acceptable before subdividing. Lower values produce more
//!    accurate curves but require more samples; higher values improve performance at the cost of
//!    accuracy.
//!
//! The three sampling functions (`cartesian`, `polar`, `parametric`) share this core
//! algorithm but differ in how they map mathematical coordinates to screen space.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::modifier_runtime::ir::{CompiledExpr, compile_expr, evaluate_compiled_expr};
use super::{CapturedEnv, Environment, EvalError, Value};
use crate::ast::Expr;
use crate::easing::{Easing, apply_easing};

// ─────────────────────────────────────────────────────────────
// Plot curve kind
// ─────────────────────────────────────────────────────────────

/// Discriminant for the four sampling strategies of `PlotCurve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PlotCurveKind {
    Cartesian,
    Polar,
    Parametric,
    Implicit,
}

impl PlotCurveKind {
    /// Parse a `kind` property value.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cartesian" => Some(Self::Cartesian),
            "polar" => Some(Self::Polar),
            "parametric" => Some(Self::Parametric),
            "implicit" => Some(Self::Implicit),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Func transition data model
// ─────────────────────────────────────────────────────────────

/// Source of a function for a `PlotCurve` `func` transition.
///
/// This type is part of the **side-channel pattern** for non-interpolatable
/// properties — see
/// [`AnimationTrack::func_transitions`](crate::timeline::dispatch::AnimationTrack::func_transitions)
/// in `dispatch.rs` and the module-level documentation for the full
/// explanation of why closures cannot use `PropertyTrack<T>`.
///
/// A `FuncSource` is either:
/// - [`Compiled`](FuncSource::Compiled): a user-authored closure `(x) => expr`, stored as argument
///   names and a compiled body. This is the steady-state form used when no transition is in progress.
/// - [`Blend`](FuncSource::Blend): a frozen mid-transition snapshot captured when a second `func`
///   transition begins before the first has finished. Rather than discarding in-progress blending
///   state, the evaluator snapshots the current `(from, to, progress)` into a `Blend` node and uses
///   it as the `from` for the next transition. This allows cascading function transitions without
///   visual discontinuities.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FuncSource {
    /// A closure defined by argument names, compiled body, and captured environment variables.
    /// The captures are build-time loop variable values needed at render time.
    Compiled(Vec<String>, Box<CompiledExpr>, CapturedEnv),
    /// A mid-transition snap: blends `from` and `to` at a frozen progress value.
    Blend {
        from: Box<FuncSource>,
        to: Box<FuncSource>,
        frozen_progress: f64,
    },
}

impl FuncSource {
    /// Compile an AST closure body into a function source.
    pub fn from_expr(args: Vec<String>, body: Expr, captures: CapturedEnv) -> Self {
        match compile_expr(&body) {
            Ok(compiled) => FuncSource::Compiled(args, Box::new(compiled), captures),
            Err(e) => {
                tracing::warn!("Failed to compile plot function body: {e}");
                FuncSource::Compiled(args, Box::new(CompiledExpr::Const(Value::Num(0.0))), captures)
            },
        }
    }

    /// Return the number of arguments this function source expects.
    /// For `Blend`, delegates to the inner `to` source (which has the target arity).
    pub fn arity(&self) -> usize {
        match self {
            FuncSource::Compiled(args, _, _) => args.len(),
            FuncSource::Blend { to, .. } => to.arity(),
        }
    }
}

/// How a [`FuncTransition`] combines its two function sources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FuncBlendMode {
    /// Blend evaluated function outputs at every sample point (default).
    #[default]
    Output,
    /// Render the `from` and `to` visual outputs as separate opacity layers.
    Opacity,
}

/// One keyframe-driven transition between two [`FuncSource`] values.
///
/// This is the core unit of the **side-channel pattern** for the `func`
/// property on plot actors. Because closures cannot implement
/// [`Interpolate`](crate::easing::Interpolate), `func` transitions are stored
/// in a parallel `Vec<FuncTransition>` on `AnimationTrack` (the
/// `func_transitions` field in `dispatch.rs`) rather than in a standard
/// `PropertyTrack<T>`.
///
/// ## Lifecycle
///
/// 1. The build stage appends a new `FuncTransition` whenever it encounters a `func = <expr>
///    [easing, duration]` keyframe on a supported plot actor.
/// 2. At frame evaluation time, `sample_procedural_plot_at` iterates `func_transitions` and calls
///    [`active_at`] to find the transition covering the current time.
/// 3. If one is active, both `from` and `to` are evaluated at each sample point and combined by
///    [`FuncBlendMode`].
/// 4. [`is_complete_at`] identifies the last completed transition so its `to` source serves as the
///    static baseline between transitions.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FuncTransition {
    pub start_ms: u64,
    pub end_ms: u64,
    pub easing: Easing,
    pub from: FuncSource,
    pub to: FuncSource,
    /// How `from` and `to` are combined while this transition is active.
    #[cfg_attr(feature = "serde", serde(default))]
    pub blend_mode: FuncBlendMode,
}

impl FuncTransition {
    /// Returns `(eased_progress, from, to, easing)` if this transition is active
    /// at `time_ms` (i.e., `start_ms <= time_ms <= end_ms`).
    pub fn active_at(&self, time_ms: u64) -> Option<(f64, &FuncSource, &FuncSource, &Easing)> {
        if time_ms < self.start_ms || time_ms > self.end_ms {
            return None;
        }
        let duration = (self.end_ms - self.start_ms) as f64;
        let raw_progress = if duration <= 0.0 {
            1.0_f64
        } else {
            (time_ms - self.start_ms) as f64 / duration
        };
        let eased = apply_easing(raw_progress as f32, self.easing) as f64;
        Some((eased, &self.from, &self.to, &self.easing))
    }

    /// Returns `true` if this transition completed before `time_ms`.
    pub fn is_complete_at(&self, time_ms: u64) -> bool {
        time_ms > self.end_ms
    }
}

/// Evaluate a [`FuncSource`] at a single scalar argument, returning the scalar
/// result. Clones `env` locally to avoid mutating the caller's environment.
#[allow(dead_code)] // Reserved for future VectorField/Heatmap/ContourSet transition support
pub fn resolve_func_source(
    source: &FuncSource,
    env: &Environment,
    arg_name: &str,
    arg_val: f64,
) -> Result<f64, EvalError> {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let name = args.first().map(String::as_str).unwrap_or(arg_name);
            let mut local_env = env.clone();
            captures.merge_missing_into(&mut local_env);
            local_env.set_binding(name, Value::Num(arg_val));
            evaluate_compiled_expr(body, &local_env).map(|v| v.as_num())
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                sum += weight * resolve_func_source(src, env, arg_name, arg_val)?;
            }
            Ok(sum)
        },
    }
}

/// Evaluate a [`FuncSource`] at scalar `t`, returning a 2-element array
/// (for parametric plots). Non-Vec2 results become `[NaN, NaN]`.
#[allow(dead_code)] // Reserved for future parametric plot transition support
pub fn resolve_func_source_vec2(
    source: &FuncSource,
    env: &Environment,
    arg_name: &str,
    arg_val: f64,
) -> Result<[f64; 2], EvalError> {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let name = args.first().map(String::as_str).unwrap_or(arg_name);
            let mut local_env = env.clone();
            captures.merge_missing_into(&mut local_env);
            local_env.set_binding(name, Value::Num(arg_val));
            evaluate_compiled_expr(body, &local_env).map(|v| match v {
                Value::Vec2(arr) => arr,
                other => [other.as_num(), f64::NAN],
            })
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            for (weight, src) in flat {
                let [vx, vy] = resolve_func_source_vec2(src, env, arg_name, arg_val)?;
                sum_x += weight * vx;
                sum_y += weight * vy;
            }
            Ok([sum_x, sum_y])
        },
    }
}

// ─────────────────────────────────────────────────────────────
// Adaptive sampling: blended function reference
// ─────────────────────────────────────────────────────────────

/// Function reference used during per-frame adaptive sampling.
///
/// `Single` re-uses the existing one-function path unchanged.
/// `Blended` evaluates both sources and lerps the outputs at `progress`.
pub(crate) enum PlotFuncRef<'a> {
    /// Evaluate a single function source.
    Single(&'a FuncSource),
    /// Lerp between two function sources at `progress` ∈ \[0, 1\].
    Blended {
        from: &'a FuncSource,
        to: &'a FuncSource,
        progress: f64,
    },
}

/// Evaluate `source` at scalar `x`, using `cache` to avoid redundant evaluations.
pub(crate) fn eval_source_scalar(
    source: &FuncSource,
    env: &mut Environment,
    arg_name: &str,
    x: f64,
    cache: &mut HashMap<u64, Value>,
) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let name = args.first().map(String::as_str).unwrap_or(arg_name);
            let key = x.to_bits();
            let val = cache.get(&key).cloned().unwrap_or_else(|| {
                // Inject captured variables on first evaluation for this x
                let inserted = captures.merge_missing_into(env);
                env.set_binding(name, Value::Num(x));
                let result = evaluate_compiled_expr(body, env).unwrap_or(Value::Num(f64::NAN));
                env.clear_bindings();
                for key in inserted {
                    env.overrides.remove(&key);
                }
                cache.insert(key, result.clone());
                result
            });
            val.as_num()
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                let mut cache = HashMap::new();
                sum += weight * eval_source_scalar(src, env, arg_name, x, &mut cache);
            }
            sum
        },
    }
}

/// Evaluate `source` at `t` returning a Vec2, using `cache` to avoid redundant evaluations.
fn eval_source_vec2(
    source: &FuncSource,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    cache: &mut HashMap<u64, Value>,
) -> [f64; 2] {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            let name = args.first().map(String::as_str).unwrap_or(arg_name);
            let key = t.to_bits();
            let val = cache.get(&key).cloned().unwrap_or_else(|| {
                // Inject captured variables on first evaluation for this t
                let inserted = captures.merge_missing_into(env);
                env.set_binding(name, Value::Num(t));
                let result =
                    evaluate_compiled_expr(body, env).unwrap_or(Value::Vec2([f64::NAN, f64::NAN]));
                env.clear_bindings();
                for key in inserted {
                    env.overrides.remove(&key);
                }
                cache.insert(key, result.clone());
                result
            });
            match val {
                Value::Vec2(arr) => arr,
                _ => [f64::NAN, f64::NAN],
            }
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum_x = 0.0;
            let mut sum_y = 0.0;
            for (weight, src) in flat {
                let mut cache = HashMap::new();
                let [vx, vy] = eval_source_vec2(src, env, arg_name, t, &mut cache);
                sum_x += weight * vx;
                sum_y += weight * vy;
            }
            [sum_x, sum_y]
        },
    }
}

/// Evaluate a [`PlotFuncRef`] at scalar `x`, lerping if blended.
fn eval_scalar(
    func: &PlotFuncRef<'_>,
    env: &mut Environment,
    arg_name: &str,
    x: f64,
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
) -> f64 {
    match func {
        PlotFuncRef::Single(src) => eval_source_scalar(src, env, arg_name, x, from_cache),
        PlotFuncRef::Blended { from, to, progress } => {
            let fv = eval_source_scalar(from, env, arg_name, x, from_cache);
            let tv = eval_source_scalar(to, env, arg_name, x, to_cache);
            fv + (tv - fv) * progress
        },
    }
}

/// Evaluate a [`PlotFuncRef`] at `t` returning a Vec2, lerping if blended.
fn eval_vec2(
    func: &PlotFuncRef<'_>,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
) -> [f64; 2] {
    match func {
        PlotFuncRef::Single(src) => eval_source_vec2(src, env, arg_name, t, from_cache),
        PlotFuncRef::Blended { from, to, progress } => {
            let [fx, fy] = eval_source_vec2(from, env, arg_name, t, from_cache);
            let [tx, ty] = eval_source_vec2(to, env, arg_name, t, to_cache);
            [fx + (tx - fx) * progress, fy + (ty - fy) * progress]
        },
    }
}

/// Compute the depth of nested [`FuncSource::Blend`] trees.
///
/// Returns 0 for [`FuncSource::Compiled`], and 1 + max(blend_depth of children)
/// for [`FuncSource::Blend`]. Used by the adaptive quality system to reduce
/// sampling quality during cascading transitions.
pub(crate) fn blend_depth(source: &FuncSource) -> usize {
    match source {
        FuncSource::Compiled(..) => 0,
        FuncSource::Blend { from, to, .. } => 1 + blend_depth(from).max(blend_depth(to)),
    }
}

/// Flatten nested [`FuncSource::Blend`] trees into a linear list of
/// `(weight, base_source)` pairs for O(N) weighted-sum evaluation.
///
/// Each base [`FuncSource::Compiled`] appears exactly once in the output list.
/// The lerp formula `from*(1-p) + to*p` is distributed through the tree
/// so that a depth-N cascade produces N+1 leaf entries instead of 2^N
/// recursive evaluations.
pub(crate) fn flatten_blend(source: &FuncSource) -> Vec<(f64, &FuncSource)> {
    match source {
        FuncSource::Compiled(..) => vec![(1.0, source)],
        FuncSource::Blend {
            from,
            to,
            frozen_progress,
        } => {
            let mut result = Vec::new();
            // from contributes with weight (1 - frozen_progress)
            for (w, s) in flatten_blend(from) {
                result.push((w * (1.0 - frozen_progress), s));
            }
            // to contributes with weight frozen_progress
            for (w, s) in flatten_blend(to) {
                result.push((w * frozen_progress, s));
            }
            result
        },
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_recursive_cartesian(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let screen_height = p_size[1];

    let margin_y = screen_height * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    if (p0.y < min_screen_y && p1.y < min_screen_y) || (p0.y > max_screen_y && p1.y > max_screen_y)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;
    if (p0.x < min_screen_x && p1.x < min_screen_x) || (p0.x > max_screen_x && p1.x > max_screen_x)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let math_y = eval_scalar(func, env, arg_name, mid_t, from_cache, to_cache);
    let math_x = mid_t;

    let (screen_x, screen_y) =
        math_to_screen_padded(math_x, math_y, p_x_domain, p_y_domain, p_size, padding);

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_cartesian(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
        sample_recursive_cartesian(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_recursive_polar(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let margin_y = p_size[1] * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > (p_size[0].max(p_size[1])).powi(2) * 4.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let math_r = eval_scalar(func, env, arg_name, mid_t, from_cache, to_cache);
    let math_x = math_r * mid_t.cos();
    let math_y = math_r * mid_t.sin();

    let (screen_x, screen_y) =
        math_to_screen_padded(math_x, math_y, p_x_domain, p_y_domain, p_size, padding);

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_polar(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
        sample_recursive_polar(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sample_recursive_parametric(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let margin_y = p_size[1] * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > (p_size[0].max(p_size[1])).powi(2) * 4.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let [math_x, math_y] = eval_vec2(func, env, arg_name, mid_t, from_cache, to_cache);
    if math_x.is_nan() || math_y.is_nan() {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    let (screen_x, screen_y) =
        math_to_screen_padded(math_x, math_y, p_x_domain, p_y_domain, p_size, padding);

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_parametric(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
        sample_recursive_parametric(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            func,
            p_x_domain,
            p_y_domain,
            p_size,
            padding,
            from_cache,
            to_cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

/// Convert math coordinates to screen coordinates relative to the graph actor center.
///
/// `p_size` is the half-extent of the graph `[hw, hh]`.  `padding` is `[left, right, top, bottom]`
/// in the same pixel units.  Returns coordinates in the local (relative-to-center) space.
pub(crate) fn math_to_screen_padded(
    math_x: f64,
    math_y: f64,
    x_domain: &[f64; 2],
    y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
) -> (f64, f64) {
    let plot_w = p_size[0] - padding[0] - padding[1];
    let plot_h = p_size[1] - padding[2] - padding[3];
    let shift_x = (padding[0] - padding[1]) / 2.0;
    let shift_y = (padding[2] - padding[3]) / 2.0;
    let x_range = x_domain[1] - x_domain[0];
    let y_range = y_domain[1] - y_domain[0];
    let norm_x = if x_range.abs() > f64::EPSILON {
        (math_x - x_domain[0]) / x_range
    } else {
        0.5
    };
    let norm_y = if y_range.abs() > f64::EPSILON {
        (math_y - y_domain[0]) / y_range
    } else {
        0.5
    };
    let screen_x = shift_x + (norm_x - 0.5) * plot_w;
    let screen_y = shift_y + (0.5 - norm_y) * plot_h;
    (screen_x, screen_y)
}

pub(crate) fn implicit_intersection(
    p0: (f64, f64, f64),
    p1: (f64, f64, f64),
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
) -> kurbo::Point {
    let (x0, y0, v0) = p0;
    let (x1, y1, v1) = p1;
    let t = if (v1 - v0).abs() <= f64::EPSILON {
        0.5
    } else {
        (-v0 / (v1 - v0)).clamp(0.0, 1.0)
    };
    let x = x0 + (x1 - x0) * t;
    let y = y0 + (y1 - y0) * t;
    let (screen_x, screen_y) = math_to_screen_padded(x, y, p_x_domain, p_y_domain, p_size, padding);
    kurbo::Point::new(screen_x, screen_y)
}

/// Evaluate a [`FuncSource`] at (x, y) coordinates, returning a scalar value
/// for implicit contour detection. Supports blended transitions via recursive
/// evaluation of `Blend` nodes.
pub(crate) fn eval_implicit_source(
    source: &FuncSource,
    env: &mut Environment,
    x: f64,
    y: f64,
) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            // Merge captured variables into the environment
            let inserted = captures.merge_missing_into(env);
            // Bind both arguments
            if args.len() >= 2 {
                env.set(&args[0], Value::Num(x));
                env.set(&args[1], Value::Num(y));
            }
            // Evaluate and return scalar
            let result = match evaluate_compiled_expr(body, env) {
                Ok(Value::Num(n)) => n,
                _ => f64::NAN,
            };
            for key in inserted {
                env.overrides.remove(&key);
            }
            result
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                sum += weight * eval_implicit_source(src, env, x, y);
            }
            sum
        },
    }
}

/// Build an implicit plot contour path from a [`FuncSource`], evaluating
/// the scalar field on a grid and extracting zero-contours via marching
/// squares. Supports function transitions via blend sources.
pub fn build_implicit_plot_path_from_source(
    env: &mut Environment,
    source: &FuncSource,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    let x_cells = resolution.max(8);
    let aspect = if p_size[0] <= f64::EPSILON {
        1.0
    } else {
        p_size[1] / p_size[0]
    };
    let y_cells = ((x_cells as f64) * aspect).round().max(8.0) as usize;
    let dx = (p_x_domain[1] - p_x_domain[0]) / x_cells as f64;
    let dy = (p_y_domain[1] - p_y_domain[0]) / y_cells as f64;

    // Pre-evaluate the function on a grid to avoid redundant AST evaluations.
    let mut grid = vec![vec![f64::NAN; x_cells + 1]; y_cells + 1];
    crate::timeline::utils::disable_eval_cache();
    for (yi, row) in grid.iter_mut().enumerate() {
        let y = p_y_domain[0] + yi as f64 * dy;
        for (xi, val) in row.iter_mut().enumerate() {
            let x = p_x_domain[0] + xi as f64 * dx;
            *val = eval_implicit_source(source, env, x, y);
        }
    }
    crate::timeline::utils::enable_eval_cache();

    for yi in 0..y_cells {
        let y0 = p_y_domain[0] + yi as f64 * dy;
        let y1 = y0 + dy;
        for xi in 0..x_cells {
            let x0 = p_x_domain[0] + xi as f64 * dx;
            let x1 = x0 + dx;

            let bl = (x0, y0, grid[yi][xi]);
            let br = (x1, y0, grid[yi][xi + 1]);
            let tr = (x1, y1, grid[yi + 1][xi + 1]);
            let tl = (x0, y1, grid[yi + 1][xi]);

            if [bl.2, br.2, tr.2, tl.2].iter().any(|v| !v.is_finite()) {
                continue;
            }

            let bl_in = bl.2 >= 0.0;
            let br_in = br.2 >= 0.0;
            let tr_in = tr.2 >= 0.0;
            let tl_in = tl.2 >= 0.0;

            let mut intersections = Vec::new();
            if bl_in != br_in {
                intersections.push((
                    0,
                    implicit_intersection(bl, br, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if br_in != tr_in {
                intersections.push((
                    1,
                    implicit_intersection(br, tr, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tr_in != tl_in {
                intersections.push((
                    2,
                    implicit_intersection(tr, tl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tl_in != bl_in {
                intersections.push((
                    3,
                    implicit_intersection(tl, bl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }

            match intersections.len() {
                2 => {
                    path.move_to(intersections[0].1);
                    path.line_to(intersections[1].1);
                },
                4 => {
                    let center =
                        eval_implicit_source(source, env, (x0 + x1) * 0.5, (y0 + y1) * 0.5);
                    let center_positive = center >= 0.0;
                    let edge = |idx: usize| {
                        intersections
                            .iter()
                            .find(|(edge_idx, _)| *edge_idx == idx)
                            .map(|(_, pt)| *pt)
                    };
                    if bl_in == tr_in && br_in == tl_in {
                        let first_pair = if center_positive == bl_in {
                            (0, 3)
                        } else {
                            (0, 1)
                        };
                        let second_pair = if center_positive == bl_in {
                            (1, 2)
                        } else {
                            (2, 3)
                        };
                        if let (Some(a), Some(b)) = (edge(first_pair.0), edge(first_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                        if let (Some(a), Some(b)) = (edge(second_pair.0), edge(second_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                    }
                },
                0 => {},
                1 | 3 => {
                    tracing::debug!(
                        "Implicit plot: degenerate cell with {} intersections at ({}, {})",
                        intersections.len(),
                        xi,
                        yi
                    );
                },
                n => {
                    tracing::warn!(
                        "Implicit plot: unexpected {} intersections in cell ({}, {})",
                        n,
                        xi,
                        yi
                    );
                },
            }
        }
    }

    path
}

/// Legacy wrapper that builds an implicit plot path from args and a compiled body.
/// Creates a `FuncSource::Compiled` and delegates to [`build_implicit_plot_path_from_source`].
#[deprecated(
    since = "0.5.0",
    note = "use build_implicit_plot_path_from_source instead"
)]
#[allow(dead_code)] // Legacy public API shim; kept for backward compatibility.
pub fn build_implicit_plot_path(
    env: &mut Environment,
    arg_names: &[String],
    body: &CompiledExpr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
) -> kurbo::BezPath {
    let source =
        FuncSource::Compiled(arg_names.to_vec(), Box::new(body.clone()), CapturedEnv::default());
    build_implicit_plot_path_from_source(
        env, &source, p_x_domain, p_y_domain, p_size, resolution, padding,
    )
}

// ─────────────────────────────────────────────────────────────
// Per-frame procedural plot sampling
// ─────────────────────────────────────────────────────────────

use crate::renderer::types::VelloPath;

// ─────────────────────────────────────────────────────────────
// ProceduralPlot struct + helpers
// ─────────────────────────────────────────────────────────────

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
    /// injection.  Used to decide whether to resample on every frame.
    pub fn is_dynamic(&self) -> bool {
        self.func_body.references_ident("t") || !self.param_names.is_empty()
    }
}

/// Re-sample a procedural plot at frame time using the given environment.
/// Compatibility shim for `scene_eval.rs` (pre-Task-4). Delegates to
/// [`sample_procedural_plot_at`] with no active transitions.
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
    env: &mut Environment,
    time_ms: u64,
    transitions: &[FuncTransition],
) -> Vec<VelloPath> {
    // Inject custom plot parameters as fallback defaults: only set the build-time
    // static value when the frame environment does not already carry an override
    // for the same name (e.g. from `always { freq = ... }` or a keyframe `let`).
    for (name, val) in &plot.params {
        if env.get(name).is_none() {
            env.set(name, crate::timeline::Value::Num(*val));
        }
    }

    let decl_source = FuncSource::Compiled(
        plot.func_args.clone(),
        Box::new(plot.func_body.clone()),
        plot.extra_captures.clone(),
    );
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
                from,
                actual_max_depth,
                actual_tolerance,
                actual_resolution,
            );
            let to_paths = sample_plot_source(
                plot,
                env,
                to,
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

    // Resolve the active function reference for this frame.
    let func_ref: PlotFuncRef<'_> = if let Some((progress, from, to, _)) = active {
        PlotFuncRef::Blended { from, to, progress }
    } else {
        // Use the last completed transition's target, or the declaration func.
        let last_complete = transitions.iter().rev().find(|t| t.is_complete_at(time_ms));
        match last_complete {
            Some(t) => PlotFuncRef::Single(&t.to),
            None => PlotFuncRef::Single(&decl_source),
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

    let source = match func_ref {
        PlotFuncRef::Single(src) => src.clone(),
        PlotFuncRef::Blended { from, to, progress } => FuncSource::Blend {
            from: Box::new(from.clone()),
            to: Box::new(to.clone()),
            frozen_progress: progress,
        },
    };

    sample_plot_source(plot, env, &source, actual_max_depth, actual_tolerance, actual_resolution)
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

/// Re-sample one concrete [`FuncSource`] with the procedural renderer described
/// by `plot`. Output-mode blending passes a [`FuncSource::Blend`] here;
/// opacity-mode blending calls this separately for each endpoint.
pub(crate) fn sample_plot_source(
    plot: &ProceduralPlot,
    env: &mut Environment,
    source: &FuncSource,
    actual_max_depth: usize,
    actual_tolerance: f64,
    actual_resolution: usize,
) -> Vec<VelloPath> {
    match plot.plot_type {
        ProceduralPlotKind::Curve(_) => sample_curve_plot_source(
            plot,
            env,
            source,
            actual_max_depth,
            actual_tolerance,
            actual_resolution,
        ),
        ProceduralPlotKind::VectorField => {
            let full_size = [plot.p_size[0] * 2.0, plot.p_size[1] * 2.0];
            super::build::plot::build_vector_field_paths(
                env,
                source,
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
            super::build::plot::build_heatmap_paths(
                env,
                source,
                plot.p_x_domain,
                plot.p_y_domain,
                full_size,
                actual_resolution.max(4),
                plot.fill_color,
            )
        },
        ProceduralPlotKind::ContourSet => {
            let full_size = [plot.p_size[0] * 2.0, plot.p_size[1] * 2.0];
            super::build::plot::build_contour_set_paths(
                env,
                source,
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
    env: &mut Environment,
    source: &FuncSource,
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
    let func_ref = PlotFuncRef::Single(source);

    let (min_t, max_t) = if plot.kind == PlotCurveKind::Cartesian {
        (plot.p_x_domain[0], plot.p_x_domain[1])
    } else if plot.kind == PlotCurveKind::Implicit {
        (0.0, 0.0)
    } else {
        (plot.t_domain[0], plot.t_domain[1])
    };

    if plot.kind == PlotCurveKind::Implicit {
        let path = build_implicit_plot_path_from_source(
            env,
            source,
            &plot.p_x_domain,
            &plot.p_y_domain,
            &plot.p_size,
            actual_resolution,
            &plot.padding,
        );
        vello_paths.push(VelloPath {
            path,
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
        });
        return vello_paths;
    }

    // Shared caches for from/to sources across start, end, and recursive evals.
    let mut from_cache = HashMap::<u64, Value>::new();
    let mut to_cache = HashMap::<u64, Value>::new();

    let (start_math_x, start_math_y) = if plot.kind == PlotCurveKind::Cartesian {
        let y = eval_scalar(&func_ref, env, &arg_name, min_t, &mut from_cache, &mut to_cache);
        (min_t, y)
    } else if plot.kind == PlotCurveKind::Parametric {
        let [x, y] = eval_vec2(&func_ref, env, &arg_name, min_t, &mut from_cache, &mut to_cache);
        (x, y)
    } else {
        let r = eval_scalar(&func_ref, env, &arg_name, min_t, &mut from_cache, &mut to_cache);
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
        let y = eval_scalar(&func_ref, env, &arg_name, max_t, &mut from_cache, &mut to_cache);
        (max_t, y)
    } else if plot.kind == PlotCurveKind::Parametric {
        let [x, y] = eval_vec2(&func_ref, env, &arg_name, max_t, &mut from_cache, &mut to_cache);
        (x, y)
    } else {
        let r = eval_scalar(&func_ref, env, &arg_name, max_t, &mut from_cache, &mut to_cache);
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
            actual_tolerance,
            env,
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
            actual_tolerance,
            env,
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
            actual_tolerance,
            env,
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
        path,
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
    });

    vello_paths
}

#[cfg(test)]
mod tests {
    use super::math_to_screen_padded;

    /// With zero padding the formula degenerates to the original:
    /// `screen = (norm - 0.5) * size`
    #[test]
    fn math_to_screen_padded_zero_padding_matches_original() {
        let x_domain = [-1.0_f64, 1.0];
        let y_domain = [-1.0_f64, 1.0];
        let p_size = [200.0_f64, 200.0];
        let padding = [0.0_f64; 4];

        // domain center → screen (0, 0)
        let (sx, sy) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sx).abs() < 1e-10, "expected sx=0, got {sx}");
        assert!((sy).abs() < 1e-10, "expected sy=0, got {sy}");

        // domain max-x → screen x = +100  (half of 200)
        let (sx, _) = math_to_screen_padded(1.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sx - 100.0).abs() < 1e-10, "expected sx=100, got {sx}");

        // domain min-y → screen y = +100  (screen Y increases downward)
        let (_, sy) = math_to_screen_padded(0.0, -1.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sy - 100.0).abs() < 1e-10, "expected sy=100, got {sy}");
    }

    /// Symmetric padding shrinks the plot area proportionally.
    /// With [p, p, p, p] padding the centre remains at screen (0, 0) and
    /// the domain boundary maps to (half_size - p) instead of half_size.
    #[test]
    fn math_to_screen_padded_symmetric_padding_shrinks_area() {
        let x_domain = [-1.0_f64, 1.0];
        let y_domain = [-1.0_f64, 1.0];
        let p_size = [200.0_f64, 200.0];
        let padding = [10.0_f64; 4]; // 10 px on every side

        // domain centre still maps to screen centre
        let (sx, sy) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sx).abs() < 1e-10, "centre sx should be 0, got {sx}");
        assert!((sy).abs() < 1e-10, "centre sy should be 0, got {sy}");

        // domain max-x maps to (plot_w / 2) = (200 - 10 - 10) / 2 = 90
        let (sx, _) = math_to_screen_padded(1.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sx - 90.0).abs() < 1e-10, "expected sx=90, got {sx}");
    }

    /// Asymmetric left/right padding shifts the plot center in x.
    #[test]
    fn math_to_screen_padded_asymmetric_padding_shifts_center() {
        let x_domain = [-1.0_f64, 1.0];
        let y_domain = [-1.0_f64, 1.0];
        let p_size = [200.0_f64, 200.0];
        // 20 px on the left, 0 on the right → center shifts right by 10
        let padding = [20.0, 0.0, 0.0, 0.0];

        // domain centre maps to x = (20 - 0) / 2 = 10
        let (sx, _) = math_to_screen_padded(0.0, 0.0, &x_domain, &y_domain, &p_size, &padding);
        assert!((sx - 10.0).abs() < 1e-10, "expected sx=10 with left-only padding, got {sx}");
    }
}
