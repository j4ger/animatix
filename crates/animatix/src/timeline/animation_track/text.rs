#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::renderer::types::TextPath;
use crate::timeline::property_track::PropertyTrack;

// ─────────────────────────────────────────────────────────────
// TextTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all text-related property tracks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TextTracks {
    /// Raw text content.
    pub text_content: Option<PropertyTrack<String>>,
    /// Font family name.
    pub font_family: Option<PropertyTrack<String>>,
    /// Font size in points.
    pub font_size: Option<PropertyTrack<f32>>,
    /// Font weight (100-900).
    pub font_weight: Option<PropertyTrack<f32>>,
    /// Font style ("normal" | "italic").
    pub font_style: Option<PropertyTrack<String>>,
    /// Line height multiplier.
    pub line_height: Option<PropertyTrack<f32>>,
    /// Letter spacing in points.
    pub letter_spacing: Option<PropertyTrack<f32>>,
    /// Word spacing in points.
    pub word_spacing: Option<PropertyTrack<f32>>,
    /// Max width for text wrapping (0 = no wrap).
    pub text_max_width: Option<PropertyTrack<f32>>,
    /// Text alignment ("left", "center", "right", "justify").
    pub text_align: Option<PropertyTrack<String>>,
    /// Text anchor placement relative to 'at' ("left", "center", "right").
    pub text_anchor: Option<PropertyTrack<String>>,
    /// Overflow behavior ("visible", "clip", "ellipsis").
    pub overflow: Option<PropertyTrack<String>>,
    /// Syntax-highlighting language tag for `Code` actors (Typst raw fence
    /// token, e.g. "rust"). `None`/empty = no highlighting.
    pub language: Option<PropertyTrack<String>>,
    /// Pre-built text paths for rendering.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    /// Character reveal progress (0-1) for typewriter effect.
    pub char_progress: Option<PropertyTrack<f32>>,
    /// Font ascent in scene units (points), used for baseline alignment.
    pub ascent: Option<PropertyTrack<f32>>,
    /// Font descent in scene units (points), used for baseline alignment.
    pub descent: Option<PropertyTrack<f32>>,
    /// Baseline offset from text center, used for baseline alignment.
    /// A positive value means the baseline is above the text center.
    pub baseline: Option<PropertyTrack<f32>>,
    /// Split reveal mode ("char", "word", "line").
    pub split_by: Option<PropertyTrack<String>>,
    /// Stagger between units in split reveal (0.0–1.0 fraction of duration).
    pub split_stagger: Option<PropertyTrack<f32>>,
    /// Vertical offset for split reveal emergence in scene units.
    pub split_offset_y: Option<PropertyTrack<f32>>,
    /// Whether to clip split text at line baseline.
    pub split_mask: Option<PropertyTrack<bool>>,
    /// Prebaked data for rolling Counter primitive.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub counter_data: Option<crate::primitives::counter::CounterData>,
    /// Cached static text paths for O(1) frame evaluation when not animated.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub static_paths: Option<std::sync::Arc<[TextPath]>>,
    /// Precomputed hardware glyph layout for direct Vello draw_glyphs hardware execution.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub hardware_layout: Option<std::sync::Arc<animatix_text::HardwareTextLayout>>,
    /// Precomputed resting baseline clip path for split-text masking.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub static_split_clip: Option<kurbo::BezPath>,
}
