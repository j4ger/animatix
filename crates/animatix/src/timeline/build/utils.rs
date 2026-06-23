//! Shared utilities for timeline build phase.
//!
//! Provides geometry/coordinate helpers used by multiple build submodules.

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
///
/// # Returns
/// Screen coordinates as `[x, y]` array.
pub(super) fn graph_math_to_screen(
    mx: f64,
    my: f64,
    x_domain: [f64; 2],
    y_domain: [f64; 2],
    size: [f64; 2],
    at: [f64; 2],
    padding: [f64; 4],
    relative: bool,
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

    let x_range = x_domain[1] - x_domain[0];
    let y_range = y_domain[1] - y_domain[0];

    let norm_x = if x_range != 0.0 { (mx - x_domain[0]) / x_range } else { 0.5 };
    let norm_y = if y_range != 0.0 { (my - y_domain[0]) / y_range } else { 0.5 };

    let offset_x = shift_x + (norm_x - 0.5) * plot_w;
    let offset_y = shift_y + (0.5 - norm_y) * plot_h;

    if relative {
        [offset_x, offset_y]
    } else {
        [at[0] + offset_x, at[1] + offset_y]
    }
}
