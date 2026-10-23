use std::collections::HashMap;

use crate::ast::{Action, Expr, Modifier, Stmt, TargetSegment, array_actor_label};
use crate::module::{FnTemplate, InstanceFnRegistry};

// NOTE: This module takes ownership of Vec<Stmt> and produces a new
// Vec<Stmt> (owned tree transformation with potential 1→N expansion).

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
pub(super) fn expand_fn_calls(
    stmts: Vec<Stmt>,
    registry: &InstanceFnRegistry,
    module_fns: &HashMap<String, FnTemplate>,
    diagnostics: &mut Vec<String>,
) -> Vec<Stmt> {
    let cycle_members = detect_fn_cycles(module_fns, diagnostics);
    let mut stack: Vec<String> = Vec::new();
    expand_stmt_list(stmts, registry, module_fns, &cycle_members, &mut stack, diagnostics).0
}

/// Detect recursive timeline-function call cycles before expansion.
///
/// The expander flattens nested calls across re-scan passes, so a runtime
/// call-stack guard cannot see cycles; report them up front instead.
fn detect_fn_cycles(
    module_fns: &HashMap<String, FnTemplate>,
    diagnostics: &mut Vec<String>,
) -> std::collections::HashSet<String> {
    fn called_names(stmt: &Stmt, out: &mut Vec<String>) {
        match stmt {
            Stmt::Action(action, ..) => out.push(action.verb.clone()),
            Stmt::LetDecl { value, .. } => collect_calls_in_expr(value, out),
            Stmt::Assignment { value, .. } => collect_calls_in_expr(value, out),
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_calls_in_expr(condition, out);
                for s in then_branch {
                    called_names(s, out);
                }
                if let Some(eb) = else_branch {
                    for s in eb {
                        called_names(s, out);
                    }
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_calls_in_expr(scrutinee, out);
                for (_, body) in arms {
                    for s in body {
                        called_names(s, out);
                    }
                }
            },
            Stmt::ForLoop { iterable, body, .. } => {
                collect_calls_in_expr(iterable, out);
                for s in body {
                    called_names(s, out);
                }
            },
            Stmt::Block { body, .. } => {
                for s in body {
                    called_names(s, out);
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::FnDecl { body, .. }
            | Stmt::Scene { body, .. } => {
                for s in body {
                    called_names(s, out);
                }
            },
            _ => {},
        }
    }
    fn collect_calls_in_expr(expr: &crate::ast::Expr, out: &mut Vec<String>) {
        match expr {
            crate::ast::Expr::Call(name, args) => {
                out.push(name.clone());
                for arg in args {
                    collect_calls_in_expr(arg, out);
                }
            },
            crate::ast::Expr::Tuple(items) | crate::ast::Expr::List(items) => {
                for item in items {
                    collect_calls_in_expr(item, out);
                }
            },
            crate::ast::Expr::Binary(l, _, r) => {
                collect_calls_in_expr(l, out);
                collect_calls_in_expr(r, out);
            },
            crate::ast::Expr::Unary(_, v) => collect_calls_in_expr(v, out),
            crate::ast::Expr::Method(receiver, _, args) => {
                collect_calls_in_expr(receiver, out);
                for arg in args {
                    collect_calls_in_expr(arg, out);
                }
            },
            crate::ast::Expr::Closure(_, body) => collect_calls_in_expr(body, out),
            crate::ast::Expr::LetChain(bindings, tail) => {
                for (_, value) in bindings {
                    collect_calls_in_expr(value, out);
                }
                collect_calls_in_expr(tail, out);
            },
            crate::ast::Expr::Conditional(c, t, e) => {
                collect_calls_in_expr(c, out);
                collect_calls_in_expr(t, out);
                collect_calls_in_expr(e, out);
            },
            crate::ast::Expr::Match(scrutinee, arms) => {
                collect_calls_in_expr(scrutinee, out);
                for (_, arm) in arms {
                    collect_calls_in_expr(arm, out);
                }
            },
            crate::ast::Expr::Construct(_, props) => {
                for prop in props {
                    collect_calls_in_expr(&prop.value, out);
                }
            },
            crate::ast::Expr::Index(target, index) => {
                collect_calls_in_expr(target, out);
                collect_calls_in_expr(index, out);
            },
            _ => {},
        }
    }

    // Build adjacency: fn -> fns it calls (only timeline fns call other fns).
    let mut graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (name, template) in module_fns {
        if template.return_type.is_some() {
            continue;
        }
        let mut calls = Vec::new();
        for stmt in &template.body {
            called_names(stmt, &mut calls);
        }
        graph.insert(name.clone(), calls);
    }
    // DFS cycle detection (white/gray/black).
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut colors: std::collections::HashMap<&str, Color> = std::collections::HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycle_members: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn visit<'a>(
        name: &'a str,
        graph: &'a std::collections::HashMap<String, Vec<String>>,
        colors: &mut std::collections::HashMap<&'a str, Color>,
        stack: &mut Vec<String>,
        cycle_members: &mut std::collections::HashSet<String>,
        diagnostics: &mut Vec<String>,
    ) {
        colors.insert(name, Color::Gray);
        stack.push(name.to_string());
        if let Some(callees) = graph.get(name) {
            for callee in callees {
                match colors.get(callee.as_str()).unwrap_or(&Color::White) {
                    Color::Gray => {
                        let cycle_start = stack.iter().position(|s| s == callee).unwrap_or(0);
                        let cycle = stack[cycle_start..].join(" -> ");
                        diagnostics.push(format!(
                            "recursive timeline-function cycle: {cycle} -> {callee}; recursion is not supported"
                        ));
                        for member in &stack[cycle_start..] {
                            cycle_members.insert(member.clone());
                        }
                        cycle_members.insert(callee.clone());
                    },
                    Color::White => visit(callee, graph, colors, stack, cycle_members, diagnostics),
                    Color::Black => {},
                }
            }
        }
        stack.pop();
        colors.insert(name, Color::Black);
    }
    for name in graph.keys() {
        if matches!(colors.get(name.as_str()).unwrap_or(&Color::White), Color::White) {
            visit(name, &graph, &mut colors, &mut stack, &mut cycle_members, diagnostics);
        }
    }
    cycle_members
}

use expand::*;
use params::*;
use substitute::*;

mod expand;
mod params;
mod substitute;
#[cfg(test)]
mod tests;
