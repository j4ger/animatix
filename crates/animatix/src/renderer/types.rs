use kurbo::BezPath;
use vello::peniko::Color;

/// A glyph path extracted from text, with color and opacity.
#[derive(Clone)]
pub struct TextPath {
    /// The glyph path.
    pub path: BezPath,
    /// The fill color of the glyph.
    pub color: typst::visualize::Paint,
    /// The opacity of the glyph (0.0–1.0).
    pub opacity: f32,
}

/// A path ready for Vello rendering, with optional fill and stroke.
#[derive(Clone)]
pub struct VelloPath {
    /// The bezier path geometry.
    pub path: BezPath,
    /// Optional fill color.
    pub fill: Option<Color>,
    /// Optional stroke color and width.
    pub stroke: Option<(Color, f32)>,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: u32,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: u32,
}

impl Default for VelloPath {
    fn default() -> Self {
        Self {
            path: BezPath::new(),
            fill: None,
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }
    }
}
