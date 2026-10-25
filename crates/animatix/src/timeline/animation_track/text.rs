#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::renderer::types::TextPath;
use crate::timeline::morph;
use crate::timeline::property_track::{PropertyTrack, TrackAccessor};

use super::style::StyleTracks;

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

impl TextTracks {
    /// Evaluate `ascent` at `time_ms`.
    pub fn ascent_get(&self, time_ms: u64) -> f32 {
        self.ascent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }

    /// Evaluate `descent` at `time_ms`.
    pub fn descent_get(&self, time_ms: u64) -> f32 {
        self.descent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }

    /// Evaluate `baseline` at `time_ms`.
    /// This is the offset of the baseline from the text center (0,0) after centering paths.
    pub fn baseline_get(&self, time_ms: u64) -> f32 {
        self.baseline.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }

    /// Set all three font metrics at the given time.
    pub fn set_metrics(&mut self, time_ms: u64, ascent: f32, descent: f32, baseline: f32) {
        self.ascent.ensure(0.0).add_keyframe(time_ms, ascent, Easing::Linear);
        self.descent.ensure(0.0).add_keyframe(time_ms, descent, Easing::Linear);
        self.baseline.ensure(0.0).add_keyframe(time_ms, baseline, Easing::Linear);
    }

    /// Evaluate text paths and optional baseline clip path at `time_ms`,
    /// applying morphing, char_progress typewriter truncation, and split-text baseline reveal.
    pub fn evaluate_paths_and_clip(
        &self,
        style: &StyleTracks,
        time_ms: u64,
    ) -> (Vec<TextPath>, Option<kurbo::BezPath>) {
        if let Some(content_track) = &self.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if current_text.is_empty() {
                    return (Vec::new(), None);
                }
            }
        }
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.text_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(morph::MorphOptions::default());
        let morph_track = style.morph_options.as_ref().unwrap_or(&default_morph);
        let mut paths = morph::evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            morph::interpolate_text_paths,
        );

        let clip = apply_split_text_animation(&mut paths, self, time_ms);
        (paths, clip)
    }

    /// Evaluate text paths at `time_ms`, applying morphing and char_progress truncation.
    pub fn evaluate_paths(&self, style: &StyleTracks, time_ms: u64) -> Vec<TextPath> {
        self.evaluate_paths_and_clip(style, time_ms).0
    }

    /// Latest keyframe across the text property tracks.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        macro_rules! latest {
            ($($field:ident),* $(,)?) => {
                [$(self.$field.as_ref().and_then(|t| t.last_keyframe_time())),*]
                    .into_iter()
                    .flatten()
                    .max()
            };
        }
        latest!(
            text_content,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            letter_spacing,
            word_spacing,
            text_max_width,
            text_align,
            text_anchor,
            overflow,
            language,
            text_paths,
            char_progress,
            ascent,
            descent,
            baseline,
            split_by,
            split_stagger,
            split_offset_y,
            split_mask,
        )
    }

    /// Whether any text property track has animated keyframes.
    pub fn has_any_keyframes(&self) -> bool {
        macro_rules! any_animated {
            ($($field:ident),* $(,)?) => {
                false $(|| self.$field.as_ref().is_some_and(|t| !t.is_effectively_static()))*
            };
        }
        any_animated!(
            text_content,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            letter_spacing,
            word_spacing,
            text_max_width,
            text_align,
            text_anchor,
            overflow,
            language,
            text_paths,
            char_progress,
            ascent,
            descent,
            baseline,
            split_by,
            split_stagger,
            split_offset_y,
            split_mask,
        )
    }
}

/// Applies split-text baseline reveal and typewriter animation to a collection of text paths.
/// Returns an optional baseline clip path if `split_mask` is active.
pub fn apply_split_text_animation(
    paths: &mut Vec<TextPath>,
    text: &TextTracks,
    time_ms: u64,
) -> Option<kurbo::BezPath> {
    let cp_track = text.char_progress.as_ref()?;
    let progress = cp_track.evaluate(time_ms).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return None;
    }

    let split_by = text
        .split_by
        .as_ref()
        .map(|t| t.evaluate(time_ms))
        .unwrap_or_else(|| "char".to_string());
    let split_stagger = text.split_stagger.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0);
    let split_offset_y = text.split_offset_y.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0);
    let split_mask = text.split_mask.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(false);

    // Case A: Standard typewriter truncation
    let is_split_mode = split_offset_y != 0.0
        || split_mask
        || split_stagger > 0.0
        || (split_by != "char" && split_by != "chars");

    if !is_split_mode {
        let n = (progress as f64 * paths.len() as f64).ceil() as usize;
        paths.truncate(n);
        return None;
    }

    if paths.is_empty() {
        return None;
    }

    // Case B: Split-text reveal
    let mut max_unit = 0usize;
    for g in paths.iter() {
        let u = match split_by.as_str() {
            "line" | "lines" => g.line_idx as usize,
            "word" | "words" => g.word_idx as usize,
            _ => g.char_idx as usize,
        };
        if u > max_unit {
            max_unit = u;
        }
    }
    let n_units = max_unit + 1;

    let s = if split_stagger > 0.0 {
        if split_stagger > 1.0 {
            (split_stagger / 1000.0).clamp(0.0, 0.5)
        } else {
            split_stagger
        }
    } else {
        (0.6 / n_units as f32).min(0.08)
    };

    let s_clamped = if n_units > 1 {
        s.min(0.85 / (n_units - 1) as f32)
    } else {
        0.0
    };
    let w = if n_units > 1 {
        1.0 - (n_units - 1) as f32 * s_clamped
    } else {
        1.0
    };

    let effective_offset = if split_offset_y != 0.0 {
        split_offset_y
    } else if split_mask {
        30.0
    } else {
        0.0
    };

    // Calculate baseline clip rectangles if split_mask is true
    let clip_path = if split_mask {
        if let Some(static_clip) = &text.static_split_clip {
            Some(static_clip.clone())
        } else {
            crate::renderer::text::compute_split_mask_clip(paths)
        }
    } else {
        None
    };

    for g in paths.iter_mut() {
        let u = match split_by.as_str() {
            "line" | "lines" => g.line_idx as usize,
            "word" | "words" => g.word_idx as usize,
            _ => g.char_idx as usize,
        };
        let start_u = u as f32 * s_clamped;
        let p_u = if w > 0.0 {
            ((progress - start_u) / w).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let dy = (1.0 - p_u) * effective_offset;
        if dy != 0.0 {
            g.path = kurbo::Affine::translate((0.0, dy as f64)) * g.path.clone();
            g.glyph_center[1] += dy;
            g.bounds = g.bounds + kurbo::Vec2::new(0.0, dy as f64);
        }

        if split_mask {
            let alpha = if p_u <= 0.0 {
                0.0
            } else {
                (p_u * 4.0).min(1.0)
            };
            g.opacity *= alpha;
        } else {
            g.opacity *= p_u;
        }
    }

    clip_path
}
