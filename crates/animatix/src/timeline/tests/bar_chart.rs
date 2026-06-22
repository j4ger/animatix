use super::*;

#[test]
fn bar_colors_scheme_token_single() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20), ("C", 30)},
            bar_colors: accent.danger,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
    assert!(report.output.tracks.get("chart").is_some(), "chart track should exist");
}

#[test]
fn bar_colors_scheme_token_list() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            bar_colors: {accent.danger, accent.success},
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn bar_colors_mixed_list() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            bar_colors: {accent.danger, (0, 1, 0, 1)},
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn bar_colors_auto() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            bar_colors: auto,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn bar_colors_invalid_token_does_not_crash() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            bar_colors: nonexistent.token,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // Print the diagnostics for debugging
    if !report.diagnostics.is_empty() {
        println!("DIAGS: {:?}", report.diagnostics);
    }
    // Just verify the track exists and no build errors
    assert!(report.output.tracks.get("chart").is_some(), "chart track should exist");
}

#[test]
fn bar_width_with_variable() {
    let source = r#"
        let w = 30
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            bar_width: w,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn show_axis_bool() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            show_axis: true,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn show_axis_string() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            show_axis: "false",
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}

#[test]
fn show_axis_invalid_type_does_not_crash() {
    let source = r#"
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            show_axis: 42,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    // Print the diagnostics for debugging
    if !report.diagnostics.is_empty() {
        println!("DIAGS: {:?}", report.diagnostics);
    }
    // Just verify the track exists and no build errors
    assert!(report.output.tracks.get("chart").is_some(), "chart track should exist");
}

#[test]
fn max_value_with_variable() {
    let source = r#"
        let m = 100
        chart: BarChart {
            data: {("A", 10), ("B", 20)},
            max_value: m,
        }
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", report.diagnostics);
}
