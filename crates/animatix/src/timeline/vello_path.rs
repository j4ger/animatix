use kurbo::BezPath;
use vello::peniko::Color;

#[derive(Clone)]
pub struct VelloPath {
    pub path: BezPath,
    pub fill: Option<Color>,
    pub stroke: Option<(Color, f32)>, // color, stroke_width
}
