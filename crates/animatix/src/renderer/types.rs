use kurbo::BezPath;
use vello::peniko::Color;

#[derive(Clone)]
pub struct TextPath {
    pub path: BezPath,
    pub color: typst::visualize::Paint,
}

#[derive(Clone)]
pub struct VelloPath {
    pub path: BezPath,
    pub fill: Option<Color>,
    pub stroke: Option<(Color, f32)>,
}
