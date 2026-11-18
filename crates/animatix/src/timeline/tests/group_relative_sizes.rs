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

fn group_depth_limit_fixture(group_count: usize) -> crate::timeline::BuildReport<Timeline> {
    let mut source = String::from("config { resolution: (800, 600) }\n");
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

fn mixed_group_col_fixture(count: usize) -> crate::timeline::BuildReport<Timeline> {
    let mut source =
        String::from("config { resolution: (800, 600) }\nroot: Group, size:(800,600) {\n");
    for index in 1..count {
        let ty = if index % 2 == 1 { "Col" } else { "Group" };
        source.push_str(&format!("g{index}: {ty}, size:(fill,auto) {{\n"));
    }
    source.push_str("leaf: Rect, size:(fill,20), opacity:1\n");
    for _ in 0..count {
        source.push_str("}\n");
    }
    eprintln!("mixed layout count={count}: parse");
    let ast = super::support::parse(&source);
    eprintln!("mixed layout count={count}: build");
    let report = super::support::build_ast(&ast);
    eprintln!("mixed layout count={count}: drop AST");
    drop(ast);
    eprintln!("mixed layout count={count}: assert sizes");
    report
}

fn assert_mixed_group_col_boundary(count: usize) {
    let report = mixed_group_col_fixture(count);
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(layout_size(&report.output, "root"), [400.0, 300.0]);
    for index in 1..count {
        assert_eq!(layout_size(&report.output, &format!("g{index}")), [400.0, 10.0], "g{index}");
    }
    assert_eq!(layout_size(&report.output, "leaf"), [400.0, 10.0]);
}

// Keep native stack overflows confined to a child process: these tests check
// the promised boundary, not unlimited depth, and do not enlarge the stack.
#[cfg(not(target_arch = "wasm32"))]
mod mixed_boundary_isolation {
    const INPUT: &str = "ANIMATIX_MIXED_LAYOUT_BOUNDARY";
    const COMPLETE: &str = "mixed boundary worker completed";

    #[test]
    #[ignore = "worker for native mixed-boundary tests"]
    fn worker() {
        let Ok(value) = std::env::var(INPUT) else {
            // Running --include-ignored directly is harmless. The parent
            // requires COMPLETE, so accidentally filtering zero work cannot pass.
            return;
        };
        let (count, probe) = value.split_once(':').unwrap_or((&value, "all"));
        let count: usize = count.parse().expect("boundary count");
        assert!(matches!(count, 63 | 64));
        if probe == "all" {
            super::assert_mixed_group_col_boundary(count);
        } else {
            assert_eq!(count, 63);
            let report = super::mixed_group_col_fixture(count);
            let size = |label: &str| super::layout_size(&report.output, label);
            match probe {
                "build" => {},
                "no_error" => assert!(report.diagnostics.iter().all(|d| !d.is_error())),
                "no_fallback" => {
                    assert!(!report.diagnostics.iter().any(|d| {
                        d.code == crate::diagnostics::DiagnosticCode::LayoutSizeFallback
                    }))
                },
                "root" => assert_eq!(size("root"), [400.0, 300.0]),
                "last_width" => assert_eq!(size("g62")[0], 400.0),
                "last_height" => assert_eq!(size("g62")[1], 10.0),
                "leaf_width" => assert_eq!(size("leaf")[0], 400.0),
                "leaf_height" => assert_eq!(size("leaf")[1], 10.0),
                "ancestor_widths" => {
                    for index in 1..62 {
                        assert_eq!(size(&format!("g{index}"))[0], 400.0, "g{index}");
                    }
                },
                "ancestor_heights" => {
                    for index in 1..62 {
                        assert_eq!(size(&format!("g{index}"))[1], 10.0, "g{index}");
                    }
                },
                _ => panic!("unknown isolated probe: {probe}"),
            }
        }
        eprintln!("{COMPLETE}");
    }

    fn run(count: usize) {
        run_probe(count, "all");
    }

    fn run_probe(count: usize, probe: &str) {
        let module = module_path!().split_once("::").expect("crate module").1;
        let test = format!("{module}::worker");
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args([
                "--exact",
                &test,
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(INPUT, format!("{count}:{probe}"))
            .output()
            .expect("spawn mixed boundary worker");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stderr.contains(COMPLETE),
            "mixed count={count} probe={probe}, status={}; child stdout={stdout:?}; child stderr={stderr:?}",
            output.status
        );
    }

    // Temporary named probes preserve the original aggregate assertions and
    // identify which contract failed without depending on captured panic text.
    macro_rules! probe_63 {
        ($name:ident, $probe:literal) => {
            #[test]
            fn $name() {
                run_probe(63, $probe);
            }
        };
    }

    probe_63!(mixed_63_fixture_builds_and_drops, "build");
    probe_63!(mixed_63_no_errors, "no_error");
    probe_63!(mixed_63_no_fallback, "no_fallback");
    probe_63!(mixed_63_root_dimensions, "root");
    probe_63!(mixed_63_g62_width, "last_width");
    probe_63!(mixed_63_g62_height, "last_height");
    probe_63!(mixed_63_leaf_width, "leaf_width");
    probe_63!(mixed_63_leaf_height, "leaf_height");
    probe_63!(mixed_63_ancestor_widths, "ancestor_widths");
    probe_63!(mixed_63_ancestor_heights, "ancestor_heights");

    #[test]
    fn mixed_group_col_63_levels() {
        run(63);
    }

    #[test]
    fn mixed_group_col_64_levels() {
        run(64);
    }
}

#[cfg(target_arch = "wasm32")]
#[test]
fn mixed_group_col_63_levels() {
    assert_mixed_group_col_boundary(63);
}

#[cfg(target_arch = "wasm32")]
#[test]
fn mixed_group_col_64_levels() {
    assert_mixed_group_col_boundary(64);
}
