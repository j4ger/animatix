//! Optimization passes for the modifier IR.
//!
//! Provides constant folding, dead branch elimination, constant propagation,
//! and dead let-binding elimination for both expressions and statements.

use std::collections::{HashMap, HashSet};

use super::types::{BuiltinFn, CompiledExpr, ModifierIrProgram, ModifierIrStmt};
use crate::ast::{LoopPattern, UnaryOp};
use crate::timeline::Value;
use crate::timeline::eval_shared::{eval_binary_op, eval_builtin_fn};

/// Statistics recorded during modifier IR optimization passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OptStats {
    /// Total number of IR nodes before optimization.
    pub nodes_before: usize,
    /// Total number of IR nodes after optimization.
    pub nodes_after: usize,
    /// Number of constant operations folded.
    pub folded: usize,
    /// Number of dead branches eliminated.
    pub dead_branches: usize,
}

impl OptStats {
    /// Combine counters (`folded`, `dead_branches`) from another [`OptStats`].
    pub fn combine(&mut self, other: OptStats) {
        self.folded += other.folded;
        self.dead_branches += other.dead_branches;
    }
}

/// Count total AST/IR nodes in a compiled expression.
pub fn count_expr_nodes(expr: &CompiledExpr) -> usize {
    match expr {
        CompiledExpr::Const(_)
        | CompiledExpr::LoadEnv(_)
        | CompiledExpr::PropRef { .. }
        | CompiledExpr::AnchorLookup { .. } => 1,
        CompiledExpr::Unary(_, inner) => 1 + count_expr_nodes(inner),
        CompiledExpr::Binary(left, _, right) => {
            1 + count_expr_nodes(left) + count_expr_nodes(right)
        },
        CompiledExpr::Select(cond, then_expr, else_expr) => {
            1 + count_expr_nodes(cond) + count_expr_nodes(then_expr) + count_expr_nodes(else_expr)
        },
        CompiledExpr::MakeVec(items) | CompiledExpr::MakeList(items) => {
            1 + items.iter().map(count_expr_nodes).sum::<usize>()
        },
        CompiledExpr::CallBuiltin(_, args) | CompiledExpr::CallEnv(_, args) => {
            1 + args.iter().map(count_expr_nodes).sum::<usize>()
        },
        CompiledExpr::Index(base, index) => 1 + count_expr_nodes(base) + count_expr_nodes(index),
        CompiledExpr::Method(receiver, _, args) => {
            1 + count_expr_nodes(receiver) + args.iter().map(count_expr_nodes).sum::<usize>()
        },
        CompiledExpr::Closure(_, body) => 1 + count_expr_nodes(body),
        CompiledExpr::LetChain(bindings, tail) => {
            1 + bindings.iter().map(|(_, e)| count_expr_nodes(e)).sum::<usize>()
                + count_expr_nodes(tail)
        },
        CompiledExpr::Construct(_, fields) => {
            1 + fields.iter().map(|(_, e)| count_expr_nodes(e)).sum::<usize>()
        },
    }
}

/// Count total AST/IR nodes in a modifier statement.
pub fn count_stmt_nodes(stmt: &ModifierIrStmt) -> usize {
    match stmt {
        ModifierIrStmt::Assign { value, .. } => 1 + count_expr_nodes(value),
        ModifierIrStmt::AssignIndexed { index, value, .. } => {
            1 + count_expr_nodes(index) + count_expr_nodes(value)
        },
        ModifierIrStmt::Let { value, .. } => 1 + count_expr_nodes(value),
        ModifierIrStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            1 + count_expr_nodes(condition)
                + then_branch.iter().map(count_stmt_nodes).sum::<usize>()
                + else_branch.iter().map(count_stmt_nodes).sum::<usize>()
        },
        ModifierIrStmt::For { iterable, body, .. } => {
            1 + count_expr_nodes(iterable) + body.iter().map(count_stmt_nodes).sum::<usize>()
        },
        ModifierIrStmt::Noop => 1,
    }
}

/// Count total AST/IR nodes across all statements in a program.
pub fn count_program_nodes(program: &ModifierIrProgram) -> usize {
    program.statements.iter().map(count_stmt_nodes).sum()
}

/// Map a built-in function to its canonical identifier name.
fn builtin_name(builtin: &BuiltinFn) -> &'static str {
    match builtin {
        BuiltinFn::Sin => "sin",
        BuiltinFn::Cos => "cos",
        BuiltinFn::Lerp => "lerp",
        BuiltinFn::Format => "format",
        BuiltinFn::Tan => "tan",
        BuiltinFn::Sqrt => "sqrt",
        BuiltinFn::Exp => "exp",
        BuiltinFn::Log => "ln",
        BuiltinFn::Atan2 => "atan2",
        BuiltinFn::Clamp => "clamp",
        BuiltinFn::Abs => "abs",
        BuiltinFn::Min => "min",
        BuiltinFn::Max => "max",
        BuiltinFn::Floor => "floor",
        BuiltinFn::Ceil => "ceil",
        BuiltinFn::Deg => "deg",
        BuiltinFn::Rad => "rad",
        BuiltinFn::ListSwap => "list_swap",
        BuiltinFn::ListSet => "list_set",
        BuiltinFn::Signum => "signum",
        BuiltinFn::Fract => "fract",
        BuiltinFn::Hypot => "hypot",
        BuiltinFn::Pow => "pow",
        BuiltinFn::Rem => "rem",
        BuiltinFn::Step => "step",
        BuiltinFn::Round => "round",
        BuiltinFn::Factorial => "factorial",
        BuiltinFn::SumList => "sum",
        BuiltinFn::CurveAt => "curve_at",
        BuiltinFn::CurveSmooth => "curve_smooth",
        BuiltinFn::SeededRand => "seeded_rand",
    }
}

/// Check whether a built-in function or any of its name aliases is shadowed.
fn is_builtin_shadowed(builtin: &BuiltinFn, shadow_names: &HashSet<String>) -> bool {
    if shadow_names.is_empty() {
        return false;
    }
    let name = builtin_name(builtin);
    if shadow_names.contains(name) {
        return true;
    }
    match builtin {
        BuiltinFn::Log => shadow_names.contains("log"),
        BuiltinFn::Deg => shadow_names.contains("deg_to_rad"),
        BuiltinFn::Rad => shadow_names.contains("rad_to_deg"),
        _ => false,
    }
}

/// Every canonical built-in name plus its aliases. Program-level optimization
/// cannot see ambient module/CLI variables, so it treats all built-ins as
/// potentially shadowed and leaves their calls for runtime dispatch.
fn all_builtin_shadow_names() -> HashSet<String> {
    [
        "sin",
        "cos",
        "lerp",
        "format",
        "tan",
        "sqrt",
        "exp",
        "ln",
        "log",
        "atan2",
        "clamp",
        "abs",
        "min",
        "max",
        "floor",
        "ceil",
        "deg",
        "deg_to_rad",
        "rad",
        "rad_to_deg",
        "list_swap",
        "list_set",
        "signum",
        "fract",
        "hypot",
        "pow",
        "rem",
        "step",
        "round",
        "factorial",
        "sum",
        "curve_at",
        "curve_smooth",
        "seeded_rand",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Check if an expression references an identifier, respecting lexical shadowing.
fn expr_references_ident(expr: &CompiledExpr, name: &str) -> bool {
    match expr {
        CompiledExpr::LoadEnv(id) => id == name,
        CompiledExpr::MakeVec(items) | CompiledExpr::MakeList(items) => {
            items.iter().any(|item| expr_references_ident(item, name))
        },
        CompiledExpr::Unary(_, inner) => expr_references_ident(inner, name),
        CompiledExpr::Binary(left, _, right) => {
            expr_references_ident(left, name) || expr_references_ident(right, name)
        },
        CompiledExpr::Select(cond, then_expr, else_expr) => {
            expr_references_ident(cond, name)
                || expr_references_ident(then_expr, name)
                || expr_references_ident(else_expr, name)
        },
        CompiledExpr::CallBuiltin(_, args) | CompiledExpr::CallEnv(_, args) => {
            args.iter().any(|arg| expr_references_ident(arg, name))
        },
        CompiledExpr::Index(base, index) => {
            expr_references_ident(base, name) || expr_references_ident(index, name)
        },
        CompiledExpr::Method(receiver, _, args) => {
            expr_references_ident(receiver, name)
                || args.iter().any(|arg| expr_references_ident(arg, name))
        },
        CompiledExpr::Closure(params, body) => {
            if params.iter().any(|p| p == name) {
                false
            } else {
                expr_references_ident(body, name)
            }
        },
        CompiledExpr::PropRef { label, .. } => label == name,
        CompiledExpr::LetChain(bindings, tail) => {
            for (b_name, b_expr) in bindings {
                if expr_references_ident(b_expr, name) {
                    return true;
                }
                if b_name == name {
                    return false;
                }
            }
            expr_references_ident(tail, name)
        },
        CompiledExpr::Construct(_, fields) => {
            fields.iter().any(|(_, val)| expr_references_ident(val, name))
        },
        CompiledExpr::Const(_) | CompiledExpr::AnchorLookup { .. } => false,
    }
}

/// Check if a let-binding at `from_index` is referenced by any subsequent binding or tail.
fn is_binding_referenced_after(
    bindings: &[(String, CompiledExpr)],
    tail: &CompiledExpr,
    from_index: usize,
) -> bool {
    let target = &bindings[from_index].0;
    for (name, expr) in &bindings[from_index + 1..] {
        if expr_references_ident(expr, target) {
            return true;
        }
        if name == target {
            return false;
        }
    }
    expr_references_ident(tail, target)
}

/// Optimize a compiled expression through bottom-up constant folding,
/// dead branch elimination, and constant propagation.
pub fn optimize_expr(
    expr: CompiledExpr,
    shadow_names: &HashSet<String>,
) -> (CompiledExpr, OptStats) {
    let nodes_before = count_expr_nodes(&expr);
    let mut stats = OptStats::default();
    let const_env = HashMap::new();
    let opt_expr = optimize_expr_scoped(expr, shadow_names, &const_env, &mut stats);
    let nodes_after = count_expr_nodes(&opt_expr);
    stats.nodes_before = nodes_before;
    stats.nodes_after = nodes_after;
    (opt_expr, stats)
}

fn optimize_expr_scoped(
    expr: CompiledExpr,
    shadow_names: &HashSet<String>,
    const_env: &HashMap<String, Value>,
    stats: &mut OptStats,
) -> CompiledExpr {
    match expr {
        CompiledExpr::Const(v) => CompiledExpr::Const(v),
        CompiledExpr::LoadEnv(ref name) => {
            if let Some(val) = const_env.get(name) {
                CompiledExpr::Const(val.clone())
            } else {
                expr
            }
        },
        CompiledExpr::Unary(op, inner) => {
            let inner_opt = optimize_expr_scoped(*inner, shadow_names, const_env, stats);
            match op {
                UnaryOp::Neg => {
                    if let CompiledExpr::Const(Value::Num(n)) = inner_opt {
                        stats.folded += 1;
                        return CompiledExpr::Const(Value::Num(-n));
                    }
                },
                UnaryOp::Not => {
                    if let CompiledExpr::Const(val) = inner_opt {
                        stats.folded += 1;
                        let res = if val.is_truthy() { 0.0 } else { 1.0 };
                        return CompiledExpr::Const(Value::Num(res));
                    }
                },
                UnaryOp::Ref => {},
            }
            CompiledExpr::Unary(op, Box::new(inner_opt))
        },
        CompiledExpr::Binary(left, op, right) => {
            let left_opt = optimize_expr_scoped(*left, shadow_names, const_env, stats);
            let right_opt = optimize_expr_scoped(*right, shadow_names, const_env, stats);
            if let (CompiledExpr::Const(lv), CompiledExpr::Const(rv)) = (&left_opt, &right_opt)
                && let Ok(res) = eval_binary_op(lv.clone(), &op, rv.clone())
            {
                stats.folded += 1;
                return CompiledExpr::Const(res);
            }
            CompiledExpr::Binary(Box::new(left_opt), op, Box::new(right_opt))
        },
        CompiledExpr::Select(cond, then_expr, else_expr) => {
            let cond_opt = optimize_expr_scoped(*cond, shadow_names, const_env, stats);
            let then_opt = optimize_expr_scoped(*then_expr, shadow_names, const_env, stats);
            let else_opt = optimize_expr_scoped(*else_expr, shadow_names, const_env, stats);
            if let CompiledExpr::Const(ref cond_val) = cond_opt {
                stats.dead_branches += 1;
                if cond_val.is_truthy() {
                    then_opt
                } else {
                    else_opt
                }
            } else {
                CompiledExpr::Select(Box::new(cond_opt), Box::new(then_opt), Box::new(else_opt))
            }
        },
        CompiledExpr::CallBuiltin(builtin, args) => {
            let args_opt: Vec<CompiledExpr> = args
                .into_iter()
                .map(|arg| optimize_expr_scoped(arg, shadow_names, const_env, stats))
                .collect();
            let all_const = args_opt.iter().all(|a| matches!(a, CompiledExpr::Const(_)));
            if all_const
                && builtin != BuiltinFn::SeededRand
                && !is_builtin_shadowed(&builtin, shadow_names)
            {
                let arg_values: Vec<Value> = args_opt
                    .iter()
                    .map(|a| match a {
                        CompiledExpr::Const(v) => v.clone(),
                        _ => unreachable!(),
                    })
                    .collect();
                let b_name = builtin_name(&builtin);
                if let Ok(res) = eval_builtin_fn(b_name, &arg_values) {
                    stats.folded += 1;
                    return CompiledExpr::Const(res);
                }
            }
            CompiledExpr::CallBuiltin(builtin, args_opt)
        },
        CompiledExpr::CallEnv(name, args) => {
            let args_opt = args
                .into_iter()
                .map(|arg| optimize_expr_scoped(arg, shadow_names, const_env, stats))
                .collect();
            CompiledExpr::CallEnv(name, args_opt)
        },
        CompiledExpr::Index(base, index) => {
            let base_opt = optimize_expr_scoped(*base, shadow_names, const_env, stats);
            let idx_opt = optimize_expr_scoped(*index, shadow_names, const_env, stats);
            CompiledExpr::Index(Box::new(base_opt), Box::new(idx_opt))
        },
        CompiledExpr::Method(receiver, name, args) => {
            let rec_opt = optimize_expr_scoped(*receiver, shadow_names, const_env, stats);
            let args_opt = args
                .into_iter()
                .map(|arg| optimize_expr_scoped(arg, shadow_names, const_env, stats))
                .collect();
            CompiledExpr::Method(Box::new(rec_opt), name, args_opt)
        },
        CompiledExpr::Closure(params, body) => {
            let mut inner_consts = const_env.clone();
            let mut inner_shadow = shadow_names.clone();
            for p in &params {
                inner_consts.remove(p);
                inner_shadow.insert(p.clone());
            }
            let body_opt = optimize_expr_scoped(*body, &inner_shadow, &inner_consts, stats);
            CompiledExpr::Closure(params, Box::new(body_opt))
        },
        CompiledExpr::LetChain(bindings, tail) => {
            let mut current_consts = const_env.clone();
            let mut current_shadow = shadow_names.clone();
            let mut new_bindings = Vec::with_capacity(bindings.len());

            for (name, val_expr) in bindings {
                let opt_val =
                    optimize_expr_scoped(val_expr, &current_shadow, &current_consts, stats);
                if let CompiledExpr::Const(ref c) = opt_val {
                    current_consts.insert(name.clone(), c.clone());
                } else {
                    current_consts.remove(&name);
                }
                current_shadow.insert(name.clone());
                new_bindings.push((name, opt_val));
            }

            let opt_tail = optimize_expr_scoped(*tail, &current_shadow, &current_consts, stats);

            let mut pruned_bindings = Vec::with_capacity(new_bindings.len());
            for i in 0..new_bindings.len() {
                let (_, expr) = &new_bindings[i];
                if matches!(expr, CompiledExpr::Const(_))
                    && !is_binding_referenced_after(&new_bindings, &opt_tail, i)
                {
                    continue;
                }
                pruned_bindings.push(new_bindings[i].clone());
            }

            if pruned_bindings.is_empty() {
                opt_tail
            } else {
                CompiledExpr::LetChain(pruned_bindings, Box::new(opt_tail))
            }
        },
        CompiledExpr::MakeVec(items) => {
            let items_opt = items
                .into_iter()
                .map(|item| optimize_expr_scoped(item, shadow_names, const_env, stats))
                .collect();
            CompiledExpr::MakeVec(items_opt)
        },
        CompiledExpr::MakeList(items) => {
            let items_opt = items
                .into_iter()
                .map(|item| optimize_expr_scoped(item, shadow_names, const_env, stats))
                .collect();
            CompiledExpr::MakeList(items_opt)
        },
        CompiledExpr::Construct(name, fields) => {
            let fields_opt = fields
                .into_iter()
                .map(|(fname, fexpr)| {
                    (fname, optimize_expr_scoped(fexpr, shadow_names, const_env, stats))
                })
                .collect();
            CompiledExpr::Construct(name, fields_opt)
        },
        CompiledExpr::PropRef { .. } | CompiledExpr::AnchorLookup { .. } => expr,
    }
}

/// Optimize all statements in a modifier IR program.
pub fn optimize_program(
    program: &mut ModifierIrProgram,
    shadow_names: &HashSet<String>,
) -> OptStats {
    let nodes_before = count_program_nodes(program);
    let mut stats = OptStats::default();
    // Built-in calls dispatch through the environment at runtime when a
    // same-named variable exists. Program lowering cannot enumerate module or
    // CLI bindings, so automatic optimization must not fold them.
    let mut safe_shadow = shadow_names.clone();
    safe_shadow.extend(all_builtin_shadow_names());
    let old_statements = std::mem::take(&mut program.statements);
    program.statements = optimize_stmts(old_statements, &safe_shadow, &mut stats);
    if program.fast.is_some() {
        program.fast = Some(super::fast_program::FastModifierProgram::compile(&program.statements));
    }
    let nodes_after = count_program_nodes(program);
    stats.nodes_before = nodes_before;
    stats.nodes_after = nodes_after;
    stats
}

fn optimize_stmts(
    stmts: Vec<ModifierIrStmt>,
    shadow_names: &HashSet<String>,
    stats: &mut OptStats,
) -> Vec<ModifierIrStmt> {
    let mut out = Vec::with_capacity(stmts.len());
    let mut current_shadow = shadow_names.clone();

    for stmt in stmts {
        match stmt {
            ModifierIrStmt::Assign {
                target,
                target_key,
                property,
                value,
            } => {
                let (opt_val, s) = optimize_expr(value, &current_shadow);
                stats.combine(s);
                out.push(ModifierIrStmt::Assign {
                    target,
                    target_key,
                    property,
                    value: opt_val,
                });
            },
            ModifierIrStmt::AssignIndexed {
                base,
                index,
                property,
                value,
            } => {
                let (opt_idx, s1) = optimize_expr(index, &current_shadow);
                let (opt_val, s2) = optimize_expr(value, &current_shadow);
                stats.combine(s1);
                stats.combine(s2);
                out.push(ModifierIrStmt::AssignIndexed {
                    base,
                    index: opt_idx,
                    property,
                    value: opt_val,
                });
            },
            ModifierIrStmt::Let { name, value } => {
                let (opt_val, s) = optimize_expr(value, &current_shadow);
                stats.combine(s);
                current_shadow.insert(name.clone());
                out.push(ModifierIrStmt::Let {
                    name,
                    value: opt_val,
                });
            },
            ModifierIrStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let (opt_cond, s) = optimize_expr(condition, &current_shadow);
                stats.combine(s);
                if let CompiledExpr::Const(ref cond_val) = opt_cond {
                    stats.dead_branches += 1;
                    if cond_val.is_truthy() {
                        let opt_then = optimize_stmts(then_branch, &current_shadow, stats);
                        out.extend(opt_then);
                    } else {
                        let opt_else = optimize_stmts(else_branch, &current_shadow, stats);
                        out.extend(opt_else);
                    }
                } else {
                    let opt_then = optimize_stmts(then_branch, &current_shadow, stats);
                    let opt_else = optimize_stmts(else_branch, &current_shadow, stats);
                    out.push(ModifierIrStmt::If {
                        condition: opt_cond,
                        then_branch: opt_then,
                        else_branch: opt_else,
                    });
                }
            },
            ModifierIrStmt::For {
                var,
                index_var,
                iterable,
                body,
            } => {
                let (opt_iter, s) = optimize_expr(iterable, &current_shadow);
                stats.combine(s);
                let mut body_shadow = current_shadow.clone();
                match &var {
                    LoopPattern::Single(s) => {
                        body_shadow.insert(s.clone());
                    },
                    LoopPattern::Tuple(vars) => {
                        for v in vars {
                            body_shadow.insert(v.clone());
                        }
                    },
                }
                if let Some(ref idx) = index_var {
                    body_shadow.insert(idx.clone());
                }
                let opt_body = optimize_stmts(body, &body_shadow, stats);
                out.push(ModifierIrStmt::For {
                    var,
                    index_var,
                    iterable: opt_iter,
                    body: opt_body,
                });
            },
            ModifierIrStmt::Noop => {
                out.push(ModifierIrStmt::Noop);
            },
        }
    }
    out
}
