//! Build-time scene statistics bake pass (STAT-1).
//!
//! Pre-samples authored tracks to derive aggregate scene facts:
//! - `scene.stats.motion`: aggregate motion energy across the cast
//! - `scene.stats.ink`: fraction of canvas area covered by active actors
//! - `scene.stats.focus_x` / `.focus_y`: centroid of cast weighted by size and opacity
//! - `scene.stats.spread_x` / `.spread_y`: weighted RMS distance from focus (normalized)
//! - `scene.stats.cast`: count of visible actors at time t
//!
//! Each stat is baked into `timeline.env` as a flat piecewise-linear `List<Num>`
//! before `env_base` is frozen, enabling zero-GPU-overhead dynamic ambience.
//!
//! See `docs/scene_stats.md` for full design notes and mathematical derivations.

use crate::timeline::{AnimationTrack, Timeline, Value};

/// Number of sample points P clamped between 32 and 256.
fn compute_sample_count(duration_s: f64) -> usize {
    ((duration_s * 8.0).round() as usize).clamp(32, 256)
}

/// A point on a scalar curve over time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CurvePoint {
    pub(crate) t: f64,
    pub(crate) v: f64,
}

/// Ramer–Douglas–Peucker line simplification for functional curves (t, v).
///
/// Uses vertical value deviation because t and v have different units and
/// the goal is bounding approximation error: max |v(t) - v_approx(t)| <= epsilon.
pub(crate) fn rdp_simplify(points: &[CurvePoint], epsilon: f64) -> Vec<CurvePoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }

    let first = points[0];
    let last = *points.last().unwrap();
    let dt = last.t - first.t;

    let mut max_dist = 0.0_f64;
    let mut max_idx = 0;

    for (i, p) in points.iter().enumerate().skip(1).take(points.len() - 2) {
        let expected_v = if dt.abs() <= 1e-12 {
            first.v
        } else {
            first.v + (last.v - first.v) * ((p.t - first.t) / dt)
        };
        let dist = (p.v - expected_v).abs();
        if dist > max_dist {
            max_dist = dist;
            max_idx = i;
        }
    }

    if max_dist > epsilon {
        let mut left = rdp_simplify(&points[..=max_idx], epsilon);
        let right = rdp_simplify(&points[max_idx..], epsilon);
        left.pop(); // Remove duplicate midpoint
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

/// 3-tap binomial smoothing [0.25, 0.5, 0.25] over sampled curve points.
fn smooth_samples(points: &[CurvePoint], seamless_loop: bool) -> Vec<CurvePoint> {
    let n = points.len();
    if n <= 2 {
        return points.to_vec();
    }

    let mut smoothed = Vec::with_capacity(n);
    for i in 0..n {
        let (prev_v, next_v) = if seamless_loop {
            let prev_idx = if i == 0 { n - 2 } else { i - 1 };
            let next_idx = if i == n - 1 { 1 } else { i + 1 };
            (points[prev_idx].v, points[next_idx].v)
        } else {
            let prev_idx = i.saturating_sub(1);
            let next_idx = (i + 1).min(n - 1);
            (points[prev_idx].v, points[next_idx].v)
        };
        let v = 0.25 * prev_v + 0.5 * points[i].v + 0.25 * next_v;
        smoothed.push(CurvePoint { t: points[i].t, v });
    }
    smoothed
}

/// Process raw sampled points through smoothing, RDP simplification, and loop pinning.
fn process_stat_curve(raw: &[CurvePoint], seamless_loop: bool) -> Vec<CurvePoint> {
    if raw.is_empty() {
        return Vec::new();
    }
    let smoothed = smooth_samples(raw, seamless_loop);

    let min_v = smoothed.iter().map(|p| p.v).fold(f64::INFINITY, f64::min);
    let max_v = smoothed.iter().map(|p| p.v).fold(f64::NEG_INFINITY, f64::max);
    let range = (max_v - min_v).max(0.0);
    let epsilon = (range * 0.005).max(1e-4);

    let mut simplified = rdp_simplify(&smoothed, epsilon);

    if seamless_loop && simplified.len() >= 2 {
        let avg = (simplified.first().unwrap().v + simplified.last().unwrap().v) * 0.5;
        if let Some(first) = simplified.first_mut() {
            first.v = avg;
        }
        if let Some(last) = simplified.last_mut() {
            last.v = avg;
        }
    }

    simplified
}

/// Convert simplified curve points into a flat Value::List [t0, v0, t1, v1, ...].
fn points_to_value_list(points: &[CurvePoint]) -> Value {
    let mut items = Vec::with_capacity(points.len() * 2);
    for p in points {
        items.push(Value::Num(p.t));
        items.push(Value::Num(p.v));
    }
    Value::List(items.into())
}

/// Sample an actor's effective position at time_ms.
fn sample_position(track: &AnimationTrack, time_ms: u64) -> [f64; 2] {
    track
        .geometry
        .position
        .as_ref()
        .map(|p| {
            let v = p.evaluate(time_ms);
            [v[0] as f64, v[1] as f64]
        })
        .unwrap_or([0.0, 0.0])
}

/// Sample an actor's effective size at time_ms.
fn sample_size(track: &AnimationTrack, time_ms: u64) -> [f64; 2] {
    track
        .geometry
        .size
        .as_ref()
        .map(|s| {
            let v = s.evaluate(time_ms);
            [v[0] as f64, v[1] as f64]
        })
        .unwrap_or([0.0, 0.0])
}

/// Sample an actor's effective opacity at time_ms.
fn sample_opacity(track: &AnimationTrack, time_ms: u64) -> f64 {
    if track.hidden_by_default {
        0.0
    } else {
        track.style.opacity.as_ref().map(|o| o.evaluate(time_ms) as f64).unwrap_or(1.0)
    }
}

/// Sample an actor's instantaneous velocity norm ||p'(t)|| at time_ms.
fn sample_speed(track: &AnimationTrack, time_ms: u64, duration_ms: u64) -> f64 {
    if track.geometry.position.is_none() {
        return 0.0;
    }
    let t_prev = time_ms.saturating_sub(1);
    let t_next = (time_ms + 1).min(duration_ms);
    let dt = (t_next - t_prev) as f64 / 1000.0;
    if dt <= 1e-6 {
        return 0.0;
    }
    let p_prev = sample_position(track, t_prev);
    let p_next = sample_position(track, t_next);
    let vx = (p_next[0] - p_prev[0]) / dt;
    let vy = (p_next[1] - p_prev[1]) / dt;
    (vx * vx + vy * vy).sqrt()
}

/// Bake scene statistics into `timeline.env`.
///
/// Must be called BEFORE `timeline.env_base` is frozen so that the derived
/// curves become immutable base-layer environment entries.
pub(crate) fn bake_scene_stats(timeline: &mut Timeline) {
    let duration_s = timeline.duration_seconds().max(0.1);
    let duration_ms = (duration_s * 1000.0).round() as u64;
    let (canvas_w, canvas_h) = timeline
        .resolution
        .map(|(w, h)| (w as f64, h as f64))
        .unwrap_or((1920.0, 1080.0));
    let canvas_area = (canvas_w * canvas_h).max(1.0);

    let p_count = compute_sample_count(duration_s);

    // Collect all actors that must be excluded from build-time scene statistics:
    // 1. Any actor written to by a modifier program that reads `scene.stats.*` (to prevent feedback
    //    loops / circular dependencies).
    // 2. Any actor whose spatial geometry (at, position, size, from, to) or presence (opacity) is
    //    overwritten at runtime by a modifier program (since build-time tracks lack those values).
    let mut modifier_excluded_targets = std::collections::HashSet::new();
    for p in &timeline.modifier_programs {
        p.collect_stats_excluded_targets(&mut modifier_excluded_targets);
    }

    // Eligible tracks: exclude:
    // 1. Structural Group containers (they don't render visual shapes)
    // 2. Full-viewport backgrounds (e.g. background plates)
    // 3. Actors explicitly marked legend: false / hidden
    // 4. Actors whose spatial geometry/presence is driven dynamically or that consume scene.stats
    let eligible_tracks: Vec<&AnimationTrack> = timeline
        .tracks
        .iter()
        .filter(|(label, t)| {
            if t.actor_type == "Group" {
                return false;
            }
            if crate::timeline::legend::is_full_viewport_background(t) {
                return false;
            }
            if crate::timeline::legend::legend_mode_for_track(t)
                == crate::timeline::legend::LegendMode::Hidden
            {
                return false;
            }
            if modifier_excluded_targets.contains(label.as_str()) {
                return false;
            }
            true
        })
        .map(|(_, t)| t)
        .collect();

    let mut raw_motion = Vec::with_capacity(p_count);
    let mut raw_ink = Vec::with_capacity(p_count);
    let mut raw_focus_x = Vec::with_capacity(p_count);
    let mut raw_focus_y = Vec::with_capacity(p_count);
    let mut raw_spread_x = Vec::with_capacity(p_count);
    let mut raw_spread_y = Vec::with_capacity(p_count);
    let mut raw_cast = Vec::with_capacity(p_count);

    for k in 0..p_count {
        let t_s = k as f64 * duration_s / (p_count - 1) as f64;
        let time_ms = (t_s * 1000.0).round() as u64;

        let mut sum_motion = 0.0_f64;
        let mut sum_ink = 0.0_f64;
        let mut sum_weight = 0.0_f64;
        let mut sum_wx = 0.0_f64;
        let mut sum_wy = 0.0_f64;
        let mut visible_cast = 0.0_f64;

        struct ActorSample {
            weight: f64,
            pos: [f64; 2],
        }
        let mut actor_samples = Vec::with_capacity(eligible_tracks.len());

        for track in &eligible_tracks {
            let opacity = sample_opacity(track, time_ms);
            if opacity > 0.0 {
                visible_cast += 1.0;
            }
            let size = sample_size(track, time_ms);
            let area = size[0] * size[1];
            let weight = area * opacity;

            let pos = sample_position(track, time_ms);

            let speed = sample_speed(track, time_ms, duration_ms);
            sum_motion += speed * area * opacity;
            sum_ink += area * opacity;

            if weight > 0.0 {
                sum_weight += weight;
                sum_wx += weight * pos[0];
                sum_wy += weight * pos[1];
                actor_samples.push(ActorSample { weight, pos });
            }
        }

        // Motion normalized by canvas area and duration
        let motion = sum_motion / (canvas_area * duration_s);
        let ink = sum_ink / canvas_area;

        let (fx, fy) = if sum_weight > 0.0 {
            (sum_wx / sum_weight, sum_wy / sum_weight)
        } else {
            (canvas_w * 0.5, canvas_h * 0.5)
        };

        let (sx, sy) = if sum_weight > 0.0 {
            let mut var_x = 0.0_f64;
            let mut var_y = 0.0_f64;
            for a in &actor_samples {
                let dx = a.pos[0] - fx;
                let dy = a.pos[1] - fy;
                var_x += a.weight * (dx * dx);
                var_y += a.weight * (dy * dy);
            }
            let rms_x = (var_x / sum_weight).sqrt();
            let rms_y = (var_y / sum_weight).sqrt();
            (rms_x / canvas_w, rms_y / canvas_h)
        } else {
            (0.0, 0.0)
        };

        raw_motion.push(CurvePoint { t: t_s, v: motion });
        raw_ink.push(CurvePoint { t: t_s, v: ink });
        raw_focus_x.push(CurvePoint { t: t_s, v: fx });
        raw_focus_y.push(CurvePoint { t: t_s, v: fy });
        raw_spread_x.push(CurvePoint { t: t_s, v: sx });
        raw_spread_y.push(CurvePoint { t: t_s, v: sy });
        raw_cast.push(CurvePoint {
            t: t_s,
            v: visible_cast,
        });
    }

    let loop_flag = timeline.seamless_loop;

    let motion_curve = process_stat_curve(&raw_motion, loop_flag);
    let ink_curve = process_stat_curve(&raw_ink, loop_flag);
    let focus_x_curve = process_stat_curve(&raw_focus_x, loop_flag);
    let focus_y_curve = process_stat_curve(&raw_focus_y, loop_flag);
    let spread_x_curve = process_stat_curve(&raw_spread_x, loop_flag);
    let spread_y_curve = process_stat_curve(&raw_spread_y, loop_flag);
    let cast_curve = process_stat_curve(&raw_cast, loop_flag);

    timeline.env.set("scene.stats.motion", points_to_value_list(&motion_curve));
    timeline.env.set("scene.stats.ink", points_to_value_list(&ink_curve));
    timeline.env.set("scene.stats.focus_x", points_to_value_list(&focus_x_curve));
    timeline.env.set("scene.stats.focus_y", points_to_value_list(&focus_y_curve));
    timeline.env.set("scene.stats.spread_x", points_to_value_list(&spread_x_curve));
    timeline.env.set("scene.stats.spread_y", points_to_value_list(&spread_y_curve));
    timeline.env.set("scene.stats.cast", points_to_value_list(&cast_curve));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdp_simplifies_collinear_points() {
        let points = vec![
            CurvePoint { t: 0.0, v: 0.0 },
            CurvePoint { t: 1.0, v: 10.0 },
            CurvePoint { t: 2.0, v: 20.0 },
            CurvePoint { t: 3.0, v: 30.0 },
        ];
        let simplified = rdp_simplify(&points, 0.1);
        assert_eq!(simplified.len(), 2);
        assert_eq!(simplified[0].t, 0.0);
        assert_eq!(simplified[1].t, 3.0);
    }

    #[test]
    fn rdp_preserves_significant_peaks() {
        let points = vec![
            CurvePoint { t: 0.0, v: 0.0 },
            CurvePoint { t: 1.0, v: 100.0 },
            CurvePoint { t: 2.0, v: 0.0 },
        ];
        let simplified = rdp_simplify(&points, 0.5);
        assert_eq!(simplified.len(), 3);
        assert_eq!(simplified[1].v, 100.0);
    }
}
