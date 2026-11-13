//! Plot declaration parsing and the `Timeline::process_plot_actor` lowering
//! entry point.

use super::super::property::PlotPropBag;
use super::super::*;
use super::{
    BarChartLayout, PlotActorProps, ProcessedPlotActor, TickLabelData, build_number_plane_paths,
    curves, ticks,
};
use crate::ast::{InlineItem, Property};
use crate::timeline::plot::PlotCurveKind;

/// Apply the declaration's non-bag plot property overrides onto `result`, whose
/// fields already hold the declaration and track defaults.
fn apply_plot_props(
    primitive: &PrimitiveFamilyDescriptor,
    props: &[Property],
    initial_eval_env: &Environment,
    label: &str,
    diagnostics: &mut Vec<Diagnostic>,
    result: &mut PlotActorProps,
) {
    for prop in props {
        if PlotPropBag::handles(&prop.name) {
            continue; // parsed once above into the bag
        }
        let prop_subject = format!("{}.{}", label, prop.name);
        match prop.name.as_str() {
            "color" => {
                // resolve_color_in_env handles colorscheme tokens (Vec4),
                // named colors, and tuples — the old `if let Value::Color`
                // match silently rejected every token reference.
                if let Ok(Some(c)) = resolve_color_in_env(&prop.value, initial_eval_env) {
                    result.color = c;
                    // Plot curves draw with their stroke: an authored
                    // `color:` drives the stroke too (mirrors the generic
                    // actor pipeline's plot-curve linkage).
                    if primitive.is_plot_curve() {
                        result.stroke_color = c;
                    }
                }
            },
            "stroke" | "stroke_color" => {
                if let Ok(Some(c)) = resolve_color_in_env(&prop.value, initial_eval_env) {
                    result.stroke_color = c;
                }
            },
            "stroke_width" | "width" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(0.0));
                result.stroke_width = v.as_num() as f32;
            },
            "stroke_progress" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(1.0));
                if let Value::Num(n) = v {
                    result.stroke_progress = n.clamp(0.0, 1.0) as f32;
                }
            },
            "density" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(16.0));
                result.density = v.as_num().max(2.0).round();
            },
            "levels" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(0.0));
                match v {
                    Value::List(items) => {
                        result.levels = items.iter().map(|item| item.as_num()).collect();
                    },
                    Value::Num(n) => {
                        result.levels.push(n);
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
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(0.0));
                result.grid = v.as_bool();
            },
            "ticks" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Num(0.0));
                result.ticks = v.as_bool();
            },
            "tick_labels" => {
                let v = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                    .unwrap_or(Value::Str("auto".to_string()));
                result.tick_labels = v.as_str().to_lowercase();
            },
            "x_range" => {
                match plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject) {
                    Some(Value::Vec3([min, max, step])) => result.x_range = [min, max, step],
                    Some(v) => tracing::warn!(
                        "{}: 'x_range' expects a (min, max, step) triple, got {:?}",
                        prop_subject,
                        v
                    ),
                    None => {}, // eval error already reported as a diagnostic
                }
            },
            "y_range" => {
                match plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject) {
                    Some(Value::Vec3([min, max, step])) => result.y_range = [min, max, step],
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
                if let Some(v) = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                {
                    result.x_scale = super::super::utils::ScaleType::from_str(&v.as_str());
                }
            },
            "y_scale" if primitive.is_graph_host() => {
                if let Some(v) = plot_prop_value(prop, initial_eval_env, diagnostics, &prop_subject)
                {
                    result.y_scale = super::super::utils::ScaleType::from_str(&v.as_str());
                }
            },
            // Plot-builder-specific keys only: the shared bag and the generic
            // actor pipeline own everything else.
            _ => {},
        }
    }
}

/// Evaluate a plot declaration property, keeping the lookup diagnostic internal.
fn plot_prop_value(
    prop: &Property,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<Value> {
    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, subject)
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
        existing_track_id: EntityId,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> PlotActorProps {
        let PlotPropBag {
            initial_size,
            numeric_size_axes: _,
            x_domain,
            y_domain,
            t_domain,
            func,
            tolerance,
            max_depth,
            resolution,
            kind,
        } = PlotPropBag::from_props(props, label, initial_eval_env, diagnostics);
        let kind = kind.unwrap_or(PlotCurveKind::Cartesian);

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
        let caps = self.world.caps(existing_track_id);
        let style = self.world.style(existing_track_id);
        let color = style.color.last(DEFAULT_WHITE);
        let default_stroke = caps.default_stroke_width();
        let stroke_width = style.stroke_width.last(default_stroke);
        let mut stroke_color = style.stroke_color.last(DEFAULT_WHITE);
        let stroke_progress = style.stroke_progress.last(1.0);

        // If stroke_color was not explicitly set on track, check colorscheme's default stroke
        if stroke_color == DEFAULT_WHITE
            && let Some(primitive_info) = self.primitive_registry.find(ty)
            && let Some(scheme_stroke) = self.get_default_color(primitive_info, &caps, "stroke")
        {
            stroke_color = scheme_stroke;
        }

        let mut result = PlotActorProps {
            x_domain,
            y_domain,
            t_domain,
            func,
            initial_size,
            tolerance,
            max_depth,
            resolution,
            kind,
            density: 16.0,
            levels: vec![],
            grid: false,
            ticks: false,
            tick_labels: String::from("auto"),
            x_scale: super::super::utils::ScaleType::Linear,
            y_scale: super::super::utils::ScaleType::Linear,
            x_range: [-10.0, 10.0, 2.0],
            y_range: [-10.0, 10.0, 2.0],
            graph_padding,
            color,
            stroke_width,
            stroke_color,
            stroke_progress,
        };

        apply_plot_props(primitive, props, initial_eval_env, label, diagnostics, &mut result);

        result
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
    pub(in crate::timeline::build) fn process_plot_actor(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        parent_label: Option<&str>,
        children: &[InlineItem],
        diagnostics: &mut Vec<Diagnostic>,
        existing_track_id: EntityId,
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
            existing_track_id,
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
        let existing_size = self.world.geometry(existing_track_id).size.last(default_size);
        let size = if existing_size != default_size {
            existing_size
        } else {
            parsed.initial_size
        };
        let existing_shape = self
            .world
            .get_id(label)
            .map(|id| self.world.shape_or_default(id))
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
