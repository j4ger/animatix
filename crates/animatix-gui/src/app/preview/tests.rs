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
fn test_timeline_tick_times_short() {
    // ≤2.0s → step 0.25s
    // ticks: 0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75  (8 in loop) + push 2.0 = 9
    let ticks = timeline_tick_times(2.0);
    assert_eq!(ticks.len(), 9);
    assert!((ticks[1] - 0.25).abs() < 1e-9);
    assert!((ticks[4] - 1.0).abs() < 1e-9);
    assert!((ticks[8] - 2.0).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_medium() {
    // Between 2.0 and 5.0 → step 0.5s
    // ticks: 0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5 (10) + push 5.0 = 11
    let ticks = timeline_tick_times(5.0);
    assert_eq!(ticks.len(), 11);
    assert!((ticks[0] - 0.0).abs() < 1e-9);
    assert!((ticks[2] - 1.0).abs() < 1e-9);
    assert!((ticks[10] - 5.0).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_long() {
    // Between 5.0 and 15.0 → step 1.0s
    // ticks: 0..14 (15 ticks) + push 15.0 = 16
    let ticks = timeline_tick_times(15.0);
    assert_eq!(ticks.len(), 16);
    assert!((ticks[5] - 5.0).abs() < 1e-9);
    assert!((ticks[15] - 15.0).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_very_long() {
    // Between 15.0 and 45.0 → step 5.0s
    // ticks: 0,5,10,15,20,25,30,35,40 (9) + push 45.0 = 10
    let ticks = timeline_tick_times(45.0);
    assert_eq!(ticks.len(), 10);
    assert!((ticks[3] - 15.0).abs() < 1e-9);
    assert!((ticks[9] - 45.0).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_extreme() {
    // >45.0s → step 10.0s
    // ticks: 0,10,20,30,40,50,60,70,80,90 (10) + push 100.0 = 11
    let ticks = timeline_tick_times(100.0);
    assert_eq!(ticks.len(), 11);
    assert!((ticks[5] - 50.0).abs() < 1e-9);
    assert!((ticks[10] - 100.0).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_very_short() {
    // 0.1s → clamped to min 0.1, step 0.25
    let ticks = timeline_tick_times(0.1);
    assert_eq!(ticks.len(), 2, "Only t=0 and t=0.1");
    assert!((ticks[0] - 0.0).abs() < 1e-9);
    assert!((ticks[1] - 0.1).abs() < 1e-9);
}

#[test]
fn test_timeline_tick_times_ends_with_duration() {
    let ticks = timeline_tick_times(3.7);
    assert!((ticks.last().unwrap() - 3.7).abs() < 1e-9, "last tick must equal duration_s");
}

#[test]
fn test_timeline_tick_times_strictly_increasing() {
    let ticks = timeline_tick_times(42.0);
    for pair in ticks.windows(2) {
        assert!(
            pair[0] < pair[1],
            "ticks must be strictly increasing: {} >= {}",
            pair[0],
            pair[1]
        );
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
