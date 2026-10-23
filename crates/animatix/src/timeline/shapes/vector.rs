use super::*;

/// Per-shape state enum that only holds fields relevant to each shape type.
///
/// Previously `VectorShapeState` was a flat struct with dead fields
/// (e.g. `arc_angles` on Rect, `regular_polygon_sides` on Ellipse).
/// Now each variant carries exactly the fields it needs.
#[derive(Clone, Debug, PartialEq)]
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
            ShapeType::Rect => Self::Rect(RectState {
                size,
                corner_radius: 0.0,
            }),
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
            ShapeType::Graph | ShapeType::Plot => Self::Rect(RectState {
                size,
                corner_radius: 0.0,
            }),
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
#[derive(Clone, Copy, PartialEq, Debug)]
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
pub fn default_stroke_width(actor_type: &str) -> f32 {
    animatix_std::caps_for_type(actor_type)
        .map(|caps| caps.default_stroke_width())
        .unwrap_or(0.0)
}
