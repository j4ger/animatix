//! # Non-Interpolatable Property Transitions (Side-Channel Pattern)
//!
//! The `func` property on plot actors cannot use standard `PropertyTrack<T>`
//! animation because closures (function bodies like `(x) => sin(x * freq)`)
//! cannot implement the [`crate::timeline::Interpolate`] trait.
//! There is no meaningful way to "lerp" two closure ASTs.
//!
//! Instead, `func` transitions use a **side-channel** pattern:
//!
//! - `FuncTransition` records a time range, easing, blend mode, and the `from`/`to` `FuncSource`
//!   closures in a parallel `Vec` on the plot payload.
//! - At frame time, `sample_procedural_plot_at` checks for active transitions. Output blending
//!   evaluates both sources and lerps their outputs at each sample point; opacity blending renders
//!   the two generated path sets at partial opacity.
//! - [`FuncSource::Blend`] captures a mid-flight snap when the transition is frozen mid-progress
//!   (used for combined transitions).
//!
//! This pattern is intentional. Future property types whose values cannot
//! implement `Interpolate` (e.g., AST nodes, resource handles) should use
//! the same pattern. See `PlotPayload::func_transitions` in `primitives/payload.rs`
//! for the full implementation checklist.

pub mod adaptive;
pub mod implicit;
pub mod procedural;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use adaptive::{
    CurvePathBuilder, PlotScreenTransform, PointSink, base_subdivision_depth,
    math_to_screen_padded, sample_recursive_cartesian, sample_recursive_cartesian_impl,
    sample_recursive_parametric, sample_recursive_parametric_impl, sample_recursive_polar,
    sample_recursive_polar_impl,
};
#[allow(unused_imports, deprecated)]
pub use implicit::{
    build_implicit_plot_path, build_implicit_plot_path_from_source, eval_implicit_source,
    implicit_intersection,
};
#[allow(unused_imports, deprecated)]
pub use procedural::{
    ProceduralPlot, ProceduralPlotKind, sample_plot_source, sample_procedural_plot,
    sample_procedural_plot_at,
};

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
/// [`PlotPayload::func_transitions`](crate::primitives::PlotPayload::func_transitions)
/// in `dispatch.rs` and the module-level documentation for the full
/// explanation of why closures cannot use `PropertyTrack<T>`.
///
/// A `FuncSource` is either:
/// - [`Compiled`](FuncSource::Compiled): a user-authored closure `(x) => expr`, stored as argument
///   names and a compiled body. This is the steady-state form used when no transition is in
///   progress.
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

/// How a `FuncTransition` combines its two function sources.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum FuncBlendMode {
    /// Blend evaluated function outputs at every sample point (default).
    #[default]
    Output,
    /// Render the `from` and `to` visual outputs as separate opacity layers.
    Opacity,
}

/// One keyframe-driven transition between two `FuncSource` values.
///
/// This is the core unit of the **side-channel pattern** for the `func`
/// property on plot actors. Because closures cannot implement
/// [`crate::timeline::Interpolate`], `func` transitions are stored
/// in a parallel `Vec<FuncTransition>` on the plot payload (the
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
    pub fn new(
        start_ms: u64,
        end_ms: u64,
        easing: Easing,
        from: FuncSource,
        to: FuncSource,
        blend_mode: FuncBlendMode,
    ) -> Self {
        Self {
            start_ms,
            end_ms,
            easing,
            from,
            to,
            blend_mode,
        }
    }

    /// Evaluates the progress of this transition at `time_ms`.
    ///
    /// Returns `Some((progress, from, to, easing))` where `progress` ∈ \[0, 1\] is
    /// eased. Returns `None` if `time_ms` is outside `[start_ms, end_ms]`.
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

/// One `data = {…}` assignment on a `BarChart`: the datasets before and after,
/// the window, and the easing that carries the bars between them.
///
/// This follows [`FuncTransition`]'s pattern (see this module's header) because a
/// dataset is not an `Interpolate` value: bars are matched by label, so the blend
/// happens on the *outputs* — each bar's height — rather than on the property.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct BarDataTransition {
    pub start_ms: u64,
    pub end_ms: u64,
    pub easing: Easing,
    pub from: Vec<(String, f32)>,
    pub to: Vec<(String, f32)>,
}

impl BarDataTransition {
    /// Eased progress through the transition at `time_ms`, `None` outside it.
    pub fn progress_at(&self, time_ms: u64) -> Option<f64> {
        if time_ms < self.start_ms || time_ms > self.end_ms {
            return None;
        }
        let duration = self.end_ms.saturating_sub(self.start_ms);
        let raw = if duration == 0 {
            1.0
        } else {
            (time_ms - self.start_ms) as f64 / duration as f64
        };
        Some(apply_easing(raw as f32, self.easing) as f64)
    }
}

/// The dataset a chart shows at `time_ms`: the declaration's, advanced
/// through every transition whose window has started.
///
/// A frame past the last transition still reports that transition's `to` rather
/// than the declaration, exactly like the procedural-plot sampler: the build-time
/// paths were built from the declaration, so a completed transition has to keep
/// the frame path live or the bars snap back.
pub fn bar_data_at(
    base: &[(String, f32)],
    transitions: &[BarDataTransition],
    time_ms: u64,
) -> Vec<(String, f32)> {
    let mut current: Vec<(String, f32)> = base.to_vec();
    for transition in transitions {
        if time_ms < transition.start_ms {
            break;
        }
        current = match transition.progress_at(time_ms) {
            Some(progress) => interpolate_bar_data(&transition.from, &transition.to, progress),
            None => transition.to.clone(),
        };
    }
    current
}

/// Interpolate two bar datasets, matching bars by label.
///
/// Order follows `to`, because that is the chart the scene is becoming. A label
/// only in `to` enters from height 0; a label only in `from` leaves toward 0 and
/// keeps its slot while it does, which is what makes a shrinking chart read as
/// bars falling away rather than the rest sliding sideways. At `progress` 1 the
/// result is exactly `to`.
pub fn interpolate_bar_data(
    from: &[(String, f32)],
    to: &[(String, f32)],
    progress: f64,
) -> Vec<(String, f32)> {
    let p = progress.clamp(0.0, 1.0) as f32;
    let lerp = |a: f32, b: f32| a + (b - a) * p;
    let mut out: Vec<(String, f32)> = Vec::with_capacity(to.len() + from.len());
    for (label, value) in to {
        let start = from
            .iter()
            .find(|(from_label, _)| from_label == label)
            .map_or(0.0, |(_, from_value)| *from_value);
        out.push((label.clone(), lerp(start, *value)));
    }
    if p < 1.0 {
        for (label, value) in from {
            if !to.iter().any(|(kept, _)| kept == label) {
                out.push((label.clone(), lerp(*value, 0.0)));
            }
        }
    }
    out
}

/// Evaluate a `FuncSource` at a single scalar argument, returning the scalar
/// result. Clones `env` locally to avoid mutating the caller's environment.
/// Test-only today: production sampling goes through `eval_source_scalar`.
/// Kept for the transition-model parity suite (tests/plot_transitions.rs);
/// remove alongside that suite if the transition model changes shape.
#[allow(dead_code)]
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

// ─────────────────────────────────────────────────────────────
// Adaptive sampling: blended function reference
// ─────────────────────────────────────────────────────────────

/// Function reference used during per-frame adaptive sampling.
///
/// `Single` re-uses the existing one-function path unchanged.
/// `Blended` evaluates both sources and lerps the outputs at `progress`.
pub enum PlotFuncRef<'a> {
    /// Direct declaration function from [`ProceduralPlot`] (args, body, captures).
    Declaration(&'a [String], &'a CompiledExpr, &'a CapturedEnv),
    /// Evaluate a single function source.
    Single(&'a FuncSource),
    /// Lerp between two function sources at `progress` ∈ \[0, 1\].
    Blended {
        from: &'a FuncSource,
        to: &'a FuncSource,
        progress: f64,
    },
}

impl<'a> PlotFuncRef<'a> {
    pub fn to_func_source(&self) -> FuncSource {
        match self {
            PlotFuncRef::Declaration(args, body, captures) => {
                FuncSource::Compiled(args.to_vec(), Box::new((*body).clone()), (*captures).clone())
            },
            PlotFuncRef::Single(src) => (*src).clone(),
            PlotFuncRef::Blended { from, to, progress } => FuncSource::Blend {
                from: Box::new((*from).clone()),
                to: Box::new((*to).clone()),
                frozen_progress: *progress,
            },
        }
    }
}

pub fn eval_compiled_parts_scalar(
    args: &[String],
    body: &CompiledExpr,
    captures: &CapturedEnv,
    env: &mut Environment,
    arg_name: &str,
    x: f64,
    cache: &mut HashMap<u64, Value>,
) -> f64 {
    let name = args.first().map(String::as_str).unwrap_or(arg_name);
    let key = x.to_bits();
    let val = cache.get(&key).cloned().unwrap_or_else(|| {
        let inserted = captures.merge_missing_into(env);
        env.set_binding(name, Value::Num(x));
        let result = evaluate_compiled_expr(body, env).unwrap_or(Value::Num(f64::NAN));
        env.clear_bindings();
        for key in inserted {
            env.overrides.remove(&key);
            env.mark_mutated();
        }
        cache.insert(key, result.clone());
        result
    });
    val.as_num()
}

pub fn eval_compiled_parts_vec2(
    args: &[String],
    body: &CompiledExpr,
    captures: &CapturedEnv,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    cache: &mut HashMap<u64, Value>,
) -> [f64; 2] {
    let name = args.first().map(String::as_str).unwrap_or(arg_name);
    let key = t.to_bits();
    let val = cache.get(&key).cloned().unwrap_or_else(|| {
        let inserted = captures.merge_missing_into(env);
        env.set_binding(name, Value::Num(t));
        let result = evaluate_compiled_expr(body, env).unwrap_or(Value::Vec2([f64::NAN, f64::NAN]));
        env.clear_bindings();
        for key in inserted {
            env.overrides.remove(&key);
            env.mark_mutated();
        }
        cache.insert(key, result.clone());
        result
    });
    match val {
        Value::Vec2(arr) => arr,
        _ => [f64::NAN, f64::NAN],
    }
}

/// Evaluate `source` at scalar `x`, using `cache` to avoid redundant evaluations.
pub fn eval_source_scalar(
    source: &FuncSource,
    env: &mut Environment,
    arg_name: &str,
    x: f64,
    cache: &mut HashMap<u64, Value>,
) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            eval_compiled_parts_scalar(args, body, captures, env, arg_name, x, cache)
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
pub fn eval_source_vec2(
    source: &FuncSource,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    cache: &mut HashMap<u64, Value>,
) -> [f64; 2] {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            eval_compiled_parts_vec2(args, body, captures, env, arg_name, t, cache)
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
pub fn eval_scalar(
    func: &PlotFuncRef<'_>,
    env: &mut Environment,
    arg_name: &str,
    x: f64,
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
) -> f64 {
    match func {
        PlotFuncRef::Declaration(args, body, captures) => {
            eval_compiled_parts_scalar(args, body, captures, env, arg_name, x, from_cache)
        },
        PlotFuncRef::Single(src) => eval_source_scalar(src, env, arg_name, x, from_cache),
        PlotFuncRef::Blended { from, to, progress } => {
            let fv = eval_source_scalar(from, env, arg_name, x, from_cache);
            let tv = eval_source_scalar(to, env, arg_name, x, to_cache);
            fv + (tv - fv) * progress
        },
    }
}

/// Evaluate a [`PlotFuncRef`] at `t` returning a Vec2, lerping if blended.
pub fn eval_vec2(
    func: &PlotFuncRef<'_>,
    env: &mut Environment,
    arg_name: &str,
    t: f64,
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
) -> [f64; 2] {
    match func {
        PlotFuncRef::Declaration(args, body, captures) => {
            eval_compiled_parts_vec2(args, body, captures, env, arg_name, t, from_cache)
        },
        PlotFuncRef::Single(src) => eval_source_vec2(src, env, arg_name, t, from_cache),
        PlotFuncRef::Blended { from, to, progress } => {
            let [fx, fy] = eval_source_vec2(from, env, arg_name, t, from_cache);
            let [tx, ty] = eval_source_vec2(to, env, arg_name, t, to_cache);
            [fx + (tx - fx) * progress, fy + (ty - fy) * progress]
        },
    }
}

/// Compute the depth of nested [`FuncSource::Blend`] trees.
pub fn blend_depth(source: &FuncSource) -> usize {
    match source {
        FuncSource::Compiled(..) => 0,
        FuncSource::Blend { from, to, .. } => 1 + blend_depth(from).max(blend_depth(to)),
    }
}

/// Flatten nested [`FuncSource::Blend`] trees into a linear list of
/// `(weight, base_source)` pairs for O(N) weighted-sum evaluation.
pub fn flatten_blend(source: &FuncSource) -> Vec<(f64, &FuncSource)> {
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
