use kurbo::{BezPath, Point};

#[cfg(feature = "rich-text")]
use crate::CompiledText;
#[cfg(feature = "rich-text")]
use crate::TextPath;
#[cfg(feature = "rich-text")]
use crate::paint_to_rgba;
#[cfg(feature = "rich-text")]
use animatix_core::error::RenderError;
#[cfg(feature = "rich-text")]
use kurbo::Affine;
#[cfg(feature = "rich-text")]
use typst::layout::{Frame, FrameItem, Transform};

#[cfg(feature = "rich-text")]
use super::geometry::center_text_paths;
#[cfg(feature = "rich-text")]
use super::plain_text::is_cjk_char;

pub(crate) struct PathBuilder(pub BezPath);

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(Point::new(x1 as f64, y1 as f64), Point::new(x as f64, y as f64));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x2 as f64, y2 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[cfg(feature = "rich-text")]
/// Extract font ascent and descent from a Typst frame (uses the first text item found).
/// Returns (ascent, descent) in scene units (points).
pub fn extract_frame_metrics(frame: &Frame) -> (f32, f32) {
    // Walk the frame to find the first text item with a usable font
    let mut result: Option<(f32, f32)> = None;
    let mut stack: Vec<(&Frame, Transform)> = vec![(frame, Transform::identity())];

    while let Some((current, current_transform)) = stack.pop() {
        for (_pos, item) in current.items() {
            match item {
                FrameItem::Group(group) => {
                    let group_transform = current_transform.pre_concat(group.transform);
                    stack.push((&group.frame, group_transform));
                },
                FrameItem::Text(text) => {
                    let size = text.size.to_pt() as f32;
                    let face = text.font.ttf();
                    let units_per_em = text.font.units_per_em() as f32;
                    let font_scale = size / units_per_em;
                    let ascent = face.ascender() as f32 * font_scale;
                    let descent = face.descender() as f32 * font_scale;
                    result = Some((ascent, descent));
                    break;
                },
                _ => {},
            }
        }
        if result.is_some() {
            break;
        }
    }

    result.unwrap_or((0.0, 0.0))
}

#[cfg(all(feature = "rich-text", test))]
/// Extract glyph paths from a Typst frame.
pub fn extract_glyphs(frame: &Frame) -> Vec<TextPath> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    let _ = center_text_paths(&mut glyphs);
    glyphs
}

#[cfg(feature = "rich-text")]
/// Extract glyph paths and font metrics from a Typst frame.
pub fn extract_glyphs_with_metrics(frame: &Frame) -> CompiledText {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);

    let baseline_offset = center_text_paths(&mut glyphs);

    let (ascent, descent) = extract_frame_metrics(frame);

    CompiledText {
        glyphs,
        ascent,
        descent,
        baseline_offset,
        hardware_layout: None,
    }
}

#[cfg(feature = "rich-text")]
/// Extract glyphs from a Typst frame, grouped by explicit Equation fragment
/// metadata when present, or by top-level groups for legacy callers.
///
/// Equation frames carry paired `animatix:equation-fragment:{start,end}:{index}`
/// metadata tags. These are a private plain-string bridge between the engine
/// and this extractor; Typst frame types remain internal to this crate.
/// Marked fragments are emitted in ordinal order, including empty ranges.
///
/// Non-group text items at the top level are collected into an implicit group
/// appended at the end (only when non-empty) when no Equation markers exist.
pub fn extract_glyphs_grouped(
    frame: &Frame,
) -> Result<(Vec<TextPath>, Vec<std::ops::Range<usize>>), RenderError> {
    let mut markers = Vec::new();
    collect_fragment_markers(frame, &mut markers)?;
    if markers.is_empty() {
        return Ok(extract_glyphs_grouped_legacy(frame));
    }
    validate_fragment_markers(&markers)?;

    let mut glyphs = Vec::new();
    let mut ranges = Vec::new();
    let mut state = GlyphLayoutState::default();
    let mut active = None;
    extract_marked_frame(
        frame,
        Transform::identity(),
        &mut glyphs,
        &mut ranges,
        &mut state,
        &mut active,
    )?;
    if active.is_some() || ranges.len() * 2 != markers.len() {
        return Err(RenderError::TextCompilation(
            "unbalanced Equation fragment metadata".to_string(),
        ));
    }
    center_text_paths(&mut glyphs);
    Ok((glyphs, ranges))
}

#[cfg(feature = "rich-text")]
fn extract_glyphs_grouped_legacy(frame: &Frame) -> (Vec<TextPath>, Vec<std::ops::Range<usize>>) {
    let mut all_glyphs: Vec<TextPath> = Vec::new();
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let mut state = GlyphLayoutState::default();

    for (pos, item) in frame.items() {
        let transform = Transform::translate(pos.x, pos.y);
        match item {
            FrameItem::Group(group) => {
                let start = all_glyphs.len();
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs_with_state(
                    &group.frame,
                    group_transform,
                    &mut all_glyphs,
                    &mut state,
                );
                let end = all_glyphs.len();
                if end > start {
                    ranges.push(start..end);
                }
            },
            FrameItem::Text(text) => {
                // Top-level text not wrapped in a group — collect into an implicit group
                let start = all_glyphs.len();
                emit_text_item(text, transform, &mut all_glyphs, &mut state);
                let end = all_glyphs.len();
                if end > start {
                    ranges.push(start..end);
                }
            },
            _ => {},
        }
    }

    // Centre all glyphs as a whole (not per-group)
    center_text_paths(&mut all_glyphs);
    (all_glyphs, ranges)
}

#[cfg(feature = "rich-text")]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum FragmentMarker {
    Start(usize),
    End(usize),
}

#[cfg(feature = "rich-text")]
const EQUATION_FRAGMENT_MARKER_PREFIX: &str = "animatix:equation-fragment:";

#[cfg(feature = "rich-text")]
fn fragment_marker(item: &FrameItem) -> Result<Option<FragmentMarker>, RenderError> {
    let FrameItem::Tag(typst::introspection::Tag::Start(content, _)) = item else {
        return Ok(None);
    };
    if content.elem().name() != "metadata" {
        return Ok(None);
    }
    let Ok(typst::foundations::Value::Str(value)) = content.field_by_name("value") else {
        return Ok(None);
    };
    let value = value.as_str();
    let Some(rest) = value.strip_prefix(EQUATION_FRAGMENT_MARKER_PREFIX) else {
        return Ok(None);
    };
    let (kind, index) = rest.split_once(':').ok_or_else(|| {
        RenderError::TextCompilation("malformed Equation fragment metadata".to_string())
    })?;
    let index = index.parse().map_err(|_| {
        RenderError::TextCompilation("malformed Equation fragment ordinal".to_string())
    })?;
    match kind {
        "start" => Ok(Some(FragmentMarker::Start(index))),
        "end" => Ok(Some(FragmentMarker::End(index))),
        _ => Err(RenderError::TextCompilation(
            "unknown Equation fragment metadata kind".to_string(),
        )),
    }
}

#[cfg(feature = "rich-text")]
fn collect_fragment_markers(
    frame: &Frame,
    markers: &mut Vec<FragmentMarker>,
) -> Result<(), RenderError> {
    for (_, item) in frame.items() {
        if let Some(marker) = fragment_marker(item)? {
            markers.push(marker);
        }
        if let FrameItem::Group(group) = item {
            collect_fragment_markers(&group.frame, markers)?;
        }
    }
    Ok(())
}

#[cfg(feature = "rich-text")]
fn validate_fragment_markers(markers: &[FragmentMarker]) -> Result<(), RenderError> {
    if !markers.len().is_multiple_of(2) {
        return Err(RenderError::TextCompilation(
            "incomplete Equation fragment metadata".to_string(),
        ));
    }
    for (index, pair) in markers.as_chunks::<2>().0.iter().enumerate() {
        if *pair != [FragmentMarker::Start(index), FragmentMarker::End(index)] {
            return Err(RenderError::TextCompilation(format!(
                "invalid Equation fragment metadata at ordinal {index}"
            )));
        }
    }
    Ok(())
}

#[cfg(feature = "rich-text")]
fn extract_marked_frame(
    frame: &Frame,
    current_transform: Transform,
    glyphs: &mut Vec<TextPath>,
    ranges: &mut Vec<std::ops::Range<usize>>,
    state: &mut GlyphLayoutState,
    active: &mut Option<(usize, usize)>,
) -> Result<(), RenderError> {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                extract_marked_frame(&group.frame, group_transform, glyphs, ranges, state, active)?;
            },
            FrameItem::Text(text) => {
                // Spaces between fragments still advance word/line bookkeeping,
                // but their glyphs do not belong to either adjacent range.
                if active.is_some() {
                    emit_text_item(text, transform, glyphs, state);
                } else {
                    let mut discarded = Vec::new();
                    emit_text_item(text, transform, &mut discarded, state);
                }
            },
            _ => {
                if let Some(marker) = fragment_marker(item)? {
                    match (marker, active.take()) {
                        (FragmentMarker::Start(index), None) => {
                            *active = Some((index, glyphs.len()))
                        },
                        (FragmentMarker::End(index), Some((open, start))) if open == index => {
                            ranges.push(start..glyphs.len());
                        },
                        // The preflight validation should make these branches
                        // unreachable; leave recovery to the final balance check.
                        (FragmentMarker::Start(_), Some(open)) => *active = Some(open),
                        (FragmentMarker::End(_), open) => *active = open,
                    }
                }
            },
        }
    }
    Ok(())
}

#[cfg(feature = "rich-text")]
/// Emit one Typst text run's glyphs, updating the shared line/word indices.
///
/// Shared by the recursive frame walk and the grouped extractor's top-level
/// text handling so their index and paint conventions cannot drift.
fn emit_text_item(
    text: &typst::text::TextItem,
    transform: Transform,
    glyphs: &mut Vec<TextPath>,
    state: &mut GlyphLayoutState,
) {
    let current_y = transform.ty.to_pt();
    if let Some(last_y) = state.last_y {
        if (current_y - last_y).abs() > 2.0 {
            state.line_idx += 1;
            state.last_y = Some(current_y);
            state.last_end = 0;
        }
    } else {
        state.last_y = Some(current_y);
    }

    let text_str = text.text.as_str();
    let size = text.size.to_pt() as f32;
    let units_per_em = text.font.units_per_em() as f32;
    let font_scale = size / units_per_em;
    let face = text.font.ttf();

    let mut item_last_end = 0usize;
    let mut x_curr = 0.0;
    for glyph in &text.glyphs {
        let start = glyph.range.start as usize;
        let end = glyph.range.end as usize;
        let is_space = text_str
            .get(start..end)
            .map(|s| s.chars().all(|c| c.is_whitespace()))
            .unwrap_or(false);
        let is_cjk = text_str.get(start..end).map(|s| s.chars().any(is_cjk_char)).unwrap_or(false);
        if is_space
            || is_cjk
            || (start > item_last_end
                && text_str
                    .get(item_last_end..start)
                    .map(|s| s.chars().any(|c| c.is_whitespace() || is_cjk_char(c)))
                    .unwrap_or(false))
        {
            state.word_idx += 1;
        }
        item_last_end = end;

        let offset_x = glyph.x_offset.at(text.size).to_pt() as f32;
        let offset_y = glyph.y_offset.at(text.size).to_pt() as f32;
        let advance = glyph.x_advance.at(text.size).to_pt() as f32;

        let mut builder = PathBuilder(BezPath::new());
        if let Some(_bounds) = face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut builder) {
            let path = builder.0;
            let scale_affine = Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);
            let glyph_translate =
                Affine::translate(kurbo::Vec2::new((x_curr + offset_x) as f64, offset_y as f64));
            let item_transform = Affine::new([
                transform.sx.get(),
                transform.ky.get(),
                transform.kx.get(),
                transform.sy.get(),
                transform.tx.to_pt(),
                transform.ty.to_pt(),
            ]);
            let final_affine = item_transform * glyph_translate * scale_affine;
            let mut final_path = path;
            final_path.apply_affine(final_affine);
            let char_idx = glyphs.len() as u16;
            let mut tp = TextPath::new(final_path, paint_to_rgba(&text.fill), 1.0);
            tp.line_idx = state.line_idx;
            tp.word_idx = state.word_idx;
            tp.char_idx = char_idx;
            glyphs.push(tp);
        }
        x_curr += advance;
    }
}

#[derive(Default)]
#[cfg(feature = "rich-text")]
struct GlyphLayoutState {
    line_idx: u16,
    word_idx: u16,
    last_y: Option<f64>,
    last_end: usize,
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_glyphs_with_state(
    frame: &Frame,
    current_transform: Transform,
    glyphs: &mut Vec<TextPath>,
    state: &mut GlyphLayoutState,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs_with_state(&group.frame, group_transform, glyphs, state);
            },
            FrameItem::Text(text) => emit_text_item(text, transform, glyphs, state),
            _ => {},
        }
    }
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_glyphs(frame: &Frame, current_transform: Transform, glyphs: &mut Vec<TextPath>) {
    let mut state = GlyphLayoutState::default();
    walk_frame_for_glyphs_with_state(frame, current_transform, glyphs, &mut state);
}
