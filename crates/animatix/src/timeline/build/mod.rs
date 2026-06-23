//! # Build Module
//!
//! AST-to-`Timeline` lowering pass.  The public entry point is `Timeline::build()`
//! (via `entry.rs`).  Lowering proceeds through several phases, each in its own file.
//!
//! | File | Phase |
//! |------|-------|
//! | `entry.rs` | Build entry points: `build()`, `build_with_diagnostics()` |
//! | `colorscheme.rs` | Colorscheme seeding, config, auto-color |
//! | `node.rs` | Scene-graph node creation |
//! | `container.rs` | Container metadata, layout, inline items |
//! | `property.rs` | Property extraction, graph coordinate mapping |
//! | `shape.rs` | Vector shape state construction |
//! | `actor.rs` | Actor declaration processing, VelloPath generation, keyframes |
//! | `process.rs` | Main `process_body` AST statement dispatcher |
//! | `plot.rs` | Plot curve/axis path building (pre-existing) |
//! | `keyframe_utils.rs` | Keyframe insertion helpers (pre-existing) |

use super::*;
pub(super) use keyframe_utils::{insert_end_keyframes, insert_start_keyframes, preserve_delayed_values};
pub(super) use plot::{build_graph_axis_paths, build_plot_curve_paths, PlotCurveParams};
use crate::timeline::plot::PlotCurveKind;

mod actor;
mod colorscheme;
mod container;
mod entry;
mod keyframe_utils;
mod node;
mod plot;
mod process;
mod property;
mod shape;
mod utils;

/// Extracted actor properties parsed from an actor declaration's props.
/// Used internally by the build pipeline for graph/plot/container dispatch.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Reserved for layout-extraction hooks
struct ExtractedActorProperties {
    initial_size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    t_domain: [f64; 2],
    func: Option<(Vec<String>, Box<Expr>)>,
    tolerance: f64,
    max_depth: f64,
    resolution: f64,
    kind: Option<PlotCurveKind>,
    at_expr: Option<Expr>,
    anchor_expr: Option<Expr>,
    offset_expr: Option<Expr>,
    gap: f32,
    padding: f32,
    /// Graph plot-area padding [left, right, top, bottom] in pixels.
    graph_padding: [f64; 4],
    /// X-axis scale: "linear" (default) or "log".
    x_scale: String,
    /// Y-axis scale: "linear" (default) or "log".
    y_scale: String,
    align: Option<String>,
    cols: Option<usize>,
}