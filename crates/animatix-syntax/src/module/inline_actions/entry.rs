use std::collections::HashMap;

use crate::ast::Stmt;

use super::cycles::detect_fn_cycles;
use super::expand::expand_stmt_list;
use super::{FnTemplate, InstanceFnRegistry};

/// Replace timeline-function call statements with scoped blocks.
///
/// `pulse btn [strength: 1.3]` and `highlight_key(bars, key)` expand into a
/// `Stmt::Block` containing parameter `let` bindings followed by the callee
/// body, with `self` rewritten to the concrete target. Block scope keeps
/// function-local `let` bindings from leaking into the scene. Nested calls
/// inside the expanded body are expanded recursively (recursion is rejected
/// by a cycle guard that leaves the offending call in place).
///
/// Pure functions (`return_type: Some(_)`) are never expanded here; their
/// calls are evaluated at runtime as expressions.
pub(in crate::module) fn expand_fn_calls(
    stmts: Vec<Stmt>,
    registry: &InstanceFnRegistry,
    module_fns: &HashMap<String, FnTemplate>,
    diagnostics: &mut Vec<String>,
) -> Vec<Stmt> {
    let cycle_members = detect_fn_cycles(module_fns, diagnostics);
    let mut stack: Vec<String> = Vec::new();
    expand_stmt_list(stmts, registry, module_fns, &cycle_members, &mut stack, diagnostics).0
}
