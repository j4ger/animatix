//! Diagnostic collection from parse errors and semantic checks.

use crate::symbol_table::SymbolTable;
use animatix_syntax::ast::*;
use animatix_syntax::parser::ParseError;
use std::collections::HashSet;
use tree_sitter::Tree;

/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub line: usize,
    pub col: usize,
    pub end_line: usize,
    pub end_col: usize,
    pub message: String,
    pub code: Option<String>,
}

/// The severity of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Collect all diagnostics from the source.
pub fn collect_diagnostics(
    source: &str,
    parse_errors: &[ParseError],
    tree: Option<&Tree>,
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Tree-sitter ERROR/MISSING nodes (syntax errors)
    if let Some(tree) = tree {
        collect_ts_errors(tree.root_node(), source, &mut diagnostics);
    }

    // 2. Chumsky parse errors (structured with positions)
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

    // 3. Semantic checks (if AST is available)
    if let Some(stmts) = ast {
        collect_semantic_diagnostics(stmts, symbols, &mut diagnostics);
    }

    diagnostics
}

/// Collect syntax errors from tree-sitter ERROR/MISSING nodes.
fn collect_ts_errors(node: tree_sitter::Node, source: &str, diagnostics: &mut Vec<Diagnostic>) {
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        let end = node.end_position();

        let message = if node.is_missing() {
            format!("Missing {}", node.kind())
        } else {
            // Try to provide a more helpful error message
            let text = &source[node.byte_range()];
            if text.contains('@') {
                "Unexpected character '@'".to_string()
            } else if text.contains("!!") {
                "Unexpected token".to_string()
            } else {
                format!("Syntax error near '{}'", text.chars().take(20).collect::<String>())
            }
        };

        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            line: start.row,
            col: start.column,
            end_line: end.row,
            end_col: end.column,
            message,
            code: Some("syntax".to_string()),
        });
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_ts_errors(child, source, diagnostics);
    }
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
        let diagnostics = collect_diagnostics("", &[], None, &SymbolTable::default(), None);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn tree_sitter_errors_detected() {
        let source = "title: Text { content: }";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_animatix::language()).unwrap();
        let tree = parser.parse(source, None);

        let diagnostics = collect_diagnostics(source, &[], tree.as_ref(), &SymbolTable::default(), None);
        assert!(!diagnostics.is_empty());
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
        let diagnostics = collect_diagnostics("", &[], None, &symbols, Some(&stmts));

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
        let diagnostics = collect_diagnostics("", &[], None, &symbols, Some(&stmts));

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
        let diagnostics = collect_diagnostics("", &[], None, &symbols, Some(&stmts));

        let unknown_types: Vec<_> = diagnostics.iter()
            .filter(|d| d.code.as_deref() == Some("unknown-type"))
            .collect();
        assert_eq!(unknown_types.len(), 1);
    }
}
