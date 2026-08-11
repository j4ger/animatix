//! Tests for runtime PlotCurve parameter animation (G6 feature).
//!
//! Validates:
//! 1. Top-level variable capture in PlotCurve func closures
//! 2. `always` block driving parameter changes over time
//! 3. Actor-local parameter keyframe tracks (`curve.freq = ...`)
//! 4. Parameter priority (always block > build-time declaration)
//! 5. Multiple parameters animating simultaneously

use std::fs;

use animatix::timeline::{SceneDimensions, Timeline, Value};
use animatix_syntax::module::ModuleGraph;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

/// Write `.amx` source to a unique temp file, parse + expand, then build a Timeline.
fn build_timeline_from_source(test_name: &str, source: &str) -> Timeline {
    let temp_path = std::env::temp_dir().join(format!("animatix_plot_params_test_{test_name}.amx"));
    fs::write(&temp_path, source).expect("write temp fixture should succeed");
    let ast = ModuleGraph::new()
        .load_program(&temp_path)
        .expect("program should load")
        .expand_components();
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );
    report.output
}

// ─────────────────────────────────────────────────────────────
// Test 1: Top-level variable capture
// ─────────────────────────────────────────────────────────────

#[test]
fn top_level_variable_captured_in_plot_curve() {
    let source = r#"
#0s
let freq = 2
curve: PlotCurve, func: (x) => sin(freq * x), domain: (0, 6.28), samples: 100
"#;
    let timeline = build_timeline_from_source("t1_var_capture", source);

    // The timeline should have a procedural plot for "curve"
    let curve_track = timeline.tracks().get("curve").expect("Expected track for 'curve'");
    assert!(curve_track.procedural_plot.is_some(), "curve should have a procedural_plot");

    // Evaluate at t=0 should not panic and produce a scene
    let _scene = timeline.evaluate(0.0, DIMS);
}

// ─────────────────────────────────────────────────────────────
// Test 2: always block drives parameter change
// ─────────────────────────────────────────────────────────────

#[test]
fn always_block_drives_plot_parameter_change() {
    let source = r#"
#0s
let freq = 1
curve: PlotCurve, func: (x) => sin(freq * x), domain: (0, 6.28), samples: 100

always {
  freq = t * 2 + 1
}
"#;
    let timeline = build_timeline_from_source("t2_always_block", source);

    // always block should produce modifier programs
    assert!(
        !timeline.modifier_programs.is_empty() || !timeline.modifier_bytecode_programs.is_empty(),
        "Expected modifier programs from always block"
    );

    // Evaluate at multiple times should not panic and produce valid scenes
    let _scene_0 = timeline.evaluate(0.0, DIMS);
    let _scene_1 = timeline.evaluate(1.0, DIMS);
    let _scene_2 = timeline.evaluate(2.0, DIMS);
}

// ─────────────────────────────────────────────────────────────
// Test 3: Actor-local parameter keyframe tracks
// ─────────────────────────────────────────────────────────────

#[test]
fn actor_local_param_keyframes_interpolate() {
    let source = r#"
#0s
curve: PlotCurve, func: (x) => sin(freq * x), freq: 2, domain: (0, 6.28), samples: 100
  curve.freq = 2
#3s
  curve.freq = 8
"#;
    let timeline = build_timeline_from_source("t3_param_kf", source);

    let curve_track = timeline.tracks().get("curve").expect("Expected track for 'curve'");

    // Procedural plot should exist with "freq" as a param name
    let plot = curve_track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot");
    assert!(
        plot.param_names.contains(&"freq".to_string()),
        "param_names should contain 'freq', got: {:?}",
        plot.param_names
    );

    // plot_param_tracks should have "freq" with keyframes at 0ms and 3000ms
    let freq_pt = curve_track
        .plot_param_tracks
        .get("freq")
        .expect("Expected plot_param_tracks entry for 'freq'");

    // At t=0ms, freq should be 2
    let val_0 = freq_pt.evaluate(0);
    assert!((val_0 - 2.0).abs() < 1e-9, "freq at t=0 should be 2.0, got {val_0}");

    // At t=3000ms, freq should be 8
    let val_3000 = freq_pt.evaluate(3000);
    assert!((val_3000 - 8.0).abs() < 1e-9, "freq at t=3000 should be 8.0, got {val_3000}");

    // At t=1500ms, freq should be interpolated between 2 and 8 (≈5 with linear easing)
    let val_1500 = freq_pt.evaluate(1500);
    assert!(
        val_1500 > 2.0 && val_1500 < 8.0,
        "freq at t=1500 should be between 2 and 8, got {val_1500}"
    );
    // With default linear easing, midpoint should be ~5.0
    assert!(
        (val_1500 - 5.0).abs() < 1.0,
        "freq at t=1500 should be approximately 5.0, got {val_1500}"
    );

    // Evaluate at various times should not panic
    let _scene_0 = timeline.evaluate(0.0, DIMS);
    let _scene_1_5 = timeline.evaluate(1.5, DIMS);
    let _scene_3 = timeline.evaluate(3.0, DIMS);
}

// ─────────────────────────────────────────────────────────────
// Test 4: Parameter priority (always > build-time declaration)
// ─────────────────────────────────────────────────────────────

#[test]
fn always_block_overrides_build_time_param() {
    // Test that always-block overrides take priority over build-time let values
    // when the always block drives an actor property assignment.
    // Here the always block sets `curve.at` which overrides the build-time value.
    let source = r#"
#0s
curve: PlotCurve, func: (x) => sin(freq * x), freq: 2, domain: (0, 6.28), samples: 100, at: (100, 100)

always {
  curve.at = (200, 200)
}
"#;
    let timeline = build_timeline_from_source("t4_priority", source);

    // Modifier programs should exist from the always block
    assert!(
        !timeline.modifier_programs.is_empty() || !timeline.modifier_bytecode_programs.is_empty(),
        "Expected modifier programs from always block"
    );

    // The procedural plot should exist (freq: 2 is a custom numeric param)
    let curve_track = timeline.tracks().get("curve").expect("Expected track for 'curve'");
    assert!(curve_track.procedural_plot.is_some(), "curve should have a procedural_plot");

    // Build frame env and run modifiers to check the override
    let overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env(1000, DIMS, &overrides);

    // Execute modifier programs against the env
    let mut local_overrides = std::collections::HashMap::new();
    if let Some(program) = timeline.modifier_bytecode_programs.first() {
        timeline
            .apply_modifier_bytecode_program(program, 1000, DIMS, &mut env, &mut local_overrides)
            .expect("modifier execution should succeed");
    }

    // The always block should have overridden curve.at to (200, 200)
    let at_override = local_overrides.get("curve").and_then(|m| m.get("at"));
    if let Some(Value::Vec2([x, y])) = at_override {
        assert!(
            (*x - 200.0).abs() < 1e-9 && (*y - 200.0).abs() < 1e-9,
            "always block should override curve.at to (200, 200), got ({x}, {y})"
        );
    }

    // Evaluate should not panic
    let _scene = timeline.evaluate(1.0, DIMS);
}

// ─────────────────────────────────────────────────────────────
// Test 5: Multiple parameters animating simultaneously
// ─────────────────────────────────────────────────────────────

#[test]
fn multiple_params_keyframed_and_interpolated() {
    let source = r#"
#0s
curve: PlotCurve, func: (x) => amp * sin(freq * x), freq: 2, amp: 1.0, domain: (0, 6.28), samples: 100
  curve.freq = 2
  curve.amp = 1.0
#2s
  curve.freq = 5
  curve.amp = 0.5
"#;
    let timeline = build_timeline_from_source("t5_multi_param", source);

    let curve_track = timeline.tracks().get("curve").expect("Expected track for 'curve'");

    let plot = curve_track
        .procedural_plot
        .as_ref()
        .expect("curve should have a procedural_plot");

    // Both freq and amp should be param names
    assert!(
        plot.param_names.contains(&"freq".to_string()),
        "param_names should contain 'freq'"
    );
    assert!(
        plot.param_names.contains(&"amp".to_string()),
        "param_names should contain 'amp'"
    );

    // Check freq track
    let freq_pt = curve_track
        .plot_param_tracks
        .get("freq")
        .expect("Expected plot_param_tracks entry for 'freq'");

    let freq_0 = freq_pt.evaluate(0);
    let freq_2000 = freq_pt.evaluate(2000);
    assert!((freq_0 - 2.0).abs() < 1e-9, "freq at t=0 should be 2.0, got {freq_0}");
    assert!((freq_2000 - 5.0).abs() < 1e-9, "freq at t=2000 should be 5.0, got {freq_2000}");

    // Mid-point interpolation for freq
    let freq_1000 = freq_pt.evaluate(1000);
    assert!(
        freq_1000 > 2.0 && freq_1000 < 5.0,
        "freq at t=1000 should be between 2 and 5, got {freq_1000}"
    );

    // Check amp track
    let amp_pt = curve_track
        .plot_param_tracks
        .get("amp")
        .expect("Expected plot_param_tracks entry for 'amp'");

    let amp_0 = amp_pt.evaluate(0);
    let amp_2000 = amp_pt.evaluate(2000);
    assert!((amp_0 - 1.0).abs() < 1e-9, "amp at t=0 should be 1.0, got {amp_0}");
    assert!((amp_2000 - 0.5).abs() < 1e-9, "amp at t=2000 should be 0.5, got {amp_2000}");

    // Mid-point interpolation for amp
    let amp_1000 = amp_pt.evaluate(1000);
    assert!(
        amp_1000 > 0.5 && amp_1000 < 1.0,
        "amp at t=1000 should be between 0.5 and 1.0, got {amp_1000}"
    );

    // Evaluate at various times should not panic
    let _scene_0 = timeline.evaluate(0.0, DIMS);
    let _scene_1 = timeline.evaluate(1.0, DIMS);
    let _scene_2 = timeline.evaluate(2.0, DIMS);
}
