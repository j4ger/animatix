//! AST discovery helpers for collecting definitions and imports.

use std::collections::HashMap;
use std::path::Path;

use super::{ComponentDef, Stmt};
use crate::visit::{AstFolder, walk_fold_stmt};

pub(super) fn collect_imports(statements: &[Stmt]) -> Vec<(String, Option<String>)> {
    let mut imports = Vec::new();
    crate::walk::walk_stmts(statements, &mut |stmt| {
        if let Stmt::Import { path, alias, .. } = stmt {
            imports.push((path.clone(), alias.clone()));
        }
    });
    imports
}

/// Drops `Stmt::Import` from a statement tree.
struct ImportStripper {
    preserve_keyframe_spans: bool,
}

impl AstFolder for ImportStripper {
    fn fold_stmts(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
        stmts
            .into_iter()
            .filter(|stmt| !matches!(stmt, Stmt::Import { .. }))
            .filter_map(|stmt| {
                let folded = self.fold_stmt(stmt);
                (!drops_when_empty(&folded)).then_some(folded)
            })
            .collect()
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            // The prelude and flatten passes never rewrite scenes, and `Block`
            // is synthesized after parsing, so neither is traversed.
            Stmt::Scene { .. } | Stmt::Block { .. } => stmt,
            other => {
                let mut folded = walk_fold_stmt(self, other);
                clear_span_for_rebuilt(&mut folded, self.preserve_keyframe_spans);
                folded
            },
        }
    }
}

/// An import-only body leaves nothing behind for these container variants.
fn drops_when_empty(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            if body.is_empty()
    )
}

/// Rebuilt containers lose their position data, matching the previous
/// hand-written rebuild so diagnostics never point at stale offsets.
fn clear_span_for_rebuilt(stmt: &mut Stmt, preserve_keyframe_spans: bool) {
    match stmt {
        Stmt::Keyframe { span, .. } | Stmt::RelativeKeyframe { span, .. }
            if !preserve_keyframe_spans =>
        {
            *span = None
        },
        Stmt::Sequence { span, .. }
        | Stmt::Stagger { span, .. }
        | Stmt::Always { span, .. }
        | Stmt::ComponentDef(_, span)
        | Stmt::FnDecl { span, .. }
        | Stmt::ForLoop { span, .. }
        | Stmt::Conditional { span, .. }
        | Stmt::Match { span, .. } => *span = None,
        _ => {},
    }
}

/// Remove imports from a statement, dropping a container left empty.
pub(super) fn strip_imports(stmt: &Stmt) -> Option<Stmt> {
    strip_imports_with_keyframe_spans(stmt, false)
}

/// Strip imports from an entry file while retaining its keyframe locations.
/// Imported statements are kept spanless when flattened because their source
/// coordinates belong to a different file.
pub(super) fn strip_imports_preserving_keyframe_spans(stmt: &Stmt) -> Option<Stmt> {
    strip_imports_with_keyframe_spans(stmt, true)
}

fn strip_imports_with_keyframe_spans(stmt: &Stmt, preserve_keyframe_spans: bool) -> Option<Stmt> {
    if matches!(stmt, Stmt::Import { .. }) {
        return None;
    }
    let folded = ImportStripper {
        preserve_keyframe_spans,
    }
    .fold_stmt(stmt.clone());
    (!drops_when_empty(&folded)).then_some(folded)
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
                    definition_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    scenes
}

/// Collect timeline-function templates from a component definition body.
/// Returns a map of fn_name → function template.
pub fn collect_component_functions(
    definition: &ComponentDef,
    definition_path: &Path,
) -> HashMap<String, crate::module::FnTemplate> {
    let mut functions = HashMap::new();
    for stmt in &definition.body {
        if let Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            span,
        } = stmt
        {
            functions.insert(
                name.clone(),
                crate::module::FnTemplate {
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                    is_pub: *is_pub,
                    definition_path: definition_path.to_path_buf(),
                    span: *span,
                    instance_index: None,
                    component_captures: Vec::new(),
                },
            );
        }
    }
    functions
}
