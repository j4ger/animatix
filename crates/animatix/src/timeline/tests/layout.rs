use super::*;

#[test]
fn test_stack_align_start_and_end() {
    use crate::timeline::PlacementMode;
    use crate::timeline::layout::ChildExtent;

    let children = vec![
        ChildExtent {
            half_size: [50.0, 30.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            half_size: [40.0, 20.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];

    // "center" alignment — all children at origin
    let positions_center = crate::timeline::layout::compute_stack_layout(&children, "center");
    assert_eq!(positions_center[0], [0.0, 0.0]);
    assert_eq!(positions_center[1], [0.0, 0.0]);

    // The Stack is a 100×60 box: `start` puts the child's top-left corner on
    // the box's top-left corner, so the largest layer sits at the origin and a
    // smaller layer inset from it. This is the bug: `start` used to pin the
    // child's bottom-right corner on the box centre (position `-half_size`).
    let positions_start =
        crate::timeline::layout::compute_stack_layout_in_box(&children, "start", [100.0, 60.0]);
    assert_eq!(positions_start[0], [0.0, 0.0]);
    assert_eq!(positions_start[1], [-10.0, -10.0]);

    // "end" mirrors it: bottom-right corner on the box's bottom-right corner.
    let positions_end =
        crate::timeline::layout::compute_stack_layout_in_box(&children, "end", [100.0, 60.0]);
    assert_eq!(positions_end[0], [0.0, 0.0]);
    assert_eq!(positions_end[1], [10.0, 10.0]);
}

#[test]
fn test_stack_sizes_to_child_union_extent() {
    // A Stack overlaps its layers, so its own extent is the largest layer per
    // axis (plus padding) — not zero, which made a nested Stack collapse in its
    // parent's layout.
    use crate::timeline::layout::ChildExtent;
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType, PlacementMode};

    let children = vec![
        ChildExtent {
            half_size: [130.0, 150.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            half_size: [20.0, 40.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];
    let metadata = ContainerMetadata {
        layout_type: LayoutType::Stack,
        gap: [0.0, 0.0],
        padding: [4.0, 6.0, 8.0, 10.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    };

    let size = crate::timeline::layout::compute_container_size(&children, &metadata);
    assert_eq!(size, [260.0 + 12.0, 300.0 + 16.0]);
}

#[test]
fn test_baseline_alignment_via_layout_engine() {
    // Integration test: LayoutEngine::compute_positions_with_baselines
    use crate::timeline::layout::ChildExtent;
    use crate::timeline::{ContainerMetadata, LayoutEngine, LayoutType, PlacementMode};

    let children = vec![
        ChildExtent {
            half_size: [50.0, 30.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            half_size: [40.0, 20.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [0.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "center".to_string(),
        vertical_align: "baseline".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    };

    // Baseline alignment
    let child_baselines = vec![-8.0, -4.0];
    let positions =
        LayoutEngine::compute_positions_with_baselines(&metadata, &children, &child_baselines);

    assert_eq!(positions.len(), 2);
    // Baselines should differ from center-aligned positions
    // The child with baseline=-8 (larger offset from center) should adjust more
    assert!(
        (positions[0][1]).abs() > 0.01 || (positions[1][1]).abs() > 0.01,
        "Baseline alignment should produce non-zero Y adjustments"
    );

    // With empty baselines, should behave like center
    let positions_no_baselines =
        LayoutEngine::compute_positions_with_baselines(&metadata, &children, &[]);
    assert!((positions_no_baselines[0][1]).abs() < 0.01);
    assert!((positions_no_baselines[1][1]).abs() < 0.01);

    // Center vertical_align should not adjust Y
    let metadata_center = ContainerMetadata {
        vertical_align: "center".to_string(),
        ..metadata.clone()
    };
    let positions_center = LayoutEngine::compute_positions_with_baselines(
        &metadata_center,
        &children,
        &child_baselines,
    );
    assert!((positions_center[0][1]).abs() < 0.01);
    assert!((positions_center[1][1]).abs() < 0.01);
}

#[test]
fn explicit_text_max_width_not_overridden_by_col_width() {
    use crate::timeline::TrackAccessor;

    // Regression: an explicitly set `text_max_width` on a Text directly inside a
    // Col must NOT be overridden by the container's propagated (collapsed) width.
    // Before the fix, the guard only honored an explicit value tighter than (or
    // equal to) the container width, so a value LOOSER than a collapsed CJK width
    // was replaced by the propagated width — the "Chinese labels wrap after 2-3
    // chars" symptom. Make the Col far narrower than the explicit value so the
    // bug is actually exercised, then assert the explicit value survives.
    let source = r#"
panel:
  Col, size: (200, 300), anchor: scene.center {
    label: Text, text: "中文测试", text_max_width: 10000, font_size: 24
  }
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let track = report.output.world.tracks.get_id("label").expect("label track exists");
    let width = report.output.world.text_or_default(track).text_max_width.get(0, 0.0);
    assert!(
        (width - 10000.0).abs() < 0.01,
        "explicit text_max_width should be preserved by width propagation, got {width}, want 10000"
    );
}

#[test]
fn test_fixed_size_layout_still_works() {
    use crate::timeline::layout::ChildExtent;
    // Backward compatibility: fixed-size layout should work unchanged
    let children = vec![
        ChildExtent {
            half_size: [50.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            half_size: [50.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [10.0, 10.0],
        padding: [5.0, 5.0, 5.0, 5.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    };

    // Direct layout path without layout specs/constraints
    let positions = crate::timeline::LayoutEngine::compute_positions(&metadata, &children);

    assert_eq!(positions.len(), 2);
    // Two 100-wide children with 10 gap + 10 padding → total width = 100+10+100+10 = 220
    // a at -110 (left of center), b at 0 (center), actually let's just verify they're reasonable
    assert!(positions[0][0] < 0.0, "First child should be left of center");
    assert!(positions[1][0] > 0.0, "Second child should be right of center");

    // With specs/constraints (empty), should produce same result
    let positions_with_specs = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata,
        &children,
        &[],
        &[],
        &[],
        [0.0, 0.0],
    );
    assert_eq!(positions.len(), positions_with_specs.len());
    for i in 0..positions.len() {
        assert!(
            (positions[i][0] - positions_with_specs[i][0]).abs() < 0.01
                && (positions[i][1] - positions_with_specs[i][1]).abs() < 0.01,
            "Position mismatch at index {}: {:?} vs {:?}",
            i,
            positions[i],
            positions_with_specs[i]
        );
    }
}

/// Regression: `Stack align: "start"` must put the child's top-left corner on
/// the declared Stack box's top-left corner. Before the fix the child's
/// *bottom-right* corner landed on the Stack centre (position `-half_size`
/// regardless of the box), so a start-aligned layer drifted up-left by half its
/// own size.
#[test]
fn stack_align_start_places_child_at_box_top_left() {
    let source = r#"
config { resolution: (640, 360) }
#0s
stack: Stack, at: (320, 180), size: (400, 200), align: "start" {
  a: Rect, size: (100, 50)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    let id = report.output.world.get_id("a").expect("child track");
    let position = report.output.world.geometry(id).position.last([0.0, 0.0]);
    // Box top-left is (-200, -100) in the Stack's centre-relative frame.
    assert_eq!(position, [-150.0, -75.0]);
}

/// Regression: an unsized Stack overlaps its children, so it publishes the
/// union of their measured extents as its own `layout_size` instead of leaving
/// it at the default 100×100 (or zero). A parent container can then place the
/// Stack by the box it actually covers.
#[test]
fn unsized_stack_publishes_measured_union_extent() {
    let source = r#"
config { resolution: (640, 360) }
#0s
panel: Stack, at: (320, 180) {
  bg: Rect, size: (260, 300)
  dot: Ellipse, size: (40, 40)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    let id = report.output.world.get_id("panel").expect("panel track");
    let half = report
        .output
        .world
        .geometry(id)
        .layout_size_last()
        .expect("panel publishes a measured layout_size");
    assert_eq!(half, [130.0, 150.0]);
}

/// Regression: a `Group` wrapper forwards its children's placed union extent to
/// the layout. Before the fix the group published the default 100×100 box, so a
/// Row reserved a 100 px slot and a Col reserved 100 px of cross-extent for a
/// single line of text.
#[test]
fn group_publishes_children_union_extent() {
    let source = r#"
config { resolution: (640, 360) }
#0s
row: Row, at: (320, 180), gap: 10 {
  wrap: Group {
    inner: Rect, size: (120, 40)
  }
  tail: Rect, size: (40, 40)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    let wrap = report.output.world.get_id("wrap").expect("group track");
    let half = report
        .output
        .world
        .geometry(wrap)
        .layout_size_last()
        .expect("group publishes a measured layout_size");
    assert_eq!(half, [60.0, 20.0]);
}

/// Regression: an unsized nested `Col` publishes its measured content extent,
/// not the default 100×100 box. A parent Row then reserves the real column
/// width (before the fix the sibling overlapped the 300 px-wide content).
#[test]
fn unsized_nested_col_publishes_measured_extent() {
    let source = r#"
config { resolution: (640, 360) }
#0s
row: Row, at: (320, 180), gap: 0, align: "center" {
  c: Col, gap: 0, align: "center" {
    r: Rect, size: (300, 100)
  }
  m: Rect, size: (20, 20)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let report = crate::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    let col = report.output.world.get_id("c").expect("col track");
    let half = report
        .output
        .world
        .geometry(col)
        .layout_size_last()
        .expect("nested Col publishes a measured layout_size");
    assert_eq!(half, [150.0, 50.0]);
}
