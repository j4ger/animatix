//! BarChart data parsing, layout resolution and bar path generation.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::PlotActorProps;
use super::graph::format_float;
use super::*;
use crate::ast::{Expr, Property};

/// Parse bar chart data from a `data` property value.
///
/// Supports:
/// - `((2, 1.0), (5, 0.55), (9, 0.3))` — numeric keys (auto-labeled as "2", "5", "9")
/// - `(("2 Hz", 1.0), ("5 Hz", 0.55), ("9 Hz", 0.3))` — string-labeled tuples
/// - `()` — empty data
pub(crate) fn parse_bar_chart_data(
    props: &[Property],
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> Vec<(String, f32)> {
    let mut data: Vec<(String, f32)> = Vec::new();

    for prop in props {
        if prop.name != "data" {
            continue;
        }
        data = parse_bar_chart_data_expr(&prop.value, diagnostics, label);
        break; // Only the first `data` property wins, as before
    }
    data
}

/// The `data` list itself, split out so a `data = {…}` *assignment* can be parsed
/// by exactly the same rules as the declaration — same auto-labelled flat list,
/// same `(label, value)` tuples, same diagnostics.
pub(crate) fn parse_bar_chart_data_expr(
    expr: &Expr,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> Vec<(String, f32)> {
    let mut data: Vec<(String, f32)> = Vec::new();
    {
        // Expect a list (outer list of bars)
        if let Expr::List(items) = expr {
            // Flat number list? E.g. {10, 20, 30} — auto-label with 1-based indices.
            if !items.is_empty() && items.iter().all(|item| matches!(item, Expr::Num(_))) {
                for (i, item) in items.iter().enumerate() {
                    if let Expr::Num(n) = item {
                        data.push(((i + 1).to_string(), *n as f32));
                    }
                }
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{}' data: flat number list detected — auto-labeling with 1-based indices. \
                         To specify custom labels, use (label, value) tuples like ('A', 10), ('B', 20)",
                        label
                    ),
                ));
            } else {
                for item in items {
                    match item {
                        Expr::Tuple(bar) if bar.len() == 2 => {
                            let key_str = match &bar[0] {
                                Expr::Num(n) => format_float(*n),
                                Expr::Str(s) => s.clone(),
                                _ => {
                                    diagnostics.push(Diagnostic::warning(
                                        DiagnosticCode::InvalidPropertyValue,
                                        DiagnosticPhase::Build,
                                        format!(
                                            "BarChart '{}' data key must be a number or string",
                                            label
                                        ),
                                    ));
                                    continue;
                                },
                            };
                            let val = match &bar[1] {
                                Expr::Num(n) => *n as f32,
                                _ => {
                                    diagnostics.push(Diagnostic::warning(
                                        DiagnosticCode::InvalidPropertyValue,
                                        DiagnosticPhase::Build,
                                        format!("BarChart '{}' data value must be a number", label),
                                    ));
                                    continue;
                                },
                            };
                            data.push((key_str, val));
                        },
                        _ => {
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "BarChart '{}' data entry must be a (key, value) tuple",
                                    label
                                ),
                            ));
                        },
                    }
                }
            }
        }
    }

    data
}

/// Build VelloPaths for a BarChart.
///
/// Produces one filled-rectangle path per bar (in declaration order),
/// optionally preceded by an axis line path.
///
/// # Standalone mode
/// Bars are positioned within the chart's `size` bounds, centered on the actor.
/// Gap between bars is auto-calculated from `(plot_width - bar_count * bar_width) / (bar_count +
/// 1)`.
///
/// # Graph child mode
/// Bars use math-coordinate mapping via `p_x_domain`, `p_y_domain`, `p_size`.
/// Labels are in math-space but rendered as child Text tracks (handled by the caller).
/// The layout parameters a `BarChart` needs to turn a dataset into paths.
///
/// Every field is resolved once at build from the declaration's properties (see
/// `resolve_bar_chart_layout`); `paths_for` is the pure geometry half, so a
/// frame-time caller can rebuild the bars for a different dataset without
/// re-parsing properties or re-evaluating expressions.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug)]
pub(crate) struct BarChartLayout {
    /// Half-extents, the same convention every shape actor carries.
    size: [f32; 2],
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    /// `Some` when the chart is a `Graph` child, which maps math coordinates
    /// rather than pixels.
    parent_size: Option<[f64; 2]>,
    bar_width_auto: bool,
    bar_width_val: f32,
    gap_auto: bool,
    gap_val: f32,
    bar_colors_auto: bool,
    bar_colors: Vec<[f32; 4]>,
    show_axis: bool,
    show_labels: bool,
    max_value_auto: bool,
    max_value_val: f32,
}

/// The pixel frame and math domain resolved for one [`BarChartLayout::paths_for`]
/// call: math coordinates when hosted by a `Graph`, pixel bounds otherwise.
struct BarPlotGeometry {
    use_math_coords: bool,
    plot_w: f64,
    plot_h: f64,
    math_x0: f64,
    math_x1: f64,
    math_y1: f64,
    baseline_y: f64,
}

impl BarChartLayout {
    /// Whether this chart compiles bar captions at build. A chart that shows
    /// none has nothing that can drift out of position, so the caption guard on
    /// a `data` assignment stays quiet for it.
    pub(crate) fn has_captions(&self) -> bool {
        self.show_labels
    }

    /// The bars, the axis line and the label anchors for `data`.
    ///
    /// Returns empty for an empty dataset: the width/gap arithmetic divides by
    /// the bar count.
    pub(crate) fn paths_for(
        &self,
        data: &[(String, f32)],
    ) -> (Vec<VelloPath>, Vec<(f64, f64, String)>) {
        if data.is_empty() {
            return (vec![], vec![]);
        }
        let mut bar_labels: Vec<(f64, f64, String)> = Vec::new();
        let n = data.len() as f32;
        let full_w = (self.size[0] * 2.0) as f64;
        let full_h = (self.size[1] * 2.0) as f64;

        // Graph child mode maps math coordinates; standalone mode uses the
        // pixel bounds within `self.size`.
        let geom = self.plot_geometry(full_w, full_h);
        let use_math_coords = geom.use_math_coords;

        // Auto bar width
        let n_f64 = n as f64;
        let bw = self.bar_width(n_f64, &geom);

        // Gap
        let n_f64 = n as f64;
        let gap = self.bar_gap(n_f64, bw, &geom);

        // Determine max value for scaling (if using math coords, domain is authoritative)
        let max_val = if !self.max_value_auto {
            self.max_value_val as f64
        } else if use_math_coords {
            geom.math_y1
        } else {
            data.iter().map(|(_, v)| *v as f64).fold(0.0f64, f64::max).max(0.001)
        };

        let mut paths: Vec<VelloPath> = Vec::with_capacity(data.len() + 1);

        // Optional axis line
        if self.show_axis {
            paths.push(self.axis_path(&geom));
        }

        // Per-bar paths
        for (i, (label_text, value)) in data.iter().enumerate() {
            let (bar, bar_center_x) = self.bar_path(i, *value, bw, gap, max_val, &geom);
            paths.push(bar);

            if self.show_labels {
                bar_labels.push((bar_center_x, geom.baseline_y + 16.0, label_text.clone()));
            }
        }

        (paths, bar_labels)
    }

    /// Resolve the plot frame: math-coordinate mapping when hosted by a `Graph`,
    /// otherwise pixel bounds inside `size`.
    fn plot_geometry(&self, full_w: f64, full_h: f64) -> BarPlotGeometry {
        if let Some(p_size) = self.parent_size {
            // Inside a Graph — map math coordinates to pixels relative to actor center
            let pw = p_size[0];
            let ph = p_size[1];
            let baseline_norm = if self.y_domain[0] <= 0.0 && self.y_domain[1] >= 0.0 {
                // Baseline at y=0 in math coords
                (0.0 - self.y_domain[0]) / (self.y_domain[1] - self.y_domain[0])
            } else {
                // Baseline at min y
                0.0
            };
            // In local coordinates centered at (0, 0), Y points down,
            // so normalized y=0 is at +ph/2 (bottom) and y=1 is at -ph/2 (top).
            let baseline = (0.5 - baseline_norm) * ph;
            BarPlotGeometry {
                use_math_coords: true,
                plot_w: pw,
                plot_h: ph,
                math_x0: self.x_domain[0],
                math_x1: self.x_domain[1],
                math_y1: self.y_domain[1],
                baseline_y: baseline,
            }
        } else {
            // Standalone — pixel coordinates within self.size bounds
            let plot_left = -(full_w / 2.0) + 40.0; // margin for labels
            let plot_right = full_w / 2.0 - 20.0;
            let plot_top = -(full_h / 2.0) + 20.0;
            let plot_bottom = full_h / 2.0 - 40.0; // margin for labels
            BarPlotGeometry {
                use_math_coords: false,
                plot_w: plot_right - plot_left,
                plot_h: plot_bottom - plot_top,
                math_x0: plot_left,
                math_x1: plot_right,
                math_y1: plot_bottom,
                baseline_y: plot_bottom, // baseline at bottom
            }
        }
    }

    /// Bar width in pixels: auto-distributed, or the declared value converted
    /// from math x-units in graph mode.
    fn bar_width(&self, n_f64: f64, geom: &BarPlotGeometry) -> f64 {
        if self.bar_width_auto {
            let total_gap = self.gap_auto as usize as f64 * n_f64 * 4.0;
            ((geom.plot_w - total_gap) / n_f64).max(4.0)
        } else if geom.use_math_coords {
            // In graph mode, bar_width is in math x-units; convert to pixels
            let x_range_pixels = geom.plot_w;
            let x_range_math = geom.math_x1 - geom.math_x0;
            if x_range_math > 0.0 {
                self.bar_width_val as f64 * x_range_pixels / x_range_math
            } else {
                self.bar_width_val as f64
            }
        } else {
            self.bar_width_val as f64
        }
    }

    /// Gap between bars in pixels, never below 1.0.
    fn bar_gap(&self, n_f64: f64, bw: f64, geom: &BarPlotGeometry) -> f64 {
        let gap = if self.gap_auto {
            if n_f64 <= 1.0 {
                0.0
            } else {
                (geom.plot_w - bw * n_f64) / (n_f64 + 1.0)
            }
        } else if geom.use_math_coords {
            let x_range_pixels = geom.plot_w;
            let x_range_math = geom.math_x1 - geom.math_x0;
            if x_range_math > 0.0 {
                self.gap_val as f64 * x_range_pixels / x_range_math
            } else {
                self.gap_val as f64
            }
        } else {
            self.gap_val as f64
        };
        gap.max(1.0)
    }

    /// The optional baseline axis line, styled with the layout's stroke.
    fn axis_path(&self, geom: &BarPlotGeometry) -> VelloPath {
        let mut axis = kurbo::BezPath::new();
        if geom.use_math_coords {
            let ax0 = -(geom.plot_w / 2.0);
            let ax1 = geom.plot_w / 2.0;
            axis.move_to((ax0, geom.baseline_y));
            axis.line_to((ax1, geom.baseline_y));
        } else {
            axis.move_to((geom.plot_w / 2.0, geom.baseline_y));
            axis.line_to((-geom.plot_w / 2.0, geom.baseline_y));
        };
        let c = vello::peniko::Color::from_rgba8(
            (self.stroke_color[0] * 255.0) as u8,
            (self.stroke_color[1] * 255.0) as u8,
            (self.stroke_color[2] * 255.0) as u8,
            (self.stroke_color[3] * 255.0) as u8,
        );
        VelloPath {
            path: std::sync::Arc::new(axis),
            fill: None,
            stroke: Some((c, self.stroke_width.max(1.0))),
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        }
    }

    /// One bar rectangle path plus the x of its label anchor (the caller knows
    /// the baseline y).
    fn bar_path(
        &self,
        i: usize,
        value: f32,
        bw: f64,
        gap: f64,
        max_val: f64,
        geom: &BarPlotGeometry,
    ) -> (VelloPath, f64) {
        let i_f = i as f64;

        // Bar position: evenly spaced from left to right
        let bar_x_start = -(geom.plot_w / 2.0) + gap + i_f * (bw + gap);
        let bar_x_end = bar_x_start + bw;
        let bar_center_x = bar_x_start + bw / 2.0;

        // Bar height in screen coords
        let val_norm = if max_val > 0.0 {
            value as f64 / max_val
        } else {
            0.0
        };
        let bar_screen_height = val_norm * geom.plot_h;
        let bar_top_y = geom.baseline_y - bar_screen_height;

        // Build rectangle path
        let mut bp = kurbo::BezPath::new();
        bp.move_to(kurbo::Point::new(bar_x_start, geom.baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, geom.baseline_y));
        bp.line_to(kurbo::Point::new(bar_x_end, bar_top_y));
        bp.line_to(kurbo::Point::new(bar_x_start, bar_top_y));
        bp.close_path();

        // Per-bar self.color: a single `self.bar_colors` value is uniform across all
        // bars (documented at the parse site); a list assigns per bar, with
        // bars past the end of the list falling back to the actor self.color.
        let bar_c = if !self.bar_colors_auto && self.bar_colors.len() == 1 {
            self.bar_colors[0]
        } else if !self.bar_colors_auto && i < self.bar_colors.len() {
            self.bar_colors[i]
        } else {
            self.color
        };
        let fill_c = vello::peniko::Color::from_rgba8(
            (bar_c[0] * 255.0) as u8,
            (bar_c[1] * 255.0) as u8,
            (bar_c[2] * 255.0) as u8,
            (bar_c[3] * 255.0) as u8,
        );

        let bar = VelloPath {
            path: std::sync::Arc::new(bp),
            fill: Some(fill_c),
            stroke: if self.stroke_width > 0.0 {
                let sc = vello::peniko::Color::from_rgba8(
                    (self.stroke_color[0] * 255.0) as u8,
                    (self.stroke_color[1] * 255.0) as u8,
                    (self.stroke_color[2] * 255.0) as u8,
                    (self.stroke_color[3] * 255.0) as u8,
                );
                Some((sc, self.stroke_width))
            } else {
                None
            },
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        };

        (bar, bar_center_x)
    }
}

/// Resolve the declaration's bar properties into a [`BarChartLayout`], plus the
/// dataset the declaration carries.
#[allow(clippy::too_many_arguments)] // the same eleven inputs the builder took before the split
pub(crate) fn resolve_bar_chart_layout(
    props: &[Property],
    size: [f32; 2], // half-size (same convention as other plot builders)
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    parent_size: Option<[f64; 2]>, // Some() when inside a Graph
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> (BarChartLayout, Vec<(String, f32)>) {
    let data = parse_bar_chart_data(props, diagnostics, label);

    // Parse properties
    let mut bar_width_auto = true;
    let mut bar_width_val = 20.0f32;
    let mut gap_auto = true;
    let mut gap_val = 4.0f32;
    let mut bar_colors_auto = true;
    let mut bar_colors: Vec<[f32; 4]> = vec![];
    let mut show_axis = true;
    let mut show_labels = true;
    let _direction = "vertical"; // Reserved for horizontal bar support
    let mut max_value_auto = true;
    let mut max_value_val = 0.0f32;

    for prop in props {
        let subject = format!("{}.{}", label, prop.name);
        match prop.name.as_str() {
            "bar_width" => {
                (bar_width_auto, bar_width_val) = resolve_bar_number(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    "bar_width",
                    (bar_width_auto, bar_width_val),
                );
            },
            "gap" => {
                (gap_auto, gap_val) = resolve_bar_number(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    "gap",
                    (gap_auto, gap_val),
                );
            },
            "bar_colors" => {
                (bar_colors_auto, bar_colors) = resolve_bar_colors(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    (bar_colors_auto, bar_colors),
                );
            },
            "show_axis" => {
                show_axis = resolve_bar_bool(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    "show_axis",
                    show_axis,
                );
            },
            "show_labels" => {
                show_labels = resolve_bar_bool(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    "show_labels",
                    show_labels,
                );
            },
            "direction" => {
                // Parsed but not yet used; reserved for horizontal bar support
            },
            "max_value" => {
                (max_value_auto, max_value_val) = resolve_bar_max_value(
                    prop,
                    env,
                    diagnostics,
                    label,
                    &subject,
                    (max_value_auto, max_value_val),
                );
            },
            _ => {}, // Non-bar-chart properties are handled by the general actor pipeline.
        }
    }

    (
        BarChartLayout {
            size,
            color,
            stroke_color,
            stroke_width,
            x_domain,
            y_domain,
            parent_size,
            bar_width_auto,
            bar_width_val,
            gap_auto,
            gap_val,
            bar_colors_auto,
            bar_colors,
            show_axis,
            show_labels,
            max_value_auto,
            max_value_val,
        },
        data,
    )
}

/// `bar_width`/`gap`: a number switches off `auto`; `auto` and evaluation
/// failures keep the current `(auto, value)`.
fn resolve_bar_number(
    prop: &Property,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
    subject: &str,
    prop_name: &str,
    current: (bool, f32),
) -> (bool, f32) {
    let (auto, val) = current;
    let is_auto = matches!(&prop.value, Expr::Ident(s) | Expr::Str(s) if s == "auto");
    if is_auto {
        // keep the current auto distribution
        return (auto, val);
    }
    match evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, subject) {
        Some(Value::Num(n)) => (false, n as f32),
        Some(other) => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{label}' {prop_name} expects a number or \"auto\", got {other:?}"
                    ),
                )
                .with_subject(subject),
            );
            (auto, val)
        },
        None => (auto, val),
    }
}

/// `max_value`: a positive number switches off auto scaling; non-positive
/// numbers and evaluation failures keep `auto`, while the value is recorded
/// either way.
fn resolve_bar_max_value(
    prop: &Property,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
    subject: &str,
    current: (bool, f32),
) -> (bool, f32) {
    let (auto, val) = current;
    let is_auto = matches!(&prop.value, Expr::Ident(s) | Expr::Str(s) if s == "auto");
    if is_auto {
        // keep default auto scale
        return (auto, val);
    }
    match evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, subject) {
        Some(Value::Num(n)) => {
            let n = n as f32;
            let auto = if n > 0.0 { false } else { auto };
            (auto, n)
        },
        Some(other) => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{label}' max_value expects a positive number or \"auto\", got {other:?}"
                    ),
                )
                .with_subject(subject),
            );
            (auto, val)
        },
        None => (auto, val),
    }
}

/// `show_axis`/`show_labels`: a bool, or the strings `"true"`, `"1"`, `"false"`
/// or `"0"`. Anything else warns and falls back to `true`, the original default.
fn resolve_bar_bool(
    prop: &Property,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
    subject: &str,
    prop_name: &str,
    current: bool,
) -> bool {
    let Some(v) = evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, subject)
    else {
        return current;
    };
    match v {
        Value::Bool(b) => b,
        Value::Str(ref s) => match s.as_str() {
            "true" | "1" => true,
            "false" | "0" => false,
            other => {
                // The type row admits any string, so without this
                // `show_axis: "yes"` silently reads false.
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "BarChart '{label}' {prop_name} expects a boolean, got \"{other}\""
                        ),
                    )
                    .with_subject(subject),
                );
                true
            },
        },
        _ => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("BarChart '{label}' {prop_name} expects a boolean"),
                )
                .with_subject(subject),
            );
            true
        },
    }
}

/// `bar_colors`: `auto`, a list (one color per bar) or a single color (uniform).
/// `auto` and evaluation failures keep the current `(auto, colors)`.
fn resolve_bar_colors(
    prop: &Property,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
    subject: &str,
    current: (bool, Vec<[f32; 4]>),
) -> (bool, Vec<[f32; 4]>) {
    let (auto, colors) = current;
    let is_auto = match &prop.value {
        Expr::Ident(s) | Expr::Str(s) => s == "auto",
        _ => false,
    };
    if is_auto {
        // keep the current auto colors
        return (auto, colors);
    }
    if let Expr::List(list) = &prop.value {
        let mut parsed = Vec::with_capacity(list.len());
        for c in list {
            if let Some(col) = parse_color_in_env_with_lookup_diagnostic(
                label,
                "bar_colors",
                c,
                env,
                diagnostics,
                subject,
            ) {
                parsed.push(col);
            }
        }
        if !parsed.is_empty() {
            return (false, parsed);
        }
        return (auto, colors);
    }
    // Single color (no list) → uniform color
    match resolve_color_in_env(&prop.value, env) {
        Ok(Some(col)) => (false, vec![col]),
        Ok(None) => {
            // expression didn't resolve to a color — fall back to auto
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{label}' bar_colors value is not a color; falling back to auto"
                    ),
                )
                .with_subject(subject),
            );
            (auto, colors)
        },
        Err(EvalError::UndefinedVariable(key)) => {
            let candidate_keys = env.all_keys();
            let suggestion = best_path_suggestion(&key, candidate_keys.iter().map(String::as_str));
            let hint = suggestion
                .map(|candidate| format!(" Did you mean '{candidate}'?"))
                .unwrap_or_default();
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownColorReference,
                    DiagnosticPhase::Build,
                    format!(
                        "Color value '{key}' on '{}.bar_colors' does not resolve to a known color; falling back to auto bar colors.{hint}",
                        label
                    ),
                ).with_subject(subject),
            );
            // fall back to auto (auto and colors stay as they were)
            (auto, colors)
        },
        Err(_) => {
            // other eval error — fall back to auto
            (auto, colors)
        },
    }
}

impl Timeline {
    /// Resolve a BarChart declaration into its layout, dataset and paths.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub(super) fn build_bar_chart_plot(
        &self,
        label: &str,
        parent_label: Option<&str>,
        props: &[Property],
        parsed: &PlotActorProps,
        size: [f32; 2],
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> (BarChartLayout, Vec<(String, f32)>, Vec<VelloPath>, Vec<(f64, f64, String)>) {
        // Determine parent graph context (if any)
        let p_label = parent_label.unwrap_or("").to_string();
        let size_key = crate::timeline::env_keys::side_channel(&p_label, "size");
        let parent_size = if let Some(Value::Vec2(sz)) = self.env.get(size_key.as_str()) {
            Some(sz)
        } else {
            None
        };
        let mut p_x_domain = parsed.x_domain;
        let mut p_y_domain = parsed.y_domain;

        // The parent's coordinate frame is the only source for a hosted
        // BarChart's math domain.
        if let Some(frame) = self.coordinate_frame_for(&p_label) {
            p_x_domain = frame.x_domain;
            p_y_domain = frame.y_domain;
        }

        let (layout, data) = resolve_bar_chart_layout(
            props,
            size,
            parsed.color,
            parsed.stroke_color,
            parsed.stroke_width,
            p_x_domain,
            p_y_domain,
            parent_size,
            eval_env,
            diagnostics,
            label,
        );
        let (paths, bar_labels) = layout.paths_for(&data);
        (layout, data, paths, bar_labels)
    }
}
