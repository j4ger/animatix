use animatix::primitives::RenderCommand;
use animatix::renderer::types::TextPath;
use animatix::timeline::scene_program::SceneItem;
use kurbo::{Affine, Rect, Vec2};

/// Relate complete text layouts when their outlines differ by font size and
/// anchor alone. Comparing the outlines also checks wrapping and font fallback.
pub(super) fn text_geometry(from: &SceneItem, to: &SceneItem) -> Option<(f64, Vec2)> {
    let from = paths(from)?;
    let to = paths(to)?;
    if std::ptr::eq(from, to) {
        return Some((1.0, Vec2::ZERO));
    }
    if from.len() != to.len() {
        return None;
    }
    let a = ink_bounds(from)?;
    let b = ink_bounds(to)?;
    let scale = b.height() / a.height();
    let offset = b.center().to_vec2() - a.center().to_vec2() * scale;
    let map = Affine::translate(offset) * Affine::scale(scale);
    if (a.width() * scale - b.width()).abs() > 0.12 {
        return None;
    }
    from.iter()
        .zip(to)
        .all(|(a, b)| {
            a.line_idx == b.line_idx
                && a.clips.is_empty()
                && b.clips.is_empty()
                && a.outline_matches(b, map, 0.12)
        })
        .then_some((scale, offset))
}

fn paths(item: &SceneItem) -> Option<&[TextPath]> {
    item.commands.iter().find_map(|command| match command {
        RenderCommand::Text {
            paths,
            clip_path: None,
            ..
        } if !paths.is_empty() => Some(paths.as_ref()),
        _ => None,
    })
}

fn ink_bounds(paths: &[TextPath]) -> Option<Rect> {
    paths
        .iter()
        .map(|glyph| glyph.bounds)
        .filter(|rect| rect.area() > 0.0)
        .reduce(|a, b| a.union(b))
}

pub(super) fn interpolate_local(scale: f64, offset: Vec2, p: f64, incoming: bool) -> Affine {
    let animated = Affine::translate(offset * p) * Affine::scale(1.0 + (scale - 1.0) * p);
    if incoming {
        animated * Affine::scale(1.0 / scale) * Affine::translate(-offset)
    } else {
        animated
    }
}
