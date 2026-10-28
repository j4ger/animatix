//! # Build Module
//!
//! AST-to-`Timeline` lowering pass.  The public entry points live in `entry/`
//! and all delegate to `Timeline::build_with_options()`.  Lowering proceeds
//! through several phases, each in its own file.
//!
//! | File | Phase |
//! |------|-------|
//! | `entry/` | Build entry points and the `BuildOptions` carrier |
//! | `colorscheme.rs` | Colorscheme seeding, config, auto-color |
//! | `node.rs` | Scene-graph node creation |
//! | `container.rs` | Container metadata, layout, inline items |
//! | `property.rs` | Property extraction, graph coordinate mapping |
//! | `shape.rs` | Vector shape state construction |
//! | `actor/` | Actor declaration lifecycle, VelloPath generation, keyframes |
//! | `process.rs` | Main `process_body` AST statement dispatcher |
//! | `plot/` | Plot actor lowering, graph/field/curve/bar path building |
//! | `post.rs` | Post-build passes: idle tracks, namespace exports |
//! | `keyframe_utils.rs` | Start/delay keyframe snapshots (end values live in `actor/helpers.rs`) |

pub use entry::{BuildOptions, CarryInput};
pub(super) use keyframe_utils::{insert_start_keyframes, preserve_delayed_values};
pub(super) use plot::{PlotCurveParams, build_graph_axis_paths, build_plot_curve_paths};
pub(crate) use process::{bind_loop_var, pattern_matches, remove_loop_vars};

use super::*;
use crate::timeline::plot::PlotCurveKind;

mod actor;
mod colorscheme;
mod container;
mod defines;
mod effect;
mod entry;
mod keyframe_utils;
mod node;
pub(crate) mod plot;
mod post;
mod process;
mod property;
pub(crate) mod referenced_roots;
mod shape;
pub(crate) mod speech;
pub(crate) mod utils;

/// Extracted actor properties parsed from an actor declaration's props.
/// Used internally by the build pipeline for graph/plot/container dispatch.
#[derive(Clone, Debug)]
struct ExtractedActorProperties {
    initial_size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    t_domain: [f64; 2],
    func: Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    tolerance: f64,
    max_depth: f64,
    resolution: f64,
    kind: Option<PlotCurveKind>,
    at_expr: Option<Expr>,
    anchor_expr: Option<Expr>,
    offset_expr: Option<Expr>,
    /// Graph plot-area padding [left, right, top, bottom] in pixels.
    graph_padding: [f64; 4],
    /// X-axis scale type.
    x_scale: utils::ScaleType,
    /// Y-axis scale type.
    y_scale: utils::ScaleType,
}
