//! Canonical semantic diagnostics emitted from the syntax layer.
//!
//! The analyzer and LSP convert these into their transport DTOs instead of
//! re-implementing label/property/type checks.

use std::collections::HashSet;

use crate::ast::{is_array_member_label, InlineItem, Span, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticLocation, DiagnosticPhase, DiagnosticSeverity};
use crate::symbol_table::{LabelKind, SymbolTable};
use crate::walk;

/// Built-in container types whose label may be purely structural.
const STRUCTURAL_CONTAINER_TYPES: &[&str] =
    &["Row", "Col", "Grid", "Stack", "Group", "Filter", "Mask"];

/// Collect semantic diagnostics for a parsed program.
///
/// This is the single emitter for analyzer-style lint checks. Build/typecheck
/// diagnostics remain produced by the typechecker and timeline build.
pub fn collect_semantic_diagnostics(
    stmts: &[Stmt],
    symbols: &SymbolTable,
    tree: Option<&tree_sitter::Tree>,
    source: &str,
) -> Vec<Diagnostic> {
    let structural_containers = collect_structural_container_labels(stmts);
    let mut diagnostics = Vec::new();

    let mut seen_labels = HashSet::new();
    for (name, info) in &symbols.labels {
        if !seen_labels.insert(name) {
            diagnostics.push(span_diagnostic(
                DiagnosticSeverity::Warning,
                DiagnosticCode::DuplicateLabel,
                format!("Duplicate label: {}", name),
                info.line,
                info.col,
                info.col + name.len(),
            ));
        }
    }

    for (name, info) in &symbols.labels {
        if !symbols.referenced_labels.contains(name) {
            if info.kind == LabelKind::For
                || info.kind == LabelKind::Always
                || symbols.array_labels.contains(name)
                || symbols.component_internal_labels.contains(name)
                || structural_containers.contains(name)
            {
                continue;
            }
            diagnostics.push(span_diagnostic(
                DiagnosticSeverity::Warning,
                DiagnosticCode::UnusedLabel,
                format!(
                    "Unused {}: '{}'",
                    match info.kind {
                        LabelKind::Actor => "actor",
                        LabelKind::Let => "binding",
                        LabelKind::Component => "component",
                        _ => "label",
                    },
                    name
                ),
                info.line,
                info.col,
                info.col + name.len(),
            ));
        }
    }

    walk::walk_stmts(stmts, &mut |stmt| {
        check_stmt(stmt, symbols, tree, source, &mut diagnostics);
    });

    diagnostics
}

fn span_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    line: usize,
    col: usize,
    end_col: usize,
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(line),
            column: Some(col),
            end_line: Some(line),
            end_col: Some(end_col),
            span: None,
            path: None,
            subject: None,
        },
    }
}

fn tree_range_diagnostic(
    severity: DiagnosticSeverity,
    code: DiagnosticCode,
    message: String,
    start: tree_sitter::Point,
    end: tree_sitter::Point,
) -> Diagnostic {
    Diagnostic {
        severity,
        phase: DiagnosticPhase::Build,
        code,
        message,
        location: DiagnosticLocation {
            line: Some(start.row + 1),
            column: Some(start.column + 1),
            end_line: Some(end.row + 1),
            end_col: Some(end.column + 1),
            span: None,
            path: None,
            subject: None,
        },
    }
}

fn span_positions(span: &Option<Span>) -> (usize, usize, usize, usize) {
    match span {
        Some(s) => (s.start_line, s.start_col, s.end_line, s.end_col),
        None => (1, 1, 1, 1),
    }
}

fn is_component_array_member(symbols: &SymbolTable, label: &str) -> bool {
    let Some(base) = is_array_member_label(label) else {
        return false;
    };
    let Some((instance, _)) = base.rsplit_once('.') else {
        return false;
    };
    let Some(info) = symbols.labels.get(instance) else {
        return false;
    };
    info.ty.as_deref().is_some_and(|ty| symbols.components.contains_key(ty))
}

fn collect_structural_container_labels(stmts: &[Stmt]) -> HashSet<String> {
    let mut structural = HashSet::new();
    crate::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl {
            label,
            ty,
            children,
            ..
        } = stmt
        {
            if !children.is_empty() && STRUCTURAL_CONTAINER_TYPES.contains(&ty.as_str()) {
                structural.insert(label.clone());
            }
            crate::walk::walk_inline_items(children, &mut |item| {
                if let InlineItem::Labeled {
                    label,
                    ty,
                    children,
                    ..
                } = item
                {
                    if !children.is_empty() && STRUCTURAL_CONTAINER_TYPES.contains(&ty.as_str()) {
                        structural.insert(label.clone());
                    }
                }
            });
        }
    });
    structural
}

fn find_token_range(
    node: tree_sitter::Node,
    source: &str,
    kind: &str,
    text: &str,
) -> Option<(tree_sitter::Point, tree_sitter::Point)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            if let Ok(node_text) = child.utf8_text(source.as_bytes()) {
                if node_text == text {
                    return Some((child.start_position(), child.end_position()));
                }
            }
        }
        if let Some(found) = find_token_range(child, source, kind, text) {
            return Some(found);
        }
    }
    None
}

fn check_stmt(
    stmt: &Stmt,
    symbols: &SymbolTable,
    tree: Option<&tree_sitter::Tree>,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Action(action, span) => {
            let (line, col, end_line, end_col) = span_positions(span);

            if !symbols.actions.contains(&action.verb) {
                let (vstart, vend) = tree
                    .and_then(|t| find_token_range(t.root_node(), source, "action_verb", &action.verb))
                    .unwrap_or_else(|| tree_sitter::Point::new(line - 1, col - 1).pair_with_point(tree_sitter::Point::new(end_line - 1, end_col - 1)));
                diagnostics.push(tree_range_diagnostic(
                    DiagnosticSeverity::Warning,
                    DiagnosticCode::UnknownAction,
                    format!("Unknown action: {}", action.verb),
                    vstart,
                    vend,
                ));
            }

            for target in &action.targets {
                let is_defined = symbols.labels.contains_key(target)
                    || is_array_member_label(target)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, target);
                if !is_defined {
                    let (tstart, tend) = tree
                        .and_then(|t| find_token_range(t.root_node(), source, "identifier", target))
                        .unwrap_or_else(|| tree_sitter::Point::new(line - 1, col - 1).pair_with_point(tree_sitter::Point::new(end_line - 1, end_col - 1)));
                    diagnostics.push(tree_range_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", target),
                        tstart,
                        tend,
                    ));
                }
            }
        },

        Stmt::Assignment {
            target,
            property,
            value,
            span,
            ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);

            if let Some(seg) = target.first() {
                let label = seg.label_str();
                let is_defined = symbols.labels.contains_key(label)
                    || is_array_member_label(label)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, label);
                if !is_defined {
                    diagnostics.push(span_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", label),
                        line,
                        col,
                        end_col,
                    ));
                }
            }

            if let Some(seg) = target.first() {
                let label = seg.label_str();
                if let Some(info) = symbols.labels.get(label) {
                    if let Some(ty) = &info.ty {
                        if let Some(known_props) = symbols.properties.get(ty) {
                            if !known_props.contains(property) {
                                diagnostics.push(span_diagnostic(
                                    DiagnosticSeverity::Info,
                                    DiagnosticCode::UnknownProperty,
                                    format!(
                                        "Property '{}' not commonly used on {} (may still be valid)",
                                        property, ty
                                    ),
                                    line,
                                    col,
                                    end_col,
                                ));
                            }

                            let key = (ty.clone(), property.clone());
                            if let Some(expected_type) = symbols.property_types.get(&key) {
                                let actual_type = symbols.infer_expr_type(value);
                                if !crate::typing::is_subtype(&actual_type, expected_type) {
                                    diagnostics.push(span_diagnostic(
                                        DiagnosticSeverity::Warning,
                                        DiagnosticCode::TypeMismatch,
                                        format!(
                                            "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                            label, property, expected_type, actual_type
                                        ),
                                        line,
                                        col,
                                        end_col,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        },

        Stmt::ActorDecl {
            ty, props, span, ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);

            if !symbols.types.contains(ty) {
                diagnostics.push(span_diagnostic(
                    DiagnosticSeverity::Warning,
                    DiagnosticCode::UnknownType,
                    format!("Unknown type: {}", ty),
                    line,
                    col,
                    end_col,
                ));
            }

            if let Some(known_props) = symbols.properties.get(ty) {
                for prop in props {
                    if !known_props.contains(&prop.name) {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Info,
                            DiagnosticCode::UnknownProperty,
                            format!(
                                "Property '{}' not commonly used on {} (may still be valid)",
                                prop.name, ty
                            ),
                            line,
                            col,
                            end_col,
                        ));
                    }

                    let key = (ty.clone(), prop.name.clone());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = symbols.infer_expr_type(&prop.value);
                        if !crate::typing::is_subtype(&actual_type, expected_type) {
                            diagnostics.push(span_diagnostic(
                                DiagnosticSeverity::Warning,
                                DiagnosticCode::TypeMismatch,
                                format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    ty, prop.name, expected_type, actual_type
                                ),
                                line,
                                col,
                                end_col,
                            ));
                        }
                    }
                }
            }
        },

        _ => {},
    }
}

trait PairPoint {
    fn pair_with_point(self, end: tree_sitter::Point) -> (tree_sitter::Point, tree_sitter::Point);
}

impl PairPoint for tree_sitter::Point {
    fn pair_with_point(self, end: tree_sitter::Point) -> (tree_sitter::Point, tree_sitter::Point) {
        (self, end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unused_labels(stmts: &[Stmt]) -> Vec<String> {
        let symbols = SymbolTable::build_from_ast(stmts);
        collect_semantic_diagnostics(stmts, &symbols, None, "")
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::UnusedLabel)
            .map(|d| d.message.clone())
            .collect()
    }

    #[test]
    fn structural_container_with_children_is_not_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "cards".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "card".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(
            !labels.iter().any(|m| m.contains("'cards'")),
            "structural container label should be exempt: {labels:?}"
        );
    }

    #[test]
    fn empty_container_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "holder".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("holder")));
    }

    #[test]
    fn non_container_with_children_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "wrapper".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "child".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("wrapper")));
    }
}
