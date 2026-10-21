//! Implicit plot marching squares contour generation.

use super::adaptive::math_to_screen_padded;
use super::{CapturedEnv, Environment, FuncSource, Value, flatten_blend};
use crate::timeline::modifier_runtime::ir::{CompiledExpr, evaluate_compiled_expr};

pub fn implicit_intersection(
    p0: (f64, f64, f64),
    p1: (f64, f64, f64),
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
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
    let (screen_x, screen_y) = math_to_screen_padded(x, y, p_x_domain, p_y_domain, p_size, padding);
    kurbo::Point::new(screen_x, screen_y)
}

/// Evaluate a `FuncSource` at (x, y) coordinates, returning a scalar value
/// for implicit contour detection. Supports blended transitions via recursive
/// evaluation of `Blend` nodes.
pub fn eval_implicit_source(source: &FuncSource, env: &mut Environment, x: f64, y: f64) -> f64 {
    match source {
        FuncSource::Compiled(args, body, captures) => {
            // Merge captured variables into the environment
            let inserted = captures.merge_missing_into(env);
            // Bind both arguments
            if args.len() >= 2 {
                env.set(&args[0], Value::Num(x));
                env.set(&args[1], Value::Num(y));
            }
            // Evaluate and return scalar
            let result = match evaluate_compiled_expr(body, env) {
                Ok(Value::Num(n)) => n,
                _ => f64::NAN,
            };
            for key in inserted {
                env.overrides.remove(&key);
                env.mark_mutated();
            }
            result
        },
        FuncSource::Blend { .. } => {
            let flat = flatten_blend(source);
            let mut sum = 0.0;
            for (weight, src) in flat {
                sum += weight * eval_implicit_source(src, env, x, y);
            }
            sum
        },
    }
}

/// Build an implicit plot contour path from a `FuncSource`, evaluating
/// the scalar field on a grid and extracting zero-contours via marching
/// squares. Supports function transitions via blend sources.
pub fn build_implicit_plot_path_from_source(
    env: &mut Environment,
    source: &FuncSource,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
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
    crate::timeline::utils::disable_eval_cache();
    for (yi, row) in grid.iter_mut().enumerate() {
        let y = p_y_domain[0] + yi as f64 * dy;
        for (xi, val) in row.iter_mut().enumerate() {
            let x = p_x_domain[0] + xi as f64 * dx;
            *val = eval_implicit_source(source, env, x, y);
        }
    }
    crate::timeline::utils::enable_eval_cache();

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
                    implicit_intersection(bl, br, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if br_in != tr_in {
                intersections.push((
                    1,
                    implicit_intersection(br, tr, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tr_in != tl_in {
                intersections.push((
                    2,
                    implicit_intersection(tr, tl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }
            if tl_in != bl_in {
                intersections.push((
                    3,
                    implicit_intersection(tl, bl, p_x_domain, p_y_domain, p_size, padding),
                ));
            }

            match intersections.len() {
                2 => {
                    path.move_to(intersections[0].1);
                    path.line_to(intersections[1].1);
                },
                4 => {
                    let center =
                        eval_implicit_source(source, env, (x0 + x1) * 0.5, (y0 + y1) * 0.5);
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
                },
                0 => {},
                1 | 3 => {
                    tracing::debug!(
                        "Implicit plot: degenerate cell with {} intersections at ({}, {})",
                        intersections.len(),
                        xi,
                        yi
                    );
                },
                n => {
                    tracing::warn!(
                        "Implicit plot: unexpected {} intersections in cell ({}, {})",
                        n,
                        xi,
                        yi
                    );
                },
            }
        }
    }

    path
}

/// Legacy wrapper that builds an implicit plot path from args and a compiled body.
/// Creates a `FuncSource::Compiled` and delegates to [`build_implicit_plot_path_from_source`].
#[deprecated(
    since = "0.5.0",
    note = "use build_implicit_plot_path_from_source instead"
)]
#[allow(dead_code)] // Legacy public API shim; kept for backward compatibility.
pub fn build_implicit_plot_path(
    env: &mut Environment,
    arg_names: &[String],
    body: &CompiledExpr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    resolution: usize,
    padding: &[f64; 4],
) -> kurbo::BezPath {
    let source =
        FuncSource::Compiled(arg_names.to_vec(), Box::new(body.clone()), CapturedEnv::default());
    build_implicit_plot_path_from_source(
        env, &source, p_x_domain, p_y_domain, p_size, resolution, padding,
    )
}
