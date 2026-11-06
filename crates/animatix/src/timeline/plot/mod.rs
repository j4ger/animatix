//! # Non-Interpolatable Property Transitions (Side-Channel Pattern)
//!
//! `func` on plot actors cannot use `PropertyTrack<T>` because closures cannot
//! implement [`crate::timeline::Interpolate`]. `FuncTransition` records the
//! range, easing, blend mode, and `from`/`to` sources in a parallel `Vec` on the
//! plot payload; frame sampling evaluates both sides and blends outputs (or
//! opacity). See `PlotPayload::func_transitions` for the checklist.

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
#[allow(unused_imports)]
pub use implicit::{
    FastImplicitEvaluator, build_implicit_plot_path_from_source,
    build_implicit_plot_path_from_source_cached, eval_implicit_source, implicit_intersection,
};
#[allow(unused_imports)]
pub(crate) use procedural::compile_fast_plot_program;
#[allow(unused_imports)]
pub use procedural::{
    FastExprKind, FastPlotProgram, ProceduralPlot, ProceduralPlotKind, sample_plot_source_cached,
    sample_procedural_plot_at, sample_procedural_plot_at_cached,
};

mod api;

// The frame-time evaluation helpers keep their `timeline::plot::*` paths; this
// module is the façade over them and the sibling sampling modules.
#[cfg(test)]
pub use api::resolve_func_source;
#[allow(unused_imports)]
pub use api::{
    bar_data_at, blend_depth, eval_compiled_parts_scalar, eval_compiled_parts_vec2, eval_scalar,
    eval_source_scalar, eval_source_vec2, eval_vec2, flatten_blend, interpolate_bar_data,
};

use super::modifier_runtime::ir::CompiledExpr;
use super::{CapturedEnv, Environment, Value};
use crate::easing::Easing;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
/// Discriminant for the four sampling strategies of `PlotCurve`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PlotCurveKind {
    Cartesian,
    Polar,
    Parametric,
    Implicit,
}

/// Source of a function for a `PlotCurve` `func` transition.
///
/// Part of the side-channel pattern for non-interpolatable properties (see
/// [`PlotPayload::func_transitions`](crate::primitives::PlotPayload::func_transitions)
/// and the module docs). `Compiled` is the steady-state closure; `Blend` freezes
/// a mid-flight `(from, to, progress)` so cascading transitions do not jump.
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
/// The unit of the `func` side-channel: stored in a parallel
/// `Vec<FuncTransition>` on the plot payload instead of a `PropertyTrack<T>`.
/// The build appends one per `func = <expr> [easing, duration]` keyframe; frame
/// sampling calls [`active_at`] and blends by [`FuncBlendMode`];
/// [`is_complete_at`] identifies the baseline `to` between transitions.
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

/// One `data = {…}` assignment on a `BarChart`: datasets before/after, the
/// window, and easing. Bars match by label, so the blend happens on each bar's
/// height (the outputs), not on the property.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct BarDataTransition {
    pub start_ms: u64,
    pub end_ms: u64,
    pub easing: Easing,
    pub from: Vec<(String, f32)>,
    pub to: Vec<(String, f32)>,
}

/// Function reference used during per-frame adaptive sampling. `Single` reuses
/// the one-function path; `Blended` lerps both outputs at `progress`.
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
