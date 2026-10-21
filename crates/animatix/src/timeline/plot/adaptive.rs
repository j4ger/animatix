//! Adaptive subdivision and coordinate transform for plot curve sampling.

use std::collections::HashMap;

use super::{Environment, PlotFuncRef, Value, eval_scalar, eval_vec2};

/// Precomputed affine mapping and screen culling bounds for plot curve sampling.
#[derive(Clone, Copy, Debug)]
pub struct PlotScreenTransform {
    pub sx: f64,
    pub ox: f64,
    pub sy: f64,
    pub oy: f64,
    pub min_screen_x: f64,
    pub max_screen_x: f64,
    pub min_screen_y: f64,
    pub max_screen_y: f64,
    pub jump_dist_sq: f64,
}

impl PlotScreenTransform {
    pub fn new(
        x_domain: &[f64; 2],
        y_domain: &[f64; 2],
        p_size: &[f64; 2],
        padding: &[f64; 4],
    ) -> Self {
        let plot_w = p_size[0] - padding[0] - padding[1];
        let plot_h = p_size[1] - padding[2] - padding[3];
        let shift_x = (padding[0] - padding[1]) / 2.0;
        let shift_y = (padding[2] - padding[3]) / 2.0;
        let x_range = x_domain[1] - x_domain[0];
        let y_range = y_domain[1] - y_domain[0];

        let (sx, ox) = if x_range.abs() > f64::EPSILON {
            let sx = plot_w / x_range;
            let ox = shift_x - 0.5 * plot_w - x_domain[0] * sx;
            (sx, ox)
        } else {
            (0.0, shift_x)
        };

        let (sy, oy) = if y_range.abs() > f64::EPSILON {
            let sy = -plot_h / y_range;
            let oy = shift_y + 0.5 * plot_h - y_domain[0] * sy;
            (sy, oy)
        } else {
            (0.0, shift_y)
        };

        let margin_x = p_size[0] * 2.0;
        let min_screen_x = -(p_size[0] / 2.0) - margin_x;
        let max_screen_x = (p_size[0] / 2.0) + margin_x;

        let margin_y = p_size[1] * 2.0;
        let min_screen_y = -(p_size[1] / 2.0) - margin_y;
        let max_screen_y = (p_size[1] / 2.0) + margin_y;

        let jump_dist_sq = (p_size[0].max(p_size[1])).powi(2) * 4.0;

        Self {
            sx,
            ox,
            sy,
            oy,
            min_screen_x,
            max_screen_x,
            min_screen_y,
            max_screen_y,
            jump_dist_sq,
        }
    }

    #[inline(always)]
    pub fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (x * self.sx + self.ox, y * self.sy + self.oy)
    }

    #[inline(always)]
    pub fn map_point(&self, x: f64, y: f64) -> kurbo::Point {
        kurbo::Point::new(x * self.sx + self.ox, y * self.sy + self.oy)
    }
}

/// Destination sink for sampled curve points.
pub trait PointSink {
    fn push(&mut self, pt: kurbo::Point);
}

impl PointSink for Vec<kurbo::Point> {
    #[inline(always)]
    fn push(&mut self, pt: kurbo::Point) {
        self.push(pt);
    }
}

/// Direct path streaming builder for curve sampling without intermediate point buffers.
pub struct CurvePathBuilder {
    pub path: kurbo::BezPath,
    pub need_move_to: bool,
}

impl CurvePathBuilder {
    pub fn new() -> Self {
        Self {
            path: kurbo::BezPath::new(),
            need_move_to: true,
        }
    }
}

impl Default for CurvePathBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PointSink for CurvePathBuilder {
    #[inline(always)]
    fn push(&mut self, pt: kurbo::Point) {
        if pt.x.is_nan() || pt.y.is_nan() {
            self.need_move_to = true;
        } else if self.need_move_to {
            self.path.move_to((pt.x, pt.y));
            self.need_move_to = false;
        } else {
            self.path.line_to((pt.x, pt.y));
        }
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_cartesian_impl<F: FnMut(f64) -> f64, S: PointSink>(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    transform: &PlotScreenTransform,
    pts: &mut S,
    eval: &mut F,
) {
    let min_screen_y = transform.min_screen_y;
    let max_screen_y = transform.max_screen_y;

    if (p0.y < min_screen_y && p1.y < min_screen_y) || (p0.y > max_screen_y && p1.y > max_screen_y)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let min_screen_x = transform.min_screen_x;
    let max_screen_x = transform.max_screen_x;
    if (p0.x < min_screen_x && p1.x < min_screen_x) || (p0.x > max_screen_x && p1.x > max_screen_x)
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dx = (p1.x - p0.x).abs();
    let dy = (p1.y - p0.y).abs();
    if dx > 0.0 && (dy / dx) > 1000.0 {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let math_y = eval(mid_t);
    let math_x = mid_t;

    let p_mid = transform.map_point(math_x, math_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    // A NaN midpoint (failed evaluation) must force subdivision to the
    // depth floor: the chord-deviation test is always-false on NaN, so
    // without this the failure is swallowed and the curve renders empty.
    if dist_sq > tolerance || depth < min_depth || p_mid.x.is_nan() || p_mid.y.is_nan() {
        sample_recursive_cartesian_impl(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
        sample_recursive_cartesian_impl(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
    } else {
        pts.push(p1);
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_cartesian(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let transform = PlotScreenTransform::new(p_x_domain, p_y_domain, p_size, padding);
    let mut eval = |t: f64| eval_scalar(func, env, arg_name, t, from_cache, to_cache);
    sample_recursive_cartesian_impl(
        min_t, max_t, p0, p1, depth, max_depth, min_depth, tolerance, &transform, pts, &mut eval,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_polar_impl<F: FnMut(f64) -> f64, S: PointSink>(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    transform: &PlotScreenTransform,
    pts: &mut S,
    eval: &mut F,
) {
    let min_screen_y = transform.min_screen_y;
    let max_screen_y = transform.max_screen_y;

    let min_screen_x = transform.min_screen_x;
    let max_screen_x = transform.max_screen_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > transform.jump_dist_sq {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let math_r = eval(mid_t);
    let math_x = math_r * mid_t.cos();
    let math_y = math_r * mid_t.sin();

    let p_mid = transform.map_point(math_x, math_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < min_depth || p_mid.x.is_nan() || p_mid.y.is_nan() {
        sample_recursive_polar_impl(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
        sample_recursive_polar_impl(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
    } else {
        pts.push(p1);
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_polar(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let transform = PlotScreenTransform::new(p_x_domain, p_y_domain, p_size, padding);
    let mut eval = |t: f64| eval_scalar(func, env, arg_name, t, from_cache, to_cache);
    sample_recursive_polar_impl(
        min_t, max_t, p0, p1, depth, max_depth, min_depth, tolerance, &transform, pts, &mut eval,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_parametric_impl<F: FnMut(f64) -> [f64; 2], S: PointSink>(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    transform: &PlotScreenTransform,
    pts: &mut S,
    eval: &mut F,
) {
    let min_screen_y = transform.min_screen_y;
    let max_screen_y = transform.max_screen_y;

    let min_screen_x = transform.min_screen_x;
    let max_screen_x = transform.max_screen_x;

    if ((p0.y < min_screen_y && p1.y < min_screen_y)
        || (p0.y > max_screen_y && p1.y > max_screen_y))
        && ((p0.x < min_screen_x && p1.x < min_screen_x)
            || (p0.x > max_screen_x && p1.x > max_screen_x))
    {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        return;
    }

    let dist_sq_jump = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
    if dist_sq_jump > transform.jump_dist_sq {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    if depth >= max_depth {
        pts.push(p1);
        return;
    }

    let mid_t = (min_t + max_t) / 2.0;
    let [math_x, math_y] = eval(mid_t);
    if math_x.is_nan() || math_y.is_nan() {
        pts.push(kurbo::Point::new(f64::NAN, f64::NAN));
        pts.push(p1);
        return;
    }

    let p_mid = transform.map_point(math_x, math_y);

    let expected_mid_x = (p0.x + p1.x) / 2.0;
    let expected_mid_y = (p0.y + p1.y) / 2.0;
    let dist_sq = (p_mid.x - expected_mid_x).powi(2) + (p_mid.y - expected_mid_y).powi(2);

    if dist_sq > tolerance || depth < min_depth || p_mid.x.is_nan() || p_mid.y.is_nan() {
        sample_recursive_parametric_impl(
            min_t,
            mid_t,
            p0,
            p_mid,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
        sample_recursive_parametric_impl(
            mid_t,
            max_t,
            p_mid,
            p1,
            depth + 1,
            max_depth,
            min_depth,
            tolerance,
            transform,
            pts,
            eval,
        );
    } else {
        pts.push(p1);
    }
}

// Recursive plot samplers thread many independent sampling/styling params;
// grouping them into a struct is a separate refactor.
#[allow(clippy::too_many_arguments)]
pub fn sample_recursive_parametric(
    min_t: f64,
    max_t: f64,
    p0: kurbo::Point,
    p1: kurbo::Point,
    depth: usize,
    max_depth: usize,
    min_depth: usize,
    tolerance: f64,
    env: &mut Environment,
    arg_name: &str,
    func: &PlotFuncRef<'_>,
    p_x_domain: &[f64; 2],
    p_y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
    from_cache: &mut HashMap<u64, Value>,
    to_cache: &mut HashMap<u64, Value>,
    pts: &mut Vec<kurbo::Point>,
) {
    let transform = PlotScreenTransform::new(p_x_domain, p_y_domain, p_size, padding);
    let mut eval = |t: f64| eval_vec2(func, env, arg_name, t, from_cache, to_cache);
    sample_recursive_parametric_impl(
        min_t, max_t, p0, p1, depth, max_depth, min_depth, tolerance, &transform, pts, &mut eval,
    );
}

/// Convert math coordinates to screen coordinates relative to the graph actor center.
///
/// `p_size` is the full width and height of the graph `[w, h]`. `padding` is `[left, right, top,
/// bottom]` in the same pixel units. Returns coordinates in the local (relative-to-center) space.
pub fn math_to_screen_padded(
    math_x: f64,
    math_y: f64,
    x_domain: &[f64; 2],
    y_domain: &[f64; 2],
    p_size: &[f64; 2],
    padding: &[f64; 4],
) -> (f64, f64) {
    PlotScreenTransform::new(x_domain, y_domain, p_size, padding).map(math_x, math_y)
}

/// Uniform subdivision floor for a requested base sample count.
pub fn base_subdivision_depth(resolution: usize, max_depth: usize) -> usize {
    (resolution.max(2).ilog2() as usize).min(max_depth)
}
