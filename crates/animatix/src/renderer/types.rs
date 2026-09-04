use kurbo::BezPath;
use vello::peniko::Color;

/// A glyph path extracted from text, with color and opacity.
#[derive(Debug, Clone)]
pub struct TextPath {
    /// The glyph path.
    pub path: BezPath,
    /// The fill color of the glyph.
    pub color: typst::visualize::Paint,
    /// The opacity of the glyph (0.0–1.0).
    pub opacity: f32,
}

/// A path ready for Vello rendering, with optional fill and stroke.
///
/// PF-6: the geometry is shared as an `Arc` — morph/track evaluation clones
/// whole path lists every frame, and cloning 20+ `BezPath`s per actor per
/// frame was the largest remaining byte churn (alloc_driver 2026-09-04);
/// with the `Arc` those clones become refcount bumps. Consumers only ever
/// read the geometry (deref through `Arc` keeps `&path.path` call sites
/// source-compatible).
#[derive(Debug, Clone)]
pub struct VelloPath {
    /// The bezier path geometry.
    pub path: std::sync::Arc<BezPath>,
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
            path: std::sync::Arc::new(BezPath::new()),
            fill: None,
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }
    }
}
