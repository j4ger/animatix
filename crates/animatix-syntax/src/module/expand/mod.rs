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
    expand_statements_in_context(statements, components, ExpansionCtx::new())
}

pub(super) fn expand_statements_with_component_scopes(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    scopes: &HashMap<PathBuf, HashMap<String, ComponentEntry>>,
) -> crate::module::ExpandedProgram {
    let scopes = scopes
        .iter()
        .map(|(path, components)| (path.clone(), Arc::new(components.clone())))
        .collect();
    expand_statements_in_context(
        statements,
        components,
        ExpansionCtx::with_component_scopes(scopes),
    )
}

fn expand_statements_in_context(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    mut ctx: ExpansionCtx,
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
