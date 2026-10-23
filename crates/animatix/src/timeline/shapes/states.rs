/// State for a rectangle shape.
#[derive(Clone, Debug, PartialEq)]
pub struct RectState {
    /// Width and height of the rectangle.
    pub size: [f32; 2],
    /// Corner rounding radius in scene pixels; 0 keeps square corners.
    pub corner_radius: f32,
}

impl Default for RectState {
    fn default() -> Self {
        Self {
            size: [50.0, 50.0],
            corner_radius: 0.0,
        }
    }
}

/// State for an ellipse (or arc) shape.
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub struct ArrowState {
    /// Start point of the arrow.
    pub from: [f32; 2],
    /// End point of the arrow (arrowhead points here).
    pub to: [f32; 2],
    /// Size of the arrowhead triangle (length and half-width derived from this).
    pub head_size: f32,
}

/// State for a callout annotation (arrow geometry).
#[derive(Clone, Debug, Default, PartialEq)]
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
