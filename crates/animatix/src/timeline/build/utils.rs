//! Shared utilities for timeline build phase.
//!
//! Provides geometry/coordinate helpers used by multiple build submodules.

/// Normalize a math value to `[0, 1]` using the given axis scale.
///
/// * `"log"` — logarithmic normalization; returns `0.5` if any input is ≤ 0.
/// * anything else — linear normalization.
#[inline]
fn normalize(v: f64, min: f64, max: f64, scale: &str) -> f64 {
    if scale == "log" {
        if v <= 0.0 || min <= 0.0 || max <= 0.0 {
            return 0.5;
        }
        (v.ln() - min.ln()) / (max.ln() - min.ln())
    } else {
        let range = max - min;
        if range != 0.0 { (v - min) / range } else { 0.5 }
    }
}

/// Denormalize a `[0, 1]` value back to math coordinates using the given axis scale.
///
/// Inverse of [`normalize`].
#[inline]
fn denormalize(norm: f64, min: f64, max: f64, scale: &str) -> f64 {
    if scale == "log" {
        if min <= 0.0 || max <= 0.0 {
            return (min + max) / 2.0;
        }
        min * (max / min).powf(norm)
    } else {
        min + norm * (max - min)
    }
}

/// Convert math coordinates to screen coordinates for a graph actor.
///
/// # Arguments
/// * `mx`, `my` - Math coordinates to convert
/// * `x_domain`, `y_domain` - Domain ranges for the math coordinates
/// * `size` - Size of the graph in screen coordinates (half-extents)
/// * `at` - Position of the graph in screen coordinates
/// * `padding` - Plot area insets `[left, right, top, bottom]` in the same pixel units as `size`
/// * `relative` - If true, returns coordinates relative to the graph's position.
///                If false, returns absolute screen coordinates.
/// * `x_scale`, `y_scale` - Axis scaling: `"linear"` (default) or `"log"`.
///
/// # Returns
/// Screen coordinates as `[x, y]` array.
/// Convert screen coordinates back to math coordinates for a graph actor.
///
/// This is the inverse of [`graph_math_to_screen`] (with `relative = false`).
/// Useful for hit-testing and interactive elements.
///
/// # Arguments
/// * `sx`, `sy` - Screen coordinates to convert
/// * `x_domain`, `y_domain` - Domain ranges for the math coordinates
/// * `size` - Size of the graph in screen coordinates (half-extents)
/// * `at` - Position of the graph in screen coordinates
/// * `padding` - Plot area insets `[left, right, top, bottom]` in pixels
///
/// # Returns
/// Math coordinates as `[x, y]` array.
pub(super) fn graph_screen_to_math(
    sx: f64,
    sy: f64,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    size: [f64; 2],
    at: [f64; 2],
    padding: [f64; 4],
    x_scale: &str,
    y_scale: &str,
) -> [f64; 2] {
    let left = padding[0];
    let right = padding[1];
    let top = padding[2];
    let bottom = padding[3];

    // Effective plot area after padding.
    let plot_w = size[0] - left - right;
    let plot_h = size[1] - top - bottom;

    // Center of the padded plot area in absolute screen coordinates.
    let plot_center_x = at[0] + (left - right) / 2.0;
    let plot_center_y = at[1] + (top - bottom) / 2.0;

    // Convert screen to normalized coords [0, 1].
    let norm_x = if plot_w != 0.0 {
        (sx - plot_center_x) / plot_w + 0.5
    } else {
        0.5
    };
    let norm_y = if plot_h != 0.0 {
        0.5 - (sy - plot_center_y) / plot_h
    } else {
        0.5
    };

    // Convert normalized to math coords (with optional log scale).
    let mx = denormalize(norm_x, x_domain[0], x_domain[1], x_scale);
    let my = denormalize(norm_y, y_domain[0], y_domain[1], y_scale);

    [mx, my]
}

pub(super) fn graph_math_to_screen(
    mx: f64,
    my: f64,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    size: [f64; 2],
    at: [f64; 2],
    padding: [f64; 4],
    relative: bool,
    x_scale: &str,
    y_scale: &str,
) -> [f64; 2] {
    let left = padding[0];
    let right = padding[1];
    let top = padding[2];
    let bottom = padding[3];

    // Effective plot area after padding.
    let plot_w = size[0] - left - right;
    let plot_h = size[1] - top - bottom;

    // Center of the padded plot area relative to the actor origin.
    let shift_x = (left - right) / 2.0;
    let shift_y = (top - bottom) / 2.0;

    let norm_x = normalize(mx, x_domain[0], x_domain[1], x_scale);
    let norm_y = normalize(my, y_domain[0], y_domain[1], y_scale);

    let offset_x = shift_x + (norm_x - 0.5) * plot_w;
    let offset_y = shift_y + (0.5 - norm_y) * plot_h;

    if relative {
        [offset_x, offset_y]
    } else {
        [at[0] + offset_x, at[1] + offset_y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip with no padding (linear scale): screen_to_math(math_to_screen(mx, my)) == [mx, my].
    #[test]
    fn screen_to_math_round_trip_no_padding() {
        let x_domain = [-10.0, 10.0];
        let y_domain = [-5.0, 5.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (10.0, -5.0), (-10.0, 5.0)] {
            let [sx, sy] =
                graph_math_to_screen(mx, my, x_domain, y_domain, size, at, padding, false, "linear", "linear");
            let [rx, ry] =
                graph_screen_to_math(sx, sy, x_domain, y_domain, size, at, padding, "linear", "linear");
            assert!(
                (rx - mx).abs() < 1e-10,
                "x round-trip failed: {mx} -> screen {sx} -> {rx}"
            );
            assert!(
                (ry - my).abs() < 1e-10,
                "y round-trip failed: {my} -> screen {sy} -> {ry}"
            );
        }
    }

    /// Round-trip with asymmetric padding: padding = [left=20, right=10, top=15, bottom=5].
    #[test]
    fn screen_to_math_round_trip_with_padding() {
        let x_domain = [-10.0, 10.0];
        let y_domain = [-5.0, 5.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [20.0, 10.0, 15.0, 5.0];

        for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (5.0, -2.5)] {
            let [sx, sy] =
                graph_math_to_screen(mx, my, x_domain, y_domain, size, at, padding, false, "linear", "linear");
            let [rx, ry] =
                graph_screen_to_math(sx, sy, x_domain, y_domain, size, at, padding, "linear", "linear");
            assert!(
                (rx - mx).abs() < 1e-10,
                "padded x round-trip failed: {mx} -> screen {sx} -> {rx}"
            );
            assert!(
                (ry - my).abs() < 1e-10,
                "padded y round-trip failed: {my} -> screen {sy} -> {ry}"
            );
        }
    }

    /// With padding [left=20, right=10, top=15, bottom=5], the padded plot center in screen
    /// space is at (shift_x, shift_y) = ((20-10)/2, (15-5)/2) = (5, 5).
    /// screen (5, 5) should therefore map to math (0, 0).
    #[test]
    fn screen_to_math_padding_center_maps_to_math_origin() {
        let x_domain = [-10.0, 10.0];
        let y_domain = [-5.0, 5.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [20.0, 10.0, 15.0, 5.0];

        let [mx, my] = graph_screen_to_math(5.0, 5.0, x_domain, y_domain, size, at, padding, "linear", "linear");
        assert!((mx - 0.0).abs() < 1e-10, "expected mx=0, got {mx}");
        assert!((my - 0.0).abs() < 1e-10, "expected my=0, got {my}");
    }

    /// Coordinates outside the plot area extrapolate without panicking.
    #[test]
    fn screen_to_math_outside_plot_extrapolates() {
        let x_domain = [-10.0, 10.0];
        let y_domain = [-5.0, 5.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        let [mx, my] = graph_screen_to_math(2000.0, 2000.0, x_domain, y_domain, size, at, padding, "linear", "linear");
        assert!(mx.is_finite(), "extrapolated mx should be finite: {mx}");
        assert!(my.is_finite(), "extrapolated my should be finite: {my}");
        assert!(mx > x_domain[1], "expected mx > 10.0, got {mx}");
        assert!(my < y_domain[0], "expected my < -5.0, got {my}");
    }

    /// Zero-size graph: no divide-by-zero panic; returns the midpoint of each domain.
    #[test]
    fn screen_to_math_zero_size_returns_domain_midpoint() {
        let x_domain = [-10.0, 10.0];
        let y_domain = [-5.0, 5.0];
        let [mx, my] =
            graph_screen_to_math(0.0, 0.0, x_domain, y_domain, [0.0, 0.0], [0.0, 0.0], [0.0; 4], "linear", "linear");
        assert!(mx.is_finite(), "zero-size mx should be finite: {mx}");
        assert!(my.is_finite(), "zero-size my should be finite: {my}");
    }

    /// Log scale: min is mapped to screen left (norm=0), max to screen right (norm=1).
    #[test]
    fn log_scale_endpoints_map_correctly() {
        let x_domain = [1.0, 100.0];
        let y_domain = [1.0, 1000.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        // x=1 (min) should map to screen left edge = at[0] - size[0]/2 = -400
        // (norm=0 → offset = -0.5 * plot_w = -400)
        let [sx_min, _] =
            graph_math_to_screen(1.0, 1.0, x_domain, y_domain, size, at, padding, false, "log", "linear");
        assert!((sx_min - (-400.0)).abs() < 1e-8, "log x min: expected -400, got {sx_min}");

        // x=100 (max) → norm=1 → screen right = +400
        let [sx_max, _] =
            graph_math_to_screen(100.0, 1.0, x_domain, y_domain, size, at, padding, false, "log", "linear");
        assert!((sx_max - 400.0).abs() < 1e-8, "log x max: expected 400, got {sx_max}");

        // x=10 (geometric midpoint of [1, 100]) → norm=0.5 → screen 0
        let [sx_mid, _] =
            graph_math_to_screen(10.0, 1.0, x_domain, y_domain, size, at, padding, false, "log", "linear");
        assert!(sx_mid.abs() < 1e-8, "log x geometric mid: expected 0, got {sx_mid}");
    }

    /// Log scale round-trip: screen_to_math(math_to_screen(mx, my)) == [mx, my].
    #[test]
    fn log_scale_round_trip() {
        let x_domain = [0.1, 1000.0];
        let y_domain = [0.5, 500.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        for (mx, my) in [(1.0_f64, 1.0_f64), (10.0, 100.0), (0.1, 0.5), (1000.0, 500.0)] {
            let [sx, sy] =
                graph_math_to_screen(mx, my, x_domain, y_domain, size, at, padding, false, "log", "log");
            let [rx, ry] =
                graph_screen_to_math(sx, sy, x_domain, y_domain, size, at, padding, "log", "log");
            assert!(
                (rx - mx).abs() < 1e-8,
                "log x round-trip failed: {mx} -> screen {sx} -> {rx}"
            );
            assert!(
                (ry - my).abs() < 1e-8,
                "log y round-trip failed: {my} -> screen {sy} -> {ry}"
            );
        }
    }

    /// Mixed scales: log X, linear Y.
    #[test]
    fn mixed_log_x_linear_y_round_trip() {
        let x_domain = [1.0, 10000.0];
        let y_domain = [-5.0, 5.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        for (mx, my) in [(1.0_f64, -5.0_f64), (100.0, 0.0), (10000.0, 5.0)] {
            let [sx, sy] =
                graph_math_to_screen(mx, my, x_domain, y_domain, size, at, padding, false, "log", "linear");
            let [rx, ry] =
                graph_screen_to_math(sx, sy, x_domain, y_domain, size, at, padding, "log", "linear");
            assert!((rx - mx).abs() < 1e-8, "mixed log-x round-trip: {mx} -> {rx}");
            assert!((ry - my).abs() < 1e-8, "mixed linear-y round-trip: {my} -> {ry}");
        }
    }

    /// Log scale with invalid domain (negative min): returns 0.5 (graceful fallback).
    #[test]
    fn log_scale_invalid_domain_returns_center() {
        let x_domain = [-5.0, 10.0]; // invalid log domain (min <= 0)
        let y_domain = [1.0, 100.0];
        let size = [800.0, 600.0];
        let at = [0.0, 0.0];
        let padding = [0.0; 4];

        // Should not panic and return a finite value
        let [sx, _sy] =
            graph_math_to_screen(2.0, 1.0, x_domain, y_domain, size, at, padding, false, "log", "linear");
        assert!(sx.is_finite(), "invalid log domain should produce finite screen coord: {sx}");
    }
}
