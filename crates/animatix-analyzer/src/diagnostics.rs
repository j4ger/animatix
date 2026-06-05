//! Diagnostic collection from parse errors and semantic checks.

use crate::symbol_table::{SymbolTable, LabelKind};
use animatix_syntax::ast::*;
use animatix_syntax::parser::ParseError;
use std::collections::HashSet;


/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The severity level of this diagnostic.
    pub severity: DiagnosticSeverity,
    /// The 0-based line number where the issue starts.
    pub line: usize,
    /// The 0-based column number where the issue starts.
    pub col: usize,
    /// The 0-based line number where the issue ends.
    pub end_line: usize,
    /// The 0-based column number where the issue ends.
    pub end_col: usize,
    /// A human-readable description of the issue.
    pub message: String,
    /// An optional error code for categorisation.
    pub code: Option<String>,
}

/// The severity of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// An error that will prevent correct rendering.
    Error,
    /// A warning about a potential issue.
    Warning,
    /// An informative message.
    Info,
    /// A helpful suggestion or hint.
    Hint,
}

impl Diagnostic {
    /// Returns true if this diagnostic is an error.
    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }

    /// Returns true if this diagnostic is a warning.
    pub fn is_warning(&self) -> bool {
        self.severity == DiagnosticSeverity::Warning
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Hint => "hint",
        };
        let code = self.code.as_deref().unwrap_or("");
        write!(f, "{}:{}:{}: {}: {}", self.line + 1, self.col + 1, severity, code, self.message)
    }
}

/// Lint configuration for suppressing specific diagnostics.
#[derive(Debug, Default, Clone)]
pub struct LintConfig {
    /// Diagnostic codes to suppress (e.g., "unused-label", "unknown-property").
    pub disabled: HashSet<String>,
    /// Whether to disable all warnings.
    pub disable_all_warnings: bool,
}

impl LintConfig {
    /// Parse lint config from inline comments in the source.
    /// Looks for `// lint-disable: code1, code2` and `// lint-disable-all-warnings`.
    pub fn from_source(source: &str) -> Self {
        let mut config = Self::default();
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("// lint-disable:") {
                for code in rest.split(',') {
                    let code = code.trim().to_lowercase();
                    if !code.is_empty() {
                        config.disabled.insert(code);
                    }
                }
            } else if trimmed == "// lint-disable-all-warnings" {
                config.disable_all_warnings = true;
            }
        }
        config
    }

    /// Load lint config from an `.amx.toml` file.
    pub fn from_file(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        #[derive(serde::Deserialize)]
        struct LintFile {
            lint: Option<LintSection>,
        }

        #[derive(serde::Deserialize)]
        struct LintSection {
            disabled: Option<Vec<String>>,
            disable_all_warnings: Option<bool>,
        }

        match toml::from_str::<LintFile>(&content) {
            Ok(file) => {
                let mut config = Self::default();
                if let Some(lint) = file.lint {
                    if let Some(codes) = lint.disabled {
                        config.disabled = codes.into_iter().map(|c| c.to_lowercase()).collect();
                    }
                    config.disable_all_warnings = lint.disable_all_warnings.unwrap_or(false);
                }
                config
            }
            Err(_) => Self::default(),
        }
    }

    /// Merge another config into this one (combines disabled codes).
    pub fn merge(&mut self, other: &LintConfig) {
        self.disabled.extend(other.disabled.iter().cloned());
        if other.disable_all_warnings {
            self.disable_all_warnings = true;
        }
    }

    /// Check if a diagnostic code is disabled.
    pub fn is_disabled(&self, code: &str) -> bool {
        self.disabled.contains(code)
    }
}

/// Collect all diagnostics from the source.
pub fn collect_diagnostics(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
) -> Vec<Diagnostic> {
    collect_diagnostics_with_config(source, parse_errors, symbols, ast, &LintConfig::default())
}

/// Collect all diagnostics from the source with lint configuration.
pub fn collect_diagnostics_with_config(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
    config: &LintConfig,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Chumsky parse errors (structured with positions)
    for (i, error) in parse_errors.iter().enumerate() {
        let end_span = Span::from_range(source, error.span.clone());
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            line: error.line.saturating_sub(1),
            col: error.column.saturating_sub(1),
            end_line: end_span.end_line.saturating_sub(1),
            end_col: end_span.end_col.saturating_sub(1),
            message: error.message.clone(),
            code: Some(format!("parse-{}", i)),
        });
    }

    // 2. Semantic checks (if AST is available)
    if let Some(stmts) = ast {
        collect_semantic_diagnostics(source, stmts, symbols, &mut diagnostics);
    }

    // 3. Filter based on lint config
    diagnostics.retain(|d| {
        // Never suppress errors
        if d.severity == DiagnosticSeverity::Error {
            return true;
        }
        // Check if all warnings are disabled
        if config.disable_all_warnings && d.severity == DiagnosticSeverity::Warning {
            return false;
        }
        // Check if specific code is disabled
        if let Some(code) = &d.code {
            if config.is_disabled(code) {
                return false;
            }
        }
        true
    });

    diagnostics
}

/// Collect semantic diagnostics from the AST.
fn collect_semantic_diagnostics(
    source: &str,
    stmts: &[Stmt],
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Check for duplicate labels
    let mut seen_labels = HashSet::new();
    for (name, info) in &symbols.labels {
        if !seen_labels.insert(name) {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                line: info.line,
                col: info.col,
                end_line: info.line,
                end_col: info.col + name.len(),
                message: format!("Duplicate label: {}", name),
                code: Some("duplicate-label".to_string()),
            });
        }
    }

    // Check for unused labels (actors, let bindings)
    for (name, info) in &symbols.labels {
        if !symbols.referenced_labels.contains(name) {
            // Don't warn for for-loop variables or always blocks
            if info.kind == LabelKind::For || info.kind == LabelKind::Always {
                continue;
            }
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                line: info.line,
                col: info.col,
                end_line: info.line,
                end_col: info.col + name.len(),
                message: format!("Unused {}: '{}'", match info.kind {
                    LabelKind::Actor => "actor",
                    LabelKind::Let => "binding",
                    LabelKind::Component => "component",
                    _ => "label",
                }, name),
                code: Some("unused-label".to_string()),
            });
        }
    }

    // Check for missing imports
    for import in &symbols.imports {
        if let Some(span) = &import.span {
            let import_path = std::path::Path::new(&import.path);
            if !import_path.exists() {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    line: span.start_line.saturating_sub(1),
                    col: span.start_col.saturating_sub(1),
                    end_line: span.end_line.saturating_sub(1),
                    end_col: span.end_col.saturating_sub(1),
                    message: format!("Import file not found: '{}'", import.path),
                    code: Some("missing-import".to_string()),
                });
            }
        }
    }
    // Check each statement
    for stmt in stmts {
        check_stmt(source, stmt, symbols, diagnostics);
    }
}

/// Convert an optional Span to (line, col, end_line, end_col) 0-based positions.
fn span_to_diag(span: &Option<animatix_syntax::ast::Span>) -> (usize, usize, usize, usize) {
    match span {
        Some(s) => (
            s.start_line.saturating_sub(1),
            s.start_col.saturating_sub(1),
            s.end_line.saturating_sub(1),
            s.end_col.saturating_sub(1),
        ),
        None => (0, 0, 0, 0),
    }
}

/// Check a single statement for semantic issues.
fn check_stmt(source: &str, stmt: &Stmt, symbols: &SymbolTable, diagnostics: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::Action(action, span) => {
            let (line, col, end_line, end_col) = span_to_diag(span);

            // Check if action verb is known
            if !symbols.actions.contains(&action.verb) {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    line,
                    col,
                    end_line,
                    end_col,
                    message: format!("Unknown action: {}", action.verb),
                    code: Some("unknown-action".to_string()),
                });
            }

            // Check if target labels exist
            for target in &action.targets {
                if !symbols.labels.contains_key(target) {
                    diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        line,
                        col,
                        end_line,
                        end_col,
                        message: format!("Undefined label: {}", target),
                        code: Some("undefined-label".to_string()),
                    });
                }
            }
        }

        Stmt::Assignment { target, property, value, span, .. } => {
            let (line, col, end_line, end_col) = span_to_diag(span);

            // Check if target label exists
            if let Some(label) = target.first() {
                if !symbols.labels.contains_key(label) {
                    diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        line,
                        col,
                        end_line,
                        end_col,
                        message: format!("Undefined label: {}", label),
                        code: Some("undefined-label".to_string()),
                    });
                }
            }

            // Check if property is known for the target type
            if let Some(label) = target.first() {
                if let Some(info) = symbols.labels.get(label) {
                    if let Some(ty) = &info.ty {
                        if let Some(known_props) = symbols.properties.get(ty) {
                            if !known_props.contains(property) {
                                diagnostics.push(Diagnostic {
                                    severity: DiagnosticSeverity::Info,
                                    line,
                                    col,
                                    end_line,
                                    end_col,
                                    message: format!(
                                        "Property '{}' not commonly used on {} (may still be valid)",
                                        property, ty
                                    ),
                                    code: Some("unknown-property".to_string()),
                                });
                            }

                            // Check property type
                            let key = (ty.clone(), property.clone());
                            if let Some(expected_type) = symbols.property_types.get(&key) {
                                let actual_type = crate::symbol_table::infer_expr_type(value);
                                if actual_type != crate::symbol_table::PropertyType::Any
                                    && expected_type != &crate::symbol_table::PropertyType::Any
                                    && actual_type != *expected_type
                                {
                                    diagnostics.push(Diagnostic {
                                        severity: DiagnosticSeverity::Warning,
                                        line,
                                        col,
                                        end_line,
                                        end_col,
                                        message: format!(
                                            "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                            label, property, expected_type, actual_type
                                        ),
                                        code: Some("type-mismatch".to_string()),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        Stmt::ActorDecl { ty, props, span, .. } => {
            let (line, col, end_line, end_col) = span_to_diag(span);

            // Check if type is known
            if !symbols.types.contains(ty) {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    line,
                    col,
                    end_line,
                    end_col,
                    message: format!("Unknown type: {}", ty),
                    code: Some("unknown-type".to_string()),
                });
            }

            // Check properties against known properties for this type
            if let Some(known_props) = symbols.properties.get(ty) {
                for prop in props {
                    if !known_props.contains(&prop.name) {
                        diagnostics.push(Diagnostic {
                            severity: DiagnosticSeverity::Info,
                            line,
                            col,
                            end_line,
                            end_col,
                            message: format!(
                                "Property '{}' not commonly used on {} (may still be valid)",
                                prop.name, ty
                            ),
                            code: Some("unknown-property".to_string()),
                        });
                    }

                    // Check property type if we have type info
                    let key = (ty.clone(), prop.name.clone());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = crate::symbol_table::infer_expr_type(&prop.value);
                        if actual_type != crate::symbol_table::PropertyType::Any
                            && expected_type != &crate::symbol_table::PropertyType::Any
                            && actual_type != *expected_type
                        {
                            diagnostics.push(Diagnostic {
                                severity: DiagnosticSeverity::Warning,
                                line,
                                col,
                                end_line,
                                end_col,
                                message: format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    ty, prop.name, expected_type, actual_type
                                ),
                                code: Some("type-mismatch".to_string()),
                            });
                        }
                    }
                }
            }
        }

        // Recurse into blocks
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            for stmt in body {
                check_stmt(source, stmt, symbols, diagnostics);
            }
        }
        Stmt::Sequence { body, .. } | Stmt::Stagger { body, .. } | Stmt::Always { body, .. } => {
            for stmt in body {
                check_stmt(source, stmt, symbols, diagnostics);
            }
        }
        Stmt::Conditional { then_branch, else_branch, .. } => {
            for stmt in then_branch {
                check_stmt(source, stmt, symbols, diagnostics);
            }
            if let Some(else_stmts) = else_branch {
                for stmt in else_stmts {
                    check_stmt(source, stmt, symbols, diagnostics);
                }
            }
        }
        Stmt::ForLoop { body, .. } => {
            for stmt in body {
                check_stmt(source, stmt, symbols, diagnostics);
            }
        }
        Stmt::ComponentDef(def, ..) => {
            for stmt in &def.body {
                check_stmt(source, stmt, symbols, diagnostics);
            }
        }

        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    #[test]
    fn empty_source_has_no_diagnostics() {
        let diagnostics = collect_diagnostics("", &[], &SymbolTable::default(), None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn unknown_action_detected() {
        let stmts = vec![
            Stmt::Action(Action {
                verb: "fly".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: None,
            }, None),
        ];
        let symbols = SymbolTable::build_from_ast(&[]);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts));

        let unknown_actions: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("unknown-action"))
            .collect();
        assert_eq!(unknown_actions.len(), 1);
        assert!(unknown_actions[0].message.contains("fly"));
    }

    #[test]
    fn undefined_label_detected() {
        let stmts = vec![
            Stmt::Action(Action {
                verb: "move".to_string(),
                targets: vec!["nonexistent".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: None,
            }, None),
        ];
        let symbols = SymbolTable::build_from_ast(&[]);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts));

        let undefined: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("undefined-label"))
            .collect();
        assert_eq!(undefined.len(), 1);
        assert!(undefined[0].message.contains("nonexistent"));
    }

    #[test]
    fn unknown_type_detected() {
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "thing".to_string(),
                ty: "UnknownType".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let symbols = SymbolTable::build_from_ast(&stmts);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts));

        let unknown_types: Vec<_> = diagnostics.iter()
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
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let mut symbols = SymbolTable::build_from_ast(&stmts);
        // Don't add any references - should trigger unused-label
        symbols.collect_references(&[]);
        let config = LintConfig::from_source(source);
        let diagnostics = collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), &config);

        let unused: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("unused-label"))
            .collect();
        assert_eq!(unused.len(), 0, "unused-label should be suppressed");
    }

    #[test]
    fn lint_config_suppresses_warnings_globally() {
        let source = "// lint-disable-all-warnings\n#0s\ntitle: Text";
        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let mut symbols = SymbolTable::build_from_ast(&stmts);
        symbols.collect_references(&[]);
        let config = LintConfig::from_source(source);
        let diagnostics = collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), &config);

        let warnings: Vec<_> = diagnostics.iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .collect();
        assert_eq!(warnings.len(), 0, "all warnings should be suppressed");
    }

    #[test]
    fn type_mismatch_detected() {
        use animatix_syntax::ast::{Property, Expr};

        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![
                    Property {
                        name: "font_size".to_string(),
                        value: Expr::Str("hello".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let symbols = SymbolTable::build_from_ast(&stmts);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts));

        let type_mismatches: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("type-mismatch"))
            .collect();
        assert_eq!(type_mismatches.len(), 1);
        assert!(type_mismatches[0].message.contains("font_size"));
        assert!(type_mismatches[0].message.contains("Num"));
        assert!(type_mismatches[0].message.contains("String"));
    }

    #[test]
    fn type_mismatch_not_triggered_for_any() {
        use animatix_syntax::ast::{Property, Expr};

        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".to_string(),
                ty: "Text".to_string(),
                props: vec![
                    Property {
                        name: "font_size".to_string(),
                        value: Expr::Ident("my_var".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let symbols = SymbolTable::build_from_ast(&stmts);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts));

        let type_mismatches: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("type-mismatch"))
            .collect();
        assert_eq!(type_mismatches.len(), 0, "Should not trigger for Any type");
    }

    #[test]
    fn lint_config_does_not_suppress_errors() {
        let source = "// lint-disable: parse-0\n#0s\ntitle: Text";
        let parse_errors = vec![
            ParseError {
                message: "test error".to_string(),
                line: 1,
                column: 1,
                span: 0..10,
                expected: vec![],
                found: None,
                context: vec![],
            },
        ];
        let symbols = SymbolTable::build_from_ast(&[]);
        let config = LintConfig::from_source(source);
        let diagnostics = collect_diagnostics_with_config(source, &parse_errors, &symbols, None, &config);

        let errors: Vec<_> = diagnostics.iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "errors should not be suppressed");
    }
}
