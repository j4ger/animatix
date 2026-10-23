//! Path interpolation and morph entry points.

use super::*;

use super::geometry::*;

/// Interpolate one path into another at `t`.
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

pub(super) fn lerp_point(p1: Point, p2: Point, t: f64) -> Point {
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

pub(super) fn lerp_color(
    c1: vello::peniko::Color,
    c2: vello::peniko::Color,
    t: f32,
) -> vello::peniko::Color {
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
