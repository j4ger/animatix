//! Shared sampling clock for authored boundaries and output frame grids.

/// Convert seconds without losing (or crossing early) a whole-millisecond boundary.
///
/// Timeline tracks store integer milliseconds. The nearest f64 representation of
/// an exact millisecond in seconds (e.g. 8040 / 1000) can multiply back to just
/// below that integer. Round-trip the candidate boundary rather than rounding or
/// adding an epsilon to all samples: the representable instant before it must
/// still land in the previous tick.
pub(crate) fn seconds_to_millis(seconds: f64) -> f64 {
    let millis = seconds * 1000.0;
    if !millis.is_finite() {
        return millis;
    }
    let nearest = millis.round();
    let boundary = nearest / 1000.0;
    if seconds == boundary {
        nearest
    } else if millis == nearest && seconds < boundary {
        // Multiplication itself can round a preceding sample onto the boundary.
        millis.next_down()
    } else {
        millis
    }
}

/// Discrete tick used by the timeline evaluator and readers of actor state.
pub fn sample_tick_millis(seconds: f64) -> u64 {
    seconds_to_millis(seconds) as u64
}

/// The timestamp of a zero-based sample on a constant-rate frame grid.
pub fn frame_seconds(frame: u32, fps: f64) -> f64 {
    f64::from(frame) / fps
}

/// First frame timestamp at or after `seconds`, saturated to the index range.
/// Also the frame count for the half-open interval `[0, seconds)`.
pub fn frame_at_or_after(seconds: f64, fps: f64) -> u32 {
    let frame = (seconds * fps).floor() as u32;
    // Compare the actual sample: multiplication can round a neighboring
    // instant to the same nominal grid point.
    if frame_seconds(frame, fps) < seconds {
        frame.saturating_add(1)
    } else {
        frame
    }
}

/// Last frame timestamp at or before `seconds`, saturated to the index range.
pub fn frame_at_or_before(seconds: f64, fps: f64) -> u32 {
    let frame = frame_at_or_after(seconds, fps);
    if frame_seconds(frame, fps) > seconds {
        frame.saturating_sub(1)
    } else {
        frame
    }
}

fn canonical_millis(seconds: f64) -> Option<f64> {
    let millis = (seconds * 1000.0).round();
    // A valid integer in f64, represented in seconds using the same clock.
    (millis.abs() <= (1_u64 << 52) as f64 && millis / 1000.0 == seconds).then_some(millis)
}

/// Preserve integer-millisecond sums without quantizing fractional offsets.
pub fn add_seconds(a: f64, b: f64) -> f64 {
    match (canonical_millis(a), canonical_millis(b)) {
        (Some(a_ms), Some(b_ms)) => (a_ms + b_ms) / 1000.0,
        _ => a + b,
    }
}

/// Compare global input to the placed local millisecond boundary. A raw
/// subtraction can send the boundary (or its actual predecessor) to the
// wrong side. Millisecond-aligned origins keep an integer sum; other
/// origins keep their compiled fractional placement.
pub fn scene_local_seconds(global: f64, origin: f64) -> f64 {
    let local = global - origin;
    let local_ms = (local * 1000.0).round();
    let boundary_local = local_ms / 1000.0;
    let boundary_global = match canonical_millis(origin) {
        Some(origin_ms) => (origin_ms + local_ms) / 1000.0,
        None => origin + boundary_local,
    };
    if global == boundary_global {
        boundary_local
    } else if global < boundary_global && local >= boundary_local {
        boundary_local.next_down()
    } else if global > boundary_global && local <= boundary_local {
        boundary_local
    } else {
        local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_grid_preserves_the_exact_sample_and_its_neighbors() {
        for fps in [23.976, 24.0, 25.0, 29.97, 50.0, 60.0, 120.0] {
            for i in [1, 7, 47, 201, 14321] {
                let t = frame_seconds(i, fps);
                assert_eq!(frame_at_or_before(t.next_down(), fps), i - 1);
                assert_eq!(frame_at_or_after(t.next_down(), fps), i);
                assert_eq!(frame_at_or_before(t, fps), i);
                assert_eq!(frame_at_or_after(t, fps), i);
                assert_eq!(frame_at_or_before(t.next_up(), fps), i);
                assert_eq!(frame_at_or_after(t.next_up(), fps), i + 1);
            }
        }
        assert_eq!(frame_at_or_after(0.28, 25.0), 7);
        assert_eq!(frame_at_or_after(8.04, 25.0), 201);
    }

    #[test]
    fn placed_events_sample_the_same_local_tick_without_snapping_early() {
        assert_eq!(add_seconds(0.1, 0.2), 0.3);
        for (start, millis) in [(1.0, 880), (0.001, 11), (0.3, 20), (0.0005, 5)] {
            let local = millis as f64 / 1000.0;
            let global = add_seconds(start, local);
            assert_eq!(scene_local_seconds(global, start), local);
            assert_eq!(
                sample_tick_millis(scene_local_seconds(global.next_down(), start)),
                millis - 1
            );
            assert_eq!(sample_tick_millis(scene_local_seconds(global.next_up(), start)), millis);
        }
    }
}
