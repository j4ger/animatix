//! Encoder inputs captured by the Animatix Vello GPU experiment.
#![allow(
    missing_docs,
    reason = "temporary encoder bridge for the Animatix experiment"
)]

use peniko::{
    BlendMode, Brush, Color, Fill, FontData,
    kurbo::{Affine, BezPath, Rect, Stroke},
};

#[derive(Clone)]
pub enum Draw {
    Fill {
        path: BezPath,
        rule: Fill,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
    },
    Stroke {
        path: BezPath,
        style: Stroke,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
    },
    Push {
        clip: BezPath,
        fill: Fill,
        transform: Affine,
        blend: Option<BlendMode>,
        alpha: f32,
    },
    Pop,
    BlurredRect {
        clip: BezPath,
        transform: Affine,
        rect: Rect,
        color: Color,
        corner: f32,
        std_dev: f32,
    },
    Glyphs {
        font: FontData,
        size: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        coords: Vec<vello_encoding::NormalizedCoord>,
        glyphs: Vec<vello_encoding::Glyph>,
        hint: bool,
        color: Color,
    },
    Texture {
        id: u64,
        rect: Rect,
        opacity: f32,
        transform: Affine,
    },
    Unsupported(&'static str),
}

impl Draw {
    pub(crate) fn transformed(mut self, outer: Affine) -> Self {
        match &mut self {
            Self::Fill { transform, .. }
            | Self::Stroke { transform, .. }
            | Self::Push { transform, .. }
            | Self::BlurredRect { transform, .. }
            | Self::Glyphs { transform, .. }
            | Self::Texture { transform, .. } => *transform = outer * *transform,
            Self::Pop | Self::Unsupported(_) => {},
        }
        self
    }
}
