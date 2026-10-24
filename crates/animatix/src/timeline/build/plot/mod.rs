//! Plot actor lowering.
//!
//! Declaration property parsing and the `Timeline::process_plot_actor` entry
//! point live here; each plot family lives in its own sibling module:
//!
//! | Module | Contents |
//! |--------|----------|
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
use crate::ast::{InlineItem, Property};
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

impl Timeline {
    /// Parse the declaration's plot properties into [`PlotActorProps`].
    #[allow(clippy::too_many_arguments)]
    fn parse_plot_props(
        &self,
        primitive: &PrimitiveFamilyDescriptor,
        ty: &str,
        label: &str,
        props: &[Property],
        initial_eval_env: &Environment,
        existing_track: &AnimationTrack,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> PlotActorProps {
        let mut x_domain = [-10.0, 10.0];
        let mut y_domain = [-10.0, 10.0];
        let mut t_domain = [0.0, std::f64::consts::TAU];
        let mut func = None;
        let mut initial_size = DEFAULT_LAYOUT_HALF_SIZE;
        let mut tolerance = 0.5;
        let mut max_depth = 10.0;
        let mut resolution = 96.0;
        let mut kind = PlotCurveKind::Cartesian;
        let mut density = 16.0;
        let mut levels: Vec<f64> = vec![];
        let mut grid = false;
        let mut ticks = false;
        let mut tick_labels = String::from("auto");
        let mut x_scale = super::utils::ScaleType::Linear;
        let mut y_scale = super::utils::ScaleType::Linear;
        let mut x_range = [-10.0, 10.0, 2.0];
        let mut y_range = [-10.0, 10.0, 2.0];

        // Graph plot-area padding [left, right, top, bottom] in pixels.
        // Pre-scan props for padding so it's available before the main match loop.
        let graph_padding: [f64; 4] = if primitive.is_graph_host() {
            props
                .iter()
                .find(|p| p.name == "padding")
                .and_then(|p| {
                    match crate::timeline::utils::evaluate_expr(&p.value, initial_eval_env) {
                        Ok(Value::Vec4([l, r, t, b])) => Some([l, r, t, b]),
                        Ok(Value::Num(n)) => Some([n, n, n, n]),
                        _ => None,
                    }
                })
                .unwrap_or([0.0; 4])
        } else {
            [0.0; 4]
        };

        // Start with track defaults, override from props.
        let caps = self.world.caps(existing_track.id);
        let style = self.world.style(existing_track.id);
        let mut color = style.color.last(DEFAULT_WHITE);
        let default_stroke = caps.default_stroke_width();
        let mut stroke_width = style.stroke_width.last(default_stroke);
        let mut stroke_color = style.stroke_color.last(DEFAULT_WHITE);
        let mut stroke_progress = style.stroke_progress.last(1.0);

        // If stroke_color was not explicitly set on track, check colorscheme's default stroke
        if stroke_color == DEFAULT_WHITE {
            if let Some(primitive_info) = self.primitive_registry.find(ty) {
                if let Some(scheme_stroke) = self.get_default_color(primitive_info, &caps, "stroke")
                {
                    stroke_color = scheme_stroke;
                }
            }
        }

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "size" => {
                    let size_val = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    if let Value::Vec2([w, h]) = size_val {
                        initial_size[0] = w as f32 / 2.0;
                        initial_size[1] = h as f32 / 2.0;
                    }
                },
                "radius" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    let r = v.as_num() as f32;
                    initial_size = [r, r];
                },
                "x_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => x_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 'x_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "y_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => y_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 'y_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "t_domain" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec2([min, max])) => t_domain = [min, max],
                        Some(v) => tracing::warn!(
                            "{}: 't_domain' expects a (min, max) tuple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "func" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Closure(args, body, captures)) => {
                            func = Some((args, body, captures))
                        },
                        Some(v) => tracing::warn!(
                            "{}: 'func' expects a closure, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "color" => {
                    // resolve_color_in_env handles colorscheme tokens (Vec4),
                    // named colors, and tuples — the old `if let Value::Color`
                    // match silently rejected every token reference.
                    if let Ok(Some(c)) = resolve_color_in_env(&prop.value, initial_eval_env) {
                        color = c;
                        // Plot curves draw with their stroke: an authored
                        // `color:` drives the stroke too (mirrors the generic
                        // actor pipeline's plot-curve linkage).
                        if primitive.is_plot_curve() {
                            stroke_color = c;
                        }
                    }
                },
                "stroke" | "stroke_color" => {
                    if let Ok(Some(c)) = resolve_color_in_env(&prop.value, initial_eval_env) {
                        stroke_color = c;
                    }
                },
                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_width = v.as_num() as f32;
                },
                "stroke_progress" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0));
                    if let Value::Num(n) = v {
                        stroke_progress = n.clamp(0.0, 1.0) as f32;
                    }
                },
                "tolerance" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    tolerance = v.as_num();
                },
                "max_depth" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    max_depth = v.as_num();
                },
                "resolution" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(96.0));
                    resolution = v.as_num();
                },
                "density" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(16.0));
                    density = v.as_num().max(2.0).round();
                },
                "levels" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    match v {
                        Value::List(items) => {
                            levels = items.iter().map(|item| item.as_num()).collect();
                        },
                        Value::Num(n) => {
                            levels.push(n);
                        },
                        _ => {
                            tracing::warn!(
                                "{}: 'levels' expects a number or list of numbers, got {:?}",
                                prop_subject,
                                v
                            );
                        },
                    }
                },
                "grid" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    grid = v.as_bool();
                },
                "ticks" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    ticks = v.as_bool();
                },
                "tick_labels" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Str("auto".to_string()));
                    tick_labels = v.as_str().to_lowercase();
                },
                "kind" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        if let Some(k) = PlotCurveKind::from_str(&v.as_str().to_lowercase()) {
                            kind = k;
                        } else {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!("Invalid plot kind: '{}'", v.as_str()),
                                )
                                .with_subject(&prop_subject),
                            );
                        }
                    }
                },
                "x_range" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec3([min, max, step])) => x_range = [min, max, step],
                        Some(v) => tracing::warn!(
                            "{}: 'x_range' expects a (min, max, step) triple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "y_range" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::Vec3([min, max, step])) => y_range = [min, max, step],
                        Some(v) => tracing::warn!(
                            "{}: 'y_range' expects a (min, max, step) triple, got {:?}",
                            prop_subject,
                            v
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "padding" if primitive.is_graph_host() => {
                    // Graph padding is pre-computed above; skip here to prevent
                    // falling through to the custom plot-param collector.
                },
                "x_scale" if primitive.is_graph_host() => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        x_scale = super::utils::ScaleType::from_str(&v.as_str());
                    }
                },
                "y_scale" if primitive.is_graph_host() => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        initial_eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        y_scale = super::utils::ScaleType::from_str(&v.as_str());
                    }
                },
                _ => {}, /* Non-plot properties (color, stroke, etc.) are handled by the general
                          * actor pipeline. */
            }
        }

        PlotActorProps {
            x_domain,
            y_domain,
            t_domain,
            func,
            initial_size,
            tolerance,
            max_depth,
            resolution,
            kind,
            density,
            levels,
            grid,
            ticks,
            tick_labels,
            x_scale,
            y_scale,
            x_range,
            y_range,
            graph_padding,
            color,
            stroke_width,
            stroke_color,
            stroke_progress,
        }
    }

    /// Collect custom numeric properties as frame-time plot parameters.
    fn collect_plot_params(
        &self,
        props: &[Property],
        time_ms: f64,
        has_func: bool,
    ) -> Vec<(String, f64)> {
        // Collect custom numeric params from declaration props. Done for every
        // func-backed plot so keyframeable params get a procedural_plot even
        // when the body does not reference `t`.
        let mut plot_params: Vec<(String, f64)> = Vec::new();
        if has_func {
            for prop in props {
                // Skip known plot properties
                match prop.name.as_str() {
                    "kind" | "func" | "x_domain" | "y_domain" | "t_domain" | "tolerance"
                    | "max_depth" | "resolution" | "density" | "levels" | "grid" | "ticks"
                    | "tick_labels" | "x_range" | "y_range" | "size" | "at" | "position"
                    | "color" | "opacity" | "stroke" | "stroke_width" | "stroke_progress"
                    | "fill_opacity" | "radius_x" | "radius_y" | "from" | "to" | "head_size"
                    | "text" | "code" | "font_size" | "font_family" | "url" | "source"
                    | "volume" | "anchor" | "offset" | "rotation" | "scale" | "transform"
                    | "blur" | "brightness" | "contrast" | "saturate" | "hue_rotate" | "sepia"
                    | "gap" | "padding" | "align" | "cols" | "data" | "bar_width"
                    | "bar_colors" | "direction" | "max_value" | "show_axis" | "show_labels" => {},
                    _ => {
                        // Treat unknown numeric props as plot parameters
                        let eval_env = self.build_eval_env(time_ms as u64);
                        if let Ok(Value::Num(n)) = evaluate_expr(&prop.value, &eval_env) {
                            plot_params.push((prop.name.clone(), n));
                        }
                    },
                }
            }
        }
        plot_params
    }

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
    ) -> Option<ProcessedPlotActor> {
        let primitive = self
            .primitive_registry
            .info_of(ty)
            .map(PrimitiveFamilyDescriptor::from_info)
            .unwrap_or_default();
        if !primitive.is_graph_host() && !primitive.is_plot() {
            return None;
        }

        use animatix_core::caps::PlotKind;
        let is_number_plane = primitive.plot == Some(PlotKind::NumberPlane);
        let initial_eval_env = self.build_eval_env(time_ms as u64);
        let parsed = self.parse_plot_props(
            &primitive,
            ty,
            label,
            props,
            &initial_eval_env,
            existing_track,
            diagnostics,
        );

        curves::validate_plot_func(
            &primitive,
            ty,
            label,
            &parsed.func,
            parsed.kind,
            &initial_eval_env,
            diagnostics,
        );

        if primitive.is_graph_host() {
            self.register_graph_host_env(label, &parsed);
        }

        self.process_inline_items(time_ms, children, label, diagnostics);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, std::f32::consts::PI];
        // Prefer the size parsed from this declaration. On first declaration
        // the pre-declaration track snapshot below still holds the [50, 50]
        // default, which made standalone plot builders (BarChart, VectorField,
        // Heatmap, ContourSet, NumberPlane) lay out in a ~40x40 box regardless
        // of `size:`. Re-declarations keep inheriting the stored track size.
        let existing_size = self.world.geometry(existing_track.id).size.last(default_size);
        let size = if existing_size != default_size {
            existing_size
        } else {
            parsed.initial_size
        };
        let existing_shape = self
            .world
            .get_id(label)
            .map(|id| self.world.shape(id))
            .cloned()
            .unwrap_or_default();
        let line_from = existing_shape.line_from.last([-50.0, 0.0]);
        let line_to = existing_shape.line_to.last([50.0, 0.0]);
        let arc_angles = existing_shape.arc_angles.last(default_arc);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let fill_opacity = 0.0f32;

        let mut vello_paths = vec![];
        let mut procedural_plot = None;
        let mut tick_label_data = TickLabelData::default();
        let mut bar_layout: Option<BarChartLayout> = None;
        let mut bar_data: Vec<(String, f32)> = Vec::new();

        let plot_params = self.collect_plot_params(props, time_ms, parsed.func.is_some());

        let is_vector_field = primitive.plot == Some(PlotKind::VectorField);
        let is_heatmap = primitive.plot == Some(PlotKind::Heatmap);
        let is_contour_set = primitive.plot == Some(PlotKind::ContourSet);
        let is_bar_chart = primitive.plot == Some(PlotKind::BarChart);

        if primitive.is_graph_host() {
            let label_x = ticks::tick_labels_has_axis(&parsed.tick_labels, 'x');
            let label_y = ticks::tick_labels_has_axis(&parsed.tick_labels, 'y');

            vello_paths = build_graph_axis_paths(
                size,
                parsed.x_domain,
                parsed.y_domain,
                parsed.stroke_color,
                parsed.grid,
                parsed.ticks,
                label_x || label_y,
                parsed.graph_padding,
                parsed.x_scale,
                parsed.y_scale,
            );

            tick_label_data = ticks::compute_graph_tick_labels(
                size,
                parsed.x_domain,
                parsed.y_domain,
                parsed.x_scale,
                parsed.y_scale,
                parsed.graph_padding,
                &parsed.tick_labels,
            );
        } else if is_vector_field || is_heatmap || is_contour_set {
            let (paths, procedural) = self.build_field_plot_paths(
                is_vector_field,
                is_heatmap,
                is_contour_set,
                label,
                &parsed,
                size,
                time_ms,
                &plot_params,
            );
            vello_paths = paths;
            procedural_plot = procedural;
        } else if is_number_plane {
            vello_paths = build_number_plane_paths(
                size,
                parsed.x_domain,
                parsed.y_domain,
                parsed.x_range,
                parsed.y_range,
                parsed.stroke_color,
            );
        } else if is_bar_chart {
            let (layout, data, paths, bar_labels) = self.build_bar_chart_plot(
                label,
                parent_label,
                props,
                &parsed,
                size,
                &initial_eval_env,
                diagnostics,
            );
            bar_layout = Some(layout);
            bar_data = data;
            vello_paths = paths;
            tick_label_data.bar_labels = bar_labels;
        } else if primitive.is_plot_curve() {
            let (paths, procedural) = self.build_curve_plot_paths(
                label,
                parent_label,
                &parsed,
                &plot_params,
                time_ms,
                diagnostics,
            );
            vello_paths = paths;
            procedural_plot = procedural;
        }

        Some(ProcessedPlotActor {
            initial_size: parsed.initial_size,
            line_from,
            line_to,
            arc_angles,
            color: parsed.color,
            stroke_width: parsed.stroke_width,
            stroke_color: parsed.stroke_color,
            stroke_progress: parsed.stroke_progress,
            fill_opacity,
            shape_type,
            vello_paths,
            procedural_plot,
            bar_layout,
            bar_data,
            tick_label_data: if (primitive.is_graph_host()
                || primitive.plot == Some(animatix_core::caps::PlotKind::BarChart))
                && (!tick_label_data.x_labels.is_empty()
                    || !tick_label_data.y_labels.is_empty()
                    || !tick_label_data.bar_labels.is_empty())
            {
                Some(tick_label_data)
            } else {
                None
            },
        })
    }
}
