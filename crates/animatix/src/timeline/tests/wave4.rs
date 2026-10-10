use crate::timeline::property_track::TrackAccessor;
use crate::timeline::tests::without_content_lints;
use crate::timeline::{SceneDimensions, Timeline};
use animatix_syntax::easing::{Easing, apply_easing, easing_derivative};

#[test]
fn test_spring_v0_derivative_and_continuity() {
    // 1. Initial velocity formula verification
    let v0 = 8.5;
    let spring = Easing::SpringV0 {
        damping: 12.0,
        frequency: 18.0,
        v0,
    };

    assert_eq!(apply_easing(0.0, spring), 0.0);
    let initial_derivative = easing_derivative(0.0, spring);
    assert!(
        (initial_derivative - v0).abs() < 1e-4,
        "Expected initial derivative {v0}, got {initial_derivative}"
    );

    // 2. Motion interruption with velocity continuity
    let source = r#"
#0s
b: Rect, at: (0, 0), size: (20, 20), color: (1, 1, 1, 1)

#1s
move b [to: (200, 100), 1s, ease: spring(15, 20)]

#1.5s
move b [to: (400, 200), 1s, ease: spring(15, 20)]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    let non_lints: Vec<_> = without_content_lints(&report.diagnostics).collect();
    assert!(non_lints.is_empty(), "unexpected diagnostics: {non_lints:?}");

    let timeline = report.output;
    let track = timeline.tracks.get("b").expect("actor track b");

    // Sample positions before and after the 1.5s interruption
    let pos_1490 = track.geometry.position.get(1490, [0.0, 0.0]);
    let pos_1500 = track.geometry.position.get(1500, [0.0, 0.0]);
    let pos_1510 = track.geometry.position.get(1510, [0.0, 0.0]);

    // Positional continuity: no teleportation at 1500ms
    let step_before = [pos_1500[0] - pos_1490[0], pos_1500[1] - pos_1490[1]];
    let step_after = [pos_1510[0] - pos_1500[0], pos_1510[1] - pos_1500[1]];

    // The velocity vector must not abruptly flip sign or jump discontinuously
    assert!(
        (step_before[0] - step_after[0]).abs() < 5.0,
        "X velocity discontinuity: {step_before:?} vs {step_after:?}"
    );
    assert!(
        (step_before[1] - step_after[1]).abs() < 5.0,
        "Y velocity discontinuity: {step_before:?} vs {step_after:?}"
    );
}

#[test]
fn test_camera_focus_target_and_padding() {
    let source = r#"
#0s
card: Rect, at: (300, 200), size: (100, 80), color: (1, 1, 1, 1)

#1s
camera.focus = card [padding: 40]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    let non_lints: Vec<_> = without_content_lints(&report.diagnostics).collect();
    assert!(non_lints.is_empty(), "unexpected diagnostics: {non_lints:?}");

    let timeline = report.output;

    // Verify camera keyframes generated at 1000ms
    assert!(
        timeline.camera.pan().unwrap().keyframes.contains_key(&1000),
        "Camera pan keyframe expected at 1000ms"
    );
    assert!(
        timeline.camera.zoom().unwrap().keyframes.contains_key(&1000),
        "Camera zoom keyframe expected at 1000ms"
    );

    let (pan_target, _) = timeline.camera.pan().unwrap().keyframes[&1000];
    let (zoom_target, _) = timeline.camera.zoom().unwrap().keyframes[&1000];

    // Card is at (300, 200) with size (100, 80). Default resolution is (1920, 1080).
    // Viewport center is (960, 540). To center card at (300, 200), pan should be
    // -target_zoom * (pos - center) = 10.0 * (660, 340) = (6600, 3400).
    assert_eq!(pan_target, [6600.0, 3400.0]);
    assert_eq!(zoom_target, 10.0);
}

#[test]
fn test_step_pause_points_and_navigation() {
    let source = r#"
#0s
b: Rect, at: (0, 0), size: (10, 10), color: (1, 1, 1, 1)
#step 1

#500ms
#pause

#1s
#step 2
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    let non_lints: Vec<_> = without_content_lints(&report.diagnostics).collect();
    assert!(non_lints.is_empty(), "unexpected diagnostics: {non_lints:?}");

    let timeline = report.output;
    assert_eq!(timeline.pause_points(), &[0, 500, 1000]);

    // Navigation checks
    assert_eq!(timeline.step_next(0), Some(500));
    assert_eq!(timeline.step_next(250), Some(500));
    assert_eq!(timeline.step_next(500), Some(1000));
    assert_eq!(timeline.step_next(1000), None);

    assert_eq!(timeline.step_prev(1000), Some(500));
    assert_eq!(timeline.step_prev(750), Some(500));
    assert_eq!(timeline.step_prev(500), Some(0));
    assert_eq!(timeline.step_prev(0), None);
}

#[test]
fn test_flip_transition_shared_id() {
    let source_a = r#"
#0s
card: Rect, at: (100, 100), size: (50, 50), color: (1, 0, 0, 1), shared_id: "hero"
"#;
    let source_b = r#"
#0s
card_large: Rect, at: (400, 300), size: (150, 150), color: (1, 0, 0, 1), shared_id: "hero"
"#;

    let (ast_a, _) = animatix_syntax::parser::parse_source(source_a);
    let (ast_b, _) = animatix_syntax::parser::parse_source(source_b);

    let report_a = Timeline::build_with_diagnostics(&ast_a.unwrap(), &Default::default());
    let report_b = Timeline::build_with_diagnostics(&ast_b.unwrap(), &Default::default());

    let timeline_a = report_a.output;
    let timeline_b = report_b.output;

    let shared_a = timeline_a.shared_id_actors(0);
    let shared_b = timeline_b.shared_id_actors(0);

    assert_eq!(shared_a.get("hero"), Some(&"card".to_string()));
    assert_eq!(shared_b.get("hero"), Some(&"card_large".to_string()));

    let dims = SceneDimensions {
        width: 800,
        height: 600,
    };
    let debug_options = Default::default();

    // Verify unsuppressed evaluation has the item
    let unsuppressed = timeline_a.evaluate_program_with_debug(0.0, dims, debug_options, &mut None);
    assert_eq!(unsuppressed.items.len(), 1);
    assert_eq!(unsuppressed.items[0].label, "card");

    // Verify suppressed evaluation suppresses the actor
    let suppressed = timeline_a.evaluate_program_with_suppressed(
        0.0,
        dims,
        debug_options,
        &mut None,
        &["card".to_string()],
    );
    // Suppressed actor has opacity 0 so its draw commands evaluate to opacity 0 and are omitted from scene
    assert_eq!(suppressed.items.len(), 1);
    assert_eq!(suppressed.items[0].opacity, 0.0);
}
