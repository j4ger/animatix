//! Tests for wavefront stagger actions and timing derivation.

use animatix_syntax::parser::parse_source;

use super::*;

#[test]
fn test_wavefront_stagger_from_center() {
    let source = r#"
#0s
c: Ellipse, radius_x: 10, radius_y: 10, at: (100, 100)
a1: Ellipse, radius_x: 10, radius_y: 10, at: (0, 100)
a2: Ellipse, radius_x: 10, radius_y: 10, at: (200, 100)
a3: Ellipse, radius_x: 10, radius_y: 10, at: (100, 0)
a4: Ellipse, radius_x: 10, radius_y: 10, at: (100, 200)

#1s
stagger [50ms, from: center] {
    fade-in c [200ms]
    fade-in a1 [200ms]
    fade-in a2 [200ms]
    fade-in a3 [200ms]
    fade-in a4 [200ms]
}
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    // Find action events for each target
    let get_start = |target: &str| -> u64 {
        timeline
            .action_events
            .iter()
            .find(|ev| ev.targets.contains(&target.to_string()))
            .map(|ev| ev.start_time_ms)
            .unwrap_or_else(|| panic!("No action event for {target}"))
    };

    assert_eq!(get_start("c"), 1000, "center ellipse should start at t=1000ms (ring 0)");
    assert_eq!(get_start("a1"), 1050, "a1 is equidistant (ring 1, t=1050ms)");
    assert_eq!(get_start("a2"), 1050, "a2 is equidistant (ring 1, t=1050ms)");
    assert_eq!(get_start("a3"), 1050, "a3 is equidistant (ring 1, t=1050ms)");
    assert_eq!(get_start("a4"), 1050, "a4 is equidistant (ring 1, t=1050ms)");
}

#[test]
fn test_wavefront_stagger_from_top_left() {
    let source = r#"
#0s
p00: Ellipse, radius_x: 10, radius_y: 10, at: (0, 0)
p10: Ellipse, radius_x: 10, radius_y: 10, at: (100, 0)
p01: Ellipse, radius_x: 10, radius_y: 10, at: (0, 100)
p11: Ellipse, radius_x: 10, radius_y: 10, at: (100, 100)

#1s
stagger [40ms, from: top-left] {
    fade-in p00 [200ms]
    fade-in p10 [200ms]
    fade-in p01 [200ms]
    fade-in p11 [200ms]
}
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    let get_start = |target: &str| -> u64 {
        timeline
            .action_events
            .iter()
            .find(|ev| ev.targets.contains(&target.to_string()))
            .map(|ev| ev.start_time_ms)
            .unwrap_or_else(|| panic!("No action event for {target}"))
    };

    assert_eq!(get_start("p00"), 1000, "top-left should start at ring 0 (t=1000ms)");
    assert_eq!(get_start("p10"), 1040, "p10 at ring 1 (t=1040ms)");
    assert_eq!(get_start("p01"), 1040, "p01 at ring 1 (t=1040ms)");
    assert_eq!(get_start("p11"), 1080, "p11 at ring 2 (t=1080ms)");
}
