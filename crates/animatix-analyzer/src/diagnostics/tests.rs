use super::*;
use crate::symbol_table::SymbolTable;

#[test]
fn empty_source_has_no_diagnostics() {
    let diagnostics = collect_diagnostics("", &[], &SymbolTable::default(), None, &[]);
    assert!(diagnostics.is_empty());
}

#[test]
fn unknown_action_detected() {
    let stmts = vec![Stmt::Action(
        Action {
            verb: "fly".to_string(),
            targets: vec!["btn".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
            target_index_position: vec![],
        },
        None,
    )];
    let symbols = SymbolTable::build_from_ast(&[]);
    let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), &[]);

    let unknown_actions: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unknown-action"))
        .collect();
    assert_eq!(unknown_actions.len(), 1);
    assert!(unknown_actions[0].message.contains("fly"));
    assert_eq!(
        unknown_actions[0].subject.as_deref(),
        Some("fly"),
        "quick-fix consumers read `subject`, not the English message"
    );
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
    let stmts = vec![Stmt::Action(
        Action {
            verb: "move".to_string(),
            targets: vec!["nonexistent".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
            target_index_position: vec![],
        },
        None,
    )];
    let symbols = SymbolTable::build_from_ast(&[]);
    let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), &[]);

    let undefined: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("undefined-label"))
        .collect();
    assert_eq!(undefined.len(), 1);
    assert!(undefined[0].message.contains("nonexistent"));
}

/// `camera` is a reserved target, never a declared actor — but only the two
/// persistence actions have an engine path for it, so the exemption is
/// verb-gated and `move camera` stays the mistake it looks like.
#[test]
fn the_reserved_camera_is_a_target_only_for_persistence_actions() {
    let undefined_for = |verb: &str| {
        let stmts = vec![Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec!["camera".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: None,
                target_index: vec![],
                target_index_position: vec![],
            },
            None,
        )];
        let symbols = SymbolTable::build_from_ast(&[]);
        collect_diagnostics("", &[], &symbols, Some(&stmts), &[])
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("undefined-label"))
            .count()
    };

    assert_eq!(undefined_for("persist"), 0, "`persist camera` is how the carry is written");
    assert_eq!(undefined_for("remove"), 0, "`remove camera` is how the carry ends");
    assert_eq!(undefined_for("move"), 1, "no other action routes the camera target");
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
fn lint_config_from_source_parses_disable() {
    let source = "// lint-disable: unused-label, unknown-action\n#0s\ntitle: Text";
    let config = LintConfig::from_source(source);
    assert!(config.is_disabled("unused-label"));
    assert!(config.is_disabled("unknown-action"));
    assert!(!config.is_disabled("unknown-type"));
}

#[test]
fn lint_config_from_source_parses_disable_all_warnings() {
    let source = "// lint-disable-all-warnings\n#0s\ntitle: Text";
    let config = LintConfig::from_source(source);
    assert!(config.disable_all_warnings);
}

#[test]
fn lint_config_suppresses_specific_code() {
    let source = "// lint-disable: unused-label\n#0s\ntitle: Text";
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
    // Don't add any references - should trigger unused-label
    symbols.collect_references(&[]);
    let config = LintConfig::from_source(source);
    let diagnostics =
        collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), &[], &config);

    let unused: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused-label"))
        .collect();
    assert_eq!(unused.len(), 0, "unused-label should be suppressed");
}

#[test]
fn lint_config_suppresses_warnings_globally() {
    let source = "// lint-disable-all-warnings\n#0s\ntitle: Text";
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
    symbols.collect_references(&[]);
    let config = LintConfig::from_source(source);
    let diagnostics =
        collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), &[], &config);

    let warnings: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Warning)
        .collect();
    assert_eq!(warnings.len(), 0, "all warnings should be suppressed");
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
        props: vec![Property {
            name: "font_size".to_string(),
            value: Expr::Str("hello".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
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
        props: vec![Property {
            name: "font_size".to_string(),
            value: Expr::Ident("my_var".to_string()),
            value_span: None,
            trailing_comment: None,
        }],
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
