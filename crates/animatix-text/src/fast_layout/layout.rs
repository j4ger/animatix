//! Shared layout internals for the fast text path.
//!
//! Everything the single-line ([`super::compile_text_fast`]) and wrapped
//! ([`super::compile_text_fast_wrapped`]) entry points rely on: face
//! resolution, tokenization, glyph emission, and centering. Both paths go
//! through these helpers so they cannot drift apart.

use animatix_core::error::RenderError;
use kurbo::{Affine, BezPath, Shape};
use std::collections::HashMap;

use crate::font_context::bundled_variable_face_data;
use crate::font_context::{
    FontContext, FontFaceData, bundled_face, bundled_face_data, resolve_font_family,
};
use crate::typst_world::{PathBuilder, center_text_paths};
use crate::{CompiledText, TextPath};

use super::fallback::fallback_faces;
use super::{
    FastPathFace, HardwareGlyph, HardwareGlyphRun, HardwarePendingGlyph, HardwareTextLayout,
    ShapedGlyph,
};

/// Resolve `family` to a primary face plus the bundled fallback faces.
///
/// `caller` names the requesting entry point in error messages (`"fast path"`,
/// `"fast path wrapped"`), so a load failure still says which layout path
/// asked for the face.
pub(crate) fn resolve_faces(
    content: &str,
    family: &str,
    weight: f32,
    style: &str,
    font_ctx: &FontContext,
    caller: &str,
) -> Result<Vec<FontFaceData>, RenderError> {
    let resolved_family = resolve_font_family(family, font_ctx);
    let primary = 'font: {
        if let Some(face) = font_ctx.registered_face_data(&resolved_family, weight, style) {
            break 'font face;
        }
        if font_ctx.include_bundled
            && let Some(face) = bundled_variable_face_data(&resolved_family, style, weight)
        {
            break 'font face;
        }
        if font_ctx.include_bundled
            && let Some(bf) = bundled_face(&resolved_family, weight, style)
        {
            break 'font bundled_face_data(bf);
        }
        if let Some(data) = font_ctx.load_face_data(&resolved_family, weight, style) {
            break 'font data;
        }
        return Err(RenderError::TextCompilation(format!(
            "Failed to load font '{}' (weight={}, style={}) for {}",
            resolved_family, weight, style, caller
        )));
    };

    if ttf_parser::Face::parse(&primary.bytes, primary.face_index).is_err() {
        return Err(RenderError::TextCompilation(format!(
            "Failed to parse font '{}' (weight={}, style={}) for {}",
            resolved_family, weight, style, caller
        )));
    }

    let mut sources = vec![primary];
    sources.extend(fallback_faces(font_ctx, &resolved_family, weight, style));
    let mut uncovered: std::collections::HashSet<char> = content
        .chars()
        .filter(|ch| !ch.is_whitespace() && !default_ignorable(*ch))
        .collect();
    for source in &sources {
        if let Ok(face) = ttf_parser::Face::parse(&source.bytes, source.face_index) {
            uncovered.retain(|ch| face.glyph_index(*ch).is_none());
        }
        if uncovered.is_empty() {
            break;
        }
    }
    // Process in source order, preserving deterministic fallback identity.
    for ch in content.chars() {
        if !uncovered.contains(&ch) {
            continue;
        }
        if let Some(fallback) = font_ctx
            .fallback_face_for_char(ch)
            .map(|face| preferred_fallback_style(font_ctx, face, &[ch], weight, style))
        {
            if let Ok(face) = ttf_parser::Face::parse(&fallback.bytes, fallback.face_index) {
                uncovered.retain(|ch| face.glyph_index(*ch).is_none());
            }
            sources.push(fallback);
        } else {
            uncovered.remove(&ch);
        }
    }
    use unicode_segmentation::UnicodeSegmentation;
    for cluster in content.graphemes(true) {
        if cluster.chars().count() < 2 {
            continue;
        }
        let chars: Vec<_> = cluster
            .chars()
            .filter(|ch| !ch.is_whitespace() && !default_ignorable(*ch))
            .collect();
        if chars.len() < 2
            || sources.iter().any(|source| {
                ttf_parser::Face::parse(&source.bytes, source.face_index)
                    .is_ok_and(|face| chars.iter().all(|ch| face.glyph_index(*ch).is_some()))
            })
        {
            continue;
        }
        if let Some(face) = font_ctx
            .fallback_face_for_cluster(&chars)
            .map(|face| preferred_fallback_style(font_ctx, face, &chars, weight, style))
        {
            sources.push(face);
        }
    }
    Ok(sources)
}

fn preferred_fallback_style(
    font_ctx: &FontContext,
    fallback: FontFaceData,
    chars: &[char],
    weight: f32,
    style: &str,
) -> FontFaceData {
    let preferred = font_ctx
        .registered_face_data(&fallback.family, weight, style)
        .or_else(|| font_ctx.load_face_data(&fallback.family, weight, style));
    preferred
        .filter(|face| {
            ttf_parser::Face::parse(&face.bytes, face.face_index)
                .is_ok_and(|parsed| chars.iter().all(|ch| parsed.glyph_index(*ch).is_some()))
        })
        .unwrap_or(fallback)
}

pub(super) fn parse_faces<'a>(
    sources: &'a [FontFaceData],
    weight: f32,
    style: &str,
) -> Vec<FastPathFace<'a>> {
    sources
        .iter()
        .filter_map(|face| FastPathFace::from_data(face, weight, style))
        .collect()
}

/// One UAX #14 breakable text segment, retaining its source position.
#[derive(Clone, Copy, Debug)]
pub(super) struct TextToken<'a> {
    pub text: &'a str,
    pub byte_start: usize,
    pub byte_end: usize,
    pub char_start: usize,
    pub space_before: bool,
}

/// Split a paragraph at Unicode line-break opportunities, while preserving
/// grapheme clusters and the source offsets used by bidi and split-text tracks.
pub(super) fn tokenize_words(content: &str) -> Vec<TextToken<'_>> {
    use unicode_linebreak::{BreakOpportunity, linebreaks};
    use unicode_segmentation::UnicodeSegmentation;

    let grapheme_ends: std::collections::HashSet<usize> = content
        .grapheme_indices(true)
        .map(|(start, grapheme)| start + grapheme.len())
        .chain(std::iter::once(0))
        .collect();
    let mut breaks: std::collections::HashSet<usize> = linebreaks(content)
        .filter_map(|(end, opportunity)| {
            (matches!(opportunity, BreakOpportunity::Allowed | BreakOpportunity::Mandatory)
                && grapheme_ends.contains(&end))
            .then_some(end)
        })
        .collect();
    // Tabs can be returned as non-breaking segment boundaries by the line-break
    // iterator when directional controls are adjacent. Keep whitespace as an
    // explicit gap token so it is never sent to Rustybuzz as visible text.
    for (start, ch) in content
        .char_indices()
        .filter(|(_, ch)| ch.is_whitespace() && *ch != '\u{a0}' && *ch != '\u{202f}')
    {
        let end = start + ch.len_utf8();
        if grapheme_ends.contains(&start) {
            breaks.insert(start);
        }
        if grapheme_ends.contains(&end) {
            breaks.insert(end);
        }
    }
    let mut breaks: Vec<usize> = breaks.into_iter().collect();
    breaks.sort_unstable();

    let mut tokens = Vec::new();
    let mut start = 0;
    let mut pending_space = false;
    let mut source_chars = 0;
    for end in breaks.into_iter().chain(std::iter::once(content.len())) {
        if end <= start {
            continue;
        }
        let segment = &content[start..end];
        let left_trimmed = segment.trim_start_matches(|ch: char| {
            ch.is_whitespace() && ch != '\u{a0}' && ch != '\u{202f}'
        });
        let leading_bytes = segment.len() - left_trimmed.len();
        let trimmed = left_trimmed
            .trim_end_matches(|ch: char| ch.is_whitespace() && ch != '\u{a0}' && ch != '\u{202f}');
        let trailing_bytes = left_trimmed.len() - trimmed.len();
        if !trimmed.is_empty() {
            let byte_start = start + leading_bytes;
            let byte_end = byte_start + trimmed.len();
            tokens.push(TextToken {
                text: trimmed,
                byte_start,
                byte_end,
                char_start: source_chars + segment[..leading_bytes].chars().count(),
                space_before: pending_space || leading_bytes > 0,
            });
            pending_space = false;
        }
        if leading_bytes > 0 || trailing_bytes > 0 || trimmed.is_empty() {
            pending_space = true;
        }
        source_chars += segment.chars().count();
        start = end;
    }
    tokens
}

#[derive(Clone, Debug)]
pub(super) struct BidiTextFragment {
    pub range: std::ops::Range<usize>,
    pub rtl: bool,
    level: u8,
}

pub(super) struct ShapedTextFragment {
    pub range: std::ops::Range<usize>,
    pub width: f64,
    pub glyphs: Vec<ShapedGlyph>,
}

#[derive(Clone, Copy)]
pub(super) enum VisualLayoutItem {
    Text {
        word_idx: usize,
        fragment_idx: usize,
    },
    Gap,
}

/// Paragraph bidi state is resolved once, then shared by shaping and wrapping.
pub(super) struct ParagraphBidi<'a> {
    text: &'a str,
    info: Option<unicode_bidi::BidiInfo<'a>>,
    logical_runs: Vec<BidiTextFragment>,
}

impl<'a> ParagraphBidi<'a> {
    pub(super) fn new(text: &'a str) -> Self {
        if !needs_bidi_resolution(text) {
            return Self {
                text,
                info: None,
                logical_runs: Vec::new(),
            };
        }
        let info = unicode_bidi::BidiInfo::new(text, None);
        let logical_runs = logical_embedding_runs(text, &info.levels);
        Self {
            text,
            info: Some(info),
            logical_runs,
        }
    }

    pub(super) fn text_fragments(&self, range: std::ops::Range<usize>) -> Vec<BidiTextFragment> {
        if self.info.is_none() {
            return (!range.is_empty()
                && self.text[range.clone()].chars().any(|ch| !default_ignorable(ch)))
            .then_some(BidiTextFragment {
                range,
                rtl: false,
                level: 0,
            })
            .into_iter()
            .collect();
        }
        self.logical_runs
            .iter()
            .filter_map(|run| {
                let start = range.start.max(run.range.start);
                let end = range.end.min(run.range.end);
                (start < end).then_some(BidiTextFragment {
                    range: start..end,
                    rtl: run.rtl,
                    level: run.level,
                })
            })
            .filter(|fragment| {
                self.text[fragment.range.clone()].chars().any(|ch| !default_ignorable(ch))
            })
            .collect()
    }

    /// Return visual order for disjoint source ranges (text fragments and gaps).
    /// Bidi fragments are grapheme-safe and already split at logical level changes.
    pub(super) fn visual_order_for_ranges(
        &self,
        ranges: &[(usize, usize)],
        line_range: std::ops::Range<usize>,
    ) -> Result<Vec<usize>, RenderError> {
        let logical: Vec<usize> = (0..ranges.len())
            .filter(|index| {
                ranges[*index].0 < line_range.end && ranges[*index].1 > line_range.start
            })
            .collect();
        let Some(info) = &self.info else {
            return Ok(logical);
        };
        let Some(paragraph) = info.paragraphs.first() else {
            return Ok(logical);
        };
        let (levels, runs) = info.visual_runs(paragraph, line_range.clone());
        let mut visual = Vec::with_capacity(logical.len());
        for run in runs {
            let mut run_items: Vec<usize> = logical
                .iter()
                .copied()
                .filter(|index| ranges[*index].0 >= run.start && ranges[*index].0 < run.end)
                .collect();
            if levels.get(run.start).is_some_and(|level| level.is_rtl()) {
                run_items.reverse();
            }
            visual.extend(run_items);
        }
        if visual.len() != logical.len() {
            return Err(RenderError::TextCompilation(
                "bidi visual runs did not cover every text fragment and gap".into(),
            ));
        }
        let mut seen = vec![false; ranges.len()];
        for item in &visual {
            if seen[*item] {
                return Err(RenderError::TextCompilation(
                    "bidi visual runs emitted a text fragment or gap more than once".into(),
                ));
            }
            seen[*item] = true;
        }
        if logical.iter().any(|item| !seen[*item]) {
            return Err(RenderError::TextCompilation(
                "bidi visual runs omitted a text fragment or gap".into(),
            ));
        }
        Ok(visual)
    }

    pub(super) fn range_has_single_level(&self, range: std::ops::Range<usize>) -> bool {
        if self.info.is_none() || range.is_empty() {
            return true;
        }
        let mut matching = self
            .logical_runs
            .iter()
            .filter(|run| run.range.start < range.end && run.range.end > range.start);
        matching
            .next()
            .is_some_and(|run| run.range.start <= range.start && run.range.end >= range.end)
            && matching.next().is_none()
    }
}

fn needs_bidi_resolution(text: &str) -> bool {
    use unicode_bidi::{BidiClass, bidi_class};
    text.chars().any(|ch| {
        matches!(
            bidi_class(ch),
            BidiClass::R
                | BidiClass::AL
                | BidiClass::RLE
                | BidiClass::LRE
                | BidiClass::RLO
                | BidiClass::LRO
                | BidiClass::PDF
                | BidiClass::LRI
                | BidiClass::RLI
                | BidiClass::FSI
                | BidiClass::PDI
        ) || matches!(ch as u32, 0x200e | 0x200f | 0x061c)
    })
}

fn logical_embedding_runs(text: &str, levels: &[unicode_bidi::Level]) -> Vec<BidiTextFragment> {
    use unicode_segmentation::UnicodeSegmentation;

    let mut runs: Vec<BidiTextFragment> = Vec::new();
    for (start, grapheme) in text.grapheme_indices(true) {
        let end = start + grapheme.len();
        let level_byte = grapheme
            .char_indices()
            .find_map(|(offset, ch)| (!default_ignorable(ch)).then_some(start + offset))
            .unwrap_or(start);
        let level = levels.get(level_byte).copied().unwrap_or_else(unicode_bidi::Level::ltr);
        let rtl = level.is_rtl();
        if let Some(last) = runs.last_mut()
            && last.level == level.number()
            && last.range.end == start
        {
            last.range.end = end;
        } else {
            runs.push(BidiTextFragment {
                range: start..end,
                rtl,
                level: level.number(),
            });
        }
    }
    runs
}

/// Advance for one inter-word space, including `word_spacing`.
///
/// Falls back to a quarter em when the resolved faces cannot shape a space.
#[inline]
pub(super) fn space_advance(
    faces: &[FastPathFace<'_>],
    size: f32,
    letter_spacing: f32,
    word_spacing: f32,
) -> f64 {
    let (w, _) =
        shape_fragment(" ", 0, faces, size, letter_spacing, rustybuzz::Direction::LeftToRight);
    if w > 0.0 {
        w + word_spacing as f64
    } else {
        (size as f64 * 0.25) + word_spacing as f64
    }
}

/// Append one shaped token's glyphs at `(x, y)`.
///
/// Every shaped glyph gets a hardware descriptor (even when the face has no
/// outline for it, so Vello can draw a fallback); outline glyphs additionally
/// become [`TextPath`]s. Shared by both layout paths and by the wrapped path's
/// ellipsis so the scale/translate convention exists once.
#[inline]
#[allow(clippy::too_many_arguments)] // One glyph run needs position, ids, and target lists.
pub(super) fn emit_shaped_glyphs(
    glyphs: &mut Vec<TextPath>,
    pending_hardware: &mut Vec<HardwarePendingGlyph>,
    faces: &[FastPathFace<'_>],
    shaped: &[ShapedGlyph],
    size: f32,
    rgba: [u8; 4],
    line_idx: u16,
    word_idx: u16,
    x: f64,
    y: f64,
) {
    for g in shaped {
        let face = &faces[g.face_idx];
        let glyph_scale = size / (face.ttf.units_per_em() as f32);

        pending_hardware.push(HardwarePendingGlyph {
            face_idx: g.face_idx,
            id: g.glyph_id as u32,
            x: (x + g.x_offset) as f32,
            y: (y + g.y_offset) as f32,
        });

        let mut builder = PathBuilder(BezPath::new());
        if face.ttf.outline_glyph(ttf_parser::GlyphId(g.glyph_id), &mut builder).is_some() {
            let path = builder.0;
            let scale_affine = Affine::scale_non_uniform(glyph_scale as f64, -glyph_scale as f64);
            let translate = Affine::translate(kurbo::Vec2::new(x + g.x_offset, y + g.y_offset));
            let final_affine = translate * scale_affine;
            let mut final_path = path;
            final_path.apply_affine(final_affine);

            let bounds = final_path.bounding_box();
            glyphs.push(TextPath {
                path: final_path,
                color: rgba,
                opacity: 1.0,
                line_idx,
                word_idx,
                char_idx: g.char_idx,
                glyph_center: [0.0, 0.0],
                bounds,
            });
        }
    }
}

/// Center compiled paths and package them with metrics and hardware runs.
///
/// `ascent`/`descent` are already scaled to points. Both layout paths finish
/// through here so the centering convention and hardware-run grouping cannot
/// drift between them.
pub(super) fn finalize_layout(
    mut glyphs: Vec<TextPath>,
    pending_hardware: Vec<HardwarePendingGlyph>,
    faces: &[FastPathFace<'_>],
    size: f32,
    ascent: f32,
    descent: f32,
) -> CompiledText {
    // Bounding box before centering: the hardware runs are shifted by the same
    // center `center_text_paths` applies to the outlines.
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for path in glyphs.iter() {
        let b = path.bounds;
        min_x = min_x.min(b.x0);
        max_x = max_x.max(b.x1);
        min_y = min_y.min(b.y0);
        max_y = max_y.max(b.y1);
    }

    let center_x = if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) / 2.0
    } else {
        0.0
    };
    let center_y = if min_y.is_finite() && max_y.is_finite() {
        (min_y + max_y) / 2.0
    } else {
        0.0
    };

    let baseline_offset = center_text_paths(&mut glyphs);

    let hardware_layout = if pending_hardware.is_empty() {
        None
    } else {
        let mut runs: Vec<HardwareGlyphRun> = Vec::new();
        for pg in pending_hardware {
            let face = &faces[pg.face_idx];
            let centered_x = pg.x - center_x as f32;
            let centered_y = pg.y - center_y as f32;
            if let Some(last_run) = runs.last_mut()
                && std::sync::Arc::ptr_eq(&last_run.font_data, &face.source.bytes)
                && last_run.font_index == face.source.face_index
                && last_run.normalized_coords.as_ref() == face.normalized_coords.as_slice()
            {
                last_run.glyphs.push(HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                });
                continue;
            }
            runs.push(HardwareGlyphRun {
                font_data: std::sync::Arc::clone(&face.source.bytes),
                font_index: face.source.face_index,
                normalized_coords: face.normalized_coords.clone().into(),
                font_size: size,
                glyphs: vec![HardwareGlyph {
                    id: pg.id,
                    x: centered_x,
                    y: centered_y,
                }],
            });
        }
        Some(HardwareTextLayout { runs })
    };

    CompiledText {
        glyphs,
        ascent,
        descent,
        baseline_offset,
        hardware_layout,
    }
}

pub(super) fn shape_fragment(
    token_text: &str,
    source_char_start: usize,
    faces: &[FastPathFace<'_>],
    size: f32,
    letter_spacing: f32,
    direction: rustybuzz::Direction,
) -> (f64, Vec<ShapedGlyph>) {
    use unicode_segmentation::UnicodeSegmentation;

    if token_text.is_empty() || faces.is_empty() {
        return (0.0, Vec::new());
    }
    let mut glyphs = Vec::new();
    let mut total_width = 0.0f64;
    let letter_spacing_f64 = letter_spacing as f64;

    let mut face_runs: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
    for (start, grapheme) in token_text.grapheme_indices(true) {
        let end = start + grapheme.len();
        let (face_idx, _) = best_face_for_cluster(grapheme, faces);
        if let Some((last_range, last_face)) = face_runs.last_mut()
            && *last_face == face_idx
            && last_range.end == start
        {
            last_range.end = end;
        } else {
            face_runs.push((start..end, face_idx));
        }
    }
    if direction == rustybuzz::Direction::RightToLeft {
        face_runs.reverse();
    }

    for (range, face_idx) in face_runs {
        let face = &faces[face_idx];
        let run_text = &token_text[range.clone()];
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(run_text);
        buffer.set_direction(direction);
        let shaped = rustybuzz::shape(&face.rb, &[], buffer);
        let font_scale = (size / face.ttf.units_per_em() as f32) as f64;
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        let mut last_glyph_for_cluster = HashMap::new();
        for (index, info) in infos.iter().enumerate() {
            last_glyph_for_cluster.insert(info.cluster, index);
        }
        let mut cluster_starts: Vec<_> = infos.iter().map(|info| info.cluster as usize).collect();
        cluster_starts.sort_unstable();
        cluster_starts.dedup();
        for (glyph_index, (info, pos)) in infos.iter().zip(positions).enumerate() {
            let cluster_ends = last_glyph_for_cluster.get(&info.cluster) == Some(&glyph_index);
            let cluster_start = info.cluster as usize;
            let next_cluster = cluster_starts.partition_point(|start| *start <= cluster_start);
            let cluster_end = cluster_starts.get(next_cluster).copied().unwrap_or(run_text.len());
            let cluster_is_visible =
                run_text[cluster_start..cluster_end].chars().any(|ch| !default_ignorable(ch));
            let tracking = if cluster_ends && cluster_is_visible {
                letter_spacing_f64
            } else {
                0.0
            };
            if info.glyph_id == 0 && run_text.trim().is_empty() {
                total_width += pos.x_advance as f64 * font_scale + tracking;
                continue;
            }
            let cluster_byte = range.start + info.cluster as usize;
            let source_char = source_char_start + token_text[..cluster_byte].chars().count();
            glyphs.push(ShapedGlyph {
                face_idx,
                glyph_id: info.glyph_id as u16,
                x_offset: total_width + pos.x_offset as f64 * font_scale,
                y_offset: -(pos.y_offset as f64) * font_scale,
                char_idx: source_char.min(u16::MAX as usize) as u16,
            });
            total_width += pos.x_advance as f64 * font_scale + tracking;
        }
    }
    (total_width, glyphs)
}

fn best_face_for_cluster(cluster: &str, faces: &[FastPathFace<'_>]) -> (usize, usize) {
    let mut best_face = 0;
    let mut best_missing = usize::MAX;
    for (index, face) in faces.iter().enumerate() {
        // Coverage is a cmap probe, not a separate shaping pass for every
        // grapheme. The whole contiguous face run is shaped exactly once.
        let missing = cluster
            .chars()
            .filter(|ch| !default_ignorable(*ch) && face.ttf.glyph_index(*ch).is_none())
            .count();
        if missing == 0 {
            return (index, 0);
        }
        if missing < best_missing {
            best_face = index;
            best_missing = missing;
        }
    }
    (best_face, best_missing)
}

/// Characters used for shaping/bidi control do not require visible glyphs.
pub(crate) fn default_ignorable(ch: char) -> bool {
    matches!(ch as u32, 0x00ad | 0x034f | 0x061c | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x206f | 0xfe00..=0xfe0f | 0xfeff | 0xe0100..=0xe01ef)
}
