use super::property_lookup::parse_numeric_vec2;
use super::{
    Diagnostic, Environment, KurboShape, VelloPath, evaluate_expr,
    evaluate_expr_with_lookup_diagnostic, parse_numeric_vec2_with_lookup_diagnostic,
};
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

#[derive(Clone, Debug)]
pub(crate) struct VectorShapeState {
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
    pub(crate) fn new(
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
pub(crate) struct VectorShapeStyle {
    pub color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    pub fill_opacity: f32,
}

pub(crate) trait VectorShapePrimitive: Sync {
    fn shape_type(&self) -> u32;

    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    fn apply_property(
        &self,
        _actor_type: &str,
        _name: &str,
        _value: &Expr,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        _state: &mut VectorShapeState,
    ) -> bool {
        false
    }

    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath;

    fn supports_fill(&self) -> bool {
        true
    }

    fn uses_custom_path(&self) -> bool {
        false
    }

    fn exposes_tip_size(&self) -> bool {
        false
    }

    fn build_vello_path(&self, state: &VectorShapeState, style: VectorShapeStyle) -> VelloPath {
        VelloPath {
            path: self.build_path(state),
            fill: if self.supports_fill() && style.fill_opacity > 0.0 {
                Some(vello::peniko::Color::from_rgba8(
                    (style.color[0] * 255.0) as u8,
                    (style.color[1] * 255.0) as u8,
                    (style.color[2] * 255.0) as u8,
                    (style.color[3] * 255.0 * style.fill_opacity) as u8,
                ))
            } else {
                None
            },
            stroke: shape_stroke(style.stroke_color, style.stroke_width),
        }
    }
}

struct RectPrimitive;
struct CirclePrimitive;
struct LinePrimitive;
struct EllipsePrimitive;
struct ArcPrimitive;
struct PolygonPrimitive;
struct PathPrimitive;
struct ArrowPrimitive;
struct SquarePrimitive;
struct DotPrimitive;
struct RegularPolygonPrimitive;

static RECT_PRIMITIVE: RectPrimitive = RectPrimitive;
static CIRCLE_PRIMITIVE: CirclePrimitive = CirclePrimitive;
static LINE_PRIMITIVE: LinePrimitive = LinePrimitive;
static ELLIPSE_PRIMITIVE: EllipsePrimitive = EllipsePrimitive;
static ARC_PRIMITIVE: ArcPrimitive = ArcPrimitive;
static POLYGON_PRIMITIVE: PolygonPrimitive = PolygonPrimitive;
static PATH_PRIMITIVE: PathPrimitive = PathPrimitive;
static ARROW_PRIMITIVE: ArrowPrimitive = ArrowPrimitive;
static SQUARE_PRIMITIVE: SquarePrimitive = SquarePrimitive;
static DOT_PRIMITIVE: DotPrimitive = DotPrimitive;
static REGULAR_POLYGON_PRIMITIVE: RegularPolygonPrimitive = RegularPolygonPrimitive;

impl VectorShapePrimitive for RectPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_RECT
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        KurboShape::Rect {
            x0: -(state.size[0] as f64),
            y0: -(state.size[1] as f64),
            x1: state.size[0] as f64,
            y1: state.size[1] as f64,
        }
        .to_path_default()
    }
}

impl VectorShapePrimitive for SquarePrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_RECT
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        if name != "side" {
            return false;
        }

        let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(crate::timeline::Value::Num(state.size[0] as f64 * 2.0));
        let side = v.as_num() as f32;
        state.size = [side / 2.0, side / 2.0];
        true
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        RECT_PRIMITIVE.build_path(state)
    }
}

impl VectorShapePrimitive for CirclePrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_CIRCLE
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        KurboShape::Circle {
            center: kurbo::Point::new(0.0, 0.0),
            radius: state.size[0] as f64,
        }
        .to_path_default()
    }
}

impl VectorShapePrimitive for DotPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_CIRCLE
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        if state.size == [50.0, 50.0] {
            state.size = [6.0, 6.0];
            state.regular_polygon_radius = 6.0;
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        CIRCLE_PRIMITIVE.build_path(state)
    }
}

impl VectorShapePrimitive for LinePrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_LINE
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "from" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    state.line_from = parsed;
                }
                true
            }
            "to" => {
                if let Some(parsed) =
                    parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    state.line_to = parsed;
                }
                true
            }
            _ => false,
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        KurboShape::Line {
            p0: kurbo::Point::new(state.line_from[0] as f64, state.line_from[1] as f64),
            p1: kurbo::Point::new(state.line_to[0] as f64, state.line_to[1] as f64),
        }
        .to_path_default()
    }

    fn supports_fill(&self) -> bool {
        false
    }
}

impl VectorShapePrimitive for EllipsePrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_ELLIPSE
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "radius_x" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[0] as f64));
                state.size[0] = v.as_num() as f32;
                true
            }
            "radius_y" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[1] as f64));
                state.size[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        KurboShape::Ellipse {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(state.size[0] as f64, state.size[1] as f64),
            rotation: state.rotation as f64,
        }
        .to_path_default()
    }
}

impl VectorShapePrimitive for ArcPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_ARC
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "radius_x" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[0] as f64));
                state.size[0] = v.as_num() as f32;
                true
            }
            "radius_y" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[1] as f64));
                state.size[1] = v.as_num() as f32;
                true
            }
            "start_angle" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.arc_angles[0] as f64));
                state.arc_angles[0] = v.as_num() as f32;
                true
            }
            "sweep_angle" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.arc_angles[1] as f64));
                state.arc_angles[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        KurboShape::Arc {
            center: kurbo::Point::new(0.0, 0.0),
            radii: kurbo::Vec2::new(state.size[0] as f64, state.size[1] as f64),
            start_angle: state.arc_angles[0] as f64,
            sweep_angle: state.arc_angles[1] as f64,
            rotation: state.rotation as f64,
        }
        .to_path_default()
    }

    fn supports_fill(&self) -> bool {
        false
    }
}

impl VectorShapePrimitive for ArrowPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_ARROW
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        if state.size == [50.0, 50.0] {
            state.size = [24.0, 18.0];
        }
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "tip_length" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[0] as f64));
                state.size[0] = v.as_num() as f32;
                true
            }
            "tip_width" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(state.size[1] as f64));
                state.size[1] = v.as_num() as f32;
                true
            }
            _ => false,
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        build_arrow_path(state.line_from, state.line_to, state.size[0], state.size[1])
    }

    fn exposes_tip_size(&self) -> bool {
        true
    }
}

impl VectorShapePrimitive for PolygonPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_POLYGON
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        if name != "points" {
            return false;
        }
        if let Some(points) = parse_point_list_expr(value, env) {
            state.custom_path = Some(KurboShape::Polygon { points }.to_path_default());
        }
        true
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        // Use dynamic points from property assignment if available
        if !state.points.is_empty() {
            let mut path = kurbo::BezPath::new();
            if let Some(first) = state.points.first() {
                path.move_to(kurbo::Point::new(first[0] as f64, first[1] as f64));
                for point in &state.points[1..] {
                    path.line_to(kurbo::Point::new(point[0] as f64, point[1] as f64));
                }
                path.close_path();
            }
            return path;
        }
        state
            .custom_path
            .clone()
            .unwrap_or_else(kurbo::BezPath::new)
    }

    fn uses_custom_path(&self) -> bool {
        true
    }
}

impl VectorShapePrimitive for RegularPolygonPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_POLYGON
    }

    fn apply_property(
        &self,
        actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        match name {
            "points" => POLYGON_PRIMITIVE.apply_property(
                actor_type,
                name,
                value,
                env,
                diagnostics,
                subject,
                state,
            ),
            "sides" => {
                let v = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .unwrap_or(crate::timeline::Value::Num(
                        state.regular_polygon_sides as f64,
                    ));
                state.regular_polygon_sides = v.as_num().round().max(3.0) as usize;
                true
            }
            _ => false,
        }
    }

    fn finalize_state(&self, _actor_type: &str, state: &mut VectorShapeState) {
        if state.custom_path.is_none() {
            state.custom_path = Some(
                KurboShape::Polygon {
                    points: regular_polygon_points(
                        state.regular_polygon_sides,
                        state.regular_polygon_radius,
                        state.rotation,
                    ),
                }
                .to_path_default(),
            );
        }
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        POLYGON_PRIMITIVE.build_path(state)
    }
}

impl VectorShapePrimitive for PathPrimitive {
    fn shape_type(&self) -> u32 {
        SHAPE_PATH
    }

    fn apply_property(
        &self,
        _actor_type: &str,
        name: &str,
        value: &Expr,
        env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        if name != "commands" {
            return false;
        }
        state.custom_path = parse_path_commands_expr(value, env);
        true
    }

    fn build_path(&self, state: &VectorShapeState) -> kurbo::BezPath {
        state
            .custom_path
            .clone()
            .unwrap_or_else(kurbo::BezPath::new)
    }

    fn uses_custom_path(&self) -> bool {
        true
    }
}

pub(crate) fn vector_shape_primitive_for_actor_type(
    ty: &str,
) -> Option<&'static dyn VectorShapePrimitive> {
    match ty {
        "Rect" => Some(&RECT_PRIMITIVE),
        "Square" => Some(&SQUARE_PRIMITIVE),
        "Circle" => Some(&CIRCLE_PRIMITIVE),
        "Dot" => Some(&DOT_PRIMITIVE),
        "Line" => Some(&LINE_PRIMITIVE),
        "Ellipse" => Some(&ELLIPSE_PRIMITIVE),
        "Arc" => Some(&ARC_PRIMITIVE),
        "Polygon" => Some(&POLYGON_PRIMITIVE),
        "RegularPolygon" => Some(&REGULAR_POLYGON_PRIMITIVE),
        "Path" => Some(&PATH_PRIMITIVE),
        "Arrow" => Some(&ARROW_PRIMITIVE),
        _ => None,
    }
}

pub(crate) fn vector_shape_primitive_for_shape_type(
    shape_type: u32,
) -> Option<&'static dyn VectorShapePrimitive> {
    match shape_type {
        SHAPE_RECT => Some(&RECT_PRIMITIVE),
        SHAPE_CIRCLE => Some(&CIRCLE_PRIMITIVE),
        SHAPE_LINE => Some(&LINE_PRIMITIVE),
        SHAPE_ELLIPSE => Some(&ELLIPSE_PRIMITIVE),
        SHAPE_ARC => Some(&ARC_PRIMITIVE),
        SHAPE_POLYGON => Some(&POLYGON_PRIMITIVE),
        SHAPE_PATH => Some(&PATH_PRIMITIVE),
        SHAPE_ARROW => Some(&ARROW_PRIMITIVE),
        _ => None,
    }
}

pub(crate) fn shape_type_for_actor(ty: &str) -> u32 {
    if let Some(primitive) = vector_shape_primitive_for_actor_type(ty) {
        return primitive.shape_type();
    }

    match ty {
        "Graph" => SHAPE_GRAPH,
        "CartesianPlot" | "PolarPlot" | "ParametricPlot" | "ImplicitPlot" => SHAPE_PLOT,
        _ => SHAPE_RECT,
    }
}

pub(crate) fn apply_vector_shape_defaults(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = vector_shape_primitive_for_actor_type(actor_type) {
        primitive.apply_defaults(state);
    }
}

pub(crate) fn apply_vector_shape_property(
    actor_type: &str,
    name: &str,
    value: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    state: &mut VectorShapeState,
) -> bool {
    if let Some(primitive) = vector_shape_primitive_for_actor_type(actor_type) {
        return primitive.apply_property(actor_type, name, value, env, diagnostics, subject, state);
    }
    false
}

pub(crate) fn finalize_vector_shape_state(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = vector_shape_primitive_for_actor_type(actor_type) {
        primitive.finalize_state(actor_type, state);
    }
}

pub(crate) fn vector_shape_exposes_tip_size(shape_type: u32) -> bool {
    vector_shape_primitive_for_shape_type(shape_type)
        .map(VectorShapePrimitive::exposes_tip_size)
        .unwrap_or(false)
}

pub(crate) fn vector_shape_uses_custom_path(shape_type: u32) -> bool {
    vector_shape_primitive_for_shape_type(shape_type)
        .map(VectorShapePrimitive::uses_custom_path)
        .unwrap_or(false)
}

pub(crate) fn build_vector_shape_vello_path(
    shape_type: u32,
    state: &VectorShapeState,
    style: VectorShapeStyle,
) -> Option<VelloPath> {
    vector_shape_primitive_for_shape_type(shape_type)
        .map(|primitive| primitive.build_vello_path(state, style))
}

pub(crate) fn regular_polygon_points(sides: usize, radius: f32, rotation: f32) -> Vec<kurbo::Point> {
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
    let state = VectorShapeState::new(size, line_from, line_to, arc_angles);
    if let Some(primitive) = vector_shape_primitive_for_shape_type(shape_type) {
        let path = primitive.build_path(&state);
        if matches!(shape_type, SHAPE_ARROW | SHAPE_POLYGON | SHAPE_PATH) {
            return KurboShape::Path { path };
        }
    }

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
    if fill_opacity <= 0.0 {
        return None;
    }

    if let Some(primitive) = vector_shape_primitive_for_shape_type(shape_type)
        && !primitive.supports_fill()
    {
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
        assert!(vector_shape_exposes_tip_size(SHAPE_ARROW));
        assert!(!vector_shape_exposes_tip_size(SHAPE_RECT));
    }

    #[test]
    fn polygon_shapes_report_custom_path_usage() {
        assert!(vector_shape_uses_custom_path(SHAPE_POLYGON));
        assert!(vector_shape_uses_custom_path(SHAPE_PATH));
        assert!(!vector_shape_uses_custom_path(SHAPE_RECT));
    }

    #[test]
    fn regular_polygon_finalizes_custom_path() {
        let mut state = VectorShapeState::new([50.0, 50.0], [-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]);
        finalize_vector_shape_state("RegularPolygon", &mut state);
        assert!(state.custom_path.is_some());
    }
}
