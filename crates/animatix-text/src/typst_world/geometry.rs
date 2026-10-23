use kurbo::{Affine, BezPath};

use crate::TextPath;

/// Centers text paths around the origin so that layout positioning works correctly.
/// The layout system positions children by their center point, so text needs to be
/// centered around (0, 0) for layout alignment to work properly.
///
/// Returns the Y offset of the baseline relative to the new center (0, 0).
/// The baseline was at Y=0 in the pre-centering coordinate system (Typst/fast-path convention).
/// After centering, the baseline is at Y = `-(min_y + max_y) / 2.0`.
pub fn center_text_paths(paths: &mut [TextPath]) -> f32 {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths.iter() {
        let bounds = path.bounds;
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let offset = Affine::translate((-center_x, -center_y));
        let vec_offset = kurbo::Vec2::new(-center_x, -center_y);

        for path in paths.iter_mut() {
            path.path.apply_affine(offset);
            path.bounds = path.bounds + vec_offset;
            let b = path.bounds;
            if b.width().is_finite() && b.height().is_finite() {
                path.glyph_center = [((b.x0 + b.x1) * 0.5) as f32, ((b.y0 + b.y1) * 0.5) as f32];
            }
        }

        // Baseline was at Y=0 before centering; after shifting by -center_y, it is at -center_y.
        -(center_y as f32)
    } else {
        0.0
    }
}

/// Measure the bounding box of text paths and return half-width and half-height.
pub fn measure_text_paths(paths: &[TextPath]) -> [f32; 2] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for text_path in paths {
        let bounds = text_path.bounds;
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        [
            ((max_x - min_x) as f32) / 2.0,
            ((max_y - min_y) as f32) / 2.0,
        ]
    } else {
        [0.0, 0.0]
    }
}

/// Compute resting baseline clip rectangles for Split-Text baseline reveal masking.
///
/// Precomputed once at build time to avoid scanning glyph boundaries per frame.
pub fn compute_split_mask_clip(paths: &[TextPath]) -> Option<BezPath> {
    if paths.is_empty() {
        return None;
    }
    let mut line_bounds: Vec<(f64, f64, f64, f64)> = Vec::new();
    for g in paths.iter() {
        let idx = g.line_idx as usize;
        if idx >= line_bounds.len() {
            line_bounds.resize(
                idx + 1,
                (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
            );
        }
        let b = g.bounds;
        let e = &mut line_bounds[idx];
        e.0 = e.0.min(b.x0);
        e.1 = e.1.min(b.y0);
        e.2 = e.2.max(b.x1);
        e.3 = e.3.max(b.y1);
    }
    let mut bez = BezPath::new();
    for (x0, y0, x1, y1) in line_bounds {
        if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
            let rx0 = x0 - 10.0;
            let ry0 = y0 - 1000.0;
            let rx1 = x1 + 10.0;
            let ry1 = y1 + 4.0;
            bez.move_to((rx0, ry0));
            bez.line_to((rx1, ry0));
            bez.line_to((rx1, ry1));
            bez.line_to((rx0, ry1));
            bez.close_path();
        }
    }
    Some(bez)
}
