use super::nice_tick_interval;

#[test]
fn nice_tick_interval_covers_ranges_and_boundaries() {
    let cases = [
        // (visible_range, expected interval at 10 target ticks)
        (100.0, 10.0),
        (50.0, 5.0),
        (0.5, 0.05),
        (0.0, 1.0),
        (-100.0, 10.0),
        (10000.0, 1000.0),
        (0.1, 0.01),
        (1.0, 0.1),
        (10.0, 1.0),
        (1000.0, 100.0),
        // Boundaries around the 1.5 / 3.5 / 7.5 nice-multiple cutoffs.
        (14.9, 1.0),
        (15.1, 2.0),
        (34.9, 2.0),
        (35.1, 5.0),
        (74.9, 5.0),
        (75.1, 10.0),
    ];
    for (range, expected) in cases {
        let interval = nice_tick_interval(range, 10.0);
        assert!(interval > 0.0, "interval must be positive for range={range}");
        assert!(
            (interval - expected).abs() < 0.001,
            "range={range}: expected {expected}, got {interval}"
        );
    }
}
