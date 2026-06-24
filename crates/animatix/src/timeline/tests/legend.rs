//! Tests for the Legend primitive

use super::*;
use crate::ast::Property;
use crate::primitives::{EvaluateCtx, LEGEND, Primitive, RenderCommand};

fn make_config() -> Stmt {
    Stmt::Config {
        settings: vec![Property {
            name: "colorscheme".to_string(),
            value: Expr::Str("editorial-dark".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
        span: None,
    }
}

/// Build a minimal timeline with a single Legend actor.
fn build_legend_timeline() -> Timeline {
    let ast = vec![
        make_config(),
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "legend".to_string(),
                array_index: None,
                ty: "Legend".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    report.output
}

#[test]
fn test_legend_basic_rendering() {
    // Test that a Legend renders color swatches and labels
    let timeline = build_legend_timeline();

    let track = timeline.get_track("legend").expect("legend track should exist");
    assert_eq!(track.kind, ActorKindId::Legend, "track kind should be Legend");

    // Verify the track has the placeholder entries
    assert!(!track.legend.entries.is_empty(), "legend should have entries");

    // Evaluate at time 0 — should not panic and should produce RenderCommands
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_legend_entries() {
    // Verify that legend entries are stored correctly
    let timeline = build_legend_timeline();
    let track = timeline.get_track("legend").expect("legend track should exist");

    // Check that the placeholder entries are present
    assert_eq!(track.legend.entries.len(), 3, "should have 3 placeholder entries");
    assert_eq!(track.legend.entries[0].0, "Series A", "first entry label should be 'Series A'");
    assert_eq!(track.legend.entries[1].0, "Series B", "second entry label should be 'Series B'");
    assert_eq!(track.legend.entries[2].0, "Series C", "third entry label should be 'Series C'");
}

#[test]
fn test_legend_empty() {
    // Verify that a Legend with no entries renders nothing.
    // Build a track manually and call evaluate directly to bypass
    // the build-time placeholder insertion.
    use kurbo::Affine;

    let mut track = AnimationTrack::new("empty_legend".to_string());
    track.kind = ActorKindId::Legend;

    // Ensure entries are empty (default)
    assert!(track.legend.entries.is_empty(), "fresh track should have empty legend entries");

    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        overrides: None,
        vector_paths: &[],
        target_resolver: None,
    };

    let result = LEGEND.evaluate(&ctx, None).expect("evaluate should succeed");
    assert!(result.is_none(), "empty legend should return None from evaluate");
}

#[test]
fn test_legend_multiple_colors() {
    // Verify that multiple colors are rendered
    let timeline = build_legend_timeline();
    let track = timeline.get_track("legend").expect("legend track should exist");

    let entries = &track.legend.entries;
    assert_eq!(entries.len(), 3, "should have 3 entries");

    // Each entry should have a color (the placeholder colors)
    // Series A: red
    assert_eq!(
        entries[0].1,
        [1.0, 0.0, 0.0, 1.0],
        "Series A should be red"
    );
    // Series B: green
    assert_eq!(
        entries[1].1,
        [0.0, 1.0, 0.0, 1.0],
        "Series B should be green"
    );
    // Series C: blue
    assert_eq!(
        entries[2].1,
        [0.0, 0.0, 1.0, 1.0],
        "Series C should be blue"
    );

    // Evaluate at time 0 — verify it doesn't panic with multiple colors
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_legend_render_commands_produced() {
    // Create a track with entries and verify evaluate returns Some(commands)
    use kurbo::Affine;

    let mut track = AnimationTrack::new("legend".to_string());
    track.kind = ActorKindId::Legend;

    // Set placeholder entries (matching what build() would set)
    track.legend.entries = vec![
        ("Series A".to_string(), [1.0, 0.0, 0.0, 1.0]),
        ("Series B".to_string(), [0.0, 1.0, 0.0, 1.0]),
        ("Series C".to_string(), [0.0, 0.0, 1.0, 1.0]),
    ];

    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        overrides: None,
        vector_paths: &[],
        target_resolver: None,
    };

    let result = LEGEND.evaluate(&ctx, None).expect("evaluate should succeed");
    assert!(result.is_some(), "legend with entries should return Some(commands)");

    let commands = result.unwrap();
    // Each entry produces 2 Path commands (swatch rect + label bg rect)
    // So 3 entries = 6 Path commands
    assert_eq!(commands.len(), 6, "3 entries should produce 6 RenderCommands (2 per entry)");

    // Verify each command is a Paths variant
    for (i, cmd) in commands.iter().enumerate() {
        match cmd {
            RenderCommand::Paths { paths } => {
                assert_eq!(paths.len(), 1, "each command should contain 1 path");
            }
            _ => panic!("Command {} should be Paths variant", i),
        }
    }
}
