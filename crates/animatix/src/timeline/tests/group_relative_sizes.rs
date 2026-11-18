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

fn deep_group_fixture(group_count: usize, mixed: bool) -> crate::timeline::BuildReport<Timeline> {
    let mut source = String::from("config { resolution: (800, 600) }\n");
    source.push_str("g0: Group, size: (800, 600) {\n");
    for index in 1..group_count {
        let ty = if mixed && index % 2 == 1 {
            "Col"
        } else {
            "Group"
        };
        let size = if mixed {
            "(fill, auto)"
        } else {
            "(fill, fill)"
        };
        source.push_str(&format!("g{index}: {ty}, size: {size} {{\n"));
    }
    let size = if mixed { "(fill, 20)" } else { "(fill, fill)" };
    source.push_str(&format!("leaf: Rect, size: {size}, opacity: 1\n"));
    for _ in 0..group_count {
        source.push_str("}\n");
    }
    eprintln!("layout DSL depth={group_count} mixed={mixed}: parsing");
    let ast = super::support::parse(&source);
    eprintln!("layout DSL depth={group_count} mixed={mixed}: building");
    let report = super::support::build_ast(&ast);
    eprintln!("layout DSL depth={group_count} mixed={mixed}: dropping AST");
    drop(ast);
    eprintln!("layout DSL depth={group_count} mixed={mixed}: built");
    report
}

fn assert_group_reflow_depth(count: usize) {
    let report = deep_group_fixture(count, false);
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "{count}: {:?}",
        report.diagnostics
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback),
        "{count}: {:?}",
        report.diagnostics
    );
    for index in 0..count {
        assert_eq!(
            layout_size(&report.output, &format!("g{index}")),
            [400.0, 300.0],
            "count={count}, group={index}"
        );
    }
    assert_eq!(layout_size(&report.output, "leaf"), [400.0, 300.0], "count={count}");
}

fn assert_mixed_reflow_depth(count: usize) {
    let report = deep_group_fixture(count, true);
    assert!(
        report.diagnostics.iter().all(|d| !d.is_error()),
        "{count}: {:?}",
        report.diagnostics
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback),
        "{count}: {:?}",
        report.diagnostics
    );
    assert_eq!(layout_size(&report.output, "leaf"), [400.0, 10.0], "count={count}");
    for index in 1..count {
        assert_eq!(
            layout_size(&report.output, &format!("g{index}")),
            [400.0, 10.0],
            "count={count}, group={index}"
        );
    }
}

#[test]
fn iterative_reflow_reports_a_malformed_hierarchy_cycle() {
    let mut timeline = deep_group_fixture(2, false).output;
    let root = timeline.world.get_id("g0").unwrap();
    let child = timeline.world.get_id("g1").unwrap();
    // Normal hierarchy construction is a tree. Inject corruption explicitly:
    // the worklist must terminate instead of overflowing or looping forever.
    timeline.world.hierarchy.get_mut(child).children.push(root);
    let mut diagnostics = Vec::new();
    timeline.finalize_group_layout("g0", 0, &mut diagnostics);
    assert_eq!(diagnostics.iter().filter(|d| d.is_error()).count(), 1, "{diagnostics:?}");
    assert!(diagnostics.iter().any(|d| d.is_error()
        && d.message.contains("cycle")
        && d.location.subject.as_deref() == Some("g0")));
}

#[test]
fn iterative_group_reflow_handles_deep_world_without_parser_or_ast_recursion() {
    use crate::timeline::taffy_layout::{ChildSizeSpec, SizeSpec};
    let mut timeline = Timeline::new();
    const DEPTH: usize = 4096;
    for index in 0..DEPTH {
        let label = format!("g{index}");
        let id = timeline.world.spawn(&label, "Group");
        let geometry = timeline.world.geometry_mut(id);
        geometry.size_spec = Some(if index == 0 {
            ChildSizeSpec::fixed([400.0, 300.0])
        } else {
            ChildSizeSpec {
                width: SizeSpec::Fill,
                height: SizeSpec::Fill,
            }
        });
        geometry.ensure_layout_size([1.0, 1.0]);
        if index > 0 {
            timeline.world.set_parent_label(&label, Some(&format!("g{}", index - 1)));
        }
    }
    let leaf = timeline.world.spawn("leaf", "Rect");
    let geometry = timeline.world.geometry_mut(leaf);
    geometry.size_spec = Some(ChildSizeSpec {
        width: SizeSpec::Fill,
        height: SizeSpec::Fill,
    });
    geometry.ensure_layout_size([1.0, 1.0]);
    timeline.world.set_parent_label("leaf", Some(&format!("g{}", DEPTH - 1)));
    let mut diagnostics = Vec::new();
    timeline.finalize_group_layout("g0", 0, &mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    for index in 0..DEPTH {
        assert_eq!(layout_size(&timeline, &format!("g{index}")), [400.0, 300.0]);
    }
    assert_eq!(layout_size(&timeline, "leaf"), [400.0, 300.0]);
}

#[test]
fn iterative_reflow_terminates_mixed_and_container_hierarchy_cycles() {
    for (source, tail, root_is_group) in [
        (
            "root: Group, size:(800,600) { col: Col, size:(fill,auto) { tail: Group, size:(fill,auto) { leaf: Rect, size:(20,20) } } }",
            "tail",
            true,
        ),
        (
            "root: Col, size:(800,600) { tail: Col, size:(fill,auto) { leaf: Rect, size:(20,20) } }",
            "tail",
            false,
        ),
    ] {
        let mut timeline = build(source).output;
        let root = timeline.world.get_id("root").unwrap();
        let tail = timeline.world.get_id(tail).unwrap();
        timeline.world.hierarchy.get_mut(tail).children.push(root);
        let mut diagnostics = Vec::new();
        if root_is_group {
            timeline.finalize_group_layout("root", 0, &mut diagnostics);
        } else {
            timeline.reflow_nested_layouts("root", 0, &mut diagnostics);
        }
        assert_eq!(diagnostics.iter().filter(|d| d.is_error()).count(), 1, "{diagnostics:?}");
        assert!(diagnostics.iter().any(|d| d.is_error()
            && d.message.contains("cycle")
            && d.location.subject.as_deref() == Some("root")));
    }
}

#[test]
fn iterative_branched_grid_reflow_keeps_sibling_order_and_wrapped_text_heights() {
    let report = build(
        r#"
#0s
root: Group, size: (500, 600) {
    grid: Grid, size: (fill, auto), cols: 2, gap: 20 {
        left: Group, size: (fill, auto) {
            left_col: Col, size: (fill, auto), gap: 8 {
                row: Row, size: (fill, 20), gap: 10 {
                    first: Rect, size: (20, 20)
                    second: Rect, size: (20, 20)
                }
                copy: Text, text: "This paragraph must wrap inside its final grid column and publish its full height before the next sibling is placed.", font_size: 20
            }
        }
        right: Group, size: (fill, auto) {
            right_col: Col, size: (fill, auto), gap: 8 {
                other: Text, text: "The right branch also wraps and must not inherit measurements from the left sibling branch.", font_size: 20
                footer: Rect, size: (40, 20)
            }
        }
    }
}
"#,
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let timeline = report.output;
    for label in ["left", "left_col", "right", "right_col"] {
        assert!((layout_size(&timeline, label)[0] * 2.0 - 240.0).abs() < 1.0, "{label}");
    }
    for label in ["copy", "other"] {
        assert!((text_wrap_width(&timeline, label) - 240.0).abs() < 1.0, "{label}");
        assert!(layout_size(&timeline, label)[1] > 20.0, "{label} must wrap");
    }
    let position = |label: &str| {
        timeline
            .world
            .geometry(timeline.world.get_id(label).unwrap())
            .position
            .get(0, [0.0; 2])
    };
    assert!(position("left")[0] < position("right")[0]);
    assert!(position("first")[0] < position("second")[0]);
    assert!(position("row")[1] < position("copy")[1]);
    assert!(position("other")[1] < position("footer")[1]);
    for (column, text) in [("left_col", "copy"), ("right_col", "other")] {
        let expected_height = layout_size(&timeline, text)[1] * 2.0 + 20.0 + 8.0;
        assert!(
            (layout_size(&timeline, column)[1] * 2.0 - expected_height).abs() < 1.0,
            "{column}"
        );
    }
}

// Native subprocesses keep a DSL parser/build/drop stack overflow from aborting
// the entire library harness. No stack-size override or reduced depth is used.
#[cfg(not(target_arch = "wasm32"))]
mod deep_dsl_isolation {
    use super::{assert_group_reflow_depth, assert_mixed_reflow_depth};

    const INPUT: &str = "ANIMATIX_LAYOUT_DSL_PROBE";
    const COMPLETE: &str = "layout DSL isolated worker completed";

    #[test]
    #[ignore = "worker invoked only by the isolated depth tests"]
    fn worker() {
        let input = std::env::var(INPUT).expect("isolated worker input");
        let (mode, count) = input.split_once(':').expect("mode:depth");
        let count: usize = count.parse().expect("numeric depth");
        match mode {
            "group" => assert_group_reflow_depth(count),
            "mixed" => assert_mixed_reflow_depth(count),
            _ => panic!("invalid worker mode"),
        }
        // Emitted after the fixture and report have been dropped, not merely
        // after layout assertions. It also proves the exact filter ran a test.
        eprintln!("{COMPLETE}");
    }

    fn run(mode: &str, depth: usize) {
        let module = module_path!().split_once("::").expect("crate-qualified module").1;
        let test = format!("{module}::worker");
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", &test, "--ignored", "--nocapture", "--test-threads=1"])
            .env(INPUT, format!("{mode}:{depth}"))
            .output()
            .expect("start isolated layout worker");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains(COMPLETE),
            "{mode} depth={depth}, status={}; stdout:\n{stdout}\nstderr:\n{stderr}",
            output.status
        );
    }

    macro_rules! isolated_depth_test {
        ($name:ident, $mode:literal, $depth:literal) => {
            #[test]
            fn $name() {
                run($mode, $depth);
            }
        };
    }

    isolated_depth_test!(group_depth_1, "group", 1);
    isolated_depth_test!(group_depth_63, "group", 63);
    isolated_depth_test!(group_depth_64, "group", 64);
    isolated_depth_test!(group_depth_65, "group", 65);
    isolated_depth_test!(group_depth_128, "group", 128);
    isolated_depth_test!(group_depth_256, "group", 256);
    isolated_depth_test!(mixed_depth_4, "mixed", 4);
    isolated_depth_test!(mixed_depth_65, "mixed", 65);
    isolated_depth_test!(mixed_depth_128, "mixed", 128);
}

#[cfg(target_arch = "wasm32")]
#[test]
fn iterative_group_reflow_resolves_every_axis_beyond_old_depth_limit() {
    for count in [1, 63, 64, 65, 128, 256] {
        assert_group_reflow_depth(count);
    }
}

#[cfg(target_arch = "wasm32")]
#[test]
fn iterative_mixed_group_col_reflow_preserves_top_down_widths() {
    for count in [4, 65, 128] {
        assert_mixed_reflow_depth(count);
    }
}
