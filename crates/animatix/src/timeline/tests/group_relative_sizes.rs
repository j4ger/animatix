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
config { resolution: (800, 600) }
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
config {{ resolution: (1200, 900) }}
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
config { resolution: (800, 600) }
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
config { resolution: (800, 600) }
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
config { resolution: (800, 600) }
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
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
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
config { resolution: (640, 360) }
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
