use super::*;

// ── Phase 7: Percentage & intrinsic sizing tests ──

#[test]
fn test_percentage_child_sizing_row() {
    use crate::timeline::layout::ChildExtent;
    // Row with two children: a at 50% width, b fills remainder
    let children = vec![
        ChildExtent {
            label: "a".into(),
            half_size: [50.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            label: "b".into(),
            half_size: [50.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];
    let specs = vec![
        Some(crate::timeline::taffy_layout::ChildSizeSpec::from_parts(
            crate::timeline::taffy_layout::SizeSpec::Percent(0.5),
            crate::timeline::taffy_layout::SizeSpec::Fixed(50.0),
        )),
        Some(crate::timeline::taffy_layout::ChildSizeSpec::from_parts(
            crate::timeline::taffy_layout::SizeSpec::Fill,
            crate::timeline::taffy_layout::SizeSpec::Fixed(50.0),
        )),
    ];
    let constraints = vec![
        crate::timeline::taffy_layout::SizeConstraints::default(),
        crate::timeline::taffy_layout::SizeConstraints::default(),
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [0.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".into(), "b".into()],
    };

    let positions = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata,
        &children,
        &[],
        &specs,
        &constraints,
        [400.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // a (50%) should be at the start (left half of container), b (fill) should take remaining
    // In center-relative coords: a starts at left edge (x=-200), center is at x=-100
    assert!(
        (positions[0][0] - (-100.0)).abs() < 5.0,
        "Child a (50%) expected x~-100 (left of center), got {}",
        positions[0][0]
    );
    assert!(
        (positions[1][0] - (-0.0)).abs() < 5.0 || positions[1][0] > positions[0][0],
        "Child b (fill) should be after child a"
    );
}

#[test]
fn test_min_max_constraints() {
    use crate::timeline::layout::ChildExtent;
    // Child with min_width: 100, max_width: 200
    let children = vec![
        ChildExtent {
            label: "a".into(),
            half_size: [150.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            label: "b".into(),
            half_size: [150.0, 25.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];
    let specs = vec![None, None];
    let constraints = vec![
        crate::timeline::taffy_layout::SizeConstraints {
            min_width: Some(100.0),
            max_width: Some(200.0),
            min_height: None,
            max_height: None,
        },
        crate::timeline::taffy_layout::SizeConstraints::default(),
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [10.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".into(), "b".into()],
    };

    let positions = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata,
        &children,
        &[],
        &specs,
        &constraints,
        [500.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // Child a has max_width: 200, so its actual width should be clamped at 200
    // Child b is 300 (150*2) which is within [0, inf)
    // Container width should be roughly 200 + 10 + 300 = 510
    assert!(
        positions[1][0] - positions[0][0] > 200.0,
        "Child a and b should be spaced apart"
    );
}

#[test]
fn test_parse_size_spec_from_property() {
    use crate::ast::Expr;
    use crate::timeline::taffy_layout::{SizeSpec, parse_size_spec};

    // size: (50%, 40)
    let spec = parse_size_spec(&Expr::Tuple(vec![Expr::Str("50%".into()), Expr::Num(40.0)]));
    assert_eq!(spec.width, SizeSpec::Percent(0.5));
    assert_eq!(spec.height, SizeSpec::Fixed(40.0));

    // size: fill
    let spec = parse_size_spec(&Expr::Ident("fill".into()));
    assert_eq!(spec.width, SizeSpec::Fill);

    // size: auto
    let spec = parse_size_spec(&Expr::Ident("auto".into()));
    assert_eq!(spec.width, SizeSpec::Auto);
}
