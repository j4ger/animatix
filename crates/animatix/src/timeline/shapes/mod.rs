use super::property_lookup::parse_numeric_vec2;
use super::{
    Diagnostic, Environment, Interpolate, KurboShape, VelloPath, evaluate_expr,
};
use crate::ast::Expr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShapeType {
    #[default]
    Rect = 0,
    Circle = 1,
    Line = 2,
    Ellipse = 3,
    Arc = 4,
    Polygon = 5,
    Path = 6,
    Arrow = 7,
    Graph = 8,
    Plot = 9,
}

impl ShapeType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Circle => "Circle",
            Self::Line => "Line",
            Self::Ellipse => "Ellipse",
            Self::Arc => "Arc",
            Self::Polygon => "Polygon",
            Self::Path => "Path",
            Self::Arrow => "Arrow",
            Self::Graph => "Graph",
            Self::Plot => "Plot",
        }
    }
}

impl std::str::FromStr for ShapeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Rect" => Ok(Self::Rect),
            "Circle" => Ok(Self::Circle),
            "Line" => Ok(Self::Line),
            "Ellipse" => Ok(Self::Ellipse),
            "Arc" => Ok(Self::Arc),
            "Polygon" => Ok(Self::Polygon),
            "Path" => Ok(Self::Path),
            "Arrow" => Ok(Self::Arrow),
            "Graph" => Ok(Self::Graph),
            "Plot" => Ok(Self::Plot),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ShapeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<u32> for ShapeType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Rect,
            1 => Self::Circle,
            2 => Self::Line,
            3 => Self::Ellipse,
            4 => Self::Arc,
            5 => Self::Polygon,
            6 => Self::Path,
            7 => Self::Arrow,
            8 => Self::Graph,
            9 => Self::Plot,
            _ => Self::Rect,
        }
    }
}

impl Interpolate for ShapeType {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Debug)]
pub struct VectorShapeState {
    pub size: [f32; 2],
    pub line_from: [f32; 2],
    pub line_to: [f32; 2],
    pub arc_angles: [f32; 2],
    pub custom_path: Option<kurbo::BezPath>,
    pub regular_polygon_sides: usize,
    pub regular_polygon_radius: f32,
    pub rotation: f32,
    pub points: Vec<[f32; 2]>,
}

impl VectorShapeState {
    pub fn new(
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
    ) -> Self {
        Self {
            size,
            line_from,
            line_to,
            arc_angles,
            custom_path: None,
            regular_polygon_sides: 5,
            regular_polygon_radius: size[0],
            rotation: 0.0,
            points: Vec::new(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct VectorShapeStyle {
    pub color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    pub fill_opacity: f32,
}
mod primitives;
#[allow(unused_imports)]
pub use primitives::*;
pub fn shape_type_for_actor(ty: &str) -> ShapeType {
    if let Some(primitive) = crate::primitives::find_primitive(ty) {
        if primitive.is_shape() {
            return match ty {
                "Rect" | "Square" => ShapeType::Rect,
                "Circle" | "Dot" => ShapeType::Circle,
                "Line" => ShapeType::Line,
                "Ellipse" => ShapeType::Ellipse,
                "Arc" => ShapeType::Arc,
                "Polygon" | "RegularPolygon" => ShapeType::Polygon,
                "Path" => ShapeType::Path,
                "Arrow" => ShapeType::Arrow,
                _ => ShapeType::Rect,
            };
        }
    }

    match ty {
        "Graph" => ShapeType::Graph,
        "CartesianPlot" | "PolarPlot" | "ParametricPlot" | "ImplicitPlot" => ShapeType::Plot,
        _ => ShapeType::Rect,
    }
}

pub fn apply_vector_shape_defaults(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.apply_defaults(state);
    }
}

pub fn apply_vector_shape_property(
    actor_type: &str,
    name: &str,
    value: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    state: &mut VectorShapeState,
) -> bool {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        return primitive.apply_property(actor_type, name, value, env, diagnostics, subject, state);
    }
    false
}

pub fn finalize_vector_shape_state(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.finalize_state(actor_type, state);
    }
}

pub fn vector_shape_exposes_tip_size(shape_type: ShapeType) -> bool {
    match shape_type {
        ShapeType::Arrow => true,
        _ => false,
    }
}

pub fn vector_shape_uses_custom_path(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Polygon | ShapeType::Path)
}

pub fn build_vector_shape_vello_path(
    shape_type: ShapeType,
    state: &VectorShapeState,
    style: VectorShapeStyle,
) -> Option<VelloPath> {
    // Map shape type back to a primitive type name for lookup
    let type_name = match shape_type {
        ShapeType::Rect => "Rect",
        ShapeType::Circle => "Circle",
        ShapeType::Line => "Line",
        ShapeType::Ellipse => "Ellipse",
        ShapeType::Arc => "Arc",
        ShapeType::Polygon => "Polygon",
        ShapeType::Path => "Path",
        ShapeType::Arrow => "Arrow",
        _ => return None,
    };
    crate::primitives::find_primitive(type_name)
        .and_then(|primitive| {
            primitive.render(&crate::primitives::RenderCtx {
                state,
                style,
                time_ms: 0,
            })
        })
        .map(|paths| paths.into_iter().next())
        .flatten()
}

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

pub fn build_shape(
    shape_type: ShapeType,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
) -> KurboShape {
    match shape_type {
        ShapeType::Circle => KurboShape::Circle {
            center: kurbo::Point::new(0.0, 0.0),
            radius: size[0] as f64,
        },
        ShapeType::Line => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        ShapeType::Ellipse => KurboShape::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            rotation: 0.0,
        },
        ShapeType::Arc => KurboShape::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
            start_angle: arc_angles[0] as f64,
            sweep_angle: arc_angles[1] as f64,
            rotation: 0.0,
        },
        ShapeType::Arrow => KurboShape::Path {
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

pub fn shape_fill_color(
    shape_type: ShapeType,
    color: [f32; 4],
    fill_opacity: f32,
) -> Option<vello::peniko::Color> {
    if fill_opacity <= 0.0 {
        return None;
    }

    match shape_type {
        ShapeType::Line | ShapeType::Arc => return None,
        _ => {}
    }

    Some(vello::peniko::Color::from_rgba8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0 * fill_opacity) as u8,
    ))
}

pub fn shape_stroke(
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

pub fn build_shape_vello_path(
    shape_type: ShapeType,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
    color: [f32; 4],
    stroke_width: f32,
    stroke_color: [f32; 4],
    fill_opacity: f32,
) -> VelloPath {
    let state = VectorShapeState::new(size, line_from, line_to, arc_angles);
    build_vector_shape_vello_path(
        shape_type,
        &state,
        VectorShapeStyle {
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
        },
    )
    .unwrap_or_else(|| VelloPath {
        path: build_shape(shape_type, size, line_from, line_to, arc_angles).to_path_default(),
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_default_size_is_specialized() {
        let mut state = VectorShapeState::new([50.0, 50.0], [-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]);
        apply_vector_shape_defaults("Dot", &mut state);
        assert_eq!(state.size, [6.0, 6.0]);
    }

    #[test]
    fn arrow_reports_tip_lookup_support() {
        assert!(vector_shape_exposes_tip_size(ShapeType::Arrow));
        assert!(!vector_shape_exposes_tip_size(ShapeType::Rect));
    }

    #[test]
    fn polygon_shapes_report_custom_path_usage() {
        assert!(vector_shape_uses_custom_path(ShapeType::Polygon));
        assert!(vector_shape_uses_custom_path(ShapeType::Path));
        assert!(!vector_shape_uses_custom_path(ShapeType::Rect));
    }

    #[test]
    fn regular_polygon_finalizes_custom_path() {
        let mut state = VectorShapeState::new([50.0, 50.0], [-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]);
        finalize_vector_shape_state("RegularPolygon", &mut state);
        assert!(state.custom_path.is_some());
    }
}
