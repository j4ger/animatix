use super::support::build_output;
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
    let timeline = build_output(source);
    let track = timeline.world.tracks.get_id("label").expect("label track exists");
    let width = timeline.world.text_or_default(track).text_max_width.get(0, 0.0);
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
    let timeline = build_output(source);
    let id = timeline.world.get_id("a").expect("child track");
    let position = timeline.world.geometry(id).position.last([0.0, 0.0]);
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
    let timeline = build_output(source);
    let id = timeline.world.get_id("panel").expect("panel track");
    let half = timeline
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
    let timeline = build_output(source);
    let wrap = timeline.world.get_id("wrap").expect("group track");
    let half = timeline
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
    let timeline = build_output(source);
    let col = timeline.world.get_id("c").expect("col track");
    let half = timeline
        .world
        .geometry(col)
        .layout_size_last()
        .expect("nested Col publishes a measured layout_size");
    assert_eq!(half, [150.0, 50.0]);
}

/// A dynamic Row must size each slot from the sampled size track, regardless
/// of whether the declaration used literals or a build-time expression.
#[test]
fn dynamic_row_reflows_literal_size_animation_with_measured_bounds() {
    use crate::timeline::SceneDimensions;

    let source = |width: &str| {
        format!(
            r#"
config {{ resolution: (640, 360), dynamic_layout: true }}
let initial_width = 90
#0s
row: Row, size: (500, 180), at: (320, 180), gap: 12 {{
  a: Rect, size: ({width}, 120)
  b: Rect, size: (90, 120)
}}
a.size = (180, 60) [2s]
"#
        )
    };
    let dimensions = SceneDimensions {
        width: 640,
        height: 360,
    };

    let bounds = |timeline: &Timeline, label: &str, time_ms: u64| {
        let affine = timeline
            .actor_world_affine(label, time_ms, dimensions)
            .expect("actor transform");
        let coeffs = affine.as_coeffs();
        let id = timeline.world.get_id(label).expect("actor entity");
        let half = timeline.world.geometry(id).size.get(time_ms, [0.0, 0.0]);
        let center = [coeffs[4] as f32, coeffs[5] as f32];
        [
            [center[0] - half[0], center[0] + half[0]],
            [center[1] - half[1], center[1] + half[1]],
        ]
    };

    let horizontal_gap = |a: [[f32; 2]; 2], b: [[f32; 2]; 2]| b[0][0] - a[0][1];
    for initial_width in ["90", "initial_width"] {
        let timeline = build_output(&source(initial_width));
        let a0 = bounds(&timeline, "a", 0);
        let b0 = bounds(&timeline, "b", 0);
        let a2 = bounds(&timeline, "a", 2000);
        let b2 = bounds(&timeline, "b", 2000);

        assert!(
            (horizontal_gap(a0, b0) - 12.0).abs() < 0.1,
            "initial gap for {initial_width}: {}",
            horizontal_gap(a0, b0)
        );
        assert!(
            (horizontal_gap(a2, b2) - 12.0).abs() < 0.1,
            "animated gap for {initial_width}: {}",
            horizontal_gap(a2, b2)
        );
        assert!((a2[1][1] - a2[1][0] - 60.0).abs() < 0.1);
        assert!((b2[1][1] - b2[1][0] - 120.0).abs() < 0.1);
        assert!((a2[1][0] + a2[1][1] - b2[1][0] - b2[1][1]).abs() < 0.1);
    }
}

#[test]
fn mixed_container_size_seeds_fixed_literal_and_expression_axes() {
    let source = r#"
config { resolution: (640, 360), dynamic_layout: true }
let W = 180
#0s
outer: Col, size: (420, 320), at: (320, 180), gap: 10 {
  fixed: Col, size: (240, auto), gap: 0 {
    fixed_child: Rect, size: (80, 40)
  }
  expression: Col, size: (W, auto), gap: 0 {
    expression_child: Rect, size: (70, 30)
  }
}
"#;
    let timeline = build_output(source);

    for (label, expected_width) in [("fixed", 240.0), ("expression", 180.0)] {
        let id = timeline.world.get_id(label).expect("column entity");
        let half_size = timeline
            .world
            .geometry(id)
            .layout_size_last()
            .expect("column measured layout size");
        assert!(
            (half_size[0] * 2.0 - expected_width).abs() < 0.1,
            "{label} column width should remain {expected_width}, got {}",
            half_size[0] * 2.0
        );
        let expected_height = if label == "fixed" { 40.0 } else { 30.0 };
        assert!(
            (half_size[1] * 2.0 - expected_height).abs() < 0.1,
            "{label} column auto height should measure {expected_height}, got {}",
            half_size[1] * 2.0
        );
    }
}

#[test]
fn dynamic_col_grid_reflow_uses_sampled_sizes_without_scaling_slots() {
    use crate::timeline::SceneDimensions;

    let dimensions = SceneDimensions {
        width: 640,
        height: 360,
    };
    let center = |timeline: &Timeline, label: &str, time_ms| {
        let affine = timeline
            .actor_world_affine(label, time_ms, dimensions)
            .expect("actor transform");
        let coeffs = affine.as_coeffs();
        [coeffs[4] as f32, coeffs[5] as f32]
    };

    for (container, positioning_axis) in [("Col", 1), ("Grid", 0)] {
        let container_props = if container == "Grid" { "cols: 2, " } else { "" };
        let source = format!(
            r#"
config {{ resolution: (640, 360), dynamic_layout: true }}
#0s
layout: {container}, {container_props}size: (400, 280), at: (320, 180), gap: 12 {{
  a: Rect, size: (90, 90), scale: 2
  b: Rect, size: (90, 90)
}}
a.size = (180, 60) [2s]
"#
        );
        let timeline = build_output(&source);
        let a0 = center(&timeline, "a", 0);
        let b0 = center(&timeline, "b", 0);
        let a2 = center(&timeline, "a", 2000);
        let b2 = center(&timeline, "b", 2000);
        let separation =
            |a: [f32; 2], b: [f32; 2]| (b[positioning_axis] - a[positioning_axis]).abs();

        let initial_expected = 45.0 + 12.0 + 45.0;
        let animated_expected = if container == "Col" {
            30.0 + 12.0 + 45.0
        } else {
            90.0 + 12.0 + 45.0
        };
        assert!(
            (separation(a0, b0) - initial_expected).abs() < 0.1,
            "{container} initial slot distance: {}",
            separation(a0, b0)
        );
        assert!(
            (separation(a2, b2) - animated_expected).abs() < 0.1,
            "{container} animated slot distance: {}",
            separation(a2, b2)
        );

        let a_id = timeline.world.get_id("a").expect("scaled child");
        let a_affine = timeline
            .actor_world_affine("a", 2000, dimensions)
            .expect("scaled actor transform");
        assert!(
            (a_affine.as_coeffs()[0].abs() - 2.0).abs() < 0.01,
            "the scale should remain a visual transform"
        );
        assert_eq!(
            timeline
                .world
                .geometry(a_id)
                .layout_size_get(2000)
                .expect("sampled layout size"),
            [90.0, 30.0],
            "scale must not change the layout size track"
        );
    }
}
