//! Vector shadows from the same fill and stroke geometry as the foreground.

use std::sync::Arc;

use kurbo::{Affine, BezPath, PathEl, Rect, Shape, Stroke};
use peniko::{BlendMode, Color, Compose, Fill, Mix};

use super::{RenderCommand, commands::path_stroke};
use crate::scene::Scene;
use crate::timeline::VectorShapeState;

const SLICES: usize = 6;
const BLUR_EXTENT: f64 = 1.5;
const SRC_OVER: BlendMode = BlendMode::new(Mix::Normal, Compose::SrcOver);

pub(super) fn command(
    state: &VectorShapeState,
    commands: &[RenderCommand],
    params: [f32; 4],
    color: [f32; 4],
) -> Option<RenderCommand> {
    if let VectorShapeState::Rect(rect) = state {
        return Some(RenderCommand::Shadow {
            size: rect.size,
            corner_radius: rect.corner_radius,
            params,
            color,
        });
    }

    let mut outlines = Vec::new();
    for command in commands {
        let RenderCommand::Paths { paths } = command else {
            continue;
        };
        for path in paths {
            if path.fill.is_some_and(|color| color.components[3] > 0.0) && !path.path.is_empty() {
                outlines.push(closed_fill(&path.path));
            }
            if let Some((color, width)) = path.stroke
                && color.components[3] > 0.0
                && width.is_finite()
                && width > 0.0
            {
                let outline = kurbo::stroke(
                    path.path.iter(),
                    &path_stroke(path, width),
                    &kurbo::StrokeOpts::default(),
                    0.1,
                );
                if !outline.is_empty() {
                    outlines.push(Arc::new(outline));
                }
            }
        }
    }
    (!outlines.is_empty()).then_some(RenderCommand::PathShadow {
        outlines,
        params,
        color,
    })
}

/// A fill implicitly closes every subpath. Make that edge explicit so spread
/// also follows the closing segment when the author omits `close()`.
fn closed_fill(path: &Arc<BezPath>) -> Arc<BezPath> {
    let mut open = false;
    let mut first_unclosed = None;
    for (index, element) in path.iter().enumerate() {
        match element {
            PathEl::MoveTo(_) => {
                if open {
                    first_unclosed = Some(index);
                    break;
                }
                open = true;
            },
            PathEl::ClosePath => open = false,
            _ => {}, // Other elements continue the current contour.
        }
    }
    let Some(first_unclosed) = first_unclosed.or_else(|| open.then_some(path.elements().len()))
    else {
        return Arc::clone(path);
    };
    let mut closed = BezPath::from_vec(path.elements()[..first_unclosed].to_vec());
    closed.close_path();
    for element in path.elements()[first_unclosed..].iter().copied() {
        if matches!(element, PathEl::MoveTo(_))
            && !matches!(closed.elements().last(), Some(PathEl::ClosePath))
        {
            closed.close_path();
        }
        closed.push(element);
    }
    if !matches!(closed.elements().last(), Some(PathEl::ClosePath)) {
        closed.close_path();
    }
    Arc::new(closed)
}

pub(super) fn bounds(outlines: &[Arc<BezPath>], params: [f32; 4]) -> Option<Rect> {
    let [x, y, blur, spread] = params.map(f64::from);
    let outset = spread + blur_extent(blur);
    let bounds = outline_bounds(outlines)?.inflate(outset, outset);
    (bounds.width() > 0.0 && bounds.height() > 0.0).then_some(bounds + kurbo::Vec2::new(x, y))
}

fn outline_bounds(outlines: &[Arc<BezPath>]) -> Option<Rect> {
    outlines.iter().map(|path| path.bounding_box()).reduce(|a, b| a.union(b))
}

fn blur_extent(blur: f64) -> f64 {
    if blur > 0.5 { blur * BLUR_EXTENT } else { 0.0 }
}

pub(super) fn draw(
    outlines: &[Arc<BezPath>],
    params: [f32; 4],
    color: [f32; 4],
    scene: &mut Scene,
    transform: Affine,
    opacity: f32,
) {
    let [r, g, b, a] = color;
    let alpha = a * opacity;
    if alpha <= 0.001 {
        return;
    }
    let Some(bounds) = outline_bounds(outlines) else {
        return;
    };
    let [x, y, blur, spread] = params.map(f64::from);
    let transform = transform * Affine::translate((x, y));
    let ink = Color::new([r, g, b, 1.0]);
    let extent = blur_extent(blur);
    if extent == 0.0 {
        draw_slice(scene, outlines, bounds, transform, spread, ink.with_alpha(alpha));
        return;
    }
    // As on rounded rectangles, approximate a soft edge with concentric
    // silhouettes. Their opacity is applied after uniting each slice's paths
    // so overlaps between fills, strokes and arrowheads cast one shadow.
    for index in 0..SLICES {
        let inner = index as f32 / SLICES as f32;
        let outer = (index + 1) as f32 / SLICES as f32;
        let weight = (3.6 * outer).tanh() - (3.6 * inner).tanh();
        draw_slice(
            scene,
            outlines,
            bounds,
            transform,
            spread + f64::from(outer) * extent,
            ink.with_alpha(weight * alpha),
        );
    }
}

fn draw_slice(
    scene: &mut Scene,
    outlines: &[Arc<BezPath>],
    bounds: Rect,
    transform: Affine,
    outset: f64,
    ink: Color,
) {
    if let [path] = outlines
        && outset == 0.0
    {
        scene.fill(Fill::NonZero, transform, ink, None, path.as_ref());
        return;
    }

    let clip = bounds.inflate(outset.max(0.0) + 1.0, outset.max(0.0) + 1.0);
    scene.push_layer(Fill::NonZero, SRC_OVER, ink.components[3], transform, &clip);
    let opaque = ink.with_alpha(1.0);
    let edge = Stroke::new(outset.abs() * 2.0).with_join(kurbo::Join::Round);
    for path in outlines {
        if outset < 0.0 {
            let inset = path.bounding_box().inflate(outset, outset);
            if inset.width() <= 0.0 || inset.height() <= 0.0 {
                continue;
            }
            // The clip keeps the area outside the stroke around the contour.
            // Intersect it with the fill to erode this silhouette before
            // uniting it with the other pieces.
            let mut inner_clip = clip.to_path(0.1);
            let edge_path = kurbo::stroke(path.iter(), &edge, &kurbo::StrokeOpts::default(), 0.1);
            inner_clip.extend(edge_path.iter());
            scene.push_clip(Fill::EvenOdd, transform, &inner_clip);
            scene.fill(Fill::NonZero, transform, opaque, None, path.as_ref());
            scene.pop_layer();
        } else {
            scene.fill(Fill::NonZero, transform, opaque, None, path.as_ref());
            if outset > 0.0 {
                scene.stroke(&edge, transform, opaque, None, path.as_ref());
            }
        }
    }
    scene.pop_layer();
}
