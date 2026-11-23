//! Relative sizing through structural Group wrappers.

use super::support::build;
use super::*;

fn layout_size(timeline: &Timeline, label: &str) -> [f32; 2] {
    let id = timeline.world.get_id(label).unwrap_or_else(|| {
        let labels: Vec<_> = timeline.world.tracks.iter().map(|(name, _)| name.as_str()).collect();
        panic!("missing entity '{label}'; expanded runtime labels: {labels:?}");
    });
    timeline
        .world
        .geometry(id)
        .layout_size_last()
        .unwrap_or_else(|| timeline.world.geometry(id).size.last([0.0, 0.0]))
}

fn text_wrap_width(timeline: &Timeline, label: &str) -> f32 {
    let id = timeline.world.get_id(label).unwrap_or_else(|| {
        let labels: Vec<_> = timeline.world.tracks.iter().map(|(name, _)| name.as_str()).collect();
        panic!("missing text entity '{label}'; expanded runtime labels: {labels:?}");
    });
    timeline.world.text_or_default(id).text_max_width.get(0, 0.0)
}

#[test]
fn fixed_group_passes_relative_width_to_col_and_wrapped_text() {
    let timeline = build(
        r#"
config { canvas_size: (800, 600) }
#0s
host: Col, size: (480, auto), at: (400, 300), gap: 0 {
  wrap: Group, size: (50%, auto) {
    body: Col, size: fill, gap: 0 {
      copy: Text, text: "A wrapped paragraph inside a structural Group should use the width inherited from its fixed parent column.", font_size: 20
    }
  }
}
"#,
    )
    .output;

    for label in ["wrap", "body"] {
        let width = layout_size(&timeline, label)[0] * 2.0;
        assert!((width - 240.0).abs() < 1.0, "{label} should inherit 240px, got {width}");
    }
    assert!((text_wrap_width(&timeline, "copy") - 240.0).abs() < 1.0);
    let copy = layout_size(&timeline, "copy");
    assert!(copy[1] > 20.0, "text should wrap into multiple lines: {copy:?}");
}

#[test]
fn grid_fill_width_reaches_group_col_and_text_at_multiple_grid_sizes() {
    for grid_width in [420.0f32, 840.0] {
        let report = build(&format!(
            r#"
config {{ canvas_size: (1200, 900) }}
#0s
grid: Grid, size: ({grid_width}, auto), cols: 2, gap: 18, padding: 12, at: (600, 450) {{
  left: Group, size: fill {{
    left_col: Col, size: fill {{
      left_text: Text, text: "The left card contains wrapping text that measures against its resolved grid slot width.", font_size: 18
    }}
  }}
  right: Group, size: fill {{
    right_col: Col, size: fill {{
      right_text: Text, text: "The right card also wraps to its final column width rather than measuring at an unconstrained size.", font_size: 18
    }}
  }}
}}
"#
        ));
        assert!(
            report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
            "unexpected layout errors: {:?}",
            report.diagnostics
        );
        let timeline = report.output;
        let expected_width = (grid_width - 24.0 - 18.0) / 2.0;
        for label in ["left", "left_col", "right", "right_col"] {
            let width = layout_size(&timeline, label)[0] * 2.0;
            assert!(
                (width - expected_width).abs() < 1.0,
                "grid width={grid_width}: {label} should be {expected_width}, got {width}"
            );
        }
        for label in ["left_text", "right_text"] {
            assert!(
                (text_wrap_width(&timeline, label) - expected_width).abs() < 1.0,
                "grid width={grid_width}: {label} should receive {expected_width}px wrap width, got {}",
                text_wrap_width(&timeline, label)
            );
            assert!(
                layout_size(&timeline, label)[1] > 18.0,
                "grid width={grid_width}: {label} should wrap to multiple lines"
            );
        }
    }
}

#[test]
fn explicit_relative_percentages_multiply_through_nested_groups() {
    let timeline = build(
        r#"
config { canvas_size: (800, 600) }
#0s
host: Col, size: (400, auto), at: (400, 300), gap: 0 {
  outer: Group, size: (50%, auto) {
    inner: Group, size: (50%, auto) {
      body: Col, size: fill {
        copy: Text, text: "Nested explicit percentages should compose once at each Group boundary.", font_size: 20
      }
    }
  }
}
"#,
    )
    .output;

    for (label, expected) in [("outer", 200.0), ("inner", 100.0), ("body", 100.0)] {
        let width = layout_size(&timeline, label)[0] * 2.0;
        assert!((width - expected).abs() < 1.0, "{label}: expected {expected}px, got {width}");
    }
    assert!((text_wrap_width(&timeline, "copy") - 100.0).abs() < 1.0);
}

#[test]
fn group_constraints_clamp_size_without_moving_fixed_children() {
    let timeline = build(
        r#"
config { canvas_size: (800, 600) }
#0s
host: Col, size: (400, 300), at: (400, 300), gap: 0 {
  minimum: Group, size: (25%, auto), min_width: 130 {
    min_child: Rect, size: (40, 30), at: (31, 17)
  }
  maximum: Group, size: (75%, auto), max_item_width: 220 {
    max_child: Rect, size: (50, 20), at: (-23, 11)
  }
}
"#,
    )
    .output;

    assert!((layout_size(&timeline, "minimum")[0] * 2.0 - 130.0).abs() < 1.0);
    assert!((layout_size(&timeline, "maximum")[0] * 2.0 - 220.0).abs() < 1.0);
    for (label, expected_position, expected_size) in [
        ("min_child", [31.0, 17.0], [20.0, 15.0]),
        ("max_child", [-23.0, 11.0], [25.0, 10.0]),
    ] {
        let id = timeline.world.get_id(label).expect("fixed child");
        let geometry = timeline.world.geometry(id);
        assert_eq!(geometry.position.get(0, [0.0, 0.0]), expected_position);
        assert_eq!(geometry.size.get(0, [0.0, 0.0]), expected_size);
    }
}

#[test]
fn root_fixed_group_provides_percent_basis_and_empty_groups_keep_fixed_axes() {
    let timeline = build(
        r#"
config { canvas_size: (800, 600) }
#0s
root: Group, size: (400, auto) {
  body: Col, size: (50%, auto) {
    copy: Text, text: "A top-level fixed Group width should resolve a percentage child column and wrapped text.", font_size: 20
  }
}
empty_width: Group, size: (260, auto)
empty_height: Group, size: (auto, 120)
"#,
    )
    .output;

    assert!((layout_size(&timeline, "root")[0] * 2.0 - 400.0).abs() < 1.0);
    assert!((layout_size(&timeline, "body")[0] * 2.0 - 200.0).abs() < 1.0);
    assert!((text_wrap_width(&timeline, "copy") - 200.0).abs() < 1.0);

    let empty_width = layout_size(&timeline, "empty_width");
    let empty_height = layout_size(&timeline, "empty_height");
    assert!((empty_width[0] * 2.0 - 260.0).abs() < 1.0);
    assert!(
        empty_width[1].abs() < 0.5,
        "auto height of an empty Group should be zero: {empty_width:?}"
    );
    assert!((empty_height[1] * 2.0 - 120.0).abs() < 1.0);
    assert!(
        empty_height[0].abs() < 0.5,
        "auto width of an empty Group should be zero: {empty_height:?}"
    );
}

#[test]
fn expanded_component_root_and_group_apply_relative_width_once_each() {
    use std::path::Path;

    use animatix_syntax::module::ModuleGraph;

    let source = r#"
pub component Card() {
  root: Col, size: (50%, auto), gap: 0 {
    wrapper: Group, size: (50%, auto) {
      body: Col, size: fill, gap: 0 {
        copy: Text, text: "Component content wraps to the width passed through the root and wrapper.", font_size: 20
      }
    }
  }
  marker: Rect, size: (12, 12)
}
#0s
host: Col, size: (400, auto), at: (400, 300), gap: 0 {
  card: Card
}
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(
            Path::new("virtual:///relative_group_component.amx"),
            Some(source),
        )
        .expect("load component program");
    let expanded = program.expand_components().expect("component expansion");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "component layout diagnostics: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    for (label, expected) in [
        ("card", 200.0),
        ("card.wrapper", 100.0),
        ("card.body", 100.0),
    ] {
        let width = layout_size(&timeline, label)[0] * 2.0;
        assert!((width - expected).abs() < 1.0, "{label}: expected {expected}px, got {width}");
    }
    assert!((text_wrap_width(&timeline, "card.copy") - 100.0).abs() < 1.0);
}

#[test]
fn auto_group_without_parent_axis_reports_relative_size_fallback() {
    let report = build(
        r#"
#0s
auto_group: Group, size: auto {
  relative: Col, size: (50%, auto) {
    copy: Text, text: "A relative child needs a definite parent axis.", font_size: 20
  }
}
"#,
    );
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
        }),
        "an auto-sized Group cannot provide a definite percentage basis: {:?}",
        report.diagnostics
    );
}

#[test]
fn group_reflow_invalidates_an_already_evaluated_frame() {
    let mut timeline = build(
        "#0s\nwrap: Group, size: (240, auto) {\n\
         column: Col, size: fill { copy: Text, text: \"wrapped content\" }\n}",
    )
    .output;
    let dimensions = SceneDimensions {
        width: 640,
        height: 360,
    };
    let _ = timeline.evaluate(0.0, dimensions);
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());
    let epoch = timeline.content_epoch();
    timeline.finalize_group_layout("wrap", 0, &mut Vec::new());
    assert!(timeline.content_epoch() > epoch);
    assert!(timeline.eval_caches.frame_cache.borrow().is_none());
    assert!(timeline.eval_caches.static_subtree_cache.borrow().is_empty());
    let _ = timeline.evaluate(0.0, dimensions);
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());
}

#[test]
fn fixed_group_constraints_define_the_descendant_percentage_basis() {
    let timeline = build(
        r#"
#0s
host: Col, size: (600, auto) {
  capped: Group, size: (300, auto), max_item_width: 220 {
    capped_body: Col, size: (50%, auto) { a: Text, text: "Wrap to the constrained Group size", font_size: 18 }
  }
  minimum: Group, size: (100, auto), min_width: 180 {
    minimum_body: Col, size: (50%, auto) { b: Text, text: "Wrap to the constrained Group size", font_size: 18 }
  }
}
"#,
    ).output;
    for (group, body, text, width) in [
        ("capped", "capped_body", "a", 220.0),
        ("minimum", "minimum_body", "b", 180.0),
    ] {
        assert!((layout_size(&timeline, group)[0] * 2.0 - width).abs() < 0.5);
        assert!((layout_size(&timeline, body)[0] * 2.0 - width / 2.0).abs() < 0.5);
        assert!((text_wrap_width(&timeline, text) - width / 2.0).abs() < 0.5);
    }
}

#[test]
fn scene_is_the_definite_parent_of_a_top_level_relative_group() {
    let report = build(
        r#"
config { canvas_size: (640, 360) }
#0s
backdrop: Group, size: (fill, fill) {
  plate: Rect, size: (fill, fill)
}

half_scene: Group, size: (50%, auto) {
  fixed: Rect, size: (40, 20)
}
"#,
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.is_error()
                || d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback),
        "top-level fill/percent uses the scene box: {:?}",
        report.diagnostics
    );
    for label in ["backdrop", "plate"] {
        assert_eq!(layout_size(&report.output, label).map(|half| half * 2.0), [640.0, 360.0]);
    }
    assert_eq!(layout_size(&report.output, "half_scene").map(|half| half * 2.0), [320.0, 20.0]);
}

#[test]
fn root_shape_and_container_resolve_relative_axes_against_canvas() {
    let report = build(
        r#"
config { canvas_size: (800, 600) }
#0s
root_box: Rect, size: (50%, 25%)
root_col: Col, size: (50%, 25%) {
  leaf: Rect, size: (50%, 20)
}
"#,
    );
    assert_eq!(layout_size(&report.output, "root_box"), [200.0, 75.0]);
    assert_eq!(layout_size(&report.output, "root_col"), [200.0, 75.0]);
    assert_eq!(layout_size(&report.output, "leaf"), [100.0, 10.0]);
}

fn group_depth_limit_fixture(group_count: usize) -> crate::timeline::BuildReport<Timeline> {
    let mut source = String::from("config { canvas_size: (800, 600) }\n");
    source.push_str("g0: Group, size: (800, 600) {\n");
    // The scalar `fill` shorthand only fills width; this fixture exercises both axes.
    for index in 1..group_count {
        source.push_str(&format!("g{index}: Group, size: (fill, fill) {{\n"));
    }
    source.push_str("leaf: Rect, size: (fill, fill), opacity: 1\n");
    for _ in 0..group_count {
        source.push_str("}\n");
    }
    build(&source)
}

#[test]
fn group_reflow_resolves_fill_through_63_and_64_groups() {
    // Root depth is zero. With 64 Groups the last Group runs at depth 63
    // and still resolves its leaf; this is not unlimited-depth support.
    for group_count in [1, 63, 64] {
        let report = group_depth_limit_fixture(group_count);
        assert!(
            !report.diagnostics.iter().any(|d| {
                d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
                    && d.message.contains("depth")
            }),
            "unexpected depth fallback: {:?}",
            report.diagnostics
        );
        assert_eq!(layout_size(&report.output, "leaf"), [400.0, 300.0]);
    }
}

#[test]
fn group_reflow_at_65_groups_warns_and_preserves_unresolved_descendants() {
    let report = group_depth_limit_fixture(65);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| { d.severity == crate::diagnostics::DiagnosticSeverity::Error }),
        "depth fallback must remain non-fatal: {:?}",
        report.diagnostics
    );
    let depth_warning_count = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
                && d.message.contains("depth")
        })
        .count();
    assert_eq!(depth_warning_count, 1, "one warning at the reflow boundary");
    let warning = report
        .diagnostics
        .iter()
        .find(|d| {
            d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
                && d.message.contains("depth")
        })
        .expect("depth-limited reflow must report its partial-layout fallback");
    assert_eq!(warning.severity, crate::diagnostics::DiagnosticSeverity::Warning);
    assert_eq!(warning.phase, crate::diagnostics::DiagnosticPhase::Build);
    assert!(warning.message.contains("64"));
    assert_eq!(warning.location.subject.as_deref(), Some("g64"));
    // The parent pass resolves g64 itself, but its children are not reflowed.
    // Keep the leaf entity and its old measurements rather than deleting it.
    assert_eq!(layout_size(&report.output, "g64"), [400.0, 300.0]);
    assert!(report.output.world.tracks.contains_key("leaf"));
    assert_ne!(layout_size(&report.output, "leaf"), [400.0, 300.0]);
}

#[test]
fn mixed_size_declarations_seed_fixed_axes_before_layout() {
    let report = build(
        r#"
root_width: Rect, size: (20, auto), opacity: 1
root_height: Rect, size: (auto, 30), opacity: 1
host: Group, size: (800, 600) {
  horizontal: Rect, size: (fill, 20), opacity: 1
  vertical: Rect, size: (30, fill), opacity: 1
}
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    assert_eq!(layout_size(&report.output, "root_width")[0], 10.0);
    assert_eq!(layout_size(&report.output, "root_height")[1], 15.0);
    for (label, expected) in [("horizontal", [400.0, 10.0]), ("vertical", [15.0, 300.0])] {
        assert_eq!(layout_size(&report.output, label), expected);
        let id = report.output.world.get_id(label).unwrap();
        assert_eq!(report.output.world.geometry(id).size.get(0, [0.0; 2]), expected);
    }
}

#[test]
fn mixed_size_fixed_axis_survives_redeclaration_and_still_animates() {
    let report = build(
        r#"
r: Rect, size: (auto, 20), opacity: 1
#1s
r: Rect, opacity: 1
#2s
r.size = (100, 60) [2s]
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let id = report.output.world.get_id("r").unwrap();
    let geometry = report.output.world.geometry(id);
    for (time, expected) in [
        (0, 10.0),
        (1000, 10.0),
        (2000, 10.0),
        (3000, 20.0),
        (4000, 30.0),
    ] {
        assert_eq!(geometry.size.get(time, [0.0; 2])[1], expected, "time={time}");
    }
}

#[test]
fn repeated_mixed_sizes_follow_shape_property_order() {
    let report = build(
        r#"
a: Ellipse, size: (auto, 20), radius_y: 30, size: auto, opacity: 1
b: Ellipse, radius_y: 30, size: (auto, 20), opacity: 1
c: Ellipse, size: (40, 20), radius_y: 30, size: auto, opacity: 1
d: Ellipse, size: (auto, 20), radius_y: 30, size: (80, 100), opacity: 1
e: Ellipse, size: auto, radius_y: 30, size: (auto, 20), opacity: 1
f: Ellipse, size: (auto, 20), radius_y: 30, size: (auto, 40), opacity: 1
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    for (label, height) in [
        ("a", 30.0),
        ("b", 10.0),
        ("c", 30.0),
        ("d", 50.0),
        ("e", 10.0),
        ("f", 20.0),
    ] {
        assert_eq!(layout_size(&report.output, label)[1], height, "{label}");
        let id = report.output.world.get_id(label).unwrap();
        assert_eq!(report.output.world.geometry(id).size.get(0, [0.0; 2])[1], height, "{label}");
    }
    assert_eq!(layout_size(&report.output, "d")[0], 40.0);
}

#[test]
fn mixed_size_numeric_expressions_and_vec2_values_keep_their_meaning() {
    let report = build(
        r#"
let height = 20
let dimensions = (40, 60)
vector: Rect, size: dimensions, opacity: 1
host: Group, size: (800, 600) {
  numeric: Rect, size: (fill, height + 10), opacity: 1
  percent: Rect, size: (50%, height + 20), opacity: 1
  quoted: Rect, size: ("25%", height), opacity: 1
}
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    for (label, expected) in [
        ("vector", [20.0, 30.0]),
        ("numeric", [400.0, 15.0]),
        ("percent", [200.0, 20.0]),
        ("quoted", [100.0, 10.0]),
    ] {
        assert_eq!(layout_size(&report.output, label), expected, "{label}");
    }
}

#[test]
fn mixed_group_col_layout_preserves_fixed_leaf_height_through_64_levels() {
    // Odd counts end in Group; even counts end in Col. Both must publish
    // the authored leaf height, not a default from an unresolved mixed tuple.
    for count in [3, 4, 63, 64] {
        let mut source =
            String::from("config { canvas_size: (800, 600) }\nroot: Group, size:(800,600) {\n");
        for index in 1..count {
            let ty = if index % 2 == 1 { "Col" } else { "Group" };
            source.push_str(&format!("g{index}: {ty}, size:(fill,auto) {{\n"));
        }
        source.push_str("leaf: Rect, size:(fill,20), opacity:1\n");
        for _ in 0..count {
            source.push_str("}\n");
        }
        let report = build(&source);
        assert!(
            !report.diagnostics.iter().any(|d| {
                d.is_error() || d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
            }),
            "count={count}: {:?}",
            report.diagnostics
        );
        assert_eq!(layout_size(&report.output, "root"), [400.0, 300.0]);
        for index in 1..count {
            assert_eq!(
                layout_size(&report.output, &format!("g{index}")),
                [400.0, 10.0],
                "count={count}, g{index}"
            );
        }
        assert_eq!(layout_size(&report.output, "leaf"), [400.0, 10.0]);
    }
}

#[test]
fn non_repeated_shape_sizes_keep_authored_order_and_animation() {
    let report = build(
        r#"
before: Ellipse, radius_y: 30, size: (40, 20), opacity: 1
after: Ellipse, size: (40, 20), radius_y: 30, opacity: 1
#2s
before.size = (80, 60) [2s]
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let before = report.output.world.get_id("before").unwrap();
    let after = report.output.world.get_id("after").unwrap();
    assert_eq!(report.output.world.geometry(after).size.get(0, [0.0; 2]), [20.0, 30.0]);
    let geometry = report.output.world.geometry(before);
    for (time, expected) in [
        (0, [20.0, 10.0]),
        (2000, [20.0, 10.0]),
        (3000, [30.0, 20.0]),
        (4000, [40.0, 30.0]),
        (0, [20.0, 10.0]),
    ] {
        assert_eq!(geometry.size.get(time, [0.0; 2]), expected, "time={time}");
        assert_eq!(geometry.layout_size_get(time), Some(expected), "time={time}");
    }
}

#[test]
fn single_shape_pass_keeps_corner_radius_validation() {
    let report = build("box: Rect, size: (80, 40), corner_radius: \"invalid\", opacity: 1");
    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                && d.message.contains("corner_radius expects a number")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(warnings[0].severity, crate::diagnostics::DiagnosticSeverity::Warning);
    assert_eq!(warnings[0].location.subject.as_deref(), Some("box.corner_radius"));
}

#[test]
fn single_shape_pass_reports_unknown_icon_once_with_instance_label() {
    let report = build("mark: Path, icon: \"definitely-not-an-icon\", opacity: 1");
    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                && d.message.contains("unknown icon")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "{:?}", report.diagnostics);
    assert_eq!(warnings[0].location.subject.as_deref(), Some("mark.icon"));
}

#[test]
fn single_shape_pass_keeps_line_and_arrow_endpoints_on_redeclaration() {
    for ty in ["Line", "Arrow"] {
        let report = build(&format!(
            "edge: {ty}, from: (10,20), to: (80,90), opacity: 1\n\
             #1s\nedge: {ty}, to: (100,120), opacity: 1\n"
        ));
        assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
        let id = report.output.world.get_id("edge").unwrap();
        let shape = report.output.world.shape_or_default(id);
        assert_eq!(shape.line_from.get(0, [0.0; 2]), [10.0, 20.0], "{ty}");
        assert_eq!(shape.line_to.get(0, [0.0; 2]), [80.0, 90.0], "{ty}");
        assert_eq!(shape.line_from.get(1000, [0.0; 2]), [10.0, 20.0], "{ty}");
        assert_eq!(shape.line_to.get(1000, [0.0; 2]), [100.0, 120.0], "{ty}");
    }
}

#[test]
fn single_shape_pass_keeps_glass_clip_corner_radius() {
    let report = build(
        r#"
card: Glass, size: (200, 100), corner_radius: 17 {
  content: Rect, size: (40, 20), opacity: 1
}
#1s
card: Glass, size: (220, 120)
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let id = report.output.world.get_id("card").unwrap();
    let shape = report.output.world.shape_or_default(id);
    assert_eq!(shape.corner_radius.get(0, 0.0), 17.0);
    assert_eq!(shape.corner_radius.get(1000, 0.0), 17.0);
}

#[test]
fn failed_corner_radius_evaluation_keeps_existing_zero_fallback() {
    let report = build(
        r#"
box: Rect, size: (80, 40), corner_radius: 17, opacity: 1
#1s
box: Rect, corner_radius: unavailable_corner.radius, opacity: 1
"#,
    );
    assert!(
        report.diagnostics.iter().any(|d| {
            d.code == crate::diagnostics::DiagnosticCode::UnknownLookupPath
                && d.location.subject.as_deref() == Some("box.corner_radius")
        }),
        "failed expression must be diagnosed: {:?}",
        report.diagnostics
    );
    let id = report.output.world.get_id("box").unwrap();
    let shape = report.output.world.shape_or_default(id);
    assert_eq!(shape.corner_radius.get(0, 0.0), 17.0);
    assert_eq!(shape.corner_radius.get(1000, 0.0), 0.0);
}
