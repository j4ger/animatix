//! Diagnostic collection from parse errors and semantic checks.

use std::collections::HashSet;

use animatix_syntax::ast::*;
use animatix_syntax::parser::ParseError;

use crate::symbol_table::SymbolTable;

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
            },
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
    tree: Option<&tree_sitter::Tree>,
) -> Vec<Diagnostic> {
    collect_diagnostics_with_config(
        source,
        parse_errors,
        symbols,
        ast,
        tree,
        &LintConfig::default(),
    )
}

/// Collect all diagnostics from the source with lint configuration.
pub fn collect_diagnostics_with_config(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
    tree: Option<&tree_sitter::Tree>,
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
        let syntax_diagnostics =
            animatix_syntax::semantic_diagnostics::collect_semantic_diagnostics(
                stmts, symbols, tree, source,
            );
        diagnostics.extend(syntax_diagnostics.into_iter().map(convert_syntax_diagnostic));
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

/// Convert a syntax-level diagnostic into the analyzer DTO with 0-based ranges.
fn convert_syntax_diagnostic(d: animatix_syntax::diagnostics::Diagnostic) -> Diagnostic {
    let severity = match d.severity {
        animatix_syntax::diagnostics::DiagnosticSeverity::Error => DiagnosticSeverity::Error,
        animatix_syntax::diagnostics::DiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
        animatix_syntax::diagnostics::DiagnosticSeverity::Info => DiagnosticSeverity::Info,
        animatix_syntax::diagnostics::DiagnosticSeverity::Hint => DiagnosticSeverity::Hint,
    };
    let line = d.location.line.map(|v| v.saturating_sub(1)).unwrap_or(0);
    let col = d.location.column.map(|v| v.saturating_sub(1)).unwrap_or(0);
    let end_line = d.location.end_line.map(|v| v.saturating_sub(1)).unwrap_or(line);
    let end_col = d.location.end_col.map(|v| v.saturating_sub(1)).unwrap_or(col + 1);
    Diagnostic {
        severity,
        line,
        col,
        end_line,
        end_col,
        message: d.message,
        code: Some(d.code.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol_table::SymbolTable;

    #[test]
    fn empty_source_has_no_diagnostics() {
        let diagnostics = collect_diagnostics("", &[], &SymbolTable::default(), None, None);
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
            },
            None,
        )];
        let symbols = SymbolTable::build_from_ast(&[]);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), None);

        let unknown_actions: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("unknown-action"))
            .collect();
        assert_eq!(unknown_actions.len(), 1);
        assert!(unknown_actions[0].message.contains("fly"));
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
            },
            None,
        )];
        let symbols = SymbolTable::build_from_ast(&[]);
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), None);

        let undefined: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code.as_deref() == Some("undefined-label"))
            .collect();
        assert_eq!(undefined.len(), 1);
        assert!(undefined[0].message.contains("nonexistent"));
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
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), None);

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
            collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), None, &config);

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
            collect_diagnostics_with_config(source, &[], &symbols, Some(&stmts), None, &config);

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
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), None);

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
        let diagnostics = collect_diagnostics("", &[], &symbols, Some(&stmts), None);

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
            collect_diagnostics_with_config(source, &parse_errors, &symbols, None, None, &config);

        let errors: Vec<_> =
            diagnostics.iter().filter(|d| d.severity == DiagnosticSeverity::Error).collect();
        assert_eq!(errors.len(), 1, "errors should not be suppressed");
    }
}
