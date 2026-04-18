use super::property_lookup::parse_numeric_vec2;
use super::{Environment, KurboShape, VelloPath, evaluate_expr};
use crate::ast::Expr;

pub(crate) const SHAPE_RECT: u32 = 0;
pub(crate) const SHAPE_CIRCLE: u32 = 1;
pub(crate) const SHAPE_LINE: u32 = 2;
pub(crate) const SHAPE_ELLIPSE: u32 = 3;
pub(crate) const SHAPE_ARC: u32 = 4;
pub(crate) const SHAPE_POLYGON: u32 = 5;
pub(crate) const SHAPE_PATH: u32 = 6;
pub(crate) const SHAPE_ARROW: u32 = 7;
pub(crate) const SHAPE_GRAPH: u32 = 8;
pub(crate) const SHAPE_PLOT: u32 = 9;

pub(crate) fn shape_type_for_actor(ty: &str) -> u32 {
    match ty {
        "Circle" | "Dot" => SHAPE_CIRCLE,
        "Line" => SHAPE_LINE,
        "Arrow" => SHAPE_ARROW,
        "Ellipse" => SHAPE_ELLIPSE,
        "Arc" => SHAPE_ARC,
        "Polygon" | "RegularPolygon" => SHAPE_POLYGON,
        "Path" => SHAPE_PATH,
        "Graph" => SHAPE_GRAPH,
        "CartesianPlot" | "PolarPlot" | "ParametricPlot" | "ImplicitPlot" => SHAPE_PLOT,
        _ => SHAPE_RECT,
    }
}

pub(crate) fn regular_polygon_points(sides: usize, radius: f32) -> Vec<kurbo::Point> {
    let sides = sides.max(3);
    let radius = radius as f64;
    let angle_step = std::f64::consts::TAU / sides as f64;
    (0..sides)
        .map(|index| {
            let angle = -std::f64::consts::FRAC_PI_2 + angle_step * index as f64;
            kurbo::Point::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

pub(crate) fn build_arrow_path(
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

pub(crate) fn parse_point_list_expr(expr: &Expr, env: &Environment) -> Option<Vec<kurbo::Point>> {
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

pub(crate) fn parse_path_commands_expr(expr: &Expr, env: &Environment) -> Option<kurbo::BezPath> {
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

pub(crate) fn build_shape(
    shape_type: u32,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
) -> KurboShape {
    match shape_type {
        SHAPE_CIRCLE => KurboShape::Circle {
            center: kurbo::Point::new(0.0, 0.0),
            radius: size[0] as f64,
        },
        SHAPE_LINE => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        SHAPE_ELLIPSE => KurboShape::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            rotation: 0.0,
        },
        SHAPE_ARC => KurboShape::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            start_angle: arc_angles[0] as f64,
            sweep_angle: arc_angles[1] as f64,
            rotation: 0.0,
        },
        SHAPE_ARROW => KurboShape::Path {
            path: build_arrow_path(line_from, line_to, size[0], size[1]),
        },
        _ => KurboShape::Rect {
            x0: -(size[0] as f64),
            y0: -(size[1] as f64),
            x1: size[0] as f64,
            y1: size[1] as f64,
        },
    }
}

pub(crate) fn shape_fill_color(
    shape_type: u32,
    color: [f32; 4],
    fill_opacity: f32,
) -> Option<vello::peniko::Color> {
    if matches!(shape_type, SHAPE_LINE | SHAPE_ARC) || fill_opacity <= 0.0 {
        return None;
    }

    Some(vello::peniko::Color::from_rgba8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0 * fill_opacity) as u8,
    ))
}

pub(crate) fn shape_stroke(
    stroke_color: [f32; 4],
    stroke_width: f32,
) -> Option<(vello::peniko::Color, f32)> {
    if stroke_width <= 0.0 {
        return None;
    }

    Some((
        vello::peniko::Color::from_rgba8(
            (stroke_color[0] * 255.0) as u8,
            (stroke_color[1] * 255.0) as u8,
            (stroke_color[2] * 255.0) as u8,
            (stroke_color[3] * 255.0) as u8,
        ),
        stroke_width,
    ))
}

pub(crate) fn build_shape_vello_path(
    shape_type: u32,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    let shape = build_shape(shape_type, size, line_from, line_to, arc_angles);

    VelloPath {
        path: shape.to_path_default(),
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
    }
}

pub(crate) fn styled_vello_path(
    path: kurbo::BezPath,
    shape_type: u32,
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    VelloPath {
        path,
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
    }
}
