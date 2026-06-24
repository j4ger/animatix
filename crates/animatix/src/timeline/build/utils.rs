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

/// Parameter bundle for graph coordinate conversions.
///
/// Passed by reference to [`graph_math_to_screen`] and [`graph_screen_to_math`] instead of
/// individual arguments. Build one at each call site from whatever values are available (some
/// may be static captures, others read from the runtime environment).
pub(super) struct GraphContext {
    pub x_domain: [f64; 2],
    pub y_domain: [f64; 2],
    /// Full size of the graph actor in screen units (half-extents in the same coordinate space
    /// used by `at`).
    pub size: [f64; 2],
    /// Position of the graph actor in screen coordinates.
    pub at: [f64; 2],
    /// Plot area insets `[left, right, top, bottom]` in the same pixel units as `size`.
    pub padding: [f64; 4],
    /// X-axis scale: `"linear"` (default) or `"log"`.
    pub x_scale: String,
    /// Y-axis scale: `"linear"` (default) or `"log"`.
    pub y_scale: String,
}

/// Convert math coordinates to screen coordinates for a graph actor.
///
/// # Arguments
/// * `mx`, `my` — Math coordinates to convert.
/// * `ctx` — Graph geometry and scale context.
/// * `relative` — If `true`, returns coordinates relative to the graph's position.
///               If `false`, returns absolute screen coordinates.
///
/// # Returns
/// Screen coordinates as `[x, y]`.
pub(super) fn graph_math_to_screen(mx: f64, my: f64, ctx: &GraphContext, relative: bool) -> [f64; 2] {
    let left = ctx.padding[0];
    let right = ctx.padding[1];
    let top = ctx.padding[2];
    let bottom = ctx.padding[3];

    // Effective plot area after padding.
    let plot_w = ctx.size[0] - left - right;
    let plot_h = ctx.size[1] - top - bottom;

    // Center of the padded plot area relative to the actor origin.
    let shift_x = (left - right) / 2.0;
    let shift_y = (top - bottom) / 2.0;

    let norm_x = normalize(mx, ctx.x_domain[0], ctx.x_domain[1], &ctx.x_scale);
    let norm_y = normalize(my, ctx.y_domain[0], ctx.y_domain[1], &ctx.y_scale);

    let offset_x = shift_x + (norm_x - 0.5) * plot_w;
    let offset_y = shift_y + (0.5 - norm_y) * plot_h;

    if relative {
        [offset_x, offset_y]
    } else {
        [ctx.at[0] + offset_x, ctx.at[1] + offset_y]
    }
}

/// Convert screen coordinates back to math coordinates for a graph actor.
///
/// This is the inverse of [`graph_math_to_screen`] (with `relative = false`).
/// Useful for hit-testing and interactive elements.
///
/// # Arguments
/// * `sx`, `sy` — Screen coordinates to convert.
/// * `ctx` — Graph geometry and scale context.
///
/// # Returns
/// Math coordinates as `[x, y]`.
pub(super) fn graph_screen_to_math(sx: f64, sy: f64, ctx: &GraphContext) -> [f64; 2] {
    let left = ctx.padding[0];
    let right = ctx.padding[1];
    let top = ctx.padding[2];
    let bottom = ctx.padding[3];

    // Effective plot area after padding.
    let plot_w = ctx.size[0] - left - right;
    let plot_h = ctx.size[1] - top - bottom;

    // Center of the padded plot area in absolute screen coordinates.
    let plot_center_x = ctx.at[0] + (left - right) / 2.0;
    let plot_center_y = ctx.at[1] + (top - bottom) / 2.0;

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
    let mx = denormalize(norm_x, ctx.x_domain[0], ctx.x_domain[1], &ctx.x_scale);
    let my = denormalize(norm_y, ctx.y_domain[0], ctx.y_domain[1], &ctx.y_scale);

    [mx, my]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_ctx(
        x_domain: [f64; 2],
        y_domain: [f64; 2],
        size: [f64; 2],
        at: [f64; 2],
        padding: [f64; 4],
    ) -> GraphContext {
        GraphContext {
            x_domain,
            y_domain,
            size,
            at,
            padding,
            x_scale: "linear".to_string(),
            y_scale: "linear".to_string(),
        }
    }

    /// Round-trip with no padding (linear scale): screen_to_math(math_to_screen(mx, my)) == [mx, my].
    #[test]
    fn screen_to_math_round_trip_no_padding() {
        let ctx = linear_ctx([-10.0, 10.0], [-5.0, 5.0], [800.0, 600.0], [0.0, 0.0], [0.0; 4]);

        for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (10.0, -5.0), (-10.0, 5.0)] {
            let [sx, sy] = graph_math_to_screen(mx, my, &ctx, false);
            let [rx, ry] = graph_screen_to_math(sx, sy, &ctx);
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
        let ctx = linear_ctx(
            [-10.0, 10.0],
            [-5.0, 5.0],
            [800.0, 600.0],
            [0.0, 0.0],
            [20.0, 10.0, 15.0, 5.0],
        );

        for (mx, my) in [(-5.0_f64, 3.0_f64), (0.0, 0.0), (5.0, -2.5)] {
            let [sx, sy] = graph_math_to_screen(mx, my, &ctx, false);
            let [rx, ry] = graph_screen_to_math(sx, sy, &ctx);
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
        let ctx = linear_ctx(
            [-10.0, 10.0],
            [-5.0, 5.0],
            [800.0, 600.0],
            [0.0, 0.0],
            [20.0, 10.0, 15.0, 5.0],
        );

        let [mx, my] = graph_screen_to_math(5.0, 5.0, &ctx);
        assert!((mx - 0.0).abs() < 1e-10, "expected mx=0, got {mx}");
        assert!((my - 0.0).abs() < 1e-10, "expected my=0, got {my}");
    }

    /// Coordinates outside the plot area extrapolate without panicking.
    #[test]
    fn screen_to_math_outside_plot_extrapolates() {
        let ctx = linear_ctx([-10.0, 10.0], [-5.0, 5.0], [800.0, 600.0], [0.0, 0.0], [0.0; 4]);

        let [mx, my] = graph_screen_to_math(2000.0, 2000.0, &ctx);
        assert!(mx.is_finite(), "extrapolated mx should be finite: {mx}");
        assert!(my.is_finite(), "extrapolated my should be finite: {my}");
        assert!(mx > 10.0, "expected mx > 10.0, got {mx}");
        assert!(my < -5.0, "expected my < -5.0, got {my}");
    }

    /// Zero-size graph: no divide-by-zero panic; returns the midpoint of each domain.
    #[test]
    fn screen_to_math_zero_size_returns_domain_midpoint() {
        let ctx = linear_ctx([-10.0, 10.0], [-5.0, 5.0], [0.0, 0.0], [0.0, 0.0], [0.0; 4]);
        let [mx, my] = graph_screen_to_math(0.0, 0.0, &ctx);
        assert!(mx.is_finite(), "zero-size mx should be finite: {mx}");
        assert!(my.is_finite(), "zero-size my should be finite: {my}");
    }

    /// Log scale: min is mapped to screen left (norm=0), max to screen right (norm=1).
    #[test]
    fn log_scale_endpoints_map_correctly() {
        let ctx = GraphContext {
            x_domain: [1.0, 100.0],
            y_domain: [1.0, 1000.0],
            size: [800.0, 600.0],
            at: [0.0, 0.0],
            padding: [0.0; 4],
            x_scale: "log".to_string(),
            y_scale: "linear".to_string(),
        };

        // x=1 (min) should map to screen left edge = at[0] - size[0]/2 = -400
        // (norm=0 → offset = -0.5 * plot_w = -400)
        let [sx_min, _] = graph_math_to_screen(1.0, 1.0, &ctx, false);
        assert!((sx_min - (-400.0)).abs() < 1e-8, "log x min: expected -400, got {sx_min}");

        // x=100 (max) → norm=1 → screen right = +400
        let [sx_max, _] = graph_math_to_screen(100.0, 1.0, &ctx, false);
        assert!((sx_max - 400.0).abs() < 1e-8, "log x max: expected 400, got {sx_max}");

        // x=10 (geometric midpoint of [1, 100]) → norm=0.5 → screen 0
        let [sx_mid, _] = graph_math_to_screen(10.0, 1.0, &ctx, false);
        assert!(sx_mid.abs() < 1e-8, "log x geometric mid: expected 0, got {sx_mid}");
    }

    /// Log scale round-trip: screen_to_math(math_to_screen(mx, my)) == [mx, my].
    #[test]
    fn log_scale_round_trip() {
        let ctx = GraphContext {
            x_domain: [0.1, 1000.0],
            y_domain: [0.5, 500.0],
            size: [800.0, 600.0],
            at: [0.0, 0.0],
            padding: [0.0; 4],
            x_scale: "log".to_string(),
            y_scale: "log".to_string(),
        };

        for (mx, my) in [(1.0_f64, 1.0_f64), (10.0, 100.0), (0.1, 0.5), (1000.0, 500.0)] {
            let [sx, sy] = graph_math_to_screen(mx, my, &ctx, false);
            let [rx, ry] = graph_screen_to_math(sx, sy, &ctx);
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
        let ctx = GraphContext {
            x_domain: [1.0, 10000.0],
            y_domain: [-5.0, 5.0],
            size: [800.0, 600.0],
            at: [0.0, 0.0],
            padding: [0.0; 4],
            x_scale: "log".to_string(),
            y_scale: "linear".to_string(),
        };

        for (mx, my) in [(1.0_f64, -5.0_f64), (100.0, 0.0), (10000.0, 5.0)] {
            let [sx, sy] = graph_math_to_screen(mx, my, &ctx, false);
            let [rx, ry] = graph_screen_to_math(sx, sy, &ctx);
            assert!((rx - mx).abs() < 1e-8, "mixed log-x round-trip: {mx} -> {rx}");
            assert!((ry - my).abs() < 1e-8, "mixed linear-y round-trip: {my} -> {ry}");
        }
    }

    /// Log scale with invalid domain (negative min): returns 0.5 (graceful fallback).
    #[test]
    fn log_scale_invalid_domain_returns_center() {
        let ctx = GraphContext {
            x_domain: [-5.0, 10.0], // invalid log domain (min <= 0)
            y_domain: [1.0, 100.0],
            size: [800.0, 600.0],
            at: [0.0, 0.0],
            padding: [0.0; 4],
            x_scale: "log".to_string(),
            y_scale: "linear".to_string(),
        };

        // Should not panic and return a finite value
        let [sx, _sy] = graph_math_to_screen(2.0, 1.0, &ctx, false);
        assert!(sx.is_finite(), "invalid log domain should produce finite screen coord: {sx}");
    }
}
