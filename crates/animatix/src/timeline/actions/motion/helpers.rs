use super::*;

pub(super) fn sample_path_along(
    path: &kurbo::BezPath,
    count: usize,
) -> Option<Vec<(kurbo::Point, f64)>> {
    // A zero-length trim yields an empty path with no current position, so the
    // route's start is taken from the path itself rather than from sample 0.
    let start = path.iter().find_map(|el| match el {
        kurbo::PathEl::MoveTo(p) => Some(p),
        _ => None,
    })?;
    let points: Vec<kurbo::Point> = (0..=count)
        .map(|i| {
            crate::timeline::path_progress::trim_path_by_progress(path, i as f64 / count as f64)
                .current_position()
                .unwrap_or(start)
        })
        .collect();
    if points.len() < 2 {
        return None;
    }

    let mut out = Vec::with_capacity(points.len());
    for (i, point) in points.iter().enumerate() {
        // The heading at a sample is the chord that reaches it; the first
        // sample has no chord behind it, so it borrows the one ahead.
        let (a, b) = if i == 0 {
            (points[0], points[1])
        } else {
            (points[i - 1], *point)
        };
        let heading = (b.y - a.y).atan2(b.x - a.x);
        out.push((*point, heading));
    }
    Some(out)
}

/// Fraction of the travel an `[anticipate: …]` counter-move leans back by.
const ANTICIPATION_LEAN: f32 = 0.12;

/// Insert the counter-move that precedes a translation: the actor leans *back*
/// by a fraction of its travel across the anticipation window, then starts the
/// real move from that leaned position. This is the anticipation beat the motion
/// craft tables put at 60–120 ms — without it a translation reads as a value
/// being dialled rather than an object deciding to go.
///
/// Returns the value the `t_start` keyframe should hold, or `None` when there is
/// nothing to anticipate from (no window, or the action starts at t=0).
pub(super) fn anticipation_start(
    track: &mut crate::timeline::AnimationTrack,
    anticipate_ms: f64,
    t_start_ms: u64,
    start_offset: [f32; 2],
    travel: [f32; 2],
) -> Option<[f32; 2]> {
    if anticipate_ms <= 0.0 || t_start_ms == 0 {
        return None;
    }
    let t_anti = t_start_ms.saturating_sub(anticipate_ms as u64);
    if t_anti >= t_start_ms {
        return None;
    }
    let leaned = [
        start_offset[0] - travel[0] * ANTICIPATION_LEAN,
        start_offset[1] - travel[1] * ANTICIPATION_LEAN,
    ];
    track.geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
        t_anti,
        start_offset,
        Easing::EaseInOut,
    );
    Some(leaned)
}
