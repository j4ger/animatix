use std::collections::HashMap;

use super::*;
use crate::timeline::layout::ChildExtent;
use crate::timeline::taffy_layout::{ChildSizeSpec, SizeConstraints, SizeSpec, parse_size_spec};

// ── Fixtures ──

fn child(half_width: f32, half_height: f32) -> ChildExtent {
    ChildExtent {
        half_size: [half_width, half_height],
        placement_mode: PlacementMode::LayoutManaged,
    }
}

/// A two-child `Row` starting at the origin and vertically centered.
fn row_metadata(gap: f32, padding: f32) -> ContainerMetadata {
    ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [gap, 0.0],
        padding: [padding; 4],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec![EntityId(0), EntityId(1)],
    }
}

fn fixed(value: f32) -> SizeSpec {
    SizeSpec::Fixed(value)
}

fn spec(width: SizeSpec, height: SizeSpec) -> Option<ChildSizeSpec> {
    Some(ChildSizeSpec::from_parts(width, height))
}

// ── Percentage & intrinsic sizing tests ──

#[test]
fn test_percentage_child_sizing_row() {
    // Row with two children: a at 50% width, b fills the remainder.
    let children = vec![child(50.0, 25.0), child(50.0, 25.0)];
    let specs = vec![
        spec(SizeSpec::Percent(0.5), fixed(50.0)),
        spec(SizeSpec::Fill, fixed(50.0)),
    ];
    let constraints = vec![SizeConstraints::default(); 2];

    let positions = LayoutEngine::compute_positions_with_specs(
        &row_metadata(0.0, 0.0),
        &children,
        &[],
        &specs,
        &constraints,
        [400.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // In center-relative coords: a starts at the left edge (x=-200); its
    // center is at x=-100.
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
    // Child a has min_width: 100, max_width: 200.
    let children = vec![child(150.0, 25.0), child(150.0, 25.0)];
    let specs = vec![None, None];
    let constraints = vec![
        SizeConstraints {
            min_width: Some(100.0),
            max_width: Some(200.0),
            ..SizeConstraints::default()
        },
        SizeConstraints::default(),
    ];

    let positions = LayoutEngine::compute_positions_with_specs(
        &row_metadata(10.0, 0.0),
        &children,
        &[],
        &specs,
        &constraints,
        [500.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // Child a clamps at 200 wide; child b is 300 wide and within [0, inf), so
    // the pair (200 + 10 gap + 300) leaves a clear gap between their centers.
    assert!(
        positions[1][0] - positions[0][0] > 200.0,
        "Child a and b should be spaced apart"
    );
}

#[test]
fn test_parse_size_spec_from_property() {
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

    // size: (25%, 80) with Expr::Percent
    let spec = parse_size_spec(&Expr::Tuple(vec![Expr::Percent(25.0), Expr::Num(80.0)]));
    assert_eq!(spec.width, SizeSpec::Percent(0.25));
    assert_eq!(spec.height, SizeSpec::Fixed(80.0));
}

#[test]
fn test_row_percentage_and_fill_sizing_end_to_end() {
    let source = r#"
#0s
row: Row, gap: 16, padding: 20, size: (900, 120), at: (640, 380) {
    a: Rect, size: (25%, 80), min_width: 80
    b: Rect, size: (50%, 80), min_width: 500
    c: Rect, size: (fill, 80)
}
"#;
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    let geometry = |label: &str| {
        let id = timeline.world.get_id(label).expect(label);
        timeline.world.geometry(id)
    };
    let size_a = geometry("a").size.get(0, [0.0, 0.0]);
    let size_b = geometry("b").size.get(0, [0.0, 0.0]);
    let size_c = geometry("c").size.get(0, [0.0, 0.0]);

    // a is 25% of 900 = 225px wide -> half-width 112.5.
    assert!(
        (size_a[0] - 112.5).abs() < 5.0,
        "Expected a width ~225 (half ~112.5), got width {}",
        size_a[0] * 2.0
    );
    assert_eq!(size_a[1], 40.0, "Expected a height 80 (half 40)");

    // b is 50% of 900 = 450, but min_width 500 raises it to 500 -> half-width 250.
    assert!(
        (size_b[0] - 250.0).abs() < 5.0,
        "Expected b width ~500 (half ~250), got width {}",
        size_b[0] * 2.0
    );
    assert_eq!(size_b[1], 40.0, "Expected b height 80 (half 40)");

    // c fills the rest: 900 - 40 padding - 32 gap - 225 - 500 = 103 -> half 51.5.
    assert!(
        (size_c[0] - 51.5).abs() < 5.0,
        "Expected c width ~103 (half ~51.5), got width {}",
        size_c[0] * 2.0
    );
    assert_eq!(size_c[1], 40.0, "Expected c height 80 (half 40)");

    // Positions should be ordered from left to right.
    let pos_a = geometry("a").position.get(0, [0.0, 0.0]);
    let pos_b = geometry("b").position.get(0, [0.0, 0.0]);
    let pos_c = geometry("c").position.get(0, [0.0, 0.0]);
    assert!(
        pos_a[0] < pos_b[0],
        "a must be to the left of b: a_x={}, b_x={}",
        pos_a[0],
        pos_b[0]
    );
    assert!(
        pos_b[0] < pos_c[0],
        "b must be to the left of c: b_x={}, c_x={}",
        pos_b[0],
        pos_c[0]
    );
}

/// Percentage and `fill` children must be admitted even before they have a
/// measured `layout_size`: the layout pass is what measures them. Dropping
/// them made the container shrink to its fixed siblings, so `25%` resolved
/// against that shrunken box instead of the container's declared width
/// (`docs/roadmap.md`, "Percentage and `fill` child sizes collapse in a Row").
#[test]
fn percentage_children_are_admitted_before_they_are_measured() {
    let source = r#"
#0s
row: Row, gap: 16, padding: 20, size: (900, 120) {
    a: Rect, size: (25%, 80)
    b: Rect, size: (fill, 80)
}
"#;
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    let width = |label: &str| {
        let id = timeline.world.get_id(label).expect(label);
        timeline.world.geometry(id).size.get(0, [0.0, 0.0])[0] * 2.0
    };
    // 25% of the declared 900px content width.
    assert!((width("a") - 225.0).abs() < 5.0, "a should be ~225px, got {}", width("a"));
    // fill = the rest of the declared width.
    assert!(width("b") > 100.0, "fill child must not collapse, got {}", width("b"));
}

/// A container that sizes children by percentage but declares no `size` has no
/// parent box to resolve against. It must say so rather than silently
/// resolving against its own content-driven width.
#[test]
fn percent_child_without_container_size_reports_a_fallback() {
    let source = r#"
#0s
row: Row, gap: 16, padding: 20 {
    a: Rect, size: (25%, 80)
}
"#;
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::LayoutSizeFallback),
        "expected a layout-size-fallback warning, got {:?}",
        report.diagnostics.iter().map(|d| d.code).collect::<Vec<_>>()
    );
}

/// `max_item_width` is the layout item width cap. It is deliberately a
/// distinct name from `max_width`, which is the text wrap width and is not a
/// layout constraint (`docs/roadmap.md`).
#[test]
fn max_item_width_caps_a_row_item() {
    let source = r#"
#0s
row: Row, gap: 0, padding: 0, size: (900, 120) {
    a: Rect, size: (50%, 80), max_item_width: 200
}
"#;
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));
    let a = timeline.world.get_id("a").expect("a");
    let width = timeline.world.geometry(a).size.get(0, [0.0, 0.0])[0] * 2.0;
    // 50% of 900 = 450, capped by max_item_width: 200.
    assert!((width - 200.0).abs() < 5.0, "expected the 200px cap to apply, got {width}");
}
