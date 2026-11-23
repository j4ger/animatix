#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use kurbo::{BezPath, Rect, Shape};

use crate::easing::{Easing, apply_easing};
use crate::renderer::types::TextPath;
use crate::timeline::morph;
use crate::timeline::property_track::{PropertyTrack, TrackAccessor};

use super::style::StyleTracks;

/// An authored paint style attached to a literal text source range.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TextSpan {
    /// Name scoped to the Text actor.
    pub name: String,
    /// Half-open Unicode scalar interval.
    pub range: std::ops::Range<u32>,
    /// Solid RGBA fill.
    pub color: [u8; 4],
    /// Opacity multiplier for this range.
    pub opacity: f32,
    /// Draw a rule beneath the range.
    pub underline: bool,
}

// TextTracks sub-struct

/// Sub-struct holding all text-related property tracks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TextTracks {
    /// Paint-only source ranges for literal Text.
    #[cfg_attr(feature = "serde", serde(default))]
    pub spans: Vec<TextSpan>,
    /// Source-row configuration for Code.
    #[cfg_attr(feature = "serde", serde(default))]
    pub code_options: animatix_text::CodeOptions,
    /// Immutable declared modules available to Typst recompilation.
    #[cfg_attr(feature = "serde", serde(default))]
    pub typst_sources: Option<animatix_text::TypstSources>,
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
    /// Lazily prepared outline alignment for the currently sampled text segment.
    #[doc(hidden)]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub morph_alignment: morph::AlignmentCache,
    /// Character reveal progress (0-1) for typewriter effect.
    pub char_progress: Option<PropertyTrack<f32>>,
    /// Geometry used by the latest named text reveal action.
    #[doc(hidden)]
    #[cfg_attr(feature = "serde", serde(default))]
    pub wipe_reveal: Option<PropertyTrack<bool>>,
    /// Fixed edge of the active wipe rectangle.
    #[doc(hidden)]
    #[cfg_attr(feature = "serde", serde(default))]
    pub wipe_direction: Option<PropertyTrack<String>>,
    /// Reverse visibility order of the active glyph groups.
    #[doc(hidden)]
    #[cfg_attr(feature = "serde", serde(default))]
    pub split_reverse: Option<PropertyTrack<bool>>,
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
}

impl TextTracks {
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

    /// Evaluate text paths and optional wipe clip at `time_ms`, after morphing
    /// and glyph reveal have been sampled.
    pub fn evaluate_paths_and_clip(
        &self,
        style: &StyleTracks,
        time_ms: u64,
    ) -> (Vec<TextPath>, Option<kurbo::BezPath>) {
        if let Some(content_track) = &self.text_content
            && !content_track.keyframes.is_empty()
        {
            let current_text = content_track.evaluate(time_ms);
            if current_text.is_empty() {
                return (Vec::new(), None);
            }
        }
        let mut paths = self.text_paths.as_ref().map_or_else(Vec::new, |track| {
            morph::sample_track(
                track,
                style.morph_options.as_ref(),
                time_ms,
                |source, target, t, options| {
                    morph::sample_text_paths(
                        source,
                        target,
                        t,
                        options,
                        Some(&self.morph_alignment),
                    )
                },
            )
        });

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
            wipe_reveal,
            wipe_direction,
            split_reverse,
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
            wipe_reveal,
            wipe_direction,
            split_reverse,
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

/// Apply glyph-group reveals and return the optional horizontal wipe clip.
pub fn apply_split_text_animation(
    paths: &mut Vec<TextPath>,
    text: &TextTracks,
    time_ms: u64,
) -> Option<BezPath> {
    let track = text.char_progress.as_ref()?;
    if paths.is_empty() {
        return None;
    }
    let progress = track.evaluate(time_ms).clamp(0.0, 1.0);
    let motion = track
        .interpolation_segment_with_duration(time_ms)
        .filter(|(_, from, to, elapsed, _, _)| from != to && *elapsed < 1.0);

    if last_option(&text.wipe_reveal, time_ms).copied().unwrap_or(false) {
        if progress <= 0.0 {
            paths.clear();
        }
        if progress <= 0.0 || progress >= 1.0 {
            return None;
        }
        let ink = paths
            .iter()
            .filter(|glyph| glyph.opacity > 0.0 && glyph.bounds.area() > 0.0)
            .map(|glyph| glyph.bounds)
            .reduce(|ink, glyph| ink.union(glyph))?;
        let progress = f64::from(progress);
        let direction = last_option(&text.wipe_direction, time_ms).map(String::as_str);
        let rect = match direction {
            Some("right") => {
                Rect::new(ink.x1 - ink.width() * progress, ink.y0 - 1.0, ink.x1, ink.y1 + 1.0)
            },
            Some("top") => {
                Rect::new(ink.x0 - 1.0, ink.y0, ink.x1 + 1.0, ink.y0 + ink.height() * progress)
            },
            Some("bottom") => {
                Rect::new(ink.x0 - 1.0, ink.y1 - ink.height() * progress, ink.x1 + 1.0, ink.y1)
            },
            _ => Rect::new(ink.x0, ink.y0 - 1.0, ink.x0 + ink.width() * progress, ink.y1 + 1.0),
        };
        return Some(rect.to_path(0.1));
    }

    let group = text.split_by.get(time_ms, String::new());
    let reverse = last_option(&text.split_reverse, time_ms).copied().unwrap_or(false);
    let unit = |glyph: &TextPath| {
        let index = match group.as_str() {
            "line" | "lines" => u32::from(glyph.line_idx),
            "word" | "words" => u32::from(glyph.word_idx),
            _ => glyph.source_range.map_or(u32::from(glyph.char_idx), |range| range[0]),
        };
        if reverse { u32::MAX - index } else { index }
    };
    let stagger = text.split_stagger.get(time_ms, 0.0);
    let offset = text.split_offset_y.get(time_ms, 0.0);
    let masked = text.split_mask.get(time_ms, false);
    let animated_units = stagger > 0.0 || offset != 0.0 || masked;

    // An easing overshoot only settles the whole group for unstaggered text.
    // In staggered text, later groups still have their own timeline windows.
    if !animated_units || motion.is_none() {
        if progress <= 0.0 {
            paths.clear();
        }
        if progress <= 0.0 || progress >= 1.0 {
            return None;
        }
    }
    if !animated_units && paths.windows(2).all(|pair| unit(&pair[0]) < unit(&pair[1])) {
        paths.truncate(visible_units(progress, paths.len()));
        return None;
    }
    // Shaping can issue several paths per cluster and use non-contiguous IDs
    // for visible words and lines. Keep the logical order of the units with ink.
    let mut units: Vec<_> = paths.iter().map(unit).collect();
    units.sort_unstable();
    units.dedup();
    if !animated_units {
        if let Some(&cutoff) = units.get(visible_units(progress, units.len())) {
            paths.retain(|glyph| unit(glyph) < cutoff);
        }
        return None;
    }

    let duration = motion.map_or(0.0, |(_, _, _, _, duration, _)| duration);
    let gaps = units.len().saturating_sub(1) as f32;
    let requested = if stagger > 1.0 {
        stagger / 1000.0
    } else if stagger > 0.0 {
        stagger
    } else {
        (0.6 / units.len() as f32).min(0.08)
    };
    // Leave at least one timeline tick for the last unit to settle.
    let max_span = if duration > 0.0 {
        1.0 - 1.0 / duration
    } else {
        0.85
    };
    let delay = requested.min(max_span / gaps.max(1.0));
    let width = (1.0 - gaps * delay).max(f32::EPSILON);
    let unit_progress = |index: usize| {
        let start = index as f32 * delay;
        let local = |value: f32| ((value - start) / width).clamp(0.0, 1.0);
        let Some((_, from, to, elapsed, _, ease)) = motion else {
            return local(progress);
        };
        // Place the stagger on the timeline, then ease each unit independently.
        let sampled = local(*from + elapsed * (*to - *from));
        let start = local(*from);
        let end = local(*to);
        if start == end {
            start
        } else {
            start + (end - start) * apply_easing((sampled - start) / (end - start), *ease)
        }
    };
    animate_units(paths, &unit, &units, offset, masked, unit_progress);
    None
}

fn last_option<T: crate::timeline::property_track::Interpolate>(
    track: &Option<PropertyTrack<T>>,
    time_ms: u64,
) -> Option<&T> {
    let keys = track.as_ref()?.keyframes().as_slice();
    let index = keys.partition_point(|key| key.time_ms <= time_ms);
    index.checked_sub(1).map(|index| &keys[index].payload.0)
}

fn visible_units(progress: f32, count: usize) -> usize {
    (f64::from(progress) * count as f64).ceil() as usize
}

fn animate_units(
    paths: &mut Vec<TextPath>,
    unit: &impl Fn(&TextPath) -> u32,
    units: &[u32],
    offset: f32,
    masked: bool,
    unit_progress: impl Fn(usize) -> f32,
) {
    let offset = if masked && offset == 0.0 {
        30.0
    } else {
        offset
    };
    let mut lines = std::collections::HashMap::<u16, Rect>::new();
    if masked {
        for glyph in paths.iter() {
            lines
                .entry(glyph.line_idx)
                .and_modify(|ink| *ink = ink.union(glyph.bounds))
                .or_insert(glyph.bounds);
        }
    }
    paths.retain_mut(|glyph| {
        let index = units.partition_point(|&id| id < unit(glyph));
        let shown = unit_progress(index).clamp(0.0, 1.0);
        if shown <= 0.0 {
            return false;
        }

        glyph.translate((0.0, f64::from((1.0 - shown) * offset)));
        if masked {
            if shown < 1.0 {
                // Keep the clip in the resting coordinates of this line. A
                // neighboring line's mask must never expose this glyph.
                let clip = lines[&glyph.line_idx].inflate(1.0, 0.0);
                if glyph.bounds.intersect(clip) != glyph.bounds {
                    glyph.apply_clip(&clip.to_path(0.1), clip);
                }
            }
            glyph.opacity *= (shown * 4.0).min(1.0);
        } else {
            glyph.opacity *= shown;
        }
        glyph.opacity > 0.0
    });
}
