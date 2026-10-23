use kurbo::{BezPath, Point};

#[cfg(feature = "rich-text")]
use crate::CompiledText;
#[cfg(feature = "rich-text")]
use crate::TextPath;
#[cfg(feature = "rich-text")]
use crate::paint_to_rgba;
#[cfg(feature = "rich-text")]
use kurbo::{Affine, Shape};
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
/// A shape extracted from a Typst frame, containing the curve and its transform.
#[derive(Clone)]
pub struct ExtractedShape {
    /// The curve geometry of the shape.
    pub curve: typst::visualize::Curve,
    /// The transform applied to the shape.
    pub transform: Transform,
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

#[cfg(feature = "rich-text")]
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

    // Compute bounding box BEFORE centering
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for glyph in &glyphs {
        let bounds = glyph.path.bounding_box();
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

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
/// Extract glyphs from a Typst frame, grouped by top-level `FrameItem::Group`.
///
/// Each `#box()[content]` wrapper in Typst produces a top-level `Group` in the
/// output frame.  This function returns:
/// - A flat list of glyph paths (centred as a whole, not per-group).
/// - A parallel list of index ranges, one per top-level group encountered.
///
/// Non-group text items at the top level are collected into an implicit group
/// appended at the end (only when non-empty).
pub fn extract_glyphs_grouped(frame: &Frame) -> (Vec<TextPath>, Vec<std::ops::Range<usize>>) {
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
            FrameItem::Text(_) => {
                // Top-level text not wrapped in a group — collect into an implicit group
                let start = all_glyphs.len();
                walk_frame_for_glyphs_text_item(item, transform, &mut all_glyphs, &mut state);
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
/// Helper: extract glyphs from a single `FrameItem::Text` (not recursing into groups).
fn walk_frame_for_glyphs_text_item(
    item: &FrameItem,
    transform: Transform,
    glyphs: &mut Vec<TextPath>,
    state: &mut GlyphLayoutState,
) {
    if let FrameItem::Text(text) = item {
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
            let is_cjk =
                text_str.get(start..end).map(|s| s.chars().any(is_cjk_char)).unwrap_or(false);
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
                let glyph_translate = Affine::translate(kurbo::Vec2::new(
                    (x_curr + offset_x) as f64,
                    offset_y as f64,
                ));
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
            FrameItem::Text(text) => {
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
                    let is_cjk = text_str
                        .get(start..end)
                        .map(|s| s.chars().any(is_cjk_char))
                        .unwrap_or(false);
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
                    if let Some(_bounds) =
                        face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut builder)
                    {
                        let path = builder.0;

                        let scale_affine =
                            Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);

                        let glyph_translate = Affine::translate(kurbo::Vec2::new(
                            (x_curr + offset_x) as f64,
                            offset_y as f64,
                        ));

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
            },
            _ => {},
        }
    }
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_glyphs(frame: &Frame, current_transform: Transform, glyphs: &mut Vec<TextPath>) {
    let mut state = GlyphLayoutState::default();
    walk_frame_for_glyphs_with_state(frame, current_transform, glyphs, &mut state);
}

#[cfg(feature = "rich-text")]
/// Extract shapes from a Typst frame.
pub fn extract_shapes(frame: &Frame) -> Vec<ExtractedShape> {
    let mut shapes = Vec::new();
    walk_frame_for_shapes(frame, Transform::identity(), &mut shapes);
    shapes
}

#[cfg(feature = "rich-text")]
fn walk_frame_for_shapes(
    frame: &Frame,
    current_transform: Transform,
    shapes: &mut Vec<ExtractedShape>,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_shapes(&group.frame, group_transform, shapes);
            },
            FrameItem::Shape(shape, _span) => {
                if let typst::visualize::Geometry::Curve(curve) = &shape.geometry {
                    shapes.push(ExtractedShape {
                        curve: (*curve).clone(),
                        transform,
                    });
                }
            },
            _ => {},
        }
    }
}
