use super::*;

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
fn without_content_lints(
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> impl Iterator<Item = &animatix_syntax::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
}

#[test]
fn bar_colors_scheme_token_single() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20), ("C", 30)}, bar_colors: accent.danger
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn bar_colors_scheme_token_list() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_colors: {accent.danger, accent.success}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn bar_colors_mixed_list() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_colors: {accent.danger, (0, 1, 0, 1)}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn bar_colors_auto() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_colors: auto
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn bar_colors_invalid_token_emits_diagnostic() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_colors: nonexistent.token
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::UnknownColorReference),
        "Expected UnknownColorReference diagnostic, got: {:?}",
        report.diagnostics
    );
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn bar_width_with_variable() {
    let source = r#"
        let w = 30
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_width: w
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn show_axis_bool() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_axis: true
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn show_axis_string() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_axis: "false"
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn show_axis_invalid_type_emits_diagnostic() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_axis: 42
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPropertyValue),
        "Expected InvalidPropertyValue diagnostic, got: {:?}",
        report.diagnostics
    );
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn bar_colors_registry_is_build_time_only() {
    let schema = crate::timeline::property_registry::lookup_property("bar_colors")
        .expect("bar_colors should be registered");
    assert_eq!(
        schema.value_type,
        crate::timeline::property_registry::ValueType::BuildTimeOnly,
        "bar_colors is parsed by the BarChart builder, not stored as a string property"
    );
}

#[test]
fn show_labels_bool_creates_child_label_tracks() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_labels: true
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    assert!(
        report.output.tracks.contains_key("chart_bar_label_0"),
        "Bar labels should create child Text tracks"
    );
    assert!(
        report.output.tracks.contains_key("chart_bar_label_1"),
        "Each bar should create a child Text label"
    );
}

#[test]
fn show_labels_string() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_labels: "false"
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    assert!(
        !report.output.tracks.contains_key("chart_bar_label_0"),
        "show_labels: \"false\" should suppress child label tracks"
    );
}

#[test]
fn show_labels_invalid_type_emits_diagnostic() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, show_labels: 42
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPropertyValue
                && d.message.contains("show_labels")),
        "Expected show_labels InvalidPropertyValue diagnostic, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn bar_width_auto_string_is_valid() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_width: "auto"
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn bar_width_invalid_type_emits_diagnostic() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, bar_width: "wide"
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPropertyValue
                && d.message.contains("bar_width")),
        "Expected bar_width InvalidPropertyValue diagnostic, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn max_value_invalid_type_emits_diagnostic() {
    let source = r#"
        chart: BarChart, data: {("A", 10), ("B", 20)}, max_value: "tall"
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == DiagnosticCode::InvalidPropertyValue
                && d.message.contains("max_value")),
        "Expected max_value InvalidPropertyValue diagnostic, got: {:?}",
        report.diagnostics
    );
}

#[test]
fn flat_number_list_auto_labeling() {
    let source = r#"
        chart: BarChart, data: {10, 20, 30}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // A diagnostic should be emitted informing about auto-labeling
    assert!(
        report.diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidPropertyValue
            && d.message.contains("flat number list detected")),
        "Expected InvalidPropertyValue diagnostic about auto-labeling, got: {:?}",
        report.diagnostics
    );
    // The chart track should still exist (auto-labeling worked)
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn flat_number_list_single_value() {
    let source = r#"
        chart: BarChart, data: {42}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // A diagnostic should be emitted for the flat number list
    assert!(
        report.diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidPropertyValue
            && d.message.contains("flat number list detected")),
        "Expected auto-labeling diagnostic, got: {:?}",
        report.diagnostics
    );
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn tuple_list_still_works_with_no_diagnostics() {
    let source = r#"
        chart: BarChart, data: {("X", 10), ("Y", 20)}
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // Normal tuple list should produce no diagnostics
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    assert!(report.output.tracks.contains_key("chart"), "chart track should exist");
}

#[test]
fn max_value_with_variable() {
    let source = r#"
        let m = 100
        chart: BarChart, data: {("A", 10), ("B", 20)}, max_value: m
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
}

/// A BarChart hosted inside a Graph must span the graph's declared axis box.
/// (Regression: `{graph}_size` was seeded as the HALF size while every
/// consumer treated it as full, so hosted plots occupied only the central
/// half of the axis.)
#[test]
fn hosted_bar_chart_spans_graph_axis() {
    let source = r#"
        g: Graph, size: (800, 360), x_domain: (0, 3), y_domain: (0, 50) {
          chart: BarChart, data: {("A", 10), ("B", 40), ("C", 25)}
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );

    // The env side-channel carries the declared full-pixel size.
    match report.output.env().get("g_size") {
        Some(crate::timeline::Value::Vec2(sz)) => {
            assert_eq!(sz, [800.0, 360.0], "env size must be the declared full size");
        },
        other => panic!("expected g_size in env, got {other:?}"),
    }

    // Hosted bars must span most of the 800px axis box (math-mode layout
    // reserves padding), not the legacy central half.
    let track = report.output.tracks.get("chart").expect("chart track");
    let paths = track.evaluate_vector_paths(0);
    assert!(!paths.is_empty(), "hosted bars should be baked");
    use kurbo::Shape as _;
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for path in &paths {
        let bbox = path.path.bounding_box();
        min_x = min_x.min(bbox.x0);
        max_x = max_x.max(bbox.x1);
    }
    let width = max_x - min_x;
    assert!(
        width > 600.0,
        "hosted bars should span most of the 800px graph, got width {width}"
    );
}

/// A cross-file `@slot` fill must reach the expanded component even when
/// the slot container sits DEEPER than the component body's top level
/// (e.g. ui.amx's Card nests its header/body slots inside a root Col).
/// Regression: resolve_slots only handled top-level slots, so nested
/// slots kept their fallbacks and ignored the fill entirely.
#[test]
fn cross_file_slot_fill_applies_at_any_depth() {
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let mut program = graph
        .load_program_with_source(std::path::Path::new("/tmp/amxrepro/repro_slot.amx"), None)
        .expect("load program");
    let _diagnostics = program.typecheck();
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");

    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let has_track = |needle: &str| report.output.tracks.keys().any(|k| k.contains(needle));
    let has_fill_text = report
        .output
        .tracks
        .iter()
        .any(|(_k, t)| t.text.text_content.get(0, String::new()).contains("SLOT FILLED"));

    // Scene-hierarchy regression: a multi-statement component body must expand
    // into ONE root node with the whole subtree linked under it, not orphan the
    // instance wrapper (empty children) with the expanded siblings as
    // independent roots. Before the fix this was:
    //   CHILDREN card: []   / ROOT NODES: ["card", "card.root"]
    assert_eq!(
        report.output.root_nodes,
        vec!["card".to_string()],
        "a multi-statement component instance must be a single root node"
    );
    let card = report.output.tracks.get("card").expect("card track exists");
    assert!(
        !card.children.is_empty(),
        "the instance wrapper 'card' must contain the expanded body as children"
    );
    // The expanded body must stay nested under the instance (not ride the
    // scene-root list alongside it).
    assert!(
        !report.output.root_nodes.iter().any(|r| r.starts_with("card.")),
        "expanded body children must not become separate root nodes"
    );

    assert!(has_fill_text, "slot fill content must reach the build");

    assert!(has_track("card.header"), "header container should exist (with the fill inside)");
    assert!(!has_track("card.header_fallback"), "filled slot must drop its fallback");
    assert!(has_track("card.body_fallback"), "unfilled slot keeps its fallback");
}

/// Scene-hierarchy regression for the same-file slot demo: each multi-statement
/// `MetricCard` instance inside the `row` container must expand to a single
/// component node (`card1`/`card2`/`card3`) whose expanded body stays nested
/// under it, and the row must contain exactly those three nodes. Before the fix
/// each instance's three body statements were spread as separate row children
/// (nine cells in the row) and no per-card node existed.
#[test]
fn components_gallery_instances_form_single_nodes() {
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let mut program = graph
        .load_program_with_source(
            std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../examples/components/09_components.amx"
            )),
            None,
        )
        .expect("load 09");
    let _diagnostics = program.typecheck();
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    let row = report.output.tracks.get("row").expect("row container exists");
    assert_eq!(
        row.children,
        vec![
            "card1".to_string(),
            "card2".to_string(),
            "card3".to_string()
        ],
        "each MetricCard instance must be a single node in the row, not three spread cells"
    );
    for card in ["card1", "card2", "card3"] {
        let track = report.output.tracks.get(card).expect("card node exists");
        assert!(
            track.children.len() == 3,
            "{card} must contain its expanded body (frame, header, value_text), got {:?}",
            track.children
        );
    }
    // The slot fills (a/b/c) must live inside each card's header, not ride the
    // scene root. (The `parent` back-reference is not always back-filled for
    // first-declaration children, so assert on the authoritative children list
    // and on the root-node set instead.)
    let header = report.output.tracks.get("card1.header").expect("card1.header track");
    assert!(
        header.children.contains(&"a".to_string()),
        "slot fill 'a' must be nested under the owning component's header"
    );
    assert!(
        !report.output.root_nodes.contains(&"a".to_string()),
        "slot fill 'a' must not be an independent root node"
    );
}

/// A component fn defined in an IMPORTED module must resolve when invoked
/// cross-file (`pulse_twice p`): the build inlines it and the analyzer's
/// semantic check accepts it via the merged action set. (Regression: the
/// invocation fell through to unknown-action cross-file.)
#[test]
fn cross_file_component_fn_resolves() {
    use animatix_syntax::module::{ModuleGraph, SourceAccess};
    use std::path::Path;

    let lib = r#"
pub component Pulsar {
  box: Rect, size: (120, 80), color: accent.primary
  fn pulse_twice {
    pulse box [300ms, intensity: 1.5]
  }
}
"#;
    let main = r#"
import "fnlib.amx"

config { resolution: (640, 360) }

p: Pulsar

#0s
fade-in p [200ms]

#0.5s
pulse_twice p
"#;
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(Path::new("/proj/fnlib.amx").to_path_buf(), lib.to_string());
    graph.upsert_source(Path::new("/proj/main.amx").to_path_buf(), main.to_string());
    let _ = graph.load_file_standalone(Path::new("/proj/fnlib.amx"));
    let _ = graph.load_file_standalone(Path::new("/proj/main.amx"));

    let mut program = graph
        .load_program_with_source(Path::new("/proj/main.amx"), None)
        .expect("load program");
    let _diagnostics = program.typecheck();
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");

    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let unknown: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.code == crate::diagnostics::DiagnosticCode::UnknownAction)
        .collect();
    assert!(
        unknown.is_empty(),
        "cross-file component fn must not report unknown-action, got: {unknown:?}"
    );
    // The fn body must inline with the component child rewritten to the
    // instance label: #0.5s { Block { pulse p [300ms, intensity: 1.5] } }.
    let pulse_inlined = expanded.iter().any(|s| {
        matches!(s, Stmt::Keyframe { body, .. } if body.iter().any(|inner| matches!(
            inner,
            Stmt::Block { body, .. } if body.iter().any(|a| matches!(
                a,
                Stmt::Action(crate::ast::Action { verb, targets, .. }, _)
                    if verb == "pulse" && targets.iter().any(|t| t == "p")
            ))
        )))
    });
    assert!(pulse_inlined, "fn body must inline with the instance target");
}

/// A hosted PlotCurve's declared `color:` must drive its baked stroke —
/// including tuple and colorscheme-token values. (Regression: the plot props
/// loop matched only `Value::Color`, so `accent.primary` (Vec4) and tuples
/// were silently rejected and every curve rendered the default white.)
#[test]
fn plot_curve_color_drives_stroke() {
    let source = r#"
        g: Graph, size: (800, 360), x_domain: (0, 3), y_domain: (0, 50) {
          curve: PlotCurve, kind: "cartesian", func: (x) => x * 10,
            color: (0.2, 0.4, 0.8, 1.0), stroke_width: 3
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("curve").expect("curve track");
    let paths = track.evaluate_vector_paths(0);
    let strokes: Vec<_> = paths.iter().filter_map(|p| p.stroke).collect();
    assert!(!strokes.is_empty(), "curve strokes should be baked");
    for (color, width) in strokes {
        assert_eq!(width, 3.0);
        let c = color.components;
        assert!(
            (c[0] - 0.2).abs() < 0.02 && (c[1] - 0.4).abs() < 0.02 && (c[2] - 0.8).abs() < 0.02,
            "stroke should carry the declared color, got {c:?}"
        );
    }
}

/// A single `bar_colors` value must color EVERY bar uniformly. (Regression:
/// only bar 0 was colored; bars past the one-element list fell back to the
/// actor default color, contradicting the documented uniform-color intent.)
#[test]
fn single_bar_colors_value_is_uniform() {
    let source = r#"
        chart: BarChart,
          data: {("A", 10), ("B", 40), ("C", 25)},
          bar_colors: accent.primary,
          show_labels: false
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("chart").expect("chart track");
    let paths = track.evaluate_vector_paths(0);
    let fills: Vec<_> = paths.iter().filter_map(|p| p.fill).collect();
    assert!(fills.len() >= 3, "expected three bar fills, got {fills:?}");
    let first = fills[0];
    assert!(
        fills.iter().all(|f| *f == first),
        "single bar_colors value must color all bars identically, got {fills:?}"
    );
}

/// `size:` must drive the baked bar-chart geometry. (Regression: the layout
/// box was read from the pre-declaration track snapshot, so every fresh
/// build — always the case for CLI export — laid bars out in the default
/// ~40x40 box regardless of `size:`.)
#[test]
fn size_property_drives_bar_chart_geometry() {
    let source = r#"
        chart: BarChart,
          data: {("A", 10), ("B", 40), ("C", 25)},
          show_axis: false,
          show_labels: false,
          size: (800, 380),
          at: (640, 400)
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("chart").expect("chart track");
    assert_eq!(track.geometry.size.last([0.0, 0.0]), [400.0, 190.0]);
    assert_eq!(track.geometry.position.last([0.0, 0.0]), [640.0, 400.0]);

    // Baked bar paths must span roughly the declared box (800x380 minus
    // hardcoded axis-label margins), not the legacy 40x40 cluster.
    let paths = track.evaluate_vector_paths(0);
    assert!(!paths.is_empty(), "bar paths should be baked");
    use kurbo::Shape as _;
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for path in &paths {
        let bbox = path.path.bounding_box();
        min[0] = min[0].min(bbox.x0);
        min[1] = min[1].min(bbox.y0);
        max[0] = max[0].max(bbox.x1);
        max[1] = max[1].max(bbox.y1);
    }
    let width = max[0] - min[0];
    let height = max[1] - min[1];
    assert!(width > 600.0, "bars should span most of the 800px box, got width {width}");
    assert!(height > 250.0, "bars should span most of the 380px box, got height {height}");
}

/// `anchor:`/`offset:` on a BarChart must produce a scene-relative position
/// binding like on any other actor, instead of being silently dropped.
#[test]
fn bar_chart_anchor_offset_produce_scene_binding() {
    let source = r#"
        chart: BarChart,
          data: {("A", 10), ("B", 20)},
          anchor: scene.top,
          offset: (0, 60)
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );

    let track = report.output.tracks.get("chart").expect("chart track");
    match track
        .geometry
        .position_binding
        .get(0, crate::timeline::PositionBinding::Absolute)
    {
        crate::timeline::PositionBinding::SceneAnchor { anchor, offset } => {
            assert_eq!(anchor, crate::timeline::SceneAnchor::Top);
            assert_eq!(offset, [0.0, 60.0]);
        },
        other => panic!("expected SceneAnchor binding, got {other:?}"),
    }
}
