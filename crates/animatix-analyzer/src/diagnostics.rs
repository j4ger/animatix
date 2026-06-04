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

/// Collect all diagnostics from the source.
pub fn collect_diagnostics(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
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
        collect_semantic_diagnostics(stmts, symbols, &mut diagnostics);
    }

    diagnostics
}

/// Collect semantic diagnostics from the AST.
fn collect_semantic_diagnostics(
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
        check_stmt(stmt, symbols, diagnostics);
    }
}

/// Check a single statement for semantic issues.
fn check_stmt(stmt: &Stmt, symbols: &SymbolTable, diagnostics: &mut Vec<Diagnostic>) {
    match stmt {
        Stmt::Action(action, ..) => {
            // Check if action verb is known
            if !symbols.actions.contains(&action.verb) {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    line: 0,
                    col: 0,
                    end_line: 0,
                    end_col: 0,
                    message: format!("Unknown action: {}", action.verb),
                    code: Some("unknown-action".to_string()),
                });
            }

            // Check if target labels exist
            for target in &action.targets {
                if !symbols.labels.contains_key(target) {
                    diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        line: 0,
                        col: 0,
                        end_line: 0,
                        end_col: 0,
                        message: format!("Undefined label: {}", target),
                        code: Some("undefined-label".to_string()),
                    });
                }
            }
        }

        Stmt::Assignment { target, property, .. } => {
            // Check if target label exists
            if let Some(label) = target.first() {
                if !symbols.labels.contains_key(label) {
                    diagnostics.push(Diagnostic {
                        severity: DiagnosticSeverity::Warning,
                        line: 0,
                        col: 0,
                        end_line: 0,
                        end_col: 0,
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
                                    line: 0,
                                    col: 0,
                                    end_line: 0,
                                    end_col: 0,
                                    message: format!(
                                        "Property '{}' not commonly used on {} (may still be valid)",
                                        property, ty
                                    ),
                                    code: Some("unknown-property".to_string()),
                                });
                            }
                        }
                    }
                }
            }
        }

        Stmt::ActorDecl { ty, props, .. } => {
            // Check if type is known
            if !symbols.types.contains(ty) {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    line: 0,
                    col: 0,
                    end_line: 0,
                    end_col: 0,
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
                            line: 0,
                            col: 0,
                            end_line: 0,
                            end_col: 0,
                            message: format!(
                                "Property '{}' not commonly used on {} (may still be valid)",
                                prop.name, ty
                            ),
                            code: Some("unknown-property".to_string()),
                        });
                    }
                }
            }
        }

        // Recurse into blocks
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            for stmt in body {
                check_stmt(stmt, symbols, diagnostics);
            }
        }
        Stmt::Sequence { body, .. } | Stmt::Stagger { body, .. } | Stmt::Always { body, .. } => {
            for stmt in body {
                check_stmt(stmt, symbols, diagnostics);
            }
        }
        Stmt::Conditional { then_branch, else_branch, .. } => {
            for stmt in then_branch {
                check_stmt(stmt, symbols, diagnostics);
            }
            if let Some(else_stmts) = else_branch {
                for stmt in else_stmts {
                    check_stmt(stmt, symbols, diagnostics);
                }
            }
        }
        Stmt::ForLoop { body, .. } => {
            for stmt in body {
                check_stmt(stmt, symbols, diagnostics);
            }
        }
        Stmt::ComponentDef(def, ..) => {
            for stmt in &def.body {
                check_stmt(stmt, symbols, diagnostics);
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
}
