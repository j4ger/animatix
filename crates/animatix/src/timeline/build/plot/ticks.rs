//! Axis ticks, tick-label formatting and graph tick-label placement.
//!
//! Split out of the former `build/plot.rs`; shared by the graph path builder
//! and the plot-actor entry point.

/// Data for tick/bar labels: positions and display text.
#[derive(Clone, Debug, Default)]
pub(crate) struct TickLabelData {
    /// (screen_x, screen_y, math_value) for each x-axis tick
    pub x_labels: Vec<(f64, f64, f64)>,
    /// (screen_x, screen_y, math_value) for each y-axis tick
    pub y_labels: Vec<(f64, f64, f64)>,
    /// (local_x, local_y, label text) for each BarChart bar
    pub bar_labels: Vec<(f64, f64, String)>,
}

/// Parse the `tick_labels` string property into a flags-like value indicating
/// which axes should have labels.
/// Accepts: "auto", "true", "false", "x", "y", "both"
pub(super) fn tick_labels_has_axis(value: &str, axis: char) -> bool {
    match value {
        "auto" | "true" | "both" => true,
        "x" => axis == 'x',
        "y" => axis == 'y',
        _ => false, // "false" or unrecognised
    }
}

/// Normalize `v` in `[min, max]` to `[0, 1]` using the given scale.
/// Returns `0.5` for invalid log domains (any input ≤ 0).
#[inline]
pub(super) fn normalize_axis(
    v: f64,
    min: f64,
    max: f64,
    scale: super::super::utils::ScaleType,
) -> f64 {
    if scale.is_log() {
        if v <= 0.0 || min <= 0.0 || max <= 0.0 {
            return 0.5;
        }
        (v.ln() - min.ln()) / (max.ln() - min.ln())
    } else {
        let range = max - min;
        if range != 0.0 { (v - min) / range } else { 0.5 }
    }
}

/// Helper to compute "nice numbers" for linear axis tick intervals (Wilkinson/Heckbert style).
fn nice_number(val: f64, round: bool) -> f64 {
    if val <= 0.0 || !val.is_finite() {
        return 1.0;
    }
    let exponent = val.log10().floor();
    let fraction = val / 10.0_f64.powf(exponent);
    let nice_fraction = if round {
        if fraction < 1.5 {
            1.0
        } else if fraction < 3.0 {
            2.0
        } else if fraction < 7.0 {
            5.0
        } else {
            10.0
        }
    } else if fraction <= 1.0 {
        1.0
    } else if fraction <= 2.0 {
        2.0
    } else if fraction <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice_fraction * 10.0_f64.powf(exponent)
}

/// Generate tick positions for an axis.
///
/// For `Log` scale, returns powers-of-10 and their multiples (1–9).
/// For linear, returns clean "nice number" ticks based on domain span.
pub(super) fn generate_axis_ticks(
    min: f64,
    max: f64,
    scale: super::super::utils::ScaleType,
) -> Vec<f64> {
    if scale.is_log() {
        if min <= 0.0 || max <= 0.0 {
            return vec![];
        }
        let mut ticks = vec![];
        let min_exp = min.log10().floor() as i32;
        let max_exp = max.log10().ceil() as i32;
        for exp in min_exp..=max_exp {
            let base = 10.0_f64.powi(exp);
            for i in 1..10u32 {
                let tick = base * i as f64;
                if tick >= min && tick <= max {
                    ticks.push(tick);
                }
            }
        }
        ticks
    } else {
        let span = (max - min).abs();
        if span <= f64::EPSILON {
            return vec![min];
        }
        let target_ticks = 8.0;
        let raw_step = span / target_ticks;
        let step = nice_number(raw_step, true);
        if step <= 0.0 || !step.is_finite() {
            return vec![];
        }

        let mut ticks = vec![];
        let start_idx = (min / step).ceil() as i64;
        let end_idx = (max / step).floor() as i64;
        for i in start_idx..=end_idx {
            let v = i as f64 * step;
            // Clean up precision artifacts near 0
            let v = if v.abs() < step * 1e-6 { 0.0 } else { v };
            if v >= min - 1e-9 && v <= max + 1e-9 {
                ticks.push(v);
            }
        }
        ticks
    }
}

/// Format an axis tick number cleanly (e.g. 1 instead of 1.0, 0.5 instead of 0.500000).
pub(crate) fn format_axis_tick_label(n: f64) -> String {
    if !n.is_finite() {
        return format!("{}", n);
    }
    let rounded = (n * 1e6).round() / 1e6;
    if rounded == rounded.floor() {
        format!("{}", rounded as i64)
    } else {
        // Strip trailing zeros after decimal point
        let s = format!("{:.4}", rounded);
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        trimmed.to_string()
    }
}

/// Compute tick-label positions for a graph host.
///
/// Uses the same padded coordinate mapping as `build_graph_axis_paths` so the
/// labels align with the padded axes.
pub(super) fn compute_graph_tick_labels(
    size: [f32; 2],
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    x_scale: super::super::utils::ScaleType,
    y_scale: super::super::utils::ScaleType,
    graph_padding: [f64; 4],
    tick_labels: &str,
) -> TickLabelData {
    let label_x = tick_labels_has_axis(tick_labels, 'x');
    let label_y = tick_labels_has_axis(tick_labels, 'y');
    let mut tick_label_data = TickLabelData::default();

    // Compute tick label positions (same logic as build_graph_axis_paths ticks section).
    // Uses padded coordinate mapping so labels align with padded axes.
    let tick_label_offset = 14.0;
    let hw = size[0] as f64;
    let hh = size[1] as f64;
    let plot_fw = 2.0 * hw - graph_padding[0] - graph_padding[1];
    let plot_fh = 2.0 * hh - graph_padding[2] - graph_padding[3];
    let shift_x = (graph_padding[0] - graph_padding[1]) / 2.0;
    let shift_y = (graph_padding[2] - graph_padding[3]) / 2.0;
    let tick_math_x_to_screen = |mx: f64| {
        let norm = normalize_axis(mx, x_domain[0], x_domain[1], x_scale);
        shift_x + (norm - 0.5) * plot_fw
    };
    let tick_math_y_to_screen = |my: f64| {
        let norm = normalize_axis(my, y_domain[0], y_domain[1], y_scale);
        shift_y + (0.5 - norm) * plot_fh
    };

    // X-axis screen position (at y=0 or bottom edge)
    let x_ticks = generate_axis_ticks(x_domain[0], x_domain[1], x_scale);
    let axis_y = if !y_scale.is_log() && y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
        tick_math_y_to_screen(0.0)
    } else {
        let hh = size[1] as f64;
        hh - graph_padding[3]
    };
    if label_x {
        for &x in &x_ticks {
            // Suppress x=0 label to prevent overlapping with y-axis or origin
            if x.abs() < 1e-6 {
                continue;
            }
            let screen_x = tick_math_x_to_screen(x);
            tick_label_data.x_labels.push((screen_x, axis_y + tick_label_offset, x));
        }
    }

    // Y-axis screen position (at x=0 or left edge)
    let y_ticks = generate_axis_ticks(y_domain[0], y_domain[1], y_scale);
    let axis_x = if !x_scale.is_log() && x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
        tick_math_x_to_screen(0.0)
    } else {
        let hw = size[0] as f64;
        -hw + graph_padding[0]
    };
    if label_y {
        for &y in &y_ticks {
            // Suppress y=0 label to prevent collision with x-axis or origin
            if y.abs() < 1e-6 {
                continue;
            }
            let screen_y = tick_math_y_to_screen(y);
            tick_label_data.y_labels.push((axis_x - tick_label_offset, screen_y, y));
        }
    }

    tick_label_data
}
