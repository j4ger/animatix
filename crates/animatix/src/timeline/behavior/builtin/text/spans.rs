use kurbo::{Rect, Shape};

use crate::primitives::EvaluateCtx;
use crate::renderer::types::TextPath;
use crate::timeline::TrackAccessor;
use crate::timeline::animation_track::TextTracks;

pub(super) fn paint(
    ctx: &EvaluateCtx<'_>,
    tracks: &TextTracks,
    paths: &[TextPath],
    cursor: bool,
) -> Vec<TextPath> {
    let time = ctx.time_ms;
    let color = ctx
        .overrides
        .and_then(|values| crate::primitives::styling::override_color(values, "color"))
        .unwrap_or_else(|| ctx.style.color.get(time, [1.0; 4]));
    let base = animatix_text::f32_color_to_rgba8(&color);
    let plan = ctx.property_plan;
    let selected = match ctx.overrides.and_then(|values| values.get("selected_span")) {
        Some(crate::timeline::Value::Str(name)) => name.clone(),
        _ => plan.sample_string_prop("selected_span", time, ""),
    };
    let span_color = ctx
        .overrides
        .and_then(|values| crate::primitives::styling::override_color(values, "span_color"))
        .or_else(|| match plan.sample_property_named("span_color", time) {
            Some(crate::timeline::PropertyValue::Color(color)) => Some(color),
            _ => None,
        })
        .map(|color| animatix_text::f32_color_to_rgba8(&color));
    let number = |name| match ctx.overrides.and_then(|values| values.get(name)) {
        Some(crate::timeline::Value::Num(value)) => *value as f32,
        _ => plan.sample_f32_prop(name, time, 1.0),
    };
    let opacity = number("span_opacity").clamp(0.0, 1.0);
    let progress = number("span_progress").clamp(0.0, 1.0);
    let size = f64::from(tracks.font_size.get(time, 16.0));
    let mut result = Vec::with_capacity(paths.len());
    let mut marks = Vec::new();
    for path in paths {
        let mut glyph = path.clone();
        glyph.color = base;
        if let Some([start, end]) = path.source_range
            && let Some(span) = tracks
                .spans
                .iter()
                .rev()
                .find(|span| span.range.start < end && start < span.range.end)
        {
            glyph.color = span.color;
            glyph.opacity *= span.opacity;
            if span.name == selected {
                if let Some(color) = span_color {
                    glyph.color = color;
                }
                let last = span.range.start + (span.range.len() as f32 * progress).ceil() as u32;
                glyph.opacity *= if progress <= 0.0 || (start >= last && progress < 1.0) {
                    0.0
                } else {
                    opacity
                };
            }
            if span.underline && glyph.opacity > 0.0 {
                let y = glyph.baseline_y + size * 0.10;
                let rect =
                    Rect::new(glyph.bounds.x0, y, glyph.bounds.x1, y + (size * 0.045).max(1.0));
                let mut mark = TextPath::new(rect.to_path(0.1), glyph.color, glyph.opacity);
                mark.clips.clone_from(&glyph.clips);
                marks.push(mark);
            }
        }
        result.push(glyph);
    }
    if cursor && let Some(caret) = caret(&result, tracks, size, base, time) {
        marks.push(caret);
    }
    result.extend(marks);
    result
}

fn caret(
    paths: &[TextPath],
    tracks: &TextTracks,
    size: f64,
    color: [u8; 4],
    time: u64,
) -> Option<TextPath> {
    let last = paths
        .iter()
        .filter(|path| path.opacity > 0.0)
        .filter_map(|path| Some((path.source_range?, path.caret?, path)))
        .max_by(|a, b| {
            a.0[1].cmp(&b.0[1]).then_with(|| {
                if a.1.1 {
                    b.1.0.total_cmp(&a.1.0)
                } else {
                    a.1.0.total_cmp(&b.1.0)
                }
            })
        });
    let baked;
    let (x, glyph) = if let Some((_, (x, _), glyph)) = last {
        (x, glyph)
    } else {
        baked = tracks.text_paths.as_ref()?.evaluate(time);
        let first = baked
            .iter()
            .filter(|path| path.caret.is_some())
            .min_by_key(|path| path.source_range)?;
        let (_, rtl) = first.caret?;
        (
            if rtl {
                first.bounds.x1
            } else {
                first.bounds.x0
            },
            first,
        )
    };
    let y = glyph.baseline_y;
    let mut caret = TextPath::new(
        Rect::new(x, y - size * 0.8, x + (size * 0.045).max(1.0), y + size * 0.15).to_path(0.1),
        color,
        1.0,
    );
    caret.clips.clone_from(&glyph.clips);
    Some(caret)
}
