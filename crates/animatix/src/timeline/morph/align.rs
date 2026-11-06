//! Path/subpath/segment alignment.

use super::*;
use kurbo::Shape;

use super::interpolate::lerp_point;

/// Pair source and target paths for morphing (automatic strategy).
pub fn align_path_lists(source: &[BezPath], target: &[BezPath]) -> Vec<(BezPath, BezPath)> {
    align_path_lists_with_strategy(source, target, MorphStrategy::Auto)
}

/// Align two lists of paths using the specified strategy.
pub fn align_path_lists_with_strategy(
    source: &[BezPath],
    target: &[BezPath],
    strategy: MorphStrategy,
) -> Vec<(BezPath, BezPath)> {
    if strategy == MorphStrategy::Nearest {
        return align_path_lists_nearest(source, target);
    }

    let mut source_paths = source.to_vec();
    let mut target_paths = target.to_vec();

    if strategy == MorphStrategy::Match {
        source_paths.sort_by(path_centroid_key);
        target_paths.sort_by(path_centroid_key);
    }

    let mut result = Vec::new();
    let max_len = source_paths.len().max(target_paths.len());

    let empty_path = BezPath::new();

    for i in 0..max_len {
        let src = if i < source_paths.len() {
            source_paths[i].clone()
        } else {
            // Degenerate source
            let centroid = get_centroid(target_paths.get(i).unwrap_or(&empty_path));
            BezPath::from_vec(vec![PathEl::MoveTo(centroid)])
        };

        let tgt = if i < target_paths.len() {
            target_paths[i].clone()
        } else {
            // Degenerate target
            let centroid = get_centroid(&src);
            BezPath::from_vec(vec![PathEl::MoveTo(centroid)])
        };

        result.push((src, tgt));
    }

    result
}

fn align_path_lists_nearest(source: &[BezPath], target: &[BezPath]) -> Vec<(BezPath, BezPath)> {
    let mut result = Vec::new();
    let mut target_used = vec![false; target.len()];

    for src in source.iter() {
        let src_centroid = get_centroid(src);
        let mut best_j = None;
        let mut best_dist = f64::INFINITY;

        for (j, tgt) in target.iter().enumerate() {
            if target_used[j] {
                continue;
            }
            let dist = src_centroid.distance(get_centroid(tgt));
            if dist < best_dist {
                best_dist = dist;
                best_j = Some(j);
            }
        }

        if let Some(j) = best_j {
            target_used[j] = true;
            result.push((src.clone(), target[j].clone()));
        } else {
            // No targets left — pair with degenerate point at source centroid
            result.push((src.clone(), BezPath::from_vec(vec![PathEl::MoveTo(src_centroid)])));
        }
    }

    // Pair remaining unused targets with degenerate sources
    for (j, tgt) in target.iter().enumerate() {
        if !target_used[j] {
            let centroid = get_centroid(tgt);
            result.push((BezPath::from_vec(vec![PathEl::MoveTo(centroid)]), tgt.clone()));
        }
    }

    result
}

/// LEVEL 2: Align Subpaths within a Path
pub fn align_subpaths(source: &BezPath, target: &BezPath) -> (BezPath, BezPath) {
    align_subpaths_with_strategy(source, target, MorphStrategy::Auto)
}

/// Align subpaths within two paths using the specified strategy.
pub fn align_subpaths_with_strategy(
    source: &BezPath,
    target: &BezPath,
    strategy: MorphStrategy,
) -> (BezPath, BezPath) {
    let mut src_subs = extract_subpaths(source);
    let mut tgt_subs = extract_subpaths(target);

    if strategy == MorphStrategy::Match {
        src_subs.sort_by(subpath_centroid_key);
        tgt_subs.sort_by(subpath_centroid_key);
    }

    let pairs = if strategy == MorphStrategy::Nearest {
        align_subpaths_nearest(&src_subs, &tgt_subs)
    } else {
        let max_len = src_subs.len().max(tgt_subs.len());
        let mut pairs = Vec::with_capacity(max_len);
        for i in 0..max_len {
            let src_sub = if i < src_subs.len() {
                src_subs[i].clone()
            } else {
                let centroid = get_subpath_centroid(tgt_subs.get(i).unwrap_or(&Vec::new()));
                vec![PathEl::MoveTo(centroid)]
            };

            let tgt_sub = if i < tgt_subs.len() {
                tgt_subs[i].clone()
            } else {
                let centroid = get_subpath_centroid(&src_sub);
                vec![PathEl::MoveTo(centroid)]
            };
            pairs.push((src_sub, tgt_sub));
        }
        pairs
    };

    let mut new_src = BezPath::new();
    let mut new_tgt = BezPath::new();

    for (src_sub, tgt_sub) in pairs {
        let (aligned_src, aligned_tgt) = align_segments_with_strategy(&src_sub, &tgt_sub, strategy);
        for el in aligned_src {
            new_src.push(el);
        }
        for el in aligned_tgt {
            new_tgt.push(el);
        }
    }

    (new_src, new_tgt)
}

fn align_subpaths_nearest(
    src_subs: &[Vec<PathEl>],
    tgt_subs: &[Vec<PathEl>],
) -> Vec<(Vec<PathEl>, Vec<PathEl>)> {
    let mut result = Vec::new();
    let mut target_used = vec![false; tgt_subs.len()];

    for src in src_subs.iter() {
        let src_centroid = get_subpath_centroid(src);
        let mut best_j = None;
        let mut best_dist = f64::INFINITY;

        for (j, tgt) in tgt_subs.iter().enumerate() {
            if target_used[j] {
                continue;
            }
            let dist = src_centroid.distance(get_subpath_centroid(tgt));
            if dist < best_dist {
                best_dist = dist;
                best_j = Some(j);
            }
        }

        if let Some(j) = best_j {
            target_used[j] = true;
            result.push((src.clone(), tgt_subs[j].clone()));
        } else {
            result.push((src.clone(), vec![PathEl::MoveTo(src_centroid)]));
        }
    }

    for (j, tgt) in tgt_subs.iter().enumerate() {
        if !target_used[j] {
            let centroid = get_subpath_centroid(tgt);
            result.push((vec![PathEl::MoveTo(centroid)], tgt.clone()));
        }
    }

    result
}

/// LEVEL 3: Align Segments within a Subpath
pub fn align_segments(
    source_subpath: &[PathEl],
    target_subpath: &[PathEl],
) -> (Vec<PathEl>, Vec<PathEl>) {
    align_segments_with_strategy(source_subpath, target_subpath, MorphStrategy::Auto)
}

/// Strategy-aware segment alignment.
///
/// `Match` additionally phase-aligns closed loops: the same geometric loop can
/// start at any vertex, so an index-based pairing of a rectangle and an
/// ellipse maps a corner to a control point and twists the morph (the visible
/// "notch" in a Rect→Ellipse transition). For closed subpaths we try every
/// rotation of the target curve list (and its reverse) and keep the one whose
/// curve start points are closest to the source's.
pub fn align_segments_with_strategy(
    source_subpath: &[PathEl],
    target_subpath: &[PathEl],
    strategy: MorphStrategy,
) -> (Vec<PathEl>, Vec<PathEl>) {
    // `Match` on two closed loops needs a correspondence, not just equal
    // segment counts: pairing a square's corners with an ellipse's axis
    // points folds the morph to half size at t=0.5. Re-sample both outlines
    // by angle around the centre — every i-th sample is the same bearing on
    // both shapes — then the pointwise interpolation is a true silhouette
    // morph.
    let (mut source_owned, mut target_owned) = (source_subpath.to_vec(), target_subpath.to_vec());
    if strategy == MorphStrategy::Match && closed_like(&source_owned) && closed_like(&target_owned)
    {
        source_owned = resample_closed_radial(&source_owned, 64);
        target_owned = resample_closed_radial(&target_owned, 64);
    }
    let mut src_curves = to_curves(&source_owned);
    let mut tgt_curves = to_curves(&target_owned);

    // `to_curves` turns a `ClosePath` into a line back to the subpath start,
    // which for an already-closed loop (Rect, Ellipse) is a zero-length
    // curve. Left in, a phase rotation can pair that degenerate segment with a
    // real arc and fold the morph into a spike, so drop degenerate segments
    // before pairing.
    remove_degenerate_curves(&mut src_curves);
    remove_degenerate_curves(&mut tgt_curves);

    // Make sure they have the same number of elements
    while src_curves.len() < tgt_curves.len() && !src_curves.is_empty() {
        if !split_longest(&mut src_curves) {
            break;
        }
    }
    while tgt_curves.len() < src_curves.len() && !tgt_curves.is_empty() {
        if !split_longest(&mut tgt_curves) {
            break;
        }
    }

    // If one is completely empty but the other isn't (e.g. degenerate single point), pad the empty
    // one
    let src_has_curve = src_curves.iter().any(|el| matches!(el, PathEl::CurveTo(..)));
    let tgt_has_curve = tgt_curves.iter().any(|el| matches!(el, PathEl::CurveTo(..)));

    if tgt_has_curve && !src_has_curve {
        let pt = get_subpath_centroid(source_subpath);
        if src_curves.is_empty() {
            src_curves.push(PathEl::MoveTo(pt));
        }
        while src_curves.len() < tgt_curves.len() {
            src_curves.push(PathEl::CurveTo(pt, pt, pt));
        }
    } else if src_has_curve && !tgt_has_curve {
        let pt = get_subpath_centroid(target_subpath);
        if tgt_curves.is_empty() {
            tgt_curves.push(PathEl::MoveTo(pt));
        }
        while tgt_curves.len() < src_curves.len() {
            tgt_curves.push(PathEl::CurveTo(pt, pt, pt));
        }
    } else if !src_has_curve && !tgt_has_curve {
        let pt_src = get_subpath_centroid(source_subpath);
        let pt_tgt = get_subpath_centroid(target_subpath);
        if src_curves.is_empty() {
            src_curves.push(PathEl::MoveTo(pt_src));
        }
        if tgt_curves.is_empty() {
            tgt_curves.push(PathEl::MoveTo(pt_tgt));
        }
        while src_curves.len() < tgt_curves.len() {
            src_curves.push(PathEl::CurveTo(pt_src, pt_src, pt_src));
        }
        while tgt_curves.len() < src_curves.len() {
            tgt_curves.push(PathEl::CurveTo(pt_tgt, pt_tgt, pt_tgt));
        }
    }

    if strategy == MorphStrategy::Match
        && src_curves.len() == tgt_curves.len()
        && src_curves.len() > 1
        && is_closed_subpath(source_subpath)
        && is_closed_subpath(target_subpath)
    {
        tgt_curves = phase_align_closed(&src_curves, &tgt_curves);
    }

    (src_curves, tgt_curves)
}

/// A subpath is closed when it carries `ClosePath` or ends where it started
/// (`extract_subpaths` drops the explicit `ClosePath` for some inputs).
fn closed_like(elements: &[PathEl]) -> bool {
    if elements.iter().any(|el| matches!(el, PathEl::ClosePath)) {
        return true;
    }
    let mut points = elements.iter().filter_map(|el| match el {
        PathEl::MoveTo(p) => Some(*p),
        PathEl::LineTo(p) => Some(*p),
        PathEl::CurveTo(_, _, p) => Some(*p),
        PathEl::QuadTo(_, p) => Some(*p),
        PathEl::ClosePath => None,
    });
    let first = points.next();
    let last = points.next_back();
    matches!((first, last), (Some(a), Some(b)) if a.distance(b) < 1e-6)
}

/// Re-sample a closed outline at `samples` evenly spaced bearings around its
/// bounding-box centre. The returned path starts at 12 o'clock and runs
/// clockwise, so two shapes share an index-to-bearing correspondence.
fn resample_closed_radial(elements: &[PathEl], samples: usize) -> Vec<PathEl> {
    let path = BezPath::from_vec(elements.to_vec());
    let bbox = path.bounding_box();
    let center = Point::new((bbox.x0 + bbox.x1) / 2.0, (bbox.y0 + bbox.y1) / 2.0);
    const STEPS_PER_SEGMENT: usize = 24;
    let mut polyline: Vec<Point> = Vec::new();
    for segment in path.segments() {
        for i in 0..=STEPS_PER_SEGMENT {
            polyline.push(segment.eval(i as f64 / STEPS_PER_SEGMENT as f64));
        }
    }
    if polyline.len() < 2 {
        return elements.to_vec();
    }
    let mut outline = Vec::with_capacity(samples);
    for i in 0..samples {
        let theta =
            -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * (i as f64) / (samples as f64);
        let dir = (theta.cos(), theta.sin());
        let mut farthest = 0.0;
        for edge in polyline.windows(2) {
            if let Some(t) = ray_segment_hit(center, dir, edge[0], edge[1])
                && t > farthest
            {
                farthest = t;
            }
        }
        outline.push(if farthest > 0.0 {
            Point::new(center.x + dir.0 * farthest, center.y + dir.1 * farthest)
        } else {
            center
        });
    }
    let mut result = Vec::with_capacity(samples + 2);
    result.push(PathEl::MoveTo(outline[0]));
    for point in outline.iter().skip(1) {
        result.push(PathEl::LineTo(*point));
    }
    result.push(PathEl::ClosePath);
    result
}

/// Ray `o + t·d` against segment `a→b`; returns `t > 0` for a hit inside the
/// segment.
fn ray_segment_hit(o: Point, d: (f64, f64), a: Point, b: Point) -> Option<f64> {
    let ex = b.x - a.x;
    let ey = b.y - a.y;
    let denom = d.0 * ey - d.1 * ex;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qx = a.x - o.x;
    let qy = a.y - o.y;
    let t = (qx * ey - qy * ex) / denom;
    let u = (d.1 * qx - d.0 * qy) / denom;
    if t > 1e-9 && (0.0..=1.0).contains(&u) {
        Some(t)
    } else {
        None
    }
}

/// Drop zero-length curve segments (the closing segment of an already-closed
/// path) so they cannot be paired with a real segment during morph alignment.
fn remove_degenerate_curves(curves: &mut Vec<PathEl>) {
    let mut current = Point::ZERO;
    let mut keep = Vec::with_capacity(curves.len());
    for el in curves.drain(..) {
        match &el {
            PathEl::MoveTo(p) => {
                current = *p;
                keep.push(el);
            },
            PathEl::CurveTo(p1, p2, p3) => {
                let degenerate = current.distance(*p1) < 1e-6
                    && current.distance(*p2) < 1e-6
                    && current.distance(*p3) < 1e-6;
                if !degenerate {
                    current = *p3;
                    keep.push(el);
                }
            },
            PathEl::LineTo(p) => {
                if current.distance(*p) >= 1e-6 {
                    current = *p;
                    keep.push(el);
                }
            },
            PathEl::ClosePath => {},
            _ => keep.push(el),
        }
    }
    *curves = keep;
}

fn is_closed_subpath(elements: &[PathEl]) -> bool {
    elements.iter().any(|el| matches!(el, PathEl::ClosePath))
}

/// One closed curve as `(start, end, element)`.
fn closed_segments(curves: &[PathEl]) -> Vec<(Point, Point, PathEl)> {
    let mut segments = Vec::new();
    let mut current = Point::ZERO;
    for el in curves {
        match el {
            PathEl::MoveTo(p) => current = *p,
            PathEl::CurveTo(_, _, p3) => {
                segments.push((current, *p3, *el));
                current = *p3;
            },
            PathEl::LineTo(p) => {
                segments.push((current, *p, *el));
                current = *p;
            },
            PathEl::ClosePath => {},
            _ => {},
        }
    }
    segments
}

fn reverse_element(el: &PathEl) -> PathEl {
    match el {
        PathEl::CurveTo(c1, c2, end) => PathEl::CurveTo(*c2, *c1, *end),
        other => *other,
    }
}

fn rotated_closed(
    segments: &[(Point, Point, PathEl)],
    start_index: usize,
    reversed: bool,
) -> Vec<PathEl> {
    let n = segments.len();
    let mut seq: Vec<(Point, Point, PathEl)> = if reversed {
        segments
            .iter()
            .rev()
            .map(|(start, end, el)| (*end, *start, reverse_element(el)))
            .collect()
    } else {
        segments.to_vec()
    };
    let offset = start_index % n;
    let start = seq[(offset + n - 1) % n].1;
    let mut out = vec![PathEl::MoveTo(start)];
    for i in 0..n {
        let (_, _, el) = seq[(offset + i) % n];
        out.push(el);
    }
    out.push(PathEl::ClosePath);
    if reversed {
        // no-op; kept explicit for symmetry with the rotation path
        seq.clear();
    }
    out
}

/// Midpoint of one curve segment. Corner-based cost (`segment start`) maps a
/// square's corners onto an ellipse's axis endpoints and rotates the morph by
/// 45°; the side midpoints are what correspond on a square→circle morph, so
/// use the geometric midpoint of each segment.
fn segment_midpoint(segment: &(Point, Point, PathEl)) -> Point {
    let (start, end, el) = segment;
    match el {
        PathEl::CurveTo(p1, p2, _) => {
            let cubic = CubicBez::new(*start, *p1, *p2, *end);
            cubic.eval(0.5)
        },
        PathEl::LineTo(_) => Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0),
        _ => *start,
    }
}

fn phase_align_closed(source: &[PathEl], target: &[PathEl]) -> Vec<PathEl> {
    let src_segments = closed_segments(source);
    let tgt_segments = closed_segments(target);
    if src_segments.is_empty() || src_segments.len() != tgt_segments.len() {
        return target.to_vec();
    }
    let n = tgt_segments.len();
    let mut best_cost = f64::INFINITY;
    let mut best = target.to_vec();
    for reversed in [false, true] {
        // Build the candidate sequence once per direction so starts can be read
        // without rebuilding a BezPath per rotation.
        let seq: Vec<(Point, Point, PathEl)> = if reversed {
            tgt_segments
                .iter()
                .rev()
                .map(|(start, end, el)| (*end, *start, reverse_element(el)))
                .collect()
        } else {
            tgt_segments.clone()
        };
        for offset in 0..n {
            let mut cost = 0.0;
            for i in 0..n {
                let src_mid = segment_midpoint(&src_segments[i]);
                let tgt_mid = segment_midpoint(&seq[(offset + i) % n]);
                cost += src_mid.distance(tgt_mid);
            }
            if cost < best_cost {
                best_cost = cost;
                best = rotated_closed(&tgt_segments, offset, reversed);
            }
        }
    }
    best
}

pub(super) fn path_centroid_key(path: &BezPath, other: &BezPath) -> std::cmp::Ordering {
    let left = get_centroid(path);
    let right = get_centroid(other);
    left.x
        .partial_cmp(&right.x)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.y.partial_cmp(&right.y).unwrap_or(std::cmp::Ordering::Equal))
}

#[allow(clippy::ptr_arg)]
pub(super) fn subpath_centroid_key(left: &Vec<PathEl>, right: &Vec<PathEl>) -> std::cmp::Ordering {
    let left_centroid = get_subpath_centroid(left);
    let right_centroid = get_subpath_centroid(right);
    left_centroid
        .x
        .partial_cmp(&right_centroid.x)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| {
            left_centroid
                .y
                .partial_cmp(&right_centroid.y)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

pub(super) fn get_centroid(path: &BezPath) -> Point {
    get_subpath_centroid(path.elements())
}

pub(super) fn get_subpath_centroid(subpath: &[PathEl]) -> Point {
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut count = 0.0;

    for el in subpath {
        match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            },
            PathEl::QuadTo(_, p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            },
            PathEl::CurveTo(_, _, p) => {
                sum_x += p.x;
                sum_y += p.y;
                count += 1.0;
            },
            PathEl::ClosePath => {},
        }
    }

    if count == 0.0 {
        Point::ZERO
    } else {
        Point::new(sum_x / count, sum_y / count)
    }
}

pub(super) fn extract_subpaths(path: &BezPath) -> Vec<Vec<PathEl>> {
    let mut subs = Vec::new();
    let mut current = Vec::new();

    for el in path.elements() {
        if matches!(el, PathEl::MoveTo(_)) && !current.is_empty() {
            subs.push(current);
            current = Vec::new();
        }
        current.push(*el);
    }
    if !current.is_empty() {
        subs.push(current);
    }

    subs
}

pub(super) fn to_curves(subpath: &[PathEl]) -> Vec<PathEl> {
    let mut result = Vec::new();
    let mut last_pt = Point::ZERO;
    let mut first_pt = Point::ZERO;

    for el in subpath {
        match el {
            PathEl::MoveTo(p) => {
                result.push(PathEl::MoveTo(*p));
                last_pt = *p;
                first_pt = *p;
            },
            PathEl::LineTo(p) => {
                // convert line to curve
                let p1 = lerp_point(last_pt, *p, 1.0 / 3.0);
                let p2 = lerp_point(last_pt, *p, 2.0 / 3.0);
                result.push(PathEl::CurveTo(p1, p2, *p));
                last_pt = *p;
            },
            PathEl::QuadTo(p1, p2) => {
                let cp1 = lerp_point(last_pt, *p1, 2.0 / 3.0);
                let cp2 = lerp_point(*p2, *p1, 2.0 / 3.0);
                result.push(PathEl::CurveTo(cp1, cp2, *p2));
                last_pt = *p2;
            },
            PathEl::CurveTo(p1, p2, p3) => {
                result.push(PathEl::CurveTo(*p1, *p2, *p3));
                last_pt = *p3;
            },
            PathEl::ClosePath => {
                // Optional: convert ClosePath to line/curve back to first_pt
                let p = first_pt;
                let p1 = lerp_point(last_pt, p, 1.0 / 3.0);
                let p2 = lerp_point(last_pt, p, 2.0 / 3.0);
                result.push(PathEl::CurveTo(p1, p2, p));
                result.push(PathEl::ClosePath);
                last_pt = p;
            },
        }
    }
    result
}

pub(super) fn split_longest(curves: &mut Vec<PathEl>) -> bool {
    let mut max_len = -1.0;
    let mut max_idx = 0;
    let mut last_pt = Point::ZERO;
    let mut max_last_pt = Point::ZERO;

    for (i, el) in curves.iter().enumerate() {
        match el {
            PathEl::MoveTo(p) => last_pt = *p,
            PathEl::CurveTo(p1, p2, p3) => {
                let dist = last_pt.distance(*p1) + p1.distance(*p2) + p2.distance(*p3);
                if dist > max_len {
                    max_len = dist;
                    max_idx = i;
                    max_last_pt = last_pt;
                }
                last_pt = *p3;
            },
            _ => {},
        }
    }

    if max_len >= 0.0
        && let PathEl::CurveTo(p1, p2, p3) = curves[max_idx]
    {
        let bez = CubicBez::new(max_last_pt, p1, p2, p3);
        let b1 = bez.subsegment(0.0..0.5);
        let b2 = bez.subsegment(0.5..1.0);
        curves[max_idx] = PathEl::CurveTo(b1.p1, b1.p2, b1.p3);
        curves.insert(max_idx + 1, PathEl::CurveTo(b2.p1, b2.p2, b2.p3));
        return true;
    }

    false
}

// Morph evaluation helpers (moved from track.rs)
