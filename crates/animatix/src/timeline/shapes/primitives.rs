use crate::timeline::property_lookup::parse_numeric_vec2;
use crate::ast::Expr;
use crate::timeline::Environment;

// ── Helper functions (kept for use by primitive implementations) ─────────

#[allow(dead_code)]
pub fn regular_polygon_points(sides: usize, radius: f32, rotation: f32) -> Vec<kurbo::Point> {
    let sides = sides.max(3);
    let radius = radius as f64;
    let angle_step = std::f64::consts::TAU / sides as f64;
    (0..sides)
        .map(|index| {
            let angle = -std::f64::consts::FRAC_PI_2 + rotation as f64 + angle_step * index as f64;
            kurbo::Point::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

#[allow(dead_code)]
pub fn build_arrow_path(
    line_from: [f32; 2],
    line_to: [f32; 2],
    tip_length: f32,
    tip_width: f32,
) -> kurbo::BezPath {
    let start = kurbo::Point::new(line_from[0] as f64, line_from[1] as f64);
    let tip = kurbo::Point::new(line_to[0] as f64, line_to[1] as f64);
    let dx = tip.x - start.x;
    let dy = tip.y - start.y;
    let length = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();
    if length <= f64::EPSILON {
        path.move_to(tip);
        path.close_path();
        return path;
    }

    let dir_x = dx / length;
    let dir_y = dy / length;
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let tip_length = tip_length.max(1.0) as f64;
    let half_tip_width = (tip_width.max(1.0) as f64) / 2.0;
    let base = kurbo::Point::new(tip.x - dir_x * tip_length, tip.y - dir_y * tip_length);
    let left = kurbo::Point::new(
        base.x + perp_x * half_tip_width,
        base.y + perp_y * half_tip_width,
    );
    let right = kurbo::Point::new(
        base.x - perp_x * half_tip_width,
        base.y - perp_y * half_tip_width,
    );

    path.move_to(start);
    path.line_to(base);
    path.move_to(tip);
    path.line_to(left);
    path.line_to(right);
    path.close_path();
    path
}

#[allow(dead_code)]
pub fn parse_point_list_expr(expr: &Expr, env: &Environment) -> Option<Vec<kurbo::Point>> {
    match expr {
        Expr::Tuple(items) => {
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let [x, y] = parse_numeric_vec2(item, env)?;
                points.push(kurbo::Point::new(x as f64, y as f64));
            }
            Some(points)
        }
        _ => None,
    }
}

#[allow(dead_code)]
pub fn parse_path_commands_expr(expr: &Expr, env: &Environment) -> Option<kurbo::BezPath> {
    let Expr::Tuple(items) = expr else {
        return None;
    };

    let mut path = kurbo::BezPath::new();

    for item in items {
        let Expr::Call(name, args) = item else {
            return None;
        };

        match name.as_str() {
            "move_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.move_to((x, y));
            }
            "line_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.line_to((x, y));
            }
            "quad_to" => {
                if args.len() != 4 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                path.quad_to((x1, y1), (x2, y2));
            }
            "curve_to" => {
                if args.len() != 6 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                let x3 = evaluate_expr(&args[4], env).ok()?.as_num();
                let y3 = evaluate_expr(&args[5], env).ok()?.as_num();
                path.curve_to((x1, y1), (x2, y2), (x3, y3));
            }
            "close" => {
                if !args.is_empty() {
                    return None;
                }
                path.close_path();
            }
            _ => return None,
        }
    }

    Some(path)
}

use crate::timeline::evaluate_expr;

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::VectorShapeState;

    #[test]
    fn ellipse_default_size_is_standard() {
        use crate::primitives::Primitive;
        let mut state = VectorShapeState::new([50.0, 50.0], [-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]);
        crate::primitives::ELLIPSE.apply_defaults("Ellipse", &mut state);
        // Ellipse uses standard size defaults
        assert_eq!(state.size, [50.0, 50.0]);
    }

    #[test]
    fn line_reports_tip_lookup_support() {
        use crate::primitives::Primitive;
        assert!(crate::primitives::LINE.exposes_tip_size());
        assert!(!crate::primitives::RECT.exposes_tip_size());
    }

    #[test]
    fn polygon_shapes_report_custom_path_usage() {
        use crate::primitives::Primitive;
        assert!(crate::primitives::POLYGON.uses_custom_path());
        assert!(crate::primitives::PATH.uses_custom_path());
        assert!(!crate::primitives::RECT.uses_custom_path());
    }

    #[test]
    fn polygon_with_sides_finalizes_custom_path() {
        use crate::primitives::Primitive;
        let mut state = VectorShapeState::new([50.0, 50.0], [-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]);
        state.regular_polygon_sides = 5;
        state.regular_polygon_radius = 50.0;
        crate::primitives::POLYGON.finalize_state("Polygon", &mut state);
        assert!(state.custom_path.is_some());
    }
}
