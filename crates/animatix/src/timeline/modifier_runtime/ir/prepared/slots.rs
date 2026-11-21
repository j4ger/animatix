use std::collections::{BTreeSet, HashMap, HashSet};

use super::super::{eval::builtin_name, numeric_closure::named_plan};
use super::{CompiledExpr, Index, LoopBody, ModifierIrStmt, Statement};
use crate::ast::LoopPattern;
use crate::timeline::{Environment, Timeline, Value};

#[derive(Default)]
pub(super) struct Locals {
    pub(super) names: Vec<String>,
    builtins: BTreeSet<&'static str>,
    closures: BTreeSet<(String, usize)>,
    iterator_names: HashSet<String>,
}

impl Locals {
    pub(super) fn matches(&self, env: &Environment) -> bool {
        self.names.is_empty()
            || (env.is_frame_phase()
                && env.bindings.iter().all(Option::is_none)
                && env.runtime_inputs.as_ref().is_none_or(|inputs| {
                    self.names
                        .iter()
                        .chain(&self.iterator_names)
                        .all(|name| !inputs.contains_key(name))
                })
                && self.builtins.iter().all(|name| {
                    crate::timeline::builtins::is_standard_binding(name, env.get_ref(name))
                })
                && self
                    .closures
                    .iter()
                    .all(|(name, arity)| named_plan(env, name, *arity).is_some()))
    }

    pub(super) fn publish(&self, env: &mut Environment) {
        for (index, name) in self.names.iter().enumerate() {
            if let Some(value) = env.modifier_locals[index].take() {
                env.set(name, value);
            }
        }
    }
}

pub(super) fn compile(
    timeline: &Timeline,
    statements: &mut [Statement],
    written: &HashSet<String>,
) -> Option<Locals> {
    let mut locals = Locals::default();
    let mut candidates = BTreeSet::new();
    let mut blocked = HashSet::new();
    let mut reads = HashSet::new();
    let mut targets = HashSet::new();
    walk(
        statements,
        &mut |stmt| {
            match stmt {
                Statement::Generic(ModifierIrStmt::Let { name, .. }) => {
                    candidates.insert(name.clone());
                },
                Statement::Generic(ModifierIrStmt::Assign { target, .. }) => {
                    // Object writes can observe or change the bare target binding.
                    targets.extend(target.first().cloned());
                },
                Statement::Loop {
                    pattern, index_var, ..
                } => {
                    match pattern {
                        LoopPattern::Single(name) => {
                            blocked.insert(name.clone());
                        },
                        LoopPattern::Tuple(names) => blocked.extend(names.iter().cloned()),
                    }
                    blocked.extend(index_var.iter().cloned());
                },
                _ => {}, // Remaining eligible statement kinds only write actor properties.
            }
            Some(())
        },
        &mut |expr| {
            match expr {
                // A dotted lookup can follow actor aliases through the environment.
                CompiledExpr::LoadEnv(name) => {
                    if name.contains('.') {
                        return None;
                    }
                    reads.insert(name.clone());
                },
                CompiledExpr::CallBuiltin(builtin, _) => {
                    let name = builtin_name(builtin);
                    // These standard functions consume only their evaluated arguments,
                    // including their non-numeric dispatch and error paths.
                    if !matches!(
                        name,
                        "sin"
                            | "cos"
                            | "tan"
                            | "sqrt"
                            | "exp"
                            | "ln"
                            | "abs"
                            | "floor"
                            | "ceil"
                            | "round"
                            | "min"
                            | "max"
                            | "pow"
                            | "clamp"
                    ) || written.contains(name)
                        || !crate::timeline::builtins::is_standard_binding(
                            name,
                            timeline.env_base.get(name),
                        )
                    {
                        return None;
                    }
                    locals.builtins.insert(name);
                },
                CompiledExpr::CallNamed(name, args) => {
                    if written.contains(name)
                        || args.iter().any(|(keyword, _)| keyword.is_some())
                        || named_plan(&timeline.env, name, args.len()).is_none()
                    {
                        return None;
                    }
                    locals.closures.insert((name.clone(), args.len()));
                },
                _ => {}, // walk_expr has checked all other node kinds.
            }
            Some(())
        },
    )?;
    locals.names = candidates
        .iter()
        .filter(|name| {
            !blocked.contains(*name)
                && !targets.contains(*name)
                && !timeline.env_base.contains_key(*name)
                && name.as_str() != crate::timeline::env::FRAME_TIME_IDENT
                && !matches!(name.as_str(), "t" | "scene_width" | "scene_height")
        })
        .cloned()
        .collect();
    if locals.names.is_empty() {
        return None;
    }

    let indices: HashMap<_, _> =
        locals.names.iter().enumerate().map(|(i, name)| (name.as_str(), i)).collect();
    let unused_iterator =
        |name: &str| !reads.contains(name) && !targets.contains(name) && !candidates.contains(name);
    walk(
        statements,
        &mut |stmt| {
            if let Statement::Generic(ModifierIrStmt::Let { name, value }) = stmt
                && let Some(&index) = indices.get(name.as_str())
            {
                *stmt = Statement::StoreLocal(
                    index,
                    std::mem::replace(value, CompiledExpr::Const(Value::Null)),
                );
            }
            if let Statement::Loop {
                pattern: LoopPattern::Single(name),
                index_var,
                body: LoopBody::Fixed(bodies),
                ..
            } = stmt
                && unused_iterator(name)
                && index_var.as_deref().is_none_or(unused_iterator)
            {
                // Prepared bodies already contain all reads of these iterator
                // values. No remaining statement can observe their environment
                // writes or the restoration at loop exit.
                *stmt = Statement::Unrolled(std::mem::take(bodies));
            }
            Some(())
        },
        &mut |expr| {
            if let CompiledExpr::LoadEnv(name) = expr
                && let Some(&index) = indices.get(name.as_str())
            {
                *expr = CompiledExpr::ModifierLocal(index, std::mem::take(name));
            }
            Some(())
        },
    )?;
    locals.iterator_names = blocked;
    Some(locals)
}

fn walk(
    statements: &mut [Statement],
    stmt_visit: &mut impl FnMut(&mut Statement) -> Option<()>,
    expr_visit: &mut impl FnMut(&mut CompiledExpr) -> Option<()>,
) -> Option<()> {
    for stmt in statements {
        match stmt {
            Statement::Generic(
                ModifierIrStmt::Let { value, .. } | ModifierIrStmt::Assign { value, .. },
            ) => {
                walk_expr(value, expr_visit)?;
            },
            Statement::Generic(ModifierIrStmt::Noop) => {},
            Statement::Branch { condition, yes, no } => {
                walk_expr(condition, expr_visit)?;
                walk(yes, stmt_visit, expr_visit)?;
                walk(no, stmt_visit, expr_visit)?;
            },
            Statement::Loop { items, body, .. } => {
                walk_expr(items, expr_visit)?;
                match body {
                    LoopBody::Fixed(bodies) => {
                        for body in bodies {
                            walk(body, stmt_visit, expr_visit)?;
                        }
                    },
                    LoopBody::Dynamic(body) => walk(body, stmt_visit, expr_visit)?,
                }
            },
            Statement::Indexed { index, value, .. } => {
                walk_expr(value, expr_visit)?;
                if let Index::Dynamic(index) = index {
                    walk_expr(index, expr_visit)?;
                }
            },
            _ => return None, // Opaque operations may inspect the frame environment.
        }
        stmt_visit(stmt)?;
    }
    Some(())
}

fn walk_expr(
    expr: &mut CompiledExpr,
    visit: &mut impl FnMut(&mut CompiledExpr) -> Option<()>,
) -> Option<()> {
    match expr {
        CompiledExpr::Const(_) | CompiledExpr::LoadEnv(_) => {},
        CompiledExpr::Unary(_, inner) | CompiledExpr::Field(inner, _) => walk_expr(inner, visit)?,
        CompiledExpr::Binary(left, _, right) => {
            walk_expr(left, visit)?;
            walk_expr(right, visit)?;
        },
        CompiledExpr::Select(condition, yes, no) => {
            walk_expr(condition, visit)?;
            walk_expr(yes, visit)?;
            walk_expr(no, visit)?;
        },
        CompiledExpr::MakeList(items)
        | CompiledExpr::MakeVec(items)
        | CompiledExpr::CallBuiltin(_, items) => {
            for item in items {
                walk_expr(item, visit)?;
            }
        },
        CompiledExpr::CallNamed(_, args) => {
            for (_, arg) in args {
                walk_expr(arg, visit)?;
            }
        },
        _ => return None, // Captures, lexical scopes and dynamic dispatch need ordinary bindings.
    }
    visit(expr)
}
