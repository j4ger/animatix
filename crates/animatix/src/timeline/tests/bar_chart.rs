use super::*;

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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
        report.diagnostics.is_empty(),
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
