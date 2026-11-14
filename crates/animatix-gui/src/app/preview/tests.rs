use super::*;

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
