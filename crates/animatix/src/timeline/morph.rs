use kurbo::{BezPath, CubicBez, ParamCurve, PathEl, Point};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::property_track::{Interpolate, PropertyTrack};
use crate::easing::apply_easing;
use crate::renderer::types::{TextPath, VelloPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Strategy for aligning paths before morphing.
pub enum MorphStrategy {
    /// Automatic alignment (index-based pairing).
    Auto,
    /// Sort paths by centroid before pairing.
    Match,
    /// Fade-based transition (cross-fade overlay).
    Fade,
    /// Pair each source path to its spatially nearest target path.
    Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Options controlling path morphing behavior.
pub struct MorphOptions {
    /// Alignment strategy to use.
    pub strategy: MorphStrategy,
    /// Arc amount for curved interpolation (radians).
    pub path_arc: f64,
    /// Whether to stretch paths to match bounding boxes.
    pub stretch: bool,
}

impl Default for MorphOptions {
    fn default() -> Self {
        Self {
            strategy: MorphStrategy::Auto,
            path_arc: 0.0,
            stretch: false,
        }
    }
}

impl MorphOptions {
    /// Stable display summary used when this compound option crosses the
    /// generic property-value boundary.
    pub fn summary(&self) -> String {
        format!(
            "strategy={:?}, path_arc={:.2}, stretch={}",
            self.strategy, self.path_arc, self.stretch
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PathBounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl PathBounds {
    fn width(self) -> f64 {
        (self.max_x - self.min_x).max(1.0)
    }

    fn height(self) -> f64 {
        (self.max_y - self.min_y).max(1.0)
    }
}

/// LEVEL 1: Align Lists of Paths
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
        let (aligned_src, aligned_tgt) = align_segments(&src_sub, &tgt_sub);
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
    let mut src_curves = to_curves(source_subpath);
    let mut tgt_curves = to_curves(target_subpath);

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

    (src_curves, tgt_curves)
}

/// LEVEL 4: Interpolate
pub fn morph_paths(source: &BezPath, target: &BezPath, t: f64) -> BezPath {
    morph_paths_with_options(source, target, t, MorphOptions::default())
}

/// Morph between two paths with custom options.
pub fn morph_paths_with_options(
    source: &BezPath,
    target: &BezPath,
    t: f64,
    options: MorphOptions,
) -> BezPath {
    let (source_path, target_path, output_bounds) = if options.stretch {
        let source_bounds = path_bounds(source);
        let target_bounds = path_bounds(target);
        let normalized_source = normalize_path_to_unit_bounds(source, source_bounds);
        let normalized_target = normalize_path_to_unit_bounds(target, target_bounds);
        let blended_bounds = interpolate_bounds(source_bounds, target_bounds, t);
        (normalized_source, normalized_target, Some(blended_bounds))
    } else {
        (source.clone(), target.clone(), None)
    };

    let mut result = BezPath::new();

    let (aligned_src, aligned_tgt) =
        align_subpaths_with_strategy(&source_path, &target_path, options.strategy);

    for (s, tr) in aligned_src.elements().iter().zip(aligned_tgt.elements().iter()) {
        match (s, tr) {
            (PathEl::MoveTo(p1), PathEl::MoveTo(p2)) => {
                result.push(PathEl::MoveTo(curved_lerp_point(*p1, *p2, t, options.path_arc)));
            },
            (PathEl::LineTo(p1), PathEl::LineTo(p2)) => {
                result.push(PathEl::LineTo(curved_lerp_point(*p1, *p2, t, options.path_arc)));
            },
            (PathEl::QuadTo(p1a, p1b), PathEl::QuadTo(p2a, p2b)) => {
                result.push(PathEl::QuadTo(
                    curved_lerp_point(*p1a, *p2a, t, options.path_arc),
                    curved_lerp_point(*p1b, *p2b, t, options.path_arc),
                ));
            },
            (PathEl::CurveTo(p1a, p1b, p1c), PathEl::CurveTo(p2a, p2b, p2c)) => {
                result.push(PathEl::CurveTo(
                    curved_lerp_point(*p1a, *p2a, t, options.path_arc),
                    curved_lerp_point(*p1b, *p2b, t, options.path_arc),
                    curved_lerp_point(*p1c, *p2c, t, options.path_arc),
                ));
            },
            (PathEl::ClosePath, PathEl::ClosePath) => {
                result.push(PathEl::ClosePath);
            },
            _ => {
                // If they are not the same type, fallback to target (should not happen if aligned
                // properly)
                result.push(*tr);
            },
        }
    }

    match output_bounds {
        Some(bounds) => denormalize_path_from_unit_bounds(&result, bounds),
        None => result,
    }
}

// Helpers

fn lerp_point(p1: Point, p2: Point, t: f64) -> Point {
    Point::new(p1.x + (p2.x - p1.x) * t, p1.y + (p2.y - p1.y) * t)
}

fn curved_lerp_point(p1: Point, p2: Point, t: f64, path_arc: f64) -> Point {
    if path_arc.abs() < f64::EPSILON {
        return lerp_point(p1, p2, t);
    }

    let midpoint = Point::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length <= f64::EPSILON {
        return lerp_point(p1, p2, t);
    }

    let normal_x = -dy / length;
    let normal_y = dx / length;
    let arc_scale = (path_arc / std::f64::consts::PI).clamp(-1.0, 1.0);
    let control = Point::new(
        midpoint.x + normal_x * length * 0.5 * arc_scale,
        midpoint.y + normal_y * length * 0.5 * arc_scale,
    );
    let one_minus_t = 1.0 - t;

    Point::new(
        one_minus_t * one_minus_t * p1.x + 2.0 * one_minus_t * t * control.x + t * t * p2.x,
        one_minus_t * one_minus_t * p1.y + 2.0 * one_minus_t * t * control.y + t * t * p2.y,
    )
}

fn path_bounds(path: &BezPath) -> PathBounds {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for element in path.elements() {
        match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                update_bounds(*p, &mut min_x, &mut max_x, &mut min_y, &mut max_y)
            },
            PathEl::QuadTo(p1, p2) => {
                update_bounds(*p1, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p2, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
            },
            PathEl::CurveTo(p1, p2, p3) => {
                update_bounds(*p1, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p2, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p3, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
            },
            PathEl::ClosePath => {},
        }
    }

    if !min_x.is_finite() {
        return PathBounds {
            min_x: 0.0,
            max_x: 1.0,
            min_y: 0.0,
            max_y: 1.0,
        };
    }

    PathBounds {
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

fn update_bounds(point: Point, min_x: &mut f64, max_x: &mut f64, min_y: &mut f64, max_y: &mut f64) {
    *min_x = (*min_x).min(point.x);
    *max_x = (*max_x).max(point.x);
    *min_y = (*min_y).min(point.y);
    *max_y = (*max_y).max(point.y);
}

fn normalize_path_to_unit_bounds(path: &BezPath, bounds: PathBounds) -> BezPath {
    map_path_points(path, |point| {
        Point::new(
            (point.x - bounds.min_x) / bounds.width(),
            (point.y - bounds.min_y) / bounds.height(),
        )
    })
}

fn denormalize_path_from_unit_bounds(path: &BezPath, bounds: PathBounds) -> BezPath {
    map_path_points(path, |point| {
        Point::new(
            bounds.min_x + point.x * bounds.width(),
            bounds.min_y + point.y * bounds.height(),
        )
    })
}

fn map_path_points(path: &BezPath, mut map_point: impl FnMut(Point) -> Point) -> BezPath {
    let mut mapped = BezPath::new();

    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => mapped.push(PathEl::MoveTo(map_point(*point))),
            PathEl::LineTo(point) => mapped.push(PathEl::LineTo(map_point(*point))),
            PathEl::QuadTo(p1, p2) => mapped.push(PathEl::QuadTo(map_point(*p1), map_point(*p2))),
            PathEl::CurveTo(p1, p2, p3) => {
                mapped.push(PathEl::CurveTo(map_point(*p1), map_point(*p2), map_point(*p3)))
            },
            PathEl::ClosePath => mapped.push(PathEl::ClosePath),
        }
    }

    mapped
}

fn interpolate_bounds(source: PathBounds, target: PathBounds, t: f64) -> PathBounds {
    PathBounds {
        min_x: source.min_x + (target.min_x - source.min_x) * t,
        max_x: source.max_x + (target.max_x - source.max_x) * t,
        min_y: source.min_y + (target.min_y - source.min_y) * t,
        max_y: source.max_y + (target.max_y - source.max_y) * t,
    }
}

fn path_centroid_key(path: &BezPath, other: &BezPath) -> std::cmp::Ordering {
    let left = get_centroid(path);
    let right = get_centroid(other);
    left.x
        .partial_cmp(&right.x)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| left.y.partial_cmp(&right.y).unwrap_or(std::cmp::Ordering::Equal))
}

#[allow(clippy::ptr_arg)]
fn subpath_centroid_key(left: &Vec<PathEl>, right: &Vec<PathEl>) -> std::cmp::Ordering {
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

fn get_centroid(path: &BezPath) -> Point {
    get_subpath_centroid(path.elements())
}

fn get_subpath_centroid(subpath: &[PathEl]) -> Point {
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

fn extract_subpaths(path: &BezPath) -> Vec<Vec<PathEl>> {
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

fn to_curves(subpath: &[PathEl]) -> Vec<PathEl> {
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

fn split_longest(curves: &mut Vec<PathEl>) -> bool {
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

    if max_len >= 0.0 {
        if let PathEl::CurveTo(p1, p2, p3) = curves[max_idx] {
            let bez = CubicBez::new(max_last_pt, p1, p2, p3);
            let b1 = bez.subsegment(0.0..0.5);
            let b2 = bez.subsegment(0.5..1.0);
            curves[max_idx] = PathEl::CurveTo(b1.p1, b1.p2, b1.p3);
            curves.insert(max_idx + 1, PathEl::CurveTo(b2.p1, b2.p2, b2.p3));
            return true;
        }
    }

    false
}

// ─────────────────────────────────────────────────────────────
// Morph evaluation helpers (moved from track.rs)
// ─────────────────────────────────────────────────────────────

fn lerp_color(c1: vello::peniko::Color, c2: vello::peniko::Color, t: f32) -> vello::peniko::Color {
    let t = t.clamp(0.0, 1.0);
    let comp1 = c1.to_rgba8();
    let comp2 = c2.to_rgba8();
    vello::peniko::Color::from_rgba8(
        (comp1.r as f32 + (comp2.r as f32 - comp1.r as f32) * t) as u8,
        (comp1.g as f32 + (comp2.g as f32 - comp1.g as f32) * t) as u8,
        (comp1.b as f32 + (comp2.b as f32 - comp1.b as f32) * t) as u8,
        (comp1.a as f32 + (comp2.a as f32 - comp1.a as f32) * t) as u8,
    )
}

impl Interpolate for Vec<TextPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_text_paths(self, other, t, MorphOptions::default())
    }
}

impl Interpolate for Vec<VelloPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_vello_paths(self, other, t, MorphOptions::default())
    }
}

/// Evaluate a path track with morph option interpolation.
///
/// Samples `paths` at `time_ms`, reads the nearest morph options from
/// `morph_options`, and calls the provided `interpolate` callback with
/// the resolved `MorphOptions`.
pub fn evaluate_paths_with_options<T: Interpolate>(
    paths: &PropertyTrack<T>,
    morph_options: &PropertyTrack<MorphOptions>,
    time_ms: u64,
    interpolate: fn(&T, &T, f32, MorphOptions) -> T,
) -> T {
    if let Some((found_time, prev_val, found_val, progress, found_easing)) =
        paths.interpolation_segment(time_ms)
    {
        let eased_progress = apply_easing(progress, *found_easing);
        let options = morph_options
            .keyframes
            .get(&found_time)
            .map(|(value, _)| *value)
            .unwrap_or_default();
        interpolate(prev_val, found_val, eased_progress, options)
    } else {
        // No interior segment - use default or boundary value
        if paths.keyframes.is_empty() {
            paths.default_value.clone()
        } else if let Some((&first_time, (val, _))) = paths.keyframes.iter().next() {
            if time_ms <= first_time {
                val.clone()
            } else {
                paths.last_value()
            }
        } else {
            paths.default_value.clone()
        }
    }
}

/// Interpolate between two `Vec<TextPath>` using morph options.
pub fn interpolate_text_paths(
    source: &Vec<TextPath>,
    target: &Vec<TextPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<TextPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(TextPath {
                path: path.path.clone(),
                color: path.color.clone(),
                opacity: path.opacity * source_alpha,
            });
        }
        for path in target {
            result.push(TextPath {
                path: path.path.clone(),
                color: path.color.clone(),
                opacity: path.opacity * target_alpha,
            });
        }
        return result;
    }

    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists =
        align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);
    aligned_lists
        .into_iter()
        .enumerate()
        .map(|(index, (source_path, target_path))| TextPath {
            path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
            color: if t < 0.5 {
                source.get(index).map(|path| path.color.clone()).unwrap_or_else(|| {
                    target.get(index).map(|path| path.color.clone()).unwrap_or_else(|| {
                        typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                    })
                })
            } else {
                target.get(index).map(|path| path.color.clone()).unwrap_or_else(|| {
                    source.get(index).map(|path| path.color.clone()).unwrap_or_else(|| {
                        typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                    })
                })
            },
            opacity: 1.0,
        })
        .collect()
}

/// Interpolate between two `Vec<VelloPath>` using morph options.
pub fn interpolate_vello_paths(
    source: &Vec<VelloPath>,
    target: &Vec<VelloPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<VelloPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(source_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(source_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
            });
        }
        for path in target {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(target_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(target_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
            });
        }
        return result;
    }

    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists =
        align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);
    aligned_lists
        .into_iter()
        .enumerate()
        .map(|(index, (source_path, target_path))| {
            let source_element = source.get(index);
            let target_element = target.get(index);
            VelloPath {
                path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
                fill: match (
                    source_element.and_then(|e| e.fill),
                    target_element.and_then(|e| e.fill),
                ) {
                    (Some(c1), Some(c2)) => Some(lerp_color(c1, c2, t)),
                    (Some(c), None) => Some(if t < 0.5 {
                        c
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    }),
                    (None, Some(c)) => Some(if t >= 0.5 {
                        c
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    }),
                    (None, None) => None,
                },
                stroke: match (
                    source_element.and_then(|e| e.stroke),
                    target_element.and_then(|e| e.stroke),
                ) {
                    (Some((c1, w1)), Some((c2, w2))) => {
                        Some((lerp_color(c1, c2, t), w1 + (w2 - w1) * t))
                    },
                    (Some((c, w)), None) => Some((
                        if t < 0.5 {
                            c
                        } else {
                            vello::peniko::Color::TRANSPARENT
                        },
                        if t < 0.5 { w } else { 0.0 },
                    )),
                    (None, Some((c, w))) => Some((
                        if t >= 0.5 {
                            c
                        } else {
                            vello::peniko::Color::TRANSPARENT
                        },
                        if t >= 0.5 { w } else { 0.0 },
                    )),
                    (None, None) => None,
                },
                line_cap: source_element.map(|e| e.line_cap).unwrap_or(0),
                line_join: source_element.map(|e| e.line_join).unwrap_or(0),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;

    #[test]
    fn test_list_alignment() {
        let p1 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 0.0)),
            PathEl::LineTo(Point::new(10.0, 0.0)),
        ]);
        let p2 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 10.0)),
            PathEl::LineTo(Point::new(10.0, 10.0)),
        ]);

        let source = vec![p1.clone(), p2.clone()];
        let target = vec![p1.clone(), p2.clone(), p1.clone(), p2.clone(), p1.clone()];

        let aligned = align_path_lists(&source, &target);
        assert_eq!(aligned.len(), 5);

        // Check that 3rd item in source is degenerate
        let (s3, _t3) = &aligned[2];
        assert_eq!(s3.elements().len(), 1);
        assert!(matches!(s3.elements()[0], PathEl::MoveTo(_)));
    }

    #[test]
    fn test_subpath_alignment() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        target.move_to(Point::new(20.0, 0.0));
        target.line_to(Point::new(30.0, 0.0));

        let (a_src, a_tgt) = align_subpaths(&source, &target);

        // 2 subpaths each
        let src_subs = extract_subpaths(&a_src);
        let tgt_subs = extract_subpaths(&a_tgt);
        assert_eq!(src_subs.len(), 2);
        assert_eq!(tgt_subs.len(), 2);
    }

    #[test]
    fn test_segment_alignment() {
        // Triangle
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));
        source.line_to(Point::new(5.0, 10.0));
        source.close_path();

        // Square
        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 0.0));
        target.line_to(Point::new(10.0, 0.0));
        target.line_to(Point::new(10.0, 10.0));
        target.line_to(Point::new(0.0, 10.0));
        target.close_path();

        let subs_src = extract_subpaths(&source);
        let subs_tgt = extract_subpaths(&target);

        let (a_src, a_tgt) = align_segments(&subs_src[0], &subs_tgt[0]);
        assert_eq!(a_src.len(), a_tgt.len());

        // Everything should be MoveTo, CurveTo, or ClosePath
        for el in &a_src {
            assert!(matches!(el, PathEl::MoveTo(_) | PathEl::CurveTo(_, _, _) | PathEl::ClosePath));
        }
    }

    #[test]
    fn test_morphing() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 10.0));
        target.line_to(Point::new(10.0, 10.0));

        let t50 = morph_paths(&source, &target, 0.5);
        let els = t50.elements();

        if let PathEl::MoveTo(p) = els[0] {
            assert_eq!(p.y, 5.0);
        } else {
            panic!("Expected MoveTo");
        }
    }

    #[test]
    fn test_match_strategy_reorders_paths_by_centroid() {
        let left = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 0.0)),
            PathEl::LineTo(Point::new(10.0, 0.0)),
        ]);
        let right = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(100.0, 0.0)),
            PathEl::LineTo(Point::new(110.0, 0.0)),
        ]);

        let auto_pairs = align_path_lists_with_strategy(
            &[left.clone(), right.clone()],
            &[right.clone(), left.clone()],
            MorphStrategy::Auto,
        );
        let match_pairs = align_path_lists_with_strategy(
            &[left.clone(), right.clone()],
            &[right.clone(), left.clone()],
            MorphStrategy::Match,
        );

        assert_ne!(get_centroid(&auto_pairs[0].0), get_centroid(&auto_pairs[0].1));
        assert_eq!(get_centroid(&match_pairs[0].0), get_centroid(&match_pairs[0].1));
    }

    #[test]
    fn test_nearest_strategy_pairs_by_proximity() {
        let left = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 0.0)),
            PathEl::LineTo(Point::new(10.0, 0.0)),
        ]);
        let right = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(100.0, 0.0)),
            PathEl::LineTo(Point::new(110.0, 0.0)),
        ]);

        // Target order is swapped: [right, left]
        let nearest_pairs = align_path_lists_with_strategy(
            &[left.clone(), right.clone()],
            &[right.clone(), left.clone()],
            MorphStrategy::Nearest,
        );

        // Nearest should pair left↔left and right↔right despite index mismatch
        assert_eq!(get_centroid(&nearest_pairs[0].0), get_centroid(&nearest_pairs[0].1));
        assert_eq!(get_centroid(&nearest_pairs[1].0), get_centroid(&nearest_pairs[1].1));
    }

    #[test]
    fn test_nearest_strategy_with_unequal_counts() {
        let p1 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(0.0, 0.0)),
            PathEl::LineTo(Point::new(10.0, 0.0)),
        ]);
        let p2 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(100.0, 0.0)),
            PathEl::LineTo(Point::new(110.0, 0.0)),
        ]);
        let p3 = BezPath::from_vec(vec![
            PathEl::MoveTo(Point::new(200.0, 0.0)),
            PathEl::LineTo(Point::new(210.0, 0.0)),
        ]);

        // 2 sources, 3 targets
        let pairs = align_path_lists_with_strategy(
            &[p1.clone(), p2.clone()],
            &[p3.clone(), p2.clone(), p1.clone()],
            MorphStrategy::Nearest,
        );

        assert_eq!(pairs.len(), 3);
        // p1 should pair with p1 (centroids match)
        // p2 should pair with p2 (centroids match)
        // p3 should pair with degenerate source
    }

    #[test]
    fn test_path_arc_curves_midpoint_interpolation() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 10.0));
        target.line_to(Point::new(10.0, 10.0));

        let curved = morph_paths_with_options(
            &source,
            &target,
            0.5,
            MorphOptions {
                strategy: MorphStrategy::Auto,
                path_arc: 1.57,
                stretch: false,
            },
        );

        match curved.elements()[0] {
            PathEl::MoveTo(point) => assert_ne!(point.x, 0.0),
            _ => panic!("Expected curved move-to point"),
        }
    }

    #[test]
    fn test_stretch_normalizes_morph_bounds() {
        let mut source = BezPath::new();
        source.move_to(Point::new(0.0, 0.0));
        source.line_to(Point::new(10.0, 0.0));
        source.line_to(Point::new(10.0, 10.0));
        source.line_to(Point::new(0.0, 10.0));
        source.close_path();

        let mut target = BezPath::new();
        target.move_to(Point::new(0.0, 0.0));
        target.line_to(Point::new(100.0, 0.0));
        target.line_to(Point::new(100.0, 10.0));
        target.line_to(Point::new(0.0, 10.0));
        target.close_path();

        let stretched = morph_paths_with_options(
            &source,
            &target,
            0.5,
            MorphOptions {
                strategy: MorphStrategy::Auto,
                path_arc: 0.0,
                stretch: true,
            },
        );
        let unstretched = morph_paths_with_options(
            &source,
            &target,
            0.5,
            MorphOptions {
                strategy: MorphStrategy::Auto,
                path_arc: 0.0,
                stretch: false,
            },
        );

        let stretched_bounds = path_bounds(&stretched);
        let unstretched_bounds = path_bounds(&unstretched);
        assert!(stretched_bounds.width() >= unstretched_bounds.width());
    }

    // ────────────────────────────────────────────────────────
    // evaluate_paths_with_options tests (moved from track.rs)
    // ────────────────────────────────────────────────────────

    fn identity_interpolate<T: Interpolate>(
        source: &T,
        target: &T,
        t: f32,
        _options: MorphOptions,
    ) -> T {
        source.interpolate(target, t)
    }

    #[test]
    fn test_evaluate_paths_with_options_empty_track() {
        let paths: PropertyTrack<f32> = PropertyTrack::new(42.0);
        let morph: PropertyTrack<MorphOptions> = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        assert_eq!(result, 42.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_before_first_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        // Before first keyframe, evaluate_paths_with_options returns the first keyframe's value
        assert_eq!(result, 100.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_after_last_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 10.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 2000, identity_interpolate);
        assert_eq!(result, 100.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_between_two_keyframes() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 0.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        assert_eq!(result, 50.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_uses_morph_at_second_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 0.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let mut morph = PropertyTrack::new(MorphOptions::default());
        // Morph at the same time as the second keyframe
        morph.add_keyframe(
            1000,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
            Easing::Linear,
        );
        // Should use Fade morph strategy (which just returns source+target)
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        // identity_interpolate just does normal interpolation regardless of morph
        // So result should be 50.0
        assert_eq!(result, 50.0);
    }

    // ────────────────────────────────────────────────────────
    // Morph interpolation tests (moved from track.rs)
    // ────────────────────────────────────────────────────────

    #[test]
    fn fade_vello_paths_at_start_returns_only_source() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(
            &source,
            &target,
            0.0,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
    }

    #[test]
    fn fade_vello_paths_at_end_returns_only_target() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(
            &source,
            &target,
            1.0,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
    }

    #[test]
    fn fade_vello_paths_at_midpoint_returns_both_halved() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
            stroke: Some((vello::peniko::Color::from_rgba8(255, 255, 255, 100), 2.0)),
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(
            &source,
            &target,
            0.5,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 2);
        // Source path alpha should be halved: 200 * 0.5 = 100
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 100);
        // Source stroke alpha should be halved: 100 * 0.5 = 50
        assert_eq!(result[0].stroke.unwrap().0.to_rgba8().a, 50);
        assert_eq!(result[0].stroke.unwrap().1, 2.0);
        // Target path alpha should be halved: 128 * 0.5 = 64
        assert_eq!(result[1].fill.unwrap().to_rgba8().a, 64);
        assert!(result[1].stroke.is_none());
    }

    #[test]
    fn fade_text_paths_at_midpoint_returns_both_halved() {
        let source = vec![TextPath {
            path: BezPath::new(),
            color: typst::visualize::Paint::Solid(typst::visualize::Color::BLACK),
            opacity: 1.0,
        }];
        let target = vec![TextPath {
            path: BezPath::new(),
            color: typst::visualize::Paint::Solid(typst::visualize::Color::WHITE),
            opacity: 0.8,
        }];
        let result = interpolate_text_paths(
            &source,
            &target,
            0.5,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].opacity, 0.5);
        assert_eq!(result[1].opacity, 0.4);
    }

    #[test]
    fn fade_vello_paths_empty_source() {
        let source: Vec<VelloPath> = vec![];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(
            &source,
            &target,
            0.25,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 32); // 128 * 0.25
    }

    #[test]
    fn fade_vello_paths_empty_target() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target: Vec<VelloPath> = vec![];
        let result = interpolate_vello_paths(
            &source,
            &target,
            0.75,
            MorphOptions {
                strategy: MorphStrategy::Fade,
                path_arc: 0.0,
                stretch: false,
            },
        );
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 50); // 200 * 0.25
    }
}
