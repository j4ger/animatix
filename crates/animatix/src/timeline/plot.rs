//!
//! # Adaptive Sampling Algorithm
//!
//! The plotting functions in this module use recursive midpoint subdivision to
//! sample mathematical curves at adaptive resolution. The algorithm works as follows:
//!
//! 1. **Recursive Midpoint Subdivision**: Start with two endpoints of a segment.
//!    Compute the midpoint and compare it against a linear interpolation between endpoints.
//!    If the deviation exceeds `tolerance`, subdivide both halves recursively.
//!
//! 2. **Coarse-to-Fine Refinement**: Begin with coarse sampling and refine only where
//!    the curve deviates significantly from a straight line. This captures detail where
//!    needed while avoiding unnecessary computation in flat regions.
//!
//! 3. **Maximum Depth Cap**: Subdivision stops when reaching `max_depth` to prevent
//!    infinite recursion and control computational cost. The minimum segment size is
//!    thus `(total_range) / 2^max_depth`.
//!
//! 4. **Discontinuity Handling**: When detecting steep jumps (asymptotes, discontinuities),
//!    inject a NaN point so Vello's path renderer breaks the stroke. This prevents
//!    erroneous straight-line connections across gaps.
//!
//! 5. **Visibility Culling**: Segments whose y-coordinates (and x-coordinates for
//!    parametric/polar) lie entirely outside the visible region with margin are culled
//!    with NaN separators, skipping unnecessary evaluation.
//!
//! 6. **Tolerance-Accuracy Tradeoff**: The `tolerance` parameter (squared distance threshold)
//!    controls how much deviation is acceptable before subdividing. Lower values produce
//!    more accurate curves but require more samples; higher values improve performance
//!    at the cost of accuracy.
//!
//! The three sampling functions (`cartesian`, `polar`, `parametric`) share this core
//! algorithm but differ in how they map mathematical coordinates to screen space.

use std::collections::HashMap;
use super::{Environment, EvalError, Value, evaluate_expr};
use crate::ast::Expr;

/// Evaluate `body` with `arg_name` bound to `arg_value`, without mutating `env`.
fn evaluate_with_binding(
    env: &Environment,
    arg_name: &str,
    arg_value: f64,
    body: &Expr,
) -> Result<Value, EvalError> {
    let mut local_env = env.clone();
    local_env.set(arg_name, Value::Num(arg_value));
    evaluate_expr(body, &local_env)
}

pub(crate) fn sample_recursive_cartesian(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let screen_height = p_size[1];

    let margin_y = screen_height * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    if (p0.y < min_screen_y && p1.y < min_screen_y) || (p0.y > max_screen_y && p1.y > max_screen_y)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;
    if (p0.x < min_screen_x && p1.x < min_screen_x) || (p0.x > max_screen_x && p1.x > max_screen_x) {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let mid_key = mid_t.to_bits();
    let val = cache.get(&mid_key).cloned().unwrap_or_else(|| {
        let result = evaluate_with_binding(env, arg_name, mid_t, body)
            .unwrap_or(Value::Num(f64::NAN));
        cache.insert(mid_key, result.clone());
        result
    });
    let math_y = val.as_num();

    let math_x = mid_t;

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_cartesian(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
        sample_recursive_cartesian(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

pub(crate) fn sample_recursive_polar(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let margin_y = p_size[1] * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > (p_size[0].max(p_size[1])).powi(2) * 4.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let mid_key = mid_t.to_bits();
    let val = cache.get(&mid_key).cloned().unwrap_or_else(|| {
        let result = evaluate_with_binding(env, arg_name, mid_t, body)
            .unwrap_or(Value::Num(f64::NAN));
        cache.insert(mid_key, result.clone());
        result
    });
    let math_r = val.as_num();

    let math_x = math_r * mid_t.cos();
    let math_y = math_r * mid_t.sin();

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_polar(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
        sample_recursive_polar(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

pub(crate) fn sample_recursive_parametric(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let margin_y = p_size[1] * 2.0;
    let min_screen_y = -(p_size[1] / 2.0) - margin_y;
    let max_screen_y = (p_size[1] / 2.0) + margin_y;

    let margin_x = p_size[0] * 2.0;
    let min_screen_x = -(p_size[0] / 2.0) - margin_x;
    let max_screen_x = (p_size[0] / 2.0) + margin_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > (p_size[0].max(p_size[1])).powi(2) * 4.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let mid_key = mid_t.to_bits();
    let val = cache.get(&mid_key).cloned().unwrap_or_else(|| {
        let result = evaluate_with_binding(env, arg_name, mid_t, body)
            .unwrap_or(Value::Vec2([f64::NAN, f64::NAN]));
        cache.insert(mid_key, result.clone());
        result
    });
    let Value::Vec2([math_x, math_y]) = val else {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    };

    let screen_x = -(p_size[0] / 2.0)
        + p_size[0] * ((math_x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y = (p_size[1] / 2.0)
        - p_size[1] * ((math_y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));

    let p_mid = kurbo::Point::new(screen_x, screen_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < 3 {
        sample_recursive_parametric(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
        sample_recursive_parametric(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            tolerance,
            env,
            arg_name,
            body,
            p_x_domain,
            p_y_domain,
            p_size,
            cache,
            pts,
        );
    } else {
        pts.push(p1);
    }
}

pub(crate) fn implicit_intersection(
    p0: (f64, f64, f64),
    p1: (f64, f64, f64),
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
) -> kurbo::Point {
    let (x0, y0, v0) = p0;
    let (x1, y1, v1) = p1;
    let t = if (v1 - v0).abs() <= f64::EPSILON {
        0.5
    } else {
        (-v0 / (v1 - v0)).clamp(0.0, 1.0)
    };
    let x = x0 + (x1 - x0) * t;
    let y = y0 + (y1 - y0) * t;
    let screen_x =
        -(p_size[0] / 2.0) + p_size[0] * ((x - p_x_domain[0]) / (p_x_domain[1] - p_x_domain[0]));
    let screen_y =
        (p_size[1] / 2.0) - p_size[1] * ((y - p_y_domain[0]) / (p_y_domain[1] - p_y_domain[0]));
    kurbo::Point::new(screen_x, screen_y)
}

pub(crate) fn evaluate_implicit_value(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> f64 {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    let mut local_env = env.clone();
    local_env.set(x_name, Value::Num(x));
    local_env.set(y_name, Value::Num(y));
    evaluate_expr(body, &local_env)
        .unwrap_or(Value::Num(f64::NAN))
        .as_num()
}

pub(crate) fn build_implicit_plot_path(
    env: &Environment,
    arg_names: &[String],
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    let x_cells = resolution.max(8);
    let aspect = if p_size[0] <= f64::EPSILON {
        1.0
    } else {
        p_size[1] / p_size[0]
    };
    let y_cells = ((x_cells as f64) * aspect).round().max(8.0) as usize;
    let dx = (p_x_domain[1] - p_x_domain[0]) / x_cells as f64;
    let dy = (p_y_domain[1] - p_y_domain[0]) / y_cells as f64;

    // Pre-evaluate the function on a grid to avoid redundant AST evaluations.
    let mut grid = vec![vec![f64::NAN; x_cells + 1]; y_cells + 1];
    for yi in 0..=y_cells {
        let y = p_y_domain[0] + yi as f64 * dy;
        for xi in 0..=x_cells {
            let x = p_x_domain[0] + xi as f64 * dx;
            grid[yi][xi] = evaluate_implicit_value(env, arg_names, body, x, y);
        }
    }

    for yi in 0..y_cells {
        let y0 = p_y_domain[0] + yi as f64 * dy;
        let y1 = y0 + dy;
        for xi in 0..x_cells {
            let x0 = p_x_domain[0] + xi as f64 * dx;
            let x1 = x0 + dx;

            let bl = (x0, y0, grid[yi][xi]);
            let br = (x1, y0, grid[yi][xi + 1]);
            let tr = (x1, y1, grid[yi + 1][xi + 1]);
            let tl = (x0, y1, grid[yi + 1][xi]);

            if [bl.2, br.2, tr.2, tl.2].iter().any(|v| !v.is_finite()) {
                continue;
            }

            let bl_in = bl.2 >= 0.0;
            let br_in = br.2 >= 0.0;
            let tr_in = tr.2 >= 0.0;
            let tl_in = tl.2 >= 0.0;

            let mut intersections = Vec::new();
            if bl_in != br_in {
                intersections.push((
                    0,
                    implicit_intersection(bl, br, p_x_domain, p_y_domain, p_size),
                ));
            }
            if br_in != tr_in {
                intersections.push((
                    1,
                    implicit_intersection(br, tr, p_x_domain, p_y_domain, p_size),
                ));
            }
            if tr_in != tl_in {
                intersections.push((
                    2,
                    implicit_intersection(tr, tl, p_x_domain, p_y_domain, p_size),
                ));
            }
            if tl_in != bl_in {
                intersections.push((
                    3,
                    implicit_intersection(tl, bl, p_x_domain, p_y_domain, p_size),
                ));
            }

            match intersections.len() {
                2 => {
                    path.move_to(intersections[0].1);
                    path.line_to(intersections[1].1);
                }
                4 => {
                    let center = evaluate_implicit_value(
                        env,
                        arg_names,
                        body,
                        (x0 + x1) * 0.5,
                        (y0 + y1) * 0.5,
                    );
                    let center_positive = center >= 0.0;
                    let edge = |idx: usize| {
                        intersections
                            .iter()
                            .find(|(edge_idx, _)| *edge_idx == idx)
                            .map(|(_, pt)| *pt)
                    };
                    if bl_in == tr_in && br_in == tl_in {
                        let first_pair = if center_positive == bl_in {
                            (0, 3)
                        } else {
                            (0, 1)
                        };
                        let second_pair = if center_positive == bl_in {
                            (1, 2)
                        } else {
                            (2, 3)
                        };
                        if let (Some(a), Some(b)) = (edge(first_pair.0), edge(first_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                        if let (Some(a), Some(b)) = (edge(second_pair.0), edge(second_pair.1)) {
                            path.move_to(a);
                            path.line_to(b);
                        }
                    }
                }
                0 => {}
                1 | 3 => {
                    tracing::debug!(
                        "Implicit plot: degenerate cell with {} intersections at ({}, {})",
                        intersections.len(),
                        xi,
                        yi
                    );
                }
                n => {
                    tracing::warn!(
                        "Implicit plot: unexpected {} intersections in cell ({}, {})",
                        n,
                        xi,
                        yi
                    );
                }
            }
        }
    }

    path
}
