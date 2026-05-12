use super::{
    build_arrow_path, parse_path_commands_expr, parse_point_list_expr, regular_polygon_points,
    shape_stroke, ShapeType, VectorShapeState, VectorShapeStyle, VelloPath,
};
use crate::ast::Expr;
use crate::timeline::property_lookup::{
    evaluate_expr_with_lookup_diagnostic,
    parse_numeric_vec2_with_lookup_diagnostic,
};
use crate::timeline::{Diagnostic, Environment};
use crate::timeline::kurbo_shapes::KurboShape;

pub trait VectorShapePrimitive: Sync {
    fn shape_type(&self) -> ShapeType;

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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Rect
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Rect
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Circle
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Circle
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Line
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Ellipse
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Arc
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Arrow
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Polygon
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Polygon
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
    fn shape_type(&self) -> ShapeType {
        ShapeType::Path
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

pub fn vector_shape_primitive_for_actor_type(
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

pub fn vector_shape_primitive_for_shape_type(
    shape_type: ShapeType,
) -> Option<&'static dyn VectorShapePrimitive> {
    match shape_type {
        ShapeType::Rect => Some(&RECT_PRIMITIVE),
        ShapeType::Circle => Some(&CIRCLE_PRIMITIVE),
        ShapeType::Line => Some(&LINE_PRIMITIVE),
        ShapeType::Ellipse => Some(&ELLIPSE_PRIMITIVE),
        ShapeType::Arc => Some(&ARC_PRIMITIVE),
        ShapeType::Polygon => Some(&POLYGON_PRIMITIVE),
        ShapeType::Path => Some(&PATH_PRIMITIVE),
        ShapeType::Arrow => Some(&ARROW_PRIMITIVE),
        _ => None,
    }
}
