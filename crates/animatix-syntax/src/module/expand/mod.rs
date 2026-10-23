use super::rewrite::rewrite_stmt;
use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, InstanceFnRegistry, MatchPattern, ParamDef,
    Property, Stmt, TargetSegment,
};

/// Maximum nesting depth for component expansion before reporting a cycle error.
const MAX_EXPANSION_DEPTH: usize = 64;

/// Mutable state threaded through the component expansion pipeline.
struct ExpansionCtx {
    /// Monotonically increasing counter for generating unique anonymous labels.
    anon_counter: usize,
    /// Current component nesting depth (for cycle detection).
    depth: usize,
}

impl ExpansionCtx {
    fn new() -> Self {
        Self {
            anon_counter: 0,
            depth: 0,
        }
    }

    /// Generate a unique label for an anonymous component instance.
    fn next_anon_label(&mut self) -> String {
        let label = format!("__anon_{}", self.anon_counter);
        self.anon_counter += 1;
        label
    }
}

pub(super) fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> (Vec<Stmt>, InstanceFnRegistry) {
    let mut ctx = ExpansionCtx::new();
    let mut expanded = Vec::new();
    let mut registry = InstanceFnRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry, &mut ctx);
    }
    (expanded, registry)
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
