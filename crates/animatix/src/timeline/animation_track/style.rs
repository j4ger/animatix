#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::renderer::types::GradientSpec;
use crate::timeline::morph::MorphOptions;
use crate::timeline::property_track::PropertyTrack;

// ─────────────────────────────────────────────────────────────
// StyleTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all style-related property tracks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StyleTracks {
    /// Fill color in RGBA.
    pub color: Option<PropertyTrack<[f32; 4]>>,
    /// Overall opacity multiplier.
    pub opacity: Option<PropertyTrack<f32>>,
    /// Stroke width.
    pub stroke_width: Option<PropertyTrack<f32>>,
    /// Stroke color in RGBA.
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    /// Stroke draw progress (0-1).
    pub stroke_progress: Option<PropertyTrack<f32>>,
    /// Fill opacity multiplier.
    pub fill_opacity: Option<PropertyTrack<f32>>,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: Option<PropertyTrack<u32>>,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: Option<PropertyTrack<u32>>,
    /// Stroke dash pattern (alternating segment/gap lengths in scene pixels;
    /// empty = solid stroke).
    pub dash_pattern: Option<PropertyTrack<Vec<f32>>>,
    /// Phase offset into the dash pattern (animatable: marching ants).
    pub dash_offset: Option<PropertyTrack<f32>>,
    /// Compositing blend mode for the node's subtree; "normal" = source-over.
    pub blend: Option<PropertyTrack<String>>,
    /// Fill paint that ramps between colors, overriding `color`'s fill.
    pub fill_gradient: Option<PropertyTrack<GradientSpec>>,
    /// Stroke paint that ramps between colors, overriding `stroke_color`.
    pub stroke_gradient: Option<PropertyTrack<GradientSpec>>,
    /// How a gradient behaves past its end stops ("pad", "repeat", "reflect").
    pub gradient_extend: Option<PropertyTrack<String>>,
    /// Color space a gradient ramps through ("oklab", "srgb").
    pub gradient_space: Option<PropertyTrack<String>>,
    /// Path morphing options.
    pub morph_options: Option<PropertyTrack<MorphOptions>>,
}
