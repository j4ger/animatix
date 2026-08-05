//! Path trimming by normalized progress for stroke animations.
//!
//! Given a `BezPath` and a `progress` value in `[0, 1]`, produces a new
//! `BezPath` containing only the fraction of path elements that fall within
//! the progress window.

use kurbo::BezPath;

/// Trim a bezier path to the first `progress` fraction of its total length.
///
/// `progress` must be in `[0.0, 1.0]`.
/// At `progress = 0.0` returns an empty path.
/// At `progress = 1.0` returns the full path unchanged.
pub fn trim_path_by_progress(path: &BezPath, progress: f64) -> BezPath {
    if progress <= 0.0 {
        return BezPath::new();
    }
    if progress >= 1.0 {
        return path.clone();
    }

    let elements = path.elements();
    if elements.is_empty() {
        return BezPath::new();
    }

    // Count segments (lines and curves). MoveTo elements start new segments
    // but are not themselves drawable segments.
    let segment_count = elements
        .iter()
        .filter(|e| {
            matches!(
                e,
                kurbo::PathEl::LineTo(_)
                    | kurbo::PathEl::QuadTo(_, _)
                    | kurbo::PathEl::CurveTo(_, _, _)
            )
        })
        .count();

    if segment_count == 0 {
        return path.clone();
    }

    let target = (progress * segment_count as f64).ceil() as usize;
    let mut segments_collected = 0usize;
    let mut result = BezPath::new();
    let mut pending_move_to: Option<kurbo::Point> = None;

    for el in elements {
        match el {
            kurbo::PathEl::MoveTo(p) => {
                // Save the move-to; emit it when we emit the first segment
                pending_move_to = Some(*p);
            },
            seg @ (kurbo::PathEl::LineTo(_)
            | kurbo::PathEl::QuadTo(_, _)
            | kurbo::PathEl::CurveTo(_, _, _)) => {
                if segments_collected < target {
                    if let Some(pt) = pending_move_to.take() {
                        result.move_to(pt);
                    }
                    match seg {
                        kurbo::PathEl::LineTo(p) => result.line_to(*p),
                        kurbo::PathEl::QuadTo(p1, p2) => result.quad_to(*p1, *p2),
                        kurbo::PathEl::CurveTo(p1, p2, p3) => result.curve_to(*p1, *p2, *p3),
                        _ => unreachable!(),
                    }
                    segments_collected += 1;
                }
            },
            kurbo::PathEl::ClosePath => {
                if segments_collected > 0 {
                    result.close_path();
                }
            },
        }
    }

    result
}
