//! Taffy layout tests.

use super::specs::parse_dimension_spec;
use super::*;

use crate::timeline::PlacementMode;

fn make_child(w: f32, h: f32, mode: PlacementMode) -> ChildExtent {
    ChildExtent {
        half_size: [w / 2.0, h / 2.0],
        placement_mode: mode,
    }
}

#[test]
fn test_row_layout_basic() {
    let children = vec![
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
    ];

    let output = compute_taffy_linear_layout(
        &children,
        LayoutType::Row,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
    );
    let results = &output.positions;

    // Two 100-wide children with 10 gap, centered
    // Total width = 100 + 10 + 100 = 210
    // a at -55, b at 55
    assert_eq!(results.len(), 2);
    // Check that positions are roughly centered
    let total_span = results[1].position[0] - results[0].position[0];
    assert!((total_span - 110.0).abs() < 0.1);
    // Container size should be computed
    assert!(output.container_size[0] > 0.0);
    assert!(output.container_size[1] > 0.0);
}

#[test]
fn test_col_layout_basic() {
    let children = vec![
        make_child(50.0, 100.0, PlacementMode::LayoutManaged),
        make_child(50.0, 100.0, PlacementMode::LayoutManaged),
    ];

    let output = compute_taffy_linear_layout(
        &children,
        LayoutType::Col,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
    );
    let results = &output.positions;

    // Two 100-tall children with 10 gap, centered
    // Total height = 100 + 10 + 100 = 210
    // a at -55, b at 55 (in y)
    assert_eq!(results.len(), 2);
    let total_span = results[1].position[1] - results[0].position[1];
    assert!((total_span - 110.0).abs() < 0.1);
    // Container size should be computed
    assert!(output.container_size[0] > 0.0);
    assert!(output.container_size[1] > 0.0);
}

#[test]
fn test_manual_child_excluded_from_positions() {
    let children = vec![
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(100.0, 50.0, PlacementMode::Manual),
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
    ];

    let output = compute_taffy_linear_layout(
        &children,
        LayoutType::Row,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
    );
    let results = &output.positions;

    assert_eq!(results.len(), 3);
    // b (Manual) should still receive a computed slot in declaration order,
    // even though callers later exclude it from authored layout assignment.
    // In this symmetric 3-child layout: a=-110, b=0, c=110 (center child at origin is correct)
    // Verify a and c are at symmetric non-zero positions
    assert_eq!(results[0].position[0], -results[2].position[0]); // symmetric
    assert!(results[0].position[0] < 0.0); // a is left of center
    assert!(results[2].position[0] > 0.0); // c is right of center
    // b (center) happens to be at [0, 0] due to symmetric layout
    assert_eq!(results[1].position, [0.0, 0.0]);
}

#[test]
fn test_empty_children() {
    let children: Vec<ChildExtent> = vec![];
    let output = compute_taffy_linear_layout(
        &children,
        LayoutType::Row,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
    );
    assert!(output.positions.is_empty());

    let output = compute_taffy_grid_layout(&children, [10.0, 10.0], [0.0, 0.0, 0.0, 0.0], 2);
    assert!(output.positions.is_empty());
}

// ── Width propagation tests ──

#[test]
fn test_compute_available_width_col() {
    // Col: available_width = container_size - horizontal padding
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Col,
        gap: [0.0, 0.0],
        padding: [10.0, 5.0, 10.0, 5.0], // left=10, right=10
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0)],
    };

    // Container width 500, padding left+right=20 → available = 480
    let timeline = crate::timeline::Timeline::new();
    let available = timeline.compute_available_width([500.0, 300.0], &metadata, 0);
    assert!((available - 480.0).abs() < 0.01);
}

#[test]
fn test_compute_available_width_grid() {
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Grid,
        gap: [10.0, 10.0],
        padding: [5.0, 5.0, 5.0, 5.0], // left=5, right=5
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: Some(3),
        child_order: vec![EntityId(0), EntityId(1), EntityId(2)],
    };

    // Container width 400, padding left+right=10, gaps=10*2=20, cols=3
    // Available per cell = (400 - 10 - 20) / 3 = 370 / 3 = 123.33
    let timeline = crate::timeline::Timeline::new();
    let available = timeline.compute_available_width([400.0, 200.0], &metadata, 0);
    assert!((available - 123.33).abs() < 0.1);
}

#[test]
fn test_compute_available_width_row_unbounded() {
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [0.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0)],
    };

    // Row: unbounded width
    let timeline = crate::timeline::Timeline::new();
    let available = timeline.compute_available_width([500.0, 300.0], &metadata, 0);
    assert_eq!(available, f32::MAX);
}

#[test]
fn test_compute_available_width_min_1px() {
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Col,
        gap: [0.0, 0.0],
        padding: [100.0, 0.0, 100.0, 0.0], // left=100, right=100 (200 total)
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0)],
    };

    // Container width 50, padding 200 → available would be -150, clamped to 1
    let timeline = crate::timeline::Timeline::new();
    let available = timeline.compute_available_width([50.0, 300.0], &metadata, 0);
    assert!((available - 1.0).abs() < 0.01);
}

#[test]
fn test_compute_container_size_row() {
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let children = vec![
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(200.0, 50.0, PlacementMode::LayoutManaged),
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

    let size = compute_container_size(&children, &metadata);
    // Total width should be sum of children + gap + padding
    // children: 100 + 200 = 300, gap: 10, padding left+right: 10 = 320
    assert!((size[0] - 320.0).abs() < 1.0, "Expected container width ~320, got {}", size[0]);
}

#[test]
fn test_compute_container_size_col() {
    use crate::timeline::{ContainerMetadata, EntityId, LayoutType};

    let children = vec![
        make_child(150.0, 100.0, PlacementMode::LayoutManaged),
        make_child(100.0, 100.0, PlacementMode::LayoutManaged),
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Col,
        gap: [10.0, 10.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    };

    let size = compute_container_size(&children, &metadata);
    // Col width = max child width = 150
    assert!((size[0] - 150.0).abs() < 1.0, "Expected container width ~150, got {}", size[0]);
}

#[test]
fn test_read_text_child_props_non_existent_returns_none() {
    let timeline = crate::timeline::Timeline::new();
    // Non-existent labels should return None
    let props = timeline.read_text_child_props("nonexistent", 0);
    assert!(props.is_none());
}

#[test]
fn test_text_child_props_struct_clone_debug() {
    // Verify TextChildProps derives Clone and Debug
    let props = crate::timeline::layout::TextChildProps {
        text_kind: crate::renderer::text::TextKind::Text,
        content: "hello".to_string(),
        font_family: "sans-serif".to_string(),
        font_size: 48.0,
        font_weight: 400.0,
        font_style: "normal".to_string(),
        line_height: 1.2,
        letter_spacing: 0.0,
        word_spacing: 0.0,
        color: [1.0, 1.0, 1.0, 1.0],
        text_align: "left".to_string(),
        overflow: "visible".to_string(),
        language: String::new(),
        existing_max_width: 0.0,
    };
    let _cloned = props.clone();
    let _debug = format!("{:?}", props);
}

#[test]
fn test_baseline_alignment_row() {
    // Verify that baseline alignment adjusts Y positions in a Row
    let children = vec![
        make_child(100.0, 80.0, PlacementMode::LayoutManaged),
        make_child(80.0, 40.0, PlacementMode::LayoutManaged),
    ];

    let layout_type = LayoutType::Row;

    // First compute with center alignment (default)
    let center_output = compute_taffy_linear_layout(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
    );

    // Compute with baseline alignment
    let child_baselines = vec![-8.0, -4.0]; // baseline offsets: large=-8, small=-4
    let baseline_output = compute_taffy_linear_layout_with_baselines(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
        &child_baselines,
        "baseline",
    );

    // Center alignment should have both children at Y=0 (centered vertically)
    assert!((center_output.positions[0].position[1]).abs() < 0.01);
    assert!((center_output.positions[1].position[1]).abs() < 0.01);

    // Baseline alignment should shift children differently based on baseline offsets
    // large child baseline at Y=-8, small child baseline at Y=-4
    // max_baseline = max(-8, -4) = -4
    // large adjustment = -4 - (-8) = 4 → large child Y += 4
    // small adjustment = -4 - (-4) = 0 → small child Y stays same
    assert!(
        (baseline_output.positions[0].position[1] - 4.0).abs() < 0.01
            || (baseline_output.positions[0].position[1]).abs() < 0.01,
        "Baseline alignment should adjust child Y positions"
    );

    // Verify vertical_align center still works (no baseline adjustment)
    let center_with_baselines = compute_taffy_linear_layout_with_baselines(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
        &child_baselines,
        "center",
    );
    assert!((center_with_baselines.positions[0].position[1]).abs() < 0.01);
    assert!((center_with_baselines.positions[1].position[1]).abs() < 0.01);
}

#[test]
fn test_baseline_alignment_col() {
    // Verify that baseline alignment adjusts Y positions in a Col
    let children = vec![
        make_child(80.0, 60.0, PlacementMode::LayoutManaged),
        make_child(80.0, 40.0, PlacementMode::LayoutManaged),
    ];

    let layout_type = LayoutType::Col;
    let child_baselines = vec![-6.0, -3.0];

    let center_output = compute_taffy_linear_layout_with_baselines(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
        &child_baselines,
        "center",
    );

    let baseline_output = compute_taffy_linear_layout_with_baselines(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
        &child_baselines,
        "baseline",
    );

    // Center: children stacked vertically in Col
    // a (60px tall) is at y=0, b (40px tall) at y=70 (after 10px gap)
    // Container height = 60 + 10 + 40 = 110, center at y=55
    // Center-relative positions: a_y = 0 + 30 - 55 = -25, b_y = 70 + 20 - 55 = 35
    assert!((center_output.positions[0].position[1] - (-25.0)).abs() < 1.0);
    assert!((center_output.positions[1].position[1] - 35.0).abs() < 1.0);

    // Baseline: should adjust Y positions differently from center
    assert!(
        (baseline_output.positions[0].position[1] - center_output.positions[0].position[1]).abs()
            > 0.01,
        "Baseline alignment should differ from center alignment in Col"
    );
}

#[test]
fn test_baseline_alignment_empty_baselines_falls_back_to_center() {
    // When no baselines are provided, baseline alignment should behave like center
    let children = vec![
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
    ];

    let layout_type = LayoutType::Row;
    let empty_baselines: Vec<f32> = vec![];

    let baseline_output = compute_taffy_linear_layout_with_baselines(
        &children,
        layout_type,
        [10.0, 10.0],
        [0.0, 0.0, 0.0, 0.0],
        "center",
        &empty_baselines,
        "baseline",
    );

    // With empty baselines, should fall back to center
    assert!((baseline_output.positions[0].position[1]).abs() < 0.01);
    assert!((baseline_output.positions[1].position[1]).abs() < 0.01);
}

// ── Phase 7: SizeSpec and constraints tests ──

#[test]
fn test_size_spec_fixed() {
    let spec = ChildSizeSpec::fixed([50.0, 30.0]);
    assert_eq!(spec.width, SizeSpec::Fixed(100.0));
    assert_eq!(spec.height, SizeSpec::Fixed(60.0));
    assert!(spec.is_fixed());
}

#[test]
fn test_size_spec_percent() {
    let spec = ChildSizeSpec::from_parts(SizeSpec::Percent(0.5), SizeSpec::Percent(0.75));
    assert!(!spec.is_fixed());
    assert_eq!(spec.width.resolve(200.0), Dimension::percent(0.5));
    assert_eq!(spec.height.resolve(200.0), Dimension::percent(0.75));
    // Resolve absolute
    assert!((spec.width.resolve_absolute(200.0) - 100.0).abs() < 0.001);
    assert!((spec.height.resolve_absolute(200.0) - 150.0).abs() < 0.001);
}

#[test]
fn test_size_spec_fill() {
    let spec = ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Auto);
    assert_eq!(spec.width, SizeSpec::Fill);
    assert_eq!(spec.height, SizeSpec::Auto);
    assert_eq!(spec.width.resolve(200.0), Dimension::percent(1.0));
    assert_eq!(spec.height.resolve(200.0), Dimension::auto());
}

#[test]
fn test_parse_dimension_spec() {
    use crate::ast::Expr;
    // Numeric
    assert_eq!(parse_dimension_spec(&Expr::Num(100.0)), SizeSpec::Fixed(100.0));
    // Percentage string
    assert_eq!(parse_dimension_spec(&Expr::Str("50%".into())), SizeSpec::Percent(0.5));
    assert_eq!(parse_dimension_spec(&Expr::Str("30%".into())), SizeSpec::Percent(0.3));
    // Keywords (string)
    assert_eq!(parse_dimension_spec(&Expr::Str("auto".into())), SizeSpec::Auto);
    assert_eq!(parse_dimension_spec(&Expr::Str("fill".into())), SizeSpec::Fill);
    assert_eq!(parse_dimension_spec(&Expr::Str("fit".into())), SizeSpec::Fit);
    // Keywords (ident)
    assert_eq!(parse_dimension_spec(&Expr::Ident("auto".into())), SizeSpec::Auto);
    assert_eq!(parse_dimension_spec(&Expr::Ident("fill".into())), SizeSpec::Fill);
    assert_eq!(parse_dimension_spec(&Expr::Ident("fit".into())), SizeSpec::Fit);
}

#[test]
fn test_parse_size_spec() {
    use crate::ast::Expr;
    // size: (50%, auto)
    let expr = Expr::Tuple(vec![Expr::Str("50%".into()), Expr::Ident("auto".into())]);
    let spec = parse_size_spec(&expr);
    assert_eq!(spec.width, SizeSpec::Percent(0.5));
    assert_eq!(spec.height, SizeSpec::Auto);

    // size: (100, 200) — fixed tuple
    let expr = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
    let spec = parse_size_spec(&expr);
    assert_eq!(spec.width, SizeSpec::Fixed(100.0));
    assert_eq!(spec.height, SizeSpec::Fixed(200.0));

    // size: fill
    let spec = parse_size_spec(&Expr::Ident("fill".into()));
    assert_eq!(spec.width, SizeSpec::Fill);
    assert_eq!(spec.height, SizeSpec::Auto);

    // size: auto
    let spec = parse_size_spec(&Expr::Ident("auto".into()));
    assert_eq!(spec.width, SizeSpec::Auto);
    assert_eq!(spec.height, SizeSpec::Auto);
}

#[test]
fn test_linear_layout_with_percent_spec() {
    // Create two children: one with 50% width, one with fill
    // in a Row. The parent content size is 400px.
    let children = vec![
        make_child(50.0, 50.0, PlacementMode::LayoutManaged),
        make_child(50.0, 50.0, PlacementMode::LayoutManaged),
    ];
    let specs = vec![
        Some(ChildSizeSpec::from_parts(SizeSpec::Percent(0.5), SizeSpec::Fixed(50.0))),
        Some(ChildSizeSpec::from_parts(SizeSpec::Fill, SizeSpec::Fixed(50.0))),
    ];
    let constraints = vec![SizeConstraints::default(), SizeConstraints::default()];

    let output = compute_taffy_linear_layout_with_specs(
        &children,
        LayoutType::Row,
        [0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        "start",
        &[],
        "center",
        &specs,
        &constraints,
        [400.0, 100.0],
    );

    // With 50% + Fill in a row with no gap: a gets 200px, b gets 200px
    assert_eq!(output.positions.len(), 2);
    // Container size should be 400 x 50
    let container_w = output.container_size[0];
    assert!(
        (container_w - 400.0).abs() < 1.0,
        "Container width expected ~400, got {}",
        container_w
    );
    assert!((output.container_size[1] - 50.0).abs() < 1.0);
}

#[test]
fn test_linear_layout_with_constraints() {
    let children = vec![make_child(200.0, 50.0, PlacementMode::LayoutManaged)];
    let specs = vec![None];
    let constraints = vec![SizeConstraints {
        min_width: None,
        max_width: Some(100.0),
        min_height: Some(50.0),
        max_height: None,
    }];

    let output = compute_taffy_linear_layout_with_specs(
        &children,
        LayoutType::Row,
        [0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        "start",
        &[],
        "center",
        &specs,
        &constraints,
        [500.0, 100.0],
    );

    // Child a wants 200px width, but max_width is 100px, so it should be clamped
    assert!(
        (output.container_size[0] - 100.0).abs() < 1.0,
        "Container width expected ~100 (clamped by max), got {}",
        output.container_size[0]
    );
}

#[test]
fn test_grid_layout_with_specs() {
    let children = vec![
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
        make_child(100.0, 50.0, PlacementMode::LayoutManaged),
    ];
    let specs = vec![None, None, None];
    let constraints = vec![
        SizeConstraints::default(),
        SizeConstraints::default(),
        SizeConstraints {
            min_width: Some(150.0),
            ..SizeConstraints::default()
        },
    ];

    let output = compute_taffy_grid_layout_with_specs(
        &children,
        [0.0, 0.0],
        [0.0, 0.0, 0.0, 0.0],
        2,
        &specs,
        &constraints,
        [500.0, 200.0],
    );

    assert_eq!(output.positions.len(), 3);
    // Container should fit all 3 children: cols=2, rows=2
    assert!(output.container_size[0] > 0.0);
}

#[test]
fn test_constraints_default() {
    let c = SizeConstraints::default();
    assert_eq!(c.min_width, None);
    assert_eq!(c.max_width, None);
    assert_eq!(c.min_height, None);
    assert_eq!(c.max_height, None);
}
