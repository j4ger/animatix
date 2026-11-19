//! Owned, ordered scene operations produced by timeline evaluation.
//!
//! Scene is the renderer boundary: it owns paths, paints, fonts, glyphs,
//! images, and effect-scope inputs without owning a GPU device or a renderer
//! scene. Backends translate Draw values into their own encoding.

pub use kurbo::Stroke;
use kurbo::{Affine, BezPath, Rect, Shape};
pub use peniko::{BlendMode, Brush, Color, Fill, FontData, ImageBrush};

use crate::timeline::effects::{EffectChain, EffectRegion, PendingBackdrop};

/// A sequence of owned drawing operations.
#[derive(Clone, Default)]
pub struct Scene {
    draws: Vec<Draw>,
}

/// Whether a filter result is drawn at its place in the scene or after the
/// ordinary scene content has been rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompositePlacement {
    /// Draw at the Filter's painter-order position.
    Inline,
    /// Draw after the main scene, matching the tail-Filter fast path.
    AfterScene,
}

/// A renderer-independent draw or compositing operation.
#[derive(Clone)]
pub enum Draw {
    /// Fill a path with a solid, gradient, or image brush.
    Fill {
        /// Geometry in path coordinates.
        path: BezPath,
        /// NonZero or EvenOdd fill rule.
        rule: Fill,
        /// Path-to-scene transform.
        transform: Affine,
        /// Paint value, including owned image data for image brushes.
        brush: Brush,
        /// Optional brush-to-path transform.
        brush_transform: Option<Affine>,
    },
    /// Stroke a path.
    Stroke {
        /// Geometry in path coordinates.
        path: BezPath,
        /// Stroke width, join, cap, and dash settings.
        style: Stroke,
        /// Path-to-scene transform.
        transform: Affine,
        /// Paint value, including owned image data for image brushes.
        brush: Brush,
        /// Optional brush-to-path transform.
        brush_transform: Option<Affine>,
    },
    /// Push a clipping or blend layer.
    Push {
        /// Clip geometry in local coordinates.
        clip: BezPath,
        /// Fill rule for the clip path.
        fill: Fill,
        /// Clip-to-scene transform.
        transform: Affine,
        /// None is a plain clip; Some applies the blend mode.
        blend: Option<BlendMode>,
        /// Layer opacity.
        alpha: f32,
    },
    /// Pop the most recently pushed clip or blend layer.
    Pop,
    /// Draw a gaussian-blurred rounded rectangle.
    BlurredRect {
        /// Clip geometry in local coordinates.
        clip: BezPath,
        /// Clip-to-scene transform.
        transform: Affine,
        /// Unblurred rounded-rectangle bounds.
        rect: Rect,
        /// Fill color.
        color: Color,
        /// Corner radius.
        corner: f32,
        /// Gaussian standard deviation.
        std_dev: f32,
    },
    /// Draw glyphs from one font run.
    Glyphs {
        /// Font bytes and collection index.
        font: FontData,
        /// Font size in scene units.
        size: f32,
        /// Run-to-scene transform.
        transform: Affine,
        /// Optional transform applied to each glyph outline.
        glyph_transform: Option<Affine>,
        /// Normalized variable-font coordinates.
        coords: Vec<i16>,
        /// Glyph ids and positions in local scene units.
        glyphs: Vec<Glyph>,
        /// Whether glyph hinting is enabled.
        hint: bool,
        /// Foreground paint.
        color: Color,
    },
    /// Draw an externally owned texture by symbolic id.
    ///
    /// The id is resolved by the renderer. It is not a GPU handle and does not
    /// own a texture or view.
    Texture {
        /// Renderer-owned texture identity.
        id: u64,
        /// Destination rectangle in scene coordinates.
        rect: Rect,
        /// Opacity multiplier.
        opacity: f32,
        /// Destination-to-scene transform.
        transform: Affine,
    },
    /// Apply a sampled effect chain to a child scene.
    Filter {
        /// Child scene content.
        scene: Box<Scene>,
        /// Optional region of interest in scene pixels.
        region: Option<EffectRegion>,
        /// Sampled effect chain.
        chain: EffectChain,
        /// Composite opacity.
        opacity: f32,
        /// Whether to composite in painter order or after the base scene.
        placement: CompositePlacement,
    },
    /// A deferred backdrop blur followed by its panel content.
    ///
    /// This remains a single queue entry so the renderer always composites the
    /// backdrop before the children and preserves Glass ordering across scopes.
    Glass {
        /// Backdrop blur and panel clip.
        backdrop: PendingBackdrop,
        /// Panel children composited above the blurred backdrop.
        children: Box<Scene>,
    },
}

/// A positioned glyph in a hardware-friendly glyph run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// Font glyph id.
    pub id: u32,
    /// X position in local scene units.
    pub x: f32,
    /// Y position in local scene units.
    pub y: f32,
}

impl Scene {
    /// Create an empty scene.
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the operations in painter order.
    pub fn draws(&self) -> &[Draw] {
        &self.draws
    }

    /// Consume the scene and return its owned operations.
    pub fn into_draws(self) -> Vec<Draw> {
        self.draws
    }

    /// Clear all operations while retaining the allocated buffer.
    pub fn reset(&mut self) {
        self.draws.clear();
    }

    /// Append another scene, optionally applying an outer transform.
    pub fn append(&mut self, other: &Self, transform: Option<Affine>) {
        let Some(transform) = transform else {
            self.draws.extend(other.draws.iter().cloned());
            return;
        };
        self.draws
            .extend(other.draws.iter().cloned().map(|draw| draw.transformed(transform)));
    }

    /// Fill a shape with a solid, gradient, or image brush.
    pub fn fill(
        &mut self,
        rule: Fill,
        transform: Affine,
        brush: impl Into<Brush>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.draws.push(Draw::Fill {
            path: shape_to_path(shape),
            rule,
            transform,
            brush: brush.into(),
            brush_transform,
        });
    }

    /// Stroke a shape.
    pub fn stroke(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<Brush>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.draws.push(Draw::Stroke {
            path: shape_to_path(shape),
            style: style.clone(),
            transform,
            brush: brush.into(),
            brush_transform,
        });
    }

    /// Push a path clip with no blend mode.
    pub fn push_clip(&mut self, fill: Fill, transform: Affine, clip: &impl Shape) {
        self.draws.push(Draw::Push {
            clip: shape_to_path(clip),
            fill,
            transform,
            blend: None,
            alpha: 1.0,
        });
    }

    /// Push a path clip with a blend mode and opacity.
    pub fn push_layer(
        &mut self,
        fill: Fill,
        blend: BlendMode,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.draws.push(Draw::Push {
            clip: shape_to_path(clip),
            fill,
            transform,
            blend: Some(blend),
            alpha,
        });
    }

    /// Pop the most recently pushed clip or blend layer.
    pub fn pop_layer(&mut self) {
        self.draws.push(Draw::Pop);
    }

    /// Draw a blurred rounded rectangle clipped to shape.
    pub fn draw_blurred_rounded_rect_in(
        &mut self,
        shape: &impl Shape,
        transform: Affine,
        rect: Rect,
        color: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.draws.push(Draw::BlurredRect {
            clip: shape_to_path(shape),
            transform,
            rect,
            color,
            corner: radius as f32,
            std_dev: std_dev as f32,
        });
    }

    /// Draw an unbounded blurred rounded rectangle.
    pub fn draw_blurred_rounded_rect(
        &mut self,
        transform: Affine,
        rect: Rect,
        color: Color,
        radius: f64,
        std_dev: f64,
    ) {
        let pad = 2.5 * std_dev;
        let clip = rect.inflate(pad, pad);
        self.draw_blurred_rounded_rect_in(&clip, transform, rect, color, radius, std_dev);
    }

    /// Create a builder for recording one glyph run.
    pub fn draw_glyphs(&mut self, font: &FontData) -> DrawGlyphs<'_> {
        DrawGlyphs {
            scene: self,
            font: font.clone(),
            size: 16.0,
            transform: Affine::IDENTITY,
            glyph_transform: None,
            coords: Vec::new(),
            hint: false,
            color: Color::BLACK,
            alpha: 1.0,
        }
    }

    /// Draw an image brush at its natural pixel dimensions.
    pub fn draw_image(&mut self, image: &ImageBrush, transform: Affine) {
        let rect = Rect::new(0.0, 0.0, f64::from(image.image.width), f64::from(image.image.height));
        self.fill(Fill::NonZero, transform, Brush::Image(image.clone()), None, &rect);
    }

    /// Record a symbolic external-texture draw.
    pub fn draw_texture(&mut self, id: u64, rect: Rect, opacity: f32, transform: Affine) {
        self.draws.push(Draw::Texture {
            id,
            rect,
            opacity,
            transform,
        });
    }

    /// Record a Filter scope operation.
    pub fn push_filter(
        &mut self,
        scene: Scene,
        region: Option<EffectRegion>,
        chain: EffectChain,
        opacity: f32,
        placement: CompositePlacement,
    ) {
        self.draws.push(Draw::Filter {
            scene: Box::new(scene),
            region,
            chain,
            opacity,
            placement,
        });
    }

    /// Record one deferred Glass scope and its child scene.
    pub fn push_glass(&mut self, backdrop: PendingBackdrop, children: Scene) {
        self.draws.push(Draw::Glass {
            backdrop,
            children: Box::new(children),
        });
    }
}

/// Builder for a glyph run.
pub struct DrawGlyphs<'a> {
    scene: &'a mut Scene,
    font: FontData,
    size: f32,
    transform: Affine,
    glyph_transform: Option<Affine>,
    coords: Vec<i16>,
    hint: bool,
    color: Color,
    alpha: f32,
}

impl DrawGlyphs<'_> {
    /// Set the run transform.
    pub fn transform(mut self, transform: Affine) -> Self {
        self.transform = transform;
        self
    }

    /// Set an optional per-glyph transform.
    pub fn glyph_transform(mut self, transform: Option<Affine>) -> Self {
        self.glyph_transform = transform;
        self
    }

    /// Set the run's font size.
    pub fn font_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the variable-font coordinates.
    pub fn normalized_coords(mut self, coords: &[i16]) -> Self {
        self.coords.clear();
        self.coords.extend_from_slice(coords);
        self
    }

    /// Set the run's foreground paint.
    pub fn brush(mut self, brush: &Brush) -> Self {
        self.color = match brush {
            Brush::Solid(color) => *color,
            _ => Color::WHITE,
        };
        self
    }

    /// Set an additional alpha multiplier for the foreground paint.
    pub fn brush_alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Enable or disable glyph hinting.
    pub fn hint(mut self, hint: bool) -> Self {
        self.hint = hint;
        self
    }

    /// Finish the run and record its glyphs.
    pub fn draw(self, _fill: Fill, glyphs: impl IntoIterator<Item = Glyph>) {
        let color = self.color.with_alpha(self.color.components[3] * self.alpha);
        self.scene.draws.push(Draw::Glyphs {
            font: self.font,
            size: self.size,
            transform: self.transform,
            glyph_transform: self.glyph_transform,
            coords: self.coords,
            glyphs: glyphs.into_iter().collect(),
            hint: self.hint,
            color,
        });
    }
}

impl Draw {
    fn transformed(mut self, outer: Affine) -> Self {
        match &mut self {
            Self::Fill { transform, .. }
            | Self::Stroke { transform, .. }
            | Self::Push { transform, .. }
            | Self::BlurredRect { transform, .. }
            | Self::Glyphs { transform, .. }
            | Self::Texture { transform, .. } => *transform = outer * *transform,
            Self::Filter { scene, .. } => {
                let mut transformed = Scene::new();
                transformed.append(scene, Some(outer));
                **scene = transformed;
            },
            Self::Glass { children, .. } => {
                let mut transformed = Scene::new();
                transformed.append(children, Some(outer));
                **children = transformed;
            },
            Self::Pop => {},
        }
        self
    }
}

fn shape_to_path(shape: &impl Shape) -> BezPath {
    let mut path = BezPath::new();
    path.extend(shape.path_elements(0.1));
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_keeps_draw_order_and_reuses_capacity_after_reset() {
        let mut scene = Scene::new();
        let path = Rect::new(0.0, 0.0, 4.0, 5.0).into_path(0.1);
        scene.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &path);
        scene.push_clip(Fill::EvenOdd, Affine::translate((2.0, 3.0)), &path);
        scene.pop_layer();

        assert!(matches!(scene.draws()[0], Draw::Fill { .. }));
        assert!(matches!(
            scene.draws()[1],
            Draw::Push {
                fill: Fill::EvenOdd,
                ..
            }
        ));
        assert!(matches!(scene.draws()[2], Draw::Pop));
        let capacity = scene.draws.capacity();
        scene.reset();
        assert!(scene.draws().is_empty());
        assert_eq!(scene.draws.capacity(), capacity);
    }

    #[test]
    fn append_applies_outer_transform_to_draw_ops() {
        let mut child = Scene::new();
        child.fill(
            Fill::NonZero,
            Affine::translate((1.0, 2.0)),
            Color::WHITE,
            None,
            &Rect::new(0.0, 0.0, 1.0, 1.0),
        );
        let mut parent = Scene::new();
        parent.append(&child, Some(Affine::translate((5.0, 7.0))));

        let Draw::Fill { transform, .. } = &parent.draws()[0] else {
            panic!("expected fill");
        };
        assert_eq!(*transform, Affine::translate((6.0, 9.0)));
    }
}
