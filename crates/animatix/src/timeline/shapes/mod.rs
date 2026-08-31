#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::lookup::parse_numeric_vec2;
use super::{Diagnostic, Environment, Interpolate, KurboShape, VelloPath, evaluate_expr};
use crate::ast::Expr;
use crate::timeline::actor_kind::{ActorKindId, ShapeKind};

/// Discriminant for the kind of geometric shape an actor represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ShapeType {
    /// Rectangle.
    #[default]
    Rect = 0,
    /// Ellipse (or arc when `arc_angles` are non-zero).
    Ellipse = 1,
    /// Line, optionally with an arrow tip.
    Line = 2,
    /// Polygon, regular or custom.
    Polygon = 3,
    /// Free-form path defined by Bézier commands.
    Path = 4,
    /// Graph / coordinate plane.
    Graph = 5,
    /// Plot curve.
    Plot = 6,
    /// Arrow with a dedicated arrowhead.
    Arrow = 7,
}

impl ShapeType {
    /// Return the static string name of this variant.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
            Self::Polygon => "Polygon",
            Self::Path => "Path",
            Self::Graph => "Graph",
            Self::Plot => "Plot",
            Self::Arrow => "Arrow",
        }
    }
}

impl std::str::FromStr for ShapeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Rect" => Ok(Self::Rect),
            "Ellipse" => Ok(Self::Ellipse),
            "Line" => Ok(Self::Line),
            "Polygon" => Ok(Self::Polygon),
            "Path" => Ok(Self::Path),
            "Graph" => Ok(Self::Graph),
            "PlotCurve" => Ok(Self::Plot),
            "Arrow" => Ok(Self::Arrow),
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
            1 => Self::Ellipse,
            2 => Self::Line,
            3 => Self::Polygon,
            4 => Self::Path,
            5 => Self::Graph,
            6 => Self::Plot,
            7 => Self::Arrow,
            _ => Self::Rect,
        }
    }
}

impl Interpolate for ShapeType {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// State for a rectangle shape.
#[derive(Clone, Debug)]
pub struct RectState {
    /// Width and height of the rectangle.
    pub size: [f32; 2],
}

impl Default for RectState {
    fn default() -> Self {
        Self { size: [50.0, 50.0] }
    }
}

/// State for an ellipse (or arc) shape.
#[derive(Clone, Debug)]
pub struct EllipseState {
    /// Width and height of the ellipse.
    pub size: [f32; 2],
    /// Start and sweep angles for an arc (zero = full ellipse).
    pub arc_angles: [f32; 2],
    /// Rotation of the ellipse in radians.
    pub rotation: f32,
}

impl Default for EllipseState {
    fn default() -> Self {
        Self {
            size: [50.0, 50.0],
            arc_angles: [0.0, 0.0],
            rotation: 0.0,
        }
    }
}

/// State for a line shape, optionally with an arrow tip.
#[derive(Clone, Debug)]
pub struct LineState {
    /// tip_length, tip_width (0.0 = no arrow)
    pub size: [f32; 2],
    /// Start point of the line.
    pub line_from: [f32; 2],
    /// End point of the line.
    pub line_to: [f32; 2],
}

impl Default for LineState {
    fn default() -> Self {
        Self {
            size: [0.0, 0.0],
            line_from: [-50.0, 0.0],
            line_to: [50.0, 0.0],
        }
    }
}

/// State for a polygon shape, regular or custom.
#[derive(Clone, Debug)]
pub struct PolygonState {
    /// Width and height of the polygon bounding box.
    pub size: [f32; 2],
    /// Number of sides for a regular polygon (0 = not regular).
    pub regular_polygon_sides: usize,
    /// Radius of the circumscribed circle for a regular polygon.
    pub regular_polygon_radius: f32,
    /// Optional custom Bézier path.
    pub custom_path: Option<kurbo::BezPath>,
    /// Rotation of the polygon in radians.
    pub rotation: f32,
    /// Explicit list of vertex points (overrides regular polygon).
    pub points: Vec<[f32; 2]>,
}

impl Default for PolygonState {
    fn default() -> Self {
        Self {
            size: [50.0, 50.0],
            regular_polygon_sides: 0,
            regular_polygon_radius: 50.0,
            custom_path: None,
            rotation: 0.0,
            points: Vec::new(),
        }
    }
}

/// State for a free-form path shape.
#[derive(Clone, Debug)]
pub struct PathState {
    /// Width and height of the path bounding box.
    pub size: [f32; 2],
    /// Optional custom Bézier path.
    pub custom_path: Option<kurbo::BezPath>,
}

impl Default for PathState {
    fn default() -> Self {
        Self {
            size: [50.0, 50.0],
            custom_path: None,
        }
    }
}

/// State for an arrow shape with a dedicated arrowhead.
#[derive(Clone, Debug)]
pub struct ArrowState {
    /// Start point of the arrow.
    pub from: [f32; 2],
    /// End point of the arrow (arrowhead points here).
    pub to: [f32; 2],
    /// Size of the arrowhead triangle (length and half-width derived from this).
    pub head_size: f32,
}

/// State for a callout annotation (arrow geometry).
#[derive(Clone, Debug, Default)]
pub struct CalloutState {
    /// Start point of the callout arrow.
    pub from: [f32; 2],
    /// End point of the callout arrow (arrowhead points here).
    pub to: [f32; 2],
    /// Size of the arrowhead triangle.
    pub head_size: f32,
}

impl Default for ArrowState {
    fn default() -> Self {
        Self {
            from: [-50.0, 0.0],
            to: [50.0, 0.0],
            head_size: 10.0,
        }
    }
}

/// Per-shape state enum that only holds fields relevant to each shape type.
///
/// Previously `VectorShapeState` was a flat struct with dead fields
/// (e.g. `arc_angles` on Rect, `regular_polygon_sides` on Ellipse).
/// Now each variant carries exactly the fields it needs.
#[derive(Clone, Debug)]
pub enum VectorShapeState {
    /// Rectangle shape state.
    Rect(RectState),
    /// Ellipse (or arc) shape state.
    Ellipse(EllipseState),
    /// Line shape state, optionally with an arrow tip.
    Line(LineState),
    /// Polygon shape state, regular or custom.
    Polygon(PolygonState),
    /// Free-form path shape state.
    Path(PathState),
    /// Arrow shape state with a dedicated arrowhead.
    Arrow(ArrowState),
    /// Callout annotation state (arrow geometry).
    Callout(CalloutState),
}

impl VectorShapeState {
    /// Create the appropriate state variant for a given shape type.
    ///
    /// `size` is shared by all shapes; shape-specific params like `line_from` /
    /// `line_to` / `arc_angles` are only stored when the variant supports them.
    pub fn new(shape_type: ShapeType, size: [f32; 2]) -> Self {
        match shape_type {
            ShapeType::Rect => Self::Rect(RectState { size }),
            ShapeType::Ellipse => Self::Ellipse(EllipseState {
                size,
                arc_angles: [0.0, 0.0],
                rotation: 0.0,
            }),
            ShapeType::Line => Self::Line(LineState {
                size: [0.0, 0.0], // no arrow tip by default
                line_from: [-50.0, 0.0],
                line_to: [50.0, 0.0],
            }),
            ShapeType::Polygon => Self::Polygon(PolygonState {
                size,
                regular_polygon_sides: 0,
                regular_polygon_radius: size[0],
                custom_path: None,
                rotation: 0.0,
                points: Vec::new(),
            }),
            ShapeType::Path => Self::Path(PathState {
                size,
                custom_path: None,
            }),
            // Graph/Plot are not vector shapes with state
            ShapeType::Graph | ShapeType::Plot => Self::Rect(RectState { size }),
            ShapeType::Arrow => Self::Arrow(ArrowState {
                from: [-50.0, 0.0],
                to: [50.0, 0.0],
                head_size: 10.0,
            }),
        }
    }

    /// Shared accessor: all shapes have a size.
    pub fn size(&self) -> [f32; 2] {
        match self {
            Self::Rect(s) => s.size,
            Self::Ellipse(s) => s.size,
            Self::Line(s) => s.size,
            Self::Polygon(s) => s.size,
            Self::Path(s) => s.size,
            Self::Arrow(_a) => [0.0, 0.0],
            Self::Callout(_c) => [0.0, 0.0],
        }
    }

    /// Shared mutable accessor for shapes that own a size. Returns `None` for
    /// Arrow/Callout, which derive their visual extent from line endpoints.
    pub fn size_mut(&mut self) -> Option<&mut [f32; 2]> {
        match self {
            Self::Rect(s) => Some(&mut s.size),
            Self::Ellipse(s) => Some(&mut s.size),
            Self::Line(s) => Some(&mut s.size),
            Self::Polygon(s) => Some(&mut s.size),
            Self::Path(s) => Some(&mut s.size),
            Self::Arrow(_) | Self::Callout(_) => None,
        }
    }
}

/// Render style shared by all vector shapes.
#[derive(Clone, Copy)]
pub struct VectorShapeStyle {
    /// Fill / default color as `[r, g, b, a]` in 0..1.
    pub color: [f32; 4],
    /// Stroke width in logical pixels (0.0 = no stroke).
    pub stroke_width: f32,
    /// Stroke color as `[r, g, b, a]` in 0..1.
    pub stroke_color: [f32; 4],
    /// Additional opacity multiplier for the fill.
    pub fill_opacity: f32,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: u32,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: u32,
}

/// Default stroke width for an actor kind.
///
/// Stroke-only actors need a visible outline by default; filled shapes do not,
/// and a hidden white outline was visible as asymmetric edge artifacts on
/// plain `Rect`s.
pub fn default_stroke_width(kind: ActorKindId) -> f32 {
    match kind {
        ActorKindId::Shape(ShapeKind::Line | ShapeKind::Arrow)
        | ActorKindId::Callout
        | ActorKindId::PlotCurve
        // Stroke-drawn plots: VectorField/ContourSet draw arrows/contours via
        // `stroke`, so a zero default width makes them invisible when the user
        // sets only `color:`. `color` is used as their stroke color, so a
        // non-zero default renders them as authored.
        | ActorKindId::VectorField
        | ActorKindId::ContourSet => 2.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod primitives;

/// Map an actor type string to its corresponding `ShapeType`, if any.
pub fn shape_type_for_actor(ty: &str) -> Option<ShapeType> {
    // Resolve through the primitive's `kind_id()` instead of a parallel
    // string table, so the mapping cannot drift from `ActorKindId`.
    match crate::primitives::find_primitive(ty).map(|p| p.kind_id()) {
        Some(ActorKindId::Shape(kind)) => Some(shape_kind_to_shape_type(kind)),
        // Plot-host kinds reuse the Graph command geometry for edit handles.
        Some(ActorKindId::Graph | ActorKindId::NumberPlane) => Some(ShapeType::Graph),
        Some(ActorKindId::PlotCurve) => Some(ShapeType::Plot),
        _ => None,
    }
}

fn shape_kind_to_shape_type(kind: ShapeKind) -> ShapeType {
    match kind {
        ShapeKind::Rect => ShapeType::Rect,
        ShapeKind::Ellipse => ShapeType::Ellipse,
        ShapeKind::Line => ShapeType::Line,
        ShapeKind::Polygon => ShapeType::Polygon,
        ShapeKind::Path => ShapeType::Path,
        ShapeKind::Arrow => ShapeType::Arrow,
    }
}

/// Apply primitive-specific default values to a `VectorShapeState`.
pub fn apply_vector_shape_defaults(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.apply_defaults(state);
    }
}

/// Apply a single property expression to a `VectorShapeState`.
///
/// Returns `true` if the property was recognised and applied.
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
        return primitive.apply_property(name, value, env, diagnostics, subject, state);
    }
    false
}

/// Run any post-processing / cleanup on a `VectorShapeState` after all properties have been
/// applied.
pub fn finalize_vector_shape_state(actor_type: &str, state: &mut VectorShapeState) {
    if let Some(primitive) = crate::primitives::find_primitive(actor_type) {
        primitive.finalize_state(state);
    }
}

/// Whether this shape type exposes a tip-size property (currently only `Line`).
pub fn vector_shape_exposes_tip_size(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Line)
}

/// Whether this shape type can use a custom Bézier path (Polygon or Path).
pub fn vector_shape_uses_custom_path(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Polygon | ShapeType::Path)
}

/// Whether this shape type represents an arrow with a dedicated arrowhead.
pub fn vector_shape_is_arrow(shape_type: ShapeType) -> bool {
    matches!(shape_type, ShapeType::Arrow)
}

/// Extract the individual shape-state values for backward-compatible APIs.
///
/// Returns `(size, line_from, line_to, arc_angles)`.
pub fn extract_shape_state_values(
    state: &VectorShapeState,
) -> ([f32; 2], [f32; 2], [f32; 2], [f32; 2]) {
    let size = state.size();
    let (line_from, line_to, arc_angles) = match state {
        VectorShapeState::Line(line) => (line.line_from, line.line_to, [0.0, 0.0]),
        VectorShapeState::Arrow(arrow) => (arrow.from, arrow.to, [0.0, 0.0]),
        VectorShapeState::Callout(callout) => (callout.from, callout.to, [0.0, 0.0]),
        VectorShapeState::Ellipse(ellipse) => ([-50.0, 0.0], [50.0, 0.0], ellipse.arc_angles),
        _ => ([-50.0, 0.0], [50.0, 0.0], [0.0, 0.0]),
    };
    (size, line_from, line_to, arc_angles)
}

/// Build a `VelloPath` for a vector shape via the primitive renderer.
pub fn build_vector_shape_vello_path(
    shape_type: ShapeType,
    state: &VectorShapeState,
    style: VectorShapeStyle,
) -> Option<VelloPath> {
    // Map shape type back to a primitive type name for lookup
    let type_name = match shape_type {
        ShapeType::Rect => "Rect",
        ShapeType::Ellipse => "Ellipse",
        ShapeType::Line => "Line",
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
        .and_then(|paths| paths.into_iter().next())
}

/// Generate the vertex points for a regular polygon.
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

/// Build a Bézier path for an arrow-tipped line.
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
    let left =
        kurbo::Point::new(base.x + perp_x * half_tip_width, base.y + perp_y * half_tip_width);
    let right =
        kurbo::Point::new(base.x - perp_x * half_tip_width, base.y - perp_y * half_tip_width);

    path.move_to(start);
    path.line_to(base);
    path.move_to(tip);
    path.line_to(left);
    path.line_to(right);
    path.close_path();
    path
}

/// Parse an AST list expression into a list of `kurbo::Point`s.
pub fn parse_point_list_expr(expr: &Expr, env: &Environment) -> Option<Vec<kurbo::Point>> {
    match expr {
        Expr::List(items) => {
            let mut points = Vec::with_capacity(items.len());
            for item in items {
                let [x, y] = parse_numeric_vec2(item, env)?;
                points.push(kurbo::Point::new(x as f64, y as f64));
            }
            Some(points)
        },
        _ => None,
    }
}

/// Parse an AST list of path commands (e.g. `move_to`, `line_to`) into a `kurbo::BezPath`.
pub fn parse_path_commands_expr(expr: &Expr, env: &Environment) -> Option<kurbo::BezPath> {
    let Expr::List(items) = expr else {
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
            },
            "line_to" => {
                if args.len() != 2 {
                    return None;
                }
                let x = evaluate_expr(&args[0], env).ok()?.as_num();
                let y = evaluate_expr(&args[1], env).ok()?.as_num();
                path.line_to((x, y));
            },
            "quad_to" => {
                if args.len() != 4 {
                    return None;
                }
                let x1 = evaluate_expr(&args[0], env).ok()?.as_num();
                let y1 = evaluate_expr(&args[1], env).ok()?.as_num();
                let x2 = evaluate_expr(&args[2], env).ok()?.as_num();
                let y2 = evaluate_expr(&args[3], env).ok()?.as_num();
                path.quad_to((x1, y1), (x2, y2));
            },
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
            },
            "close" => {
                if !args.is_empty() {
                    return None;
                }
                path.close_path();
            },
            _ => return None,
        }
    }

    Some(path)
}

/// Build a `KurboShape` from the legacy flat parameters.
pub fn build_shape(
    shape_type: ShapeType,
    size: [f32; 2],
    line_from: [f32; 2],
    line_to: [f32; 2],
    arc_angles: [f32; 2],
) -> KurboShape {
    match shape_type {
        ShapeType::Ellipse => {
            if arc_angles[1] != 0.0 {
                KurboShape::Arc {
                    center: kurbo::Point::new(0.0, 0.0),
                    radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
                    start_angle: arc_angles[0] as f64,
                    sweep_angle: arc_angles[1] as f64,
                    rotation: 0.0,
                }
            } else {
                KurboShape::Ellipse {
                    center: kurbo::Point::new(0.0, 0.0),
                    radii: kurbo::Vec2::new(size[0] as f64, size[1] as f64),
                    rotation: 0.0,
                }
            }
        },
        ShapeType::Line => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        ShapeType::Arrow => KurboShape::Line {
            p0: kurbo::Point::new(line_from[0] as f64, line_from[1] as f64),
            p1: kurbo::Point::new(line_to[0] as f64, line_to[1] as f64),
        },
        _ => KurboShape::Rect {
            x0: -(size[0] as f64),
            y0: -(size[1] as f64),
            x1: size[0] as f64,
            y1: size[1] as f64,
        },
    }
}

/// Compute the fill colour for a shape, returning `None` when transparent.
pub fn shape_fill_color(
    shape_type: ShapeType,
    color: [f32; 4],
    fill_opacity: f32,
) -> Option<vello::peniko::Color> {
    if fill_opacity <= 0.0 {
        return None;
    }

    if shape_type == ShapeType::Line || shape_type == ShapeType::Arrow {
        return None;
    }

    Some(vello::peniko::Color::from_rgba8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0 * fill_opacity) as u8,
    ))
}

/// Compute the stroke colour and width, returning `None` when invisible.
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

/// Build a `VelloPath` from a `BezPath` and the current render style.
///
/// This is the shared helper that eliminates the copy-pasted `build_vello_path`
/// in every primitive implementation.
///
/// Set `force_stroke` to `true` for stroke-only shapes (Line, Arc) that need
/// a visible stroke even when the user hasn't explicitly set one.
pub fn build_vello_path(
    path: kurbo::BezPath,
    color: [f32; 4],
    stroke_color: [f32; 4],
    stroke_width: f32,
    fill_opacity: f32,
    force_stroke: bool,
) -> VelloPath {
    VelloPath {
        path,
        fill: if fill_opacity > 0.0 {
            Some(vello::peniko::Color::from_rgba8(
                (color[0] * 255.0) as u8,
                (color[1] * 255.0) as u8,
                (color[2] * 255.0) as u8,
                (color[3] * 255.0 * fill_opacity) as u8,
            ))
        } else {
            None
        },
        stroke: shape_stroke(stroke_color, stroke_width).or_else(|| {
            if force_stroke {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (color[0] * 255.0) as u8,
                        (color[1] * 255.0) as u8,
                        (color[2] * 255.0) as u8,
                        (color[3] * 255.0) as u8,
                    ),
                    1.0,
                ))
            } else {
                None
            }
        }),
        line_cap: 0,
        line_join: 0,
    }
}

/// Build a `VelloPath` from the legacy flat parameters.
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
    let state = match shape_type {
        ShapeType::Rect => VectorShapeState::Rect(RectState { size }),
        ShapeType::Ellipse => VectorShapeState::Ellipse(EllipseState {
            size,
            arc_angles,
            rotation: 0.0,
        }),
        ShapeType::Line => VectorShapeState::Line(LineState {
            size: [0.0, 0.0],
            line_from,
            line_to,
        }),
        ShapeType::Polygon => VectorShapeState::Polygon(PolygonState {
            size,
            regular_polygon_sides: 0,
            regular_polygon_radius: size[0],
            custom_path: None,
            rotation: 0.0,
            points: Vec::new(),
        }),
        ShapeType::Path => VectorShapeState::Path(PathState {
            size,
            custom_path: None,
        }),
        ShapeType::Arrow => VectorShapeState::Arrow(ArrowState {
            from: line_from,
            to: line_to,
            head_size: 10.0,
        }),
        _ => {
            return VelloPath {
                path: build_shape(shape_type, size, line_from, line_to, arc_angles)
                    .to_path_default(),
                fill: shape_fill_color(shape_type, color, fill_opacity),
                stroke: shape_stroke(stroke_color, stroke_width),
                line_cap: 0,
                line_join: 0,
            };
        },
    };
    build_vector_shape_vello_path(
        shape_type,
        &state,
        VectorShapeStyle {
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
            line_cap: 0,
            line_join: 0,
        },
    )
    .unwrap_or_else(|| VelloPath {
        path: build_shape(shape_type, size, line_from, line_to, arc_angles).to_path_default(),
        fill: shape_fill_color(shape_type, color, fill_opacity),
        stroke: shape_stroke(stroke_color, stroke_width),
        line_cap: 0,
        line_join: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_reports_tip_lookup_support() {
        assert!(vector_shape_exposes_tip_size(ShapeType::Line));
        assert!(!vector_shape_exposes_tip_size(ShapeType::Rect));
    }

    #[test]
    fn polygon_shapes_report_custom_path_usage() {
        assert!(vector_shape_uses_custom_path(ShapeType::Polygon));
        assert!(vector_shape_uses_custom_path(ShapeType::Path));
        assert!(!vector_shape_uses_custom_path(ShapeType::Rect));
    }

    #[test]
    fn stroke_drawn_plots_get_a_default_width() {
        // Regression: VectorField/ContourSet draw arrows/contours via `stroke`,
        // so a zero default stroke width made them invisible when the user set
        // only `color:` (which is used as their stroke color).
        assert_eq!(default_stroke_width(ActorKindId::VectorField), 2.0);
        assert_eq!(default_stroke_width(ActorKindId::ContourSet), 2.0);
        // Filled shapes stay stroke-less by default.
        assert_eq!(default_stroke_width(ActorKindId::Shape(ShapeKind::Rect)), 0.0);
        assert_eq!(default_stroke_width(ActorKindId::Shape(ShapeKind::Ellipse)), 0.0);
    }
}
