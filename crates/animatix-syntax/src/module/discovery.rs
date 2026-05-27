//! AST discovery helpers for collecting definitions and imports.

use super::{ComponentDef, Import, Stmt};
use std::collections::HashMap;

pub(super) fn collect_imports(statements: &[Stmt]) -> Vec<Import> {
    let mut imports = Vec::new();
    for stmt in statements {
        collect_imports_from_stmt(stmt, &mut imports);
    }
    imports
}

fn collect_imports_from_stmt(stmt: &Stmt, imports: &mut Vec<Import>) {
    match stmt {
        Stmt::Import { path, alias, .. } => imports.push(Import { path: path.clone(), alias: alias.clone() }),
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. } => {
            for stmt in body {
                collect_imports_from_stmt(stmt, imports);
            }
        }
        _ => {}
    }
}

pub(super) fn strip_imports(stmt: &Stmt) -> Option<Stmt> {
    match stmt {
        Stmt::Import { .. } => None,
        Stmt::Keyframe { time, body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Keyframe {
                    time: time.clone(),
                    body,
                    span: None,
                })
            }
        }
        Stmt::RelativeKeyframe { offset, body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::RelativeKeyframe {
                    offset: offset.clone(),
                    body,
                    span: None,
                })
            }
        }
        Stmt::Sequence { body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Sequence { body, span: None })
            }
        }
        Stmt::Stagger { modifiers, body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Stagger {
                    modifiers: modifiers.clone(),
                    body,
                    span: None,
                })
            }
        }
        _ => Some(stmt.clone()),
    }
}

/// Collect all component definitions from a slice of statements.
pub fn collect_component_defs(statements: &[Stmt]) -> Vec<ComponentDef> {
    let mut definitions = Vec::new();
    for stmt in statements {
        collect_component_defs_from_stmt(stmt, &mut definitions);
    }
    definitions
}

fn collect_component_defs_from_stmt(stmt: &Stmt, definitions: &mut Vec<ComponentDef>) {
    match stmt {
        Stmt::ComponentDef(definition, ..) => definitions.push(definition.clone()),
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. } => {
            for stmt in body {
                collect_component_defs_from_stmt(stmt, definitions);
            }
        }
        _ => {}
    }
}

/// Collect custom action templates from a component definition body.
/// Returns a map of action_name → action template.
pub fn collect_component_actions(
    definition: &ComponentDef,
) -> HashMap<String, crate::module::ActionTemplate> {
    let mut actions = HashMap::new();
    for stmt in &definition.body {
        if let Stmt::ComponentAction { name, params, body, .. } = stmt {
            actions.insert(
                name.clone(),
                crate::module::ActionTemplate {
                    params: params.clone(),
                    body: body.clone(),
                },
            );
        }
    }
    actions
}
