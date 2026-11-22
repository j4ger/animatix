use super::*;
use crate::symbol_table::SymbolTable;

fn diagnostics_for_source(source: &str) -> Vec<Diagnostic> {
    let parsed = animatix_syntax::parser::parse_source_complete(source);
    let statements = parsed.ast.as_deref().unwrap_or_default();
    let symbols = SymbolTable::build_from_ast(statements);
    collect_diagnostics(source, &parsed.errors, &symbols, parsed.ast.as_deref(), &parsed.tokens)
}

#[test]
fn empty_source_has_no_diagnostics() {
    let diagnostics = collect_diagnostics("", &[], &SymbolTable::default(), None, &[]);
    assert!(diagnostics.is_empty());
}

#[test]
fn unknown_callable_detected() {
    let diagnostics = diagnostics_for_source("fly(target: btn, duration: 1s)\n");
    let unknowns: Vec<_> =
        diagnostics.iter().filter(|d| d.subject.as_deref() == Some("fly")).collect();
    assert_eq!(unknowns.len(), 1);
    assert!(unknowns[0].message.contains("fly"));
}

#[test]
fn span_takes_precedence_over_stale_line_columns() {
    use animatix_syntax::diagnostics::{
        Diagnostic as SyntaxDiagnostic, DiagnosticCode, DiagnosticPhase, DiagnosticSeverity,
    };

    let source = "a: Rect\nb: Rect\nb.color = red";
    let last_line = source.lines().count();
    // Placeholder 1-based line/column pointing at the file start, with a
    // byte span on the last line: the span is the authoritative position.
    let diagnostic = SyntaxDiagnostic {
        severity: DiagnosticSeverity::Warning,
        phase: DiagnosticPhase::Build,
        code: DiagnosticCode::UndefinedLabel,
        message: "Undefined label: nope".to_string(),
        location: animatix_syntax::diagnostics::DiagnosticLocation {
            line: Some(1),
            column: Some(1),
            span: Some(source.len() - 3..source.len()),
            subject: Some("nope".into()),
            ..Default::default()
        },
    };
    let converted = convert_syntax_diagnostic(source, diagnostic);
    assert_eq!(converted.line, last_line - 1);
    assert_eq!(converted.subject.as_deref(), Some("nope"));
}

#[test]
fn undefined_label_detected() {
    let diagnostics = diagnostics_for_source("move(target: nonexistent, by: (1, 0))\n");

    let undefined: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("undefined-label"))
        .collect();
    assert_eq!(undefined.len(), 1);
    assert!(undefined[0].message.contains("nonexistent"));
}

#[test]
fn actor_target_can_be_a_lexically_bound_closure_parameter() {
    let diagnostics = diagnostics_for_source(
        r#"#0s
dot: Ellipse
stagger_each({dot}, 100ms, (item) => fade_in(target: item, duration: 400ms))
fade_in(target: item, duration: 400ms)
"#,
    );
    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("undefined-label"))
        .collect();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].line, 3);
}

/// `camera` is a reserved target, never a declared actor — persistence
/// callables accept it while motion callables still require an Actor value.
#[test]
fn the_reserved_camera_is_a_target_only_for_persistence_callables() {
    let undefined_for = |call: &str| {
        diagnostics_for_source(call)
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("undefined-label"))
            .count()
    };

    assert_eq!(undefined_for("persist(target: camera)\n"), 0);
    assert_eq!(undefined_for("remove(target: camera)\n"), 0);
    assert_eq!(undefined_for("move(target: camera, by: (1, 0))\n"), 1);
}

#[test]
fn unknown_type_detected() {
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "thing".to_string(),
        array_index: None,
        ty: "UnknownType".to_string(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let symbols = SymbolTable::build_from_ast(&stmts);
    let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), &[]);

    let unknown_types: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unknown-type"))
        .collect();
    assert_eq!(unknown_types.len(), 1);
}

#[test]
fn lint_config_from_source_parses_disable_directives() {
    let specific = LintConfig::from_source(
        "// lint-disable: unused-label, unknown-callable\n#0s\ntitle: Text",
    );
    assert!(specific.is_disabled("unused-label"));
    assert!(specific.is_disabled("unknown-callable"));
    assert!(!specific.is_disabled("unknown-type"));

    let all_warnings = LintConfig::from_source("// lint-disable-all-warnings\n#0s\ntitle: Text");
    assert!(all_warnings.disable_all_warnings);
}

#[test]
fn lint_config_suppresses_specific_and_global_warnings() {
    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "title".to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let mut symbols = SymbolTable::build_from_ast(&stmts);
    // No references intentionally leaves `title` unused for both cases.
    symbols.collect_references(&[]);

    for (source, suppress_all_warnings) in [
        ("// lint-disable: unused-label\n#0s\ntitle: Text", false),
        ("// lint-disable-all-warnings\n#0s\ntitle: Text", true),
    ] {
        let config = LintConfig::from_source(source);
        let diagnostics =
            collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), &[], &config);
        if suppress_all_warnings {
            let warnings: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.severity == DiagnosticSeverity::Warning)
                .collect();
            assert_eq!(warnings.len(), 0, "all warnings should be suppressed");
        } else {
            let unused: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.code.as_deref() == Some("unused-label"))
                .collect();
            assert_eq!(unused.len(), 0, "unused-label should be suppressed");
        }
    }
}

#[test]
fn type_mismatch_detected() {
    use animatix_syntax::ast::{Expr, Property};

    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "title".to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![Property::new("font_size", Expr::Str("hello".to_string()))],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let symbols = SymbolTable::build_from_ast(&stmts);
    let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), &[]);

    let type_mismatches: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("type-mismatch"))
        .collect();
    assert_eq!(type_mismatches.len(), 1);
    assert!(type_mismatches[0].message.contains("font_size"));
    assert!(type_mismatches[0].message.contains("Num"));
    assert!(type_mismatches[0].message.contains("Str"));
}

#[test]
fn type_mismatch_not_triggered_for_any() {
    use animatix_syntax::ast::{Expr, Property};

    let stmts = vec![Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: "title".to_string(),
        array_index: None,
        ty: "Text".to_string(),
        props: vec![Property::new(
            "font_size",
            Expr::Ident("my_var".to_string()),
        )],
        modifiers: vec![],
        children: vec![],
        span: None,
    }];
    let symbols = SymbolTable::build_from_ast(&stmts);
    let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), &[]);

    let type_mismatches: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("type-mismatch"))
        .collect();
    assert_eq!(type_mismatches.len(), 0, "Should not trigger for Any type");
}

#[test]
fn lint_config_does_not_suppress_errors() {
    let source = "// lint-disable: parse-0\n#0s\ntitle: Text";
    let parse_errors = vec![ParseError {
        message: "test error".to_string(),
        line: 1,
        column: 1,
        span: 0..10,
        expected: vec![],
        found: None,
        context: vec![],
    }];
    let symbols = SymbolTable::build_from_ast(&[]);
    let config = LintConfig::from_source(source);
    let diagnostics =
        collect_diagnostics_with_config(source, &parse_errors, &symbols, None, &[], &config);

    let errors: Vec<_> =
        diagnostics.iter().filter(|d| d.severity == DiagnosticSeverity::Error).collect();
    assert_eq!(errors.len(), 1, "errors should not be suppressed");
}
