//! Plot actor lowering.
//!
//! Declaration property parsing and the `Timeline::process_plot_actor` entry
//! point live in `writer.rs`; each plot family lives in its own sibling module:
//!
//! | Module | Contents |
//! |--------|----------|
//! | `writer.rs` | Declaration property parsing and the plot-actor lowering entry point |
//! | `ticks.rs` | Nice-number ticks, axis normalisation, tick-label formatting/placement |
//! | `graph.rs` | Graph axis/grid/ticks, NumberPlane paths, `map`/`map_inverse` |
//! | `fields.rs` | VectorField/Heatmap/ContourSet sampling and path builders |
//! | `bars.rs` | BarChart data parsing, layout resolution and bar paths |
//! | `curves.rs` | Plot-curve sampling and the curve plot-actor builder |

mod bars;
mod curves;
mod fields;
mod graph;
mod ticks;
mod writer;

// The module is the façade for the former `build/plot.rs`; keep its full
// `build::plot::*` surface even where only the plot module itself uses it.
#[allow(unused_imports)]
pub(crate) use bars::{
    BarChartLayout, parse_bar_chart_data, parse_bar_chart_data_expr, resolve_bar_chart_layout,
};
pub(crate) use curves::{PlotCurveParams, build_plot_curve_paths};
pub(crate) use fields::{build_contour_set_paths, build_heatmap_paths, build_vector_field_paths};
pub(crate) use graph::{build_graph_axis_paths, build_number_plane_paths};
pub(crate) use ticks::{TickLabelData, format_axis_tick_label};

use super::*;
use crate::timeline::plot::{PlotCurveKind, ProceduralPlot};
use crate::timeline::vello_path::VelloPath;

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
    /// For a `BarChart`: the layout and dataset the declaration resolved to, kept
    /// so a `data = {…}` assignment can rebuild the bars per frame.
    pub bar_layout: Option<BarChartLayout>,
    pub bar_data: Vec<(String, f32)>,
}

/// Parsed declaration state shared by every plot-family builder.
pub(super) struct PlotActorProps {
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    t_domain: [f64; 2],
    func: Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    initial_size: [f32; 2],
    tolerance: f64,
    max_depth: f64,
    resolution: f64,
    kind: PlotCurveKind,
    density: f64,
    levels: Vec<f64>,
    grid: bool,
    ticks: bool,
    tick_labels: String,
    x_scale: super::utils::ScaleType,
    y_scale: super::utils::ScaleType,
    x_range: [f64; 3],
    y_range: [f64; 3],
    graph_padding: [f64; 4],
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    stroke_progress: f32,
}
