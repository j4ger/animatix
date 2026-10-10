use super::*;
use animatix_syntax::parser::parse_source;
use animatix_syntax::transition_registry;

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

#[test]
fn test_push_transitions_in_registry() {
    for name in ["push-left", "push-right", "push-up", "push-down"] {
        let def = transition_registry::find(name);
        assert!(def.is_some(), "Expected transition '{name}' to be recognized");
    }
}

#[test]
fn test_text_fill_gradient_build() {
    let source = r##"
#0s
heading: Text, text: "Delightful", fill_gradient: linear(135, {"#ff0000", "#0000ff"}), at: (100, 100)
"##;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    assert!(!report.output.tracks.is_empty());
    let track = report.output.tracks.get("heading").expect("heading track");
    assert!(track.style.fill_gradient.is_some(), "Text actor should accept fill_gradient");
}

#[test]
fn test_parallax_property_build() {
    let source = r#"
#0s
bg: Rect, size: (200, 200), parallax: 0.3, at: (0, 0)
fg: Rect, size: (100, 100), parallax: 1.0, at: (50, 50)
"#;
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    let bg_track = report.output.tracks.get("bg").expect("bg track");
    let fg_track = report.output.tracks.get("fg").expect("fg track");
    assert_eq!(bg_track.parallax, 0.3);
    assert_eq!(fg_track.parallax, 1.0);
}
