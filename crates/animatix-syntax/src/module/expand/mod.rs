use super::rewrite::rewrite_stmt;
use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, InstanceFnRegistry, MatchPattern, ParamDef,
    Property, Stmt, TargetSegment,
};
use context::{ExpansionCtx, MAX_EXPANSION_DEPTH};
use std::path::PathBuf;
use std::sync::Arc;

mod context;
pub(super) use context::expand_function_context;

#[cfg(test)]
pub(super) fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> crate::module::ExpandedProgram {
    let mut diagnostics = Vec::new();
    let output =
        expand_statements_in_context(statements, components, ExpansionCtx::new(), &mut diagnostics);
    finish_expansion(output, diagnostics).expect("fixture component expansion")
}

pub(super) fn expand_statements_with_component_scopes(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    scopes: &HashMap<PathBuf, HashMap<String, ComponentEntry>>,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) -> crate::module::ExpandedProgram {
    let scopes = scopes
        .iter()
        .map(|(path, components)| (path.clone(), Arc::new(components.clone())))
        .collect();
    expand_statements_in_context(
        statements,
        components,
        ExpansionCtx::with_component_scopes(scopes),
        diagnostics,
    )
}

fn expand_statements_in_context(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    mut ctx: ExpansionCtx,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
) -> crate::module::ExpandedProgram {
    let mut expanded = crate::module::ExpandedProgram::default();
    for stmt in statements {
        if let Stmt::Scene { name, .. } = stmt {
            let registry = expanded.scene_methods.entry(name.clone()).or_default();
            expand_stmt_into(stmt, components, &mut expanded.statements, registry, &mut ctx);
        } else {
            expand_stmt_into(
                stmt,
                components,
                &mut expanded.statements,
                &mut expanded.instance_methods,
                &mut ctx,
            );
        }
    }
    diagnostics.extend(ctx.diagnostics);
    expanded
}

use component::*;
use slots::*;
use stmt::*;

mod component;
mod slots;
mod stmt;
#[cfg(test)]
mod tests;

pub(crate) use component::INSTANCE_FORWARDED_PROPS;

/// Complete expansion atomically; never hand partial statements to a host.
pub(super) fn finish_expansion(
    output: crate::module::ExpandedProgram,
    diagnostics: Vec<crate::diagnostics::Diagnostic>,
) -> Result<crate::module::ExpandedProgram, crate::module::ExpansionError> {
    // The same function can appear in several lexical/exported contexts.
    // Unlike generic BuildReport deduplication, preserve distinct source paths.
    let mut unique = Vec::new();
    for diagnostic in diagnostics {
        if !unique.contains(&diagnostic) {
            unique.push(diagnostic);
        }
    }
    if unique.is_empty() {
        Ok(output)
    } else {
        Err(crate::module::ExpansionError {
            diagnostics: unique,
        })
    }
}
