pub use animatix_text::TextPath;

use kurbo::BezPath;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use vello::peniko::color::{ColorSpaceTag, DynamicColor};
use vello::peniko::{Color, ColorStop, ColorStops, Extend, Gradient};

/// One stop of a [`GradientSpec`] — a normalized offset and an RGBA color in
/// 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GradientStop {
    /// Position along the ramp, 0..1.
    pub offset: f32,
    /// `[r, g, b, a]` in 0..1.
    pub color: [f32; 4],
}

/// Geometry of a gradient, expressed in the painted shape's bounding box.
///
/// Coordinates are normalized: `(0, 0)` is the bbox's top-left and `(1, 1)` its
/// bottom-right, so a gradient authored once scales with the actor. Angles are
/// degrees using the CSS convention (`0deg` points up, `90deg` to the right).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GradientShape {
    /// A ramp along a line at `angle`, spanning the bbox's projection on it.
    Linear {
        /// Ramp direction in degrees (0 points up).
        angle: f32,
    },
    /// A ramp radiating from `center` outward by `radius` (relative to the
    /// bbox's larger side).
    Radial {
        /// Ramp origin in normalized bbox coordinates.
        center: [f32; 2],
        /// Ramp reach, relative to the bbox's larger side.
        radius: f32,
    },
    /// A ramp rotating around `center`, beginning at `angle`.
    Sweep {
        /// Rotation origin in normalized bbox coordinates.
        center: [f32; 2],
        /// Where the sweep begins, in degrees (0 points up).
        angle: f32,
    },
}

/// How a gradient behaves beyond its first and last stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GradientExtend {
    /// Clamp to the end colors (the default).
    #[default]
    Pad,
    /// Repeat the ramp.
    Repeat,
    /// Alternate the ramp direction each cycle.
    Reflect,
}

/// Color space the ramp is interpolated in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GradientSpace {
    /// Perceptual: no dark band across saturated hue ramps.
    #[default]
    Oklab,
    /// Channel-wise sRGB, matching CSS `linear-gradient`.
    Srgb,
}

/// A paint that ramps between colors, as authored by `fill_gradient:` and
/// `stroke_gradient:`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct GradientSpec {
    /// Ramp geometry in normalized bbox space.
    pub shape: GradientShape,
    /// Stops, sorted by offset; at least two.
    pub stops: Vec<GradientStop>,
    /// Behaviour past the end stops.
    pub extend: GradientExtend,
    /// Interpolation color space.
    pub space: GradientSpace,
}

impl Default for GradientSpec {
    /// A vertical top-to-bottom ramp with no stops (i.e. nothing to paint).
    fn default() -> Self {
        Self {
            shape: GradientShape::Linear { angle: 180.0 },
            stops: Vec::new(),
            extend: GradientExtend::default(),
            space: GradientSpace::default(),
        }
    }
}

impl GradientSpec {
    /// Convert to a Vello brush positioned in `bbox` space, with every stop's
    /// alpha multiplied by `alpha`.
    ///
    /// `bbox` is the painted geometry's local-space bounding box, which is what
    /// makes the normalized authoring form follow a resizing actor.
    pub fn to_peniko(&self, bbox: kurbo::Rect, alpha: f32) -> Gradient {
        let (x0, y0, x1, y1) = (bbox.x0, bbox.y0, bbox.x1, bbox.y1);
        let (w, h) = (x1 - x0, y1 - y0);
        let (cx, cy) = (x0 + w / 2.0, y0 + h / 2.0);
        let deg = std::f64::consts::PI / 180.0;

        let kind = match self.shape {
            GradientShape::Linear { angle } => {
                // CSS 0deg = to top: the ramp direction in this y-down space.
                let r = f64::from(angle) * deg;
                let (dx, dy) = (r.sin(), -r.cos());
                // Half-extent of the bbox projected onto the ramp direction, so
                // the end stops land on the box's extremes along that axis.
                let half = (dx.abs() * w + dy.abs() * h) / 2.0;
                vello::peniko::GradientKind::Linear(vello::peniko::LinearGradientPosition {
                    start: kurbo::Point::new(cx - dx * half, cy - dy * half),
                    end: kurbo::Point::new(cx + dx * half, cy + dy * half),
                })
            },
            GradientShape::Radial { center, radius } => {
                let scale = w.max(h).max(1.0) as f32;
                vello::peniko::GradientKind::Radial(vello::peniko::RadialGradientPosition::new(
                    kurbo::Point::new(x0 + f64::from(center[0]) * w, y0 + f64::from(center[1]) * h),
                    radius * scale,
                ))
            },
            GradientShape::Sweep { center, angle } => {
                // peniko sweeps are radians clockwise from +x in y-down space;
                // the CSS `0deg` (up) is therefore -90°.
                let start = (f64::from(angle) - 90.0) * deg;
                vello::peniko::GradientKind::Sweep(vello::peniko::SweepGradientPosition {
                    center: kurbo::Point::new(
                        x0 + f64::from(center[0]) * w,
                        y0 + f64::from(center[1]) * h,
                    ),
                    start_angle: start as f32,
                    end_angle: (start + std::f64::consts::TAU) as f32,
                })
            },
        };

        let mut stops = ColorStops::new();
        for s in &self.stops {
            let [r, g, b, a] = s.color;
            stops.push(ColorStop {
                offset: s.offset,
                color: DynamicColor::from_alpha_color(Color::from_rgba8(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    (a * 255.0 * alpha) as u8,
                )),
            });
        }

        Gradient {
            kind,
            extend: match self.extend {
                GradientExtend::Pad => Extend::Pad,
                GradientExtend::Repeat => Extend::Repeat,
                GradientExtend::Reflect => Extend::Reflect,
            },
            interpolation_cs: match self.space {
                GradientSpace::Oklab => ColorSpaceTag::Oklab,
                GradientSpace::Srgb => ColorSpaceTag::Srgb,
            },
            stops,
            ..Default::default()
        }
    }
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
    /// Stroke dash pattern (segment/gap lengths in scene pixels); `None` or
    /// empty renders a solid stroke.
    pub dash_pattern: Option<Vec<f32>>,
    /// Phase offset into the dash pattern, in scene pixels.
    pub dash_offset: f32,
    /// Paint that ramps across the fill, overriding `fill`.
    pub fill_gradient: Option<GradientSpec>,
    /// Paint that ramps across the stroke, overriding the stroke color.
    pub stroke_gradient: Option<GradientSpec>,
}

impl Default for VelloPath {
    fn default() -> Self {
        Self {
            path: std::sync::Arc::new(BezPath::new()),
            fill: None,
            stroke: None,
            line_cap: 0,
            line_join: 0,
            dash_pattern: None,
            dash_offset: 0.0,
            fill_gradient: None,
            stroke_gradient: None,
        }
    }
}
