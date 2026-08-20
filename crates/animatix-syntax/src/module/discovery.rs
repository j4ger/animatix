//! AST discovery helpers for collecting definitions and imports.

use std::collections::HashMap;

use super::{ComponentDef, Stmt};

pub(super) fn collect_imports(statements: &[Stmt]) -> Vec<(String, Option<String>)> {
    let mut imports = Vec::new();
    crate::walk::walk_stmts(statements, &mut |stmt| {
        if let Stmt::Import { path, alias, .. } = stmt {
            imports.push((path.clone(), alias.clone()));
        }
    });
    imports
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
        },
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
        },
        Stmt::Sequence { body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Sequence { body, span: None })
            }
        },
        Stmt::Stagger {
            modifiers, body, ..
        } => {
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
        },
        Stmt::Always { body, .. } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            Some(Stmt::Always { body, span: None })
        },
        Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            ..
        } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            Some(Stmt::FnDecl {
                is_pub: *is_pub,
                name: name.clone(),
                params: params.clone(),
                return_type: return_type.clone(),
                body,
                span: None,
            })
        },
        Stmt::ComponentDef(def, _) => {
            let body = def.body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            Some(Stmt::ComponentDef(
                ComponentDef {
                    is_pub: def.is_pub,
                    name: def.name.clone(),
                    params: def.params.clone(),
                    body,
                },
                None,
            ))
        },
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            ..
        } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            Some(Stmt::ForLoop {
                var: var.clone(),
                index_var: index_var.clone(),
                iterable: iterable.clone(),
                body,
                modifiers: modifiers.clone(),
                span: None,
            })
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let then_branch = then_branch.iter().filter_map(strip_imports).collect::<Vec<_>>();
            let else_branch = else_branch
                .as_ref()
                .map(|b| b.iter().filter_map(strip_imports).collect::<Vec<_>>());
            Some(Stmt::Conditional {
                condition: condition.clone(),
                then_branch,
                else_branch,
                span: None,
            })
        },
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            let arms = arms
                .iter()
                .map(|(pat, body)| {
                    (pat.clone(), body.iter().filter_map(strip_imports).collect::<Vec<_>>())
                })
                .collect();
            Some(Stmt::Match {
                scrutinee: scrutinee.clone(),
                arms,
                span: None,
            })
        },
        _ => Some(stmt.clone()),
    }
}

/// Collect all component definitions from a slice of statements.
pub fn collect_component_defs(statements: &[Stmt]) -> Vec<ComponentDef> {
    let mut definitions = Vec::new();
    crate::walk::walk_stmts(statements, &mut |stmt| {
        if let Stmt::ComponentDef(definition, ..) = stmt {
            definitions.push(definition.clone());
        }
    });
    definitions
}

/// Extract the file prelude: top-level statements before the first `Stmt::Scene`,
/// with `Stmt::Import` stripped out.
pub fn collect_prelude_stmts(stmts: &[Stmt]) -> Vec<Stmt> {
    let mut prelude = Vec::new();
    for stmt in stmts {
        if matches!(stmt, Stmt::Scene { .. }) {
            break;
        }
        if let Some(cleaned) = strip_imports(stmt) {
            prelude.push(cleaned);
        }
    }
    prelude
}

/// Collect scene definitions from top-level statements, attaching the file
/// prelude (shared context) to each scene.
pub fn collect_scenes_from_stmts(stmts: &[Stmt]) -> HashMap<String, super::SceneData> {
    let prelude = collect_prelude_stmts(stmts);
    let mut scenes = HashMap::new();
    for stmt in stmts {
        if let Stmt::Scene {
            name,
            config,
            body,
            span,
        } = stmt
        {
            scenes.insert(
                name.clone(),
                super::SceneData {
                    name: name.clone(),
                    config: config.clone(),
                    body: body.clone(),
                    file_prelude: prelude.clone(),
                    span: *span,
                },
            );
        }
    }
    scenes
}

/// Collect custom action templates from a component definition body.
/// Returns a map of action_name → action template.
pub fn collect_component_actions(
    definition: &ComponentDef,
) -> HashMap<String, crate::module::ActionTemplate> {
    let mut actions = HashMap::new();
    for stmt in &definition.body {
        if let Stmt::FnDecl {
            name,
            params,
            return_type,
            body,
            ..
        } = stmt
        {
            actions.insert(
                name.clone(),
                crate::module::ActionTemplate {
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                },
            );
        }
    }
    actions
}
