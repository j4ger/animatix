//! Shared utilities for timeline build phase.
//!
//! Provides geometry/coordinate helpers used by multiple build submodules.

/// Convert math coordinates to screen coordinates for a graph actor.
///
/// # Arguments
/// * `mx`, `my` - Math coordinates to convert
/// * `x_domain`, `y_domain` - Domain ranges for the math coordinates
/// * `size` - Size of the graph in screen coordinates
/// * `at` - Position of the graph in screen coordinates
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
    relative: bool,
) -> [f64; 2] {
    let half_w = size[0] / 2.0;
    let half_h = size[1] / 2.0;

    // Math-to-screen mapping:
    // offset_x = half_w * (-1.0 + 2.0 * (mx - x_min) / (x_max - x_min))
    // offset_y = half_h * (1.0 - 2.0 * (my - y_min) / (y_max - y_min))
    let x_range = x_domain[1] - x_domain[0];
    let y_range = y_domain[1] - y_domain[0];

    let offset_x = if x_range != 0.0 {
        half_w * (-1.0 + 2.0 * (mx - x_domain[0]) / x_range)
    } else {
        0.0
    };
    let offset_y = if y_range != 0.0 {
        half_h * (1.0 - 2.0 * (my - y_domain[0]) / y_range)
    } else {
        0.0
    };

    if relative {
        [offset_x, offset_y]
    } else {
        let screen_x = at[0] + offset_x;
        let screen_y = at[1] + offset_y;
        [screen_x, screen_y]
    }
}
