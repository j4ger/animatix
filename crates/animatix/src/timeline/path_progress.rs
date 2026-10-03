//! Path trimming by normalized progress for stroke animations.
//!
//! Given a `BezPath` and a `progress` value in `[0, 1]`, produces a new
//! `BezPath` containing only the leading fraction of the path's **arc
//! length**. A segment that straddles the cut point is split at the exact
//! spot (`ParamCurve::subsegment` for quads/cubics, a lerp for lines), so a
//! straight two-point stroke — underlines, brackets, icon strokes — draws
//! from its start instead of popping whole segments. Arc length is measured
//! on a 16-chord polyline per segment: exact for lines, far below drawing
//! granularity for the sampled curves the plot family feeds through here.

use kurbo::{BezPath, ParamCurve, PathSeg, Point};

/// Polylength of one segment sampled into `chords` chords.
fn segment_polylength(seg: &PathSeg, chords: usize) -> f64 {
    let mut len = 0.0;
    let mut prev = seg.start();
    for i in 1..=chords {
        let p = seg.eval(f64::from(i as u32) / f64::from(chords as u32));
        len += (p - prev).hypot();
        prev = p;
    }
    len
}

/// The leading `t` fraction of one segment, as a segment.
fn cut_segment_from_start(seg: &PathSeg, t: f64) -> PathSeg {
    match seg {
        PathSeg::Line(l) => PathSeg::Line(kurbo::Line::new(l.p0, l.eval(t))),
        PathSeg::Quad(q) => PathSeg::Quad(q.subsegment(0.0..t)),
        PathSeg::Cubic(c) => PathSeg::Cubic(c.subsegment(0.0..t)),
    }
}

/// Append `seg` to `path`; `need_move` opens the subpath first (a run of
/// consecutive segments shares one move so corner line joins survive).
fn push_segment(path: &mut BezPath, seg: &PathSeg, need_move: bool) {
    if need_move {
        path.move_to(seg.start());
    }
    match seg {
        PathSeg::Line(l) => path.line_to(l.p1),
        PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
        PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
    };
}

/// Trim a bezier path to the first `progress` fraction of its total length.
///
/// `progress` must be in `[0.0, 1.0]`.
/// At `progress = 0.0` returns an empty path.
/// At `progress = 1.0` returns the full path unchanged.
///
/// A partially-consumed segment is cut at the exact length fraction and the
/// result is left open — a closed stroke drawing at 50% renders as half an
/// outline, not half plus a chord back to the start.
pub fn trim_path_by_progress(path: &BezPath, progress: f64) -> BezPath {
    if progress <= 0.0 {
        return BezPath::new();
    }
    if progress >= 1.0 {
        return path.clone();
    }

    const CHORDS_PER_SEGMENT: usize = 16;
    let segments: Vec<PathSeg> = path.segments().collect();
    if segments.is_empty() {
        return path.clone();
    }

    let lengths: Vec<f64> =
        segments.iter().map(|seg| segment_polylength(seg, CHORDS_PER_SEGMENT)).collect();
    let total: f64 = lengths.iter().sum();
    if total <= 0.0 {
        // Zero-length geometry (a dot) cannot be meaningfully partial.
        return path.clone();
    }
    let target = progress * total;

    let mut result = BezPath::new();
    let mut consumed = 0.0;
    // `None` right after a cut — unreachable past the break, but it keeps the
    // invariant honest if the loop ever grows a second exit.
    let mut last_end: Option<Point> = None;
    for (seg, len) in segments.iter().zip(&lengths) {
        if consumed >= target {
            break;
        }
        if last_end.is_none_or(|p| p != seg.start()) {
            result.move_to(seg.start());
        }
        let local = if *len <= 0.0 {
            1.0
        } else {
            ((target - consumed) / len).clamp(0.0, 1.0)
        };
        if local >= 1.0 {
            push_segment(&mut result, seg, false);
            consumed += *len;
            last_end = Some(seg.end());
        } else {
            push_segment(&mut result, &cut_segment_from_start(seg, local), false);
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::trim_path_by_progress;
    use kurbo::{BezPath, ParamCurve, PathEl};

    fn endpoint(path: &BezPath) -> kurbo::Point {
        path.elements()
            .iter()
            .rev()
            .find_map(|el| match el {
                PathEl::LineTo(p) | PathEl::QuadTo(_, p) | PathEl::CurveTo(_, _, p) => Some(*p),
                _ => None,
            })
            .expect("trimmed path has an end point")
    }

    #[test]
    fn straight_line_is_cut_at_the_length_fraction() {
        let mut line = BezPath::new();
        line.move_to((100.0, 180.0));
        line.line_to((540.0, 180.0));

        let half = trim_path_by_progress(&line, 0.5);
        let end = endpoint(&half);
        assert!((end.x - 320.0).abs() < 1.0, "expected the midpoint, got {end:?}");
        assert!((end.y - 180.0).abs() < 0.5, "expected no y drift, got {end:?}");

        let quarter = trim_path_by_progress(&line, 0.25);
        assert!((endpoint(&quarter).x - 210.0).abs() < 1.0);
    }

    #[test]
    fn multi_segment_path_splits_the_straddling_segment() {
        // 3 equal 100-length segments: 50% = one full segment plus half of
        // the second; 100% (and beyond the clamp) returns the full path.
        let mut zigzag = BezPath::new();
        zigzag.move_to((0.0, 0.0));
        zigzag.line_to((100.0, 0.0));
        zigzag.line_to((100.0, 100.0));
        zigzag.line_to((200.0, 100.0));

        let half = trim_path_by_progress(&zigzag, 0.5);
        let end = endpoint(&half);
        assert!((end.x - 100.0).abs() < 0.5 && (end.y - 50.0).abs() < 0.5, "got {end:?}");

        let full = trim_path_by_progress(&zigzag, 1.0);
        assert_eq!(full.elements().len(), zigzag.elements().len());
        assert_eq!(trim_path_by_progress(&zigzag, 0.0).elements().len(), 0);
    }

    #[test]
    fn cubic_is_cut_inside_the_segment() {
        let mut curve = BezPath::new();
        curve.move_to((0.0, 0.0));
        curve.curve_to((100.0, 0.0), (200.0, 100.0), (300.0, 100.0));

        let half = trim_path_by_progress(&curve, 0.5);
        let end = endpoint(&half);
        let expected =
            kurbo::CubicBez::new((0.0, 0.0), (100.0, 0.0), (200.0, 100.0), (300.0, 100.0))
                .subsegment(0.0..0.5)
                .end();
        assert!((end - expected).hypot() < 1.0, "got {end:?}, want {expected:?}");
    }
}
