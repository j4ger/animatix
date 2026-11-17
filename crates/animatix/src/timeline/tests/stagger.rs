//! Tests for wavefront stagger actions and timing derivation.

use animatix_syntax::parser::parse_source;

use super::*;

type StaggerCase = (&'static str, &'static str, &'static [(&'static str, u64)]);

#[test]
fn test_wavefront_stagger() {
    let cases: [StaggerCase; 2] = [
        (
            "center",
            r#"
                #0s
                c: Ellipse, radius_x: 10, radius_y: 10, at: (100, 100)
                a1: Ellipse, radius_x: 10, radius_y: 10, at: (0, 100)
                a2: Ellipse, radius_x: 10, radius_y: 10, at: (200, 100)
                a3: Ellipse, radius_x: 10, radius_y: 10, at: (100, 0)
                a4: Ellipse, radius_x: 10, radius_y: 10, at: (100, 200)

                #1s
                stagger [50ms, from: center] {
                    fade_in(target: c, duration: 200ms)
                    fade_in(target: a1, duration: 200ms)
                    fade_in(target: a2, duration: 200ms)
                    fade_in(target: a3, duration: 200ms)
                    fade_in(target: a4, duration: 200ms)
                }
            "#,
            &[
                ("c", 1000),
                ("a1", 1050),
                ("a2", 1050),
                ("a3", 1050),
                ("a4", 1050),
            ],
        ),
        (
            "top_left",
            r#"
                #0s
                p00: Ellipse, radius_x: 10, radius_y: 10, at: (0, 0)
                p10: Ellipse, radius_x: 10, radius_y: 10, at: (100, 0)
                p01: Ellipse, radius_x: 10, radius_y: 10, at: (0, 100)
                p11: Ellipse, radius_x: 10, radius_y: 10, at: (100, 100)

                #1s
                stagger [40ms, from: top_left] {
                    fade_in(target: p00, duration: 200ms)
                    fade_in(target: p10, duration: 200ms)
                    fade_in(target: p01, duration: 200ms)
                    fade_in(target: p11, duration: 200ms)
                }
            "#,
            &[("p00", 1000), ("p10", 1040), ("p01", 1040), ("p11", 1080)],
        ),
    ];

    for (name, source, expected) in cases {
        let (stmts, errors) = parse_source(source);
        assert!(errors.is_empty(), "parse errors for {name}: {errors:?}");
        let timeline = Timeline::build(&stmts.expect("parsed AST"));

        for (target, expected_start) in expected {
            let start = timeline
                .action_events
                .iter()
                .find(|ev| ev.targets.contains(&target.to_string()))
                .map(|ev| ev.start_time_ms)
                .unwrap_or_else(|| panic!("No action event for {target} in {name}"));
            assert_eq!(start, *expected_start, "Start time for {target} in {name}");
        }
    }
}
