use super::{Environment, Value, evaluate_expr};
use crate::ast::Expr;

pub(crate) fn sample_recursive_cartesian(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
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

    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
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
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Num(0.0)).as_num();

    let math_x = mid_t;
    let math_y = val;

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
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
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
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Num(0.0)).as_num();

    let math_x = val * mid_t.cos();
    let math_y = val * mid_t.sin();

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
    env: &mut Environment,
    arg_name: &str,
    body: &Expr,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
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
    env.set(arg_name, Value::Num(mid_t));
    let val = evaluate_expr(body, env).unwrap_or(Value::Vec2([0.0, 0.0]));
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
    env: &mut Environment,
    arg_names: &[String],
    body: &Expr,
    x: f64,
    y: f64,
) -> f64 {
    let x_name = arg_names.first().map(String::as_str).unwrap_or("x");
    let y_name = arg_names.get(1).map(String::as_str).unwrap_or("y");
    env.set(x_name, Value::Num(x));
    env.set(y_name, Value::Num(y));
    evaluate_expr(body, env)
        .unwrap_or(Value::Num(f64::NAN))
        .as_num()
}

pub(crate) fn build_implicit_plot_path(
    env: &mut Environment,
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

    for yi in 0..y_cells {
        let y0 = p_y_domain[0] + yi as f64 * dy;
        let y1 = y0 + dy;
        for xi in 0..x_cells {
            let x0 = p_x_domain[0] + xi as f64 * dx;
            let x1 = x0 + dx;

            let bl = (
                x0,
                y0,
                evaluate_implicit_value(env, arg_names, body, x0, y0),
            );
            let br = (
                x1,
                y0,
                evaluate_implicit_value(env, arg_names, body, x1, y0),
            );
            let tr = (
                x1,
                y1,
                evaluate_implicit_value(env, arg_names, body, x1, y1),
            );
            let tl = (
                x0,
                y1,
                evaluate_implicit_value(env, arg_names, body, x0, y1),
            );

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
                _ => {}
            }
        }
    }

    path
}
