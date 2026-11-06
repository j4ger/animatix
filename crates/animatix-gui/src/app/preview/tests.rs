use super::*;

#[test]
fn test_local_to_world_zero_rotation() {
    let result = local_to_world([0.0, 0.0], [10.0, 20.0], 0.0);
    assert!((result.x - 10.0).abs() < 1e-6);
    assert!((result.y - 20.0).abs() < 1e-6);
}

#[test]
fn test_local_to_world_90_degrees() {
    let result = local_to_world([1.0, 0.0], [0.0, 0.0], std::f32::consts::FRAC_PI_2);
    assert!((result.x - 0.0).abs() < 1e-6, "x={}", result.x);
    assert!((result.y - 1.0).abs() < 1e-6, "y={}", result.y);
}

#[test]
fn test_local_to_world_180_degrees() {
    let result = local_to_world([1.0, 0.0], [0.0, 0.0], std::f32::consts::PI);
    assert!((result.x - (-1.0)).abs() < 1e-6, "x={}", result.x);
    assert!((result.y - 0.0).abs() < 1e-6, "y={}", result.y);
}

#[test]
fn test_local_to_world_various_positions_zero_rotation() {
    let cases = [
        ([5.0, -3.0], [100.0, 200.0], [105.0, 197.0]),
        ([-50.0, 25.0], [10.0, 20.0], [-40.0, 45.0]),
        ([0.0, 0.0], [-100.0, 300.0], [-100.0, 300.0]),
        ([33.0, 77.0], [-20.0, -50.0], [13.0, 27.0]),
    ];
    for (local, position, expected) in &cases {
        let result = local_to_world(*local, *position, 0.0);
        assert!(
            (result.x - expected[0]).abs() < 1e-6,
            "local={:?} pos={:?}: expected x={}, got x={}",
            local,
            position,
            expected[0],
            result.x,
        );
        assert!(
            (result.y - expected[1]).abs() < 1e-6,
            "local={:?} pos={:?}: expected y={}, got y={}",
            local,
            position,
            expected[1],
            result.y,
        );
    }
}

#[test]
fn test_handle_anchor_local_all_handles() {
    let size = [100.0, 50.0];
    // handle_anchor_local returns the opposite handle's local position.
    // Opposite mapping from local_handle_positions with size/2 = [50, 25]:
    //   0 (TL corner) → positions[2] = (50, 25)   [BR corner]
    //   1 (TR corner) → positions[3] = (-50, 25)  [BL corner]
    //   2 (BR corner) → positions[0] = (-50, -25) [TL corner]
    //   3 (BL corner) → positions[1] = (50, -25)  [TR corner]
    //   4 (top-mid)   → positions[6] = (0, 25)    [bottom-mid]
    //   5 (right-mid) → positions[7] = (-50, 0)   [left-mid]
    //   6 (bottom-mid)→ positions[4] = (0, -25)   [top-mid]
    //   7 (left-mid)  → positions[5] = (50, 0)    [right-mid]
    let expected: [[f32; 2]; 8] = [
        [50.0, 25.0],
        [-50.0, 25.0],
        [-50.0, -25.0],
        [50.0, -25.0],
        [0.0, 25.0],
        [-50.0, 0.0],
        [0.0, -25.0],
        [50.0, 0.0],
    ];
    for (i, expected_item) in expected.iter().enumerate() {
        let result = handle_anchor_local(i, size);
        assert!(
            (result[0] - expected_item[0]).abs() < 1e-6,
            "handle {}: expected x={}, got x={}",
            i,
            expected_item[0],
            result[0],
        );
        assert!(
            (result[1] - expected_item[1]).abs() < 1e-6,
            "handle {}: expected y={}, got y={}",
            i,
            expected_item[1],
            result[1],
        );
    }
}

#[test]
fn test_handle_constrains_axis_corners_return_false() {
    for handle in 0..4 {
        assert!(
            !handle_constrains_axis(handle),
            "handle {} (corner) should not constrain axis",
            handle,
        );
    }
}

#[test]
fn test_handle_constrains_axis_edges_return_true() {
    for handle in 4..8 {
        assert!(handle_constrains_axis(handle), "handle {} (edge) should constrain axis", handle,);
    }
}

#[test]
fn test_timeline_tick_times_scale_with_duration() {
    // (duration, expected tick count, sampled (index, time) pairs)
    type TickCase = (f64, usize, &'static [(usize, f64)]);
    let cases: &[TickCase] = &[
        (2.0, 9, &[(1, 0.25), (4, 1.0), (8, 2.0)]),
        (5.0, 11, &[(0, 0.0), (2, 1.0), (10, 5.0)]),
        (15.0, 16, &[(5, 5.0), (15, 15.0)]),
        (45.0, 10, &[(3, 15.0), (9, 45.0)]),
        (100.0, 11, &[(5, 50.0), (10, 100.0)]),
        (0.1, 2, &[(0, 0.0), (1, 0.1)]),
        (3.7, 9, &[]),
        (42.0, 10, &[]),
    ];
    for (duration, expected_len, samples) in cases {
        let ticks = timeline_tick_times(*duration);
        assert_eq!(ticks.len(), *expected_len, "duration={duration}");
        for (index, expected) in samples.iter() {
            assert!(
                (ticks[*index] - *expected).abs() < 1e-9,
                "duration={duration} tick {index}: expected {expected}, got {}",
                ticks[*index]
            );
        }
        assert!(
            (ticks.last().unwrap() - *duration).abs() < 1e-9,
            "last tick must equal duration_s"
        );
        for pair in ticks.windows(2) {
            assert!(
                pair[0] < pair[1],
                "ticks must be strictly increasing: {} >= {}",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn test_fit_preview_containment() {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    // Wide window: height constrained
    let size_wide = fit_preview(dims, Vec2::new(1600.0, 600.0));
    assert!(size_wide.x <= 1600.0 + 1e-4);
    assert!(size_wide.y <= 600.0 + 1e-4);
    assert!((size_wide.x / size_wide.y - 1920.0 / 1080.0).abs() < 1e-4);

    // Tall window: width constrained
    let size_tall = fit_preview(dims, Vec2::new(800.0, 900.0));
    assert!(size_tall.x <= 800.0 + 1e-4);
    assert!(size_tall.y <= 900.0 + 1e-4);
    assert!((size_tall.x / size_tall.y - 1920.0 / 1080.0).abs() < 1e-4);
}
