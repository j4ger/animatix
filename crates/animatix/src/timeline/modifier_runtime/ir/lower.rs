use crate::ast::{BinaryOp, Expr, MatchPattern, Stmt, TargetSegment};
use crate::timeline::Value;

use super::types::{
    BuiltinFn, CompiledExpr, IrLowerError, ModifierExpr, ModifierIrProgram, ModifierIrStmt,
};

/// Convenience wrapper that unwraps `Always`
/// statements and lowers their bodies. Kept for test compatibility.
pub fn lower_modifier_ir(program: &[Stmt]) -> Result<ModifierIrProgram, IrLowerError> {
    let mut statements = Vec::new();
    for stmt in program {
        lower_modifier_roots(stmt, &mut statements)?;
    }
    Ok(ModifierIrProgram { statements })
}

fn lower_modifier_roots(
    stmt: &Stmt,
    output: &mut Vec<ModifierIrStmt>,
) -> Result<(), IrLowerError> {
    match stmt {
        Stmt::Always { body, .. } => {
            output.extend(lower_modifier_block(body)?);
        }
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            for stmt in body {
                lower_modifier_roots(stmt, output)?;
            }
        }
        Stmt::Comment(..) => {}
        _ => {}
    }
    Ok(())
}

/// Lower a flat block of modifier body statements into IR statements.
pub fn lower_modifier_block(body: &[Stmt]) -> Result<Vec<ModifierIrStmt>, IrLowerError> {
    body.iter()
        .filter(|s| !matches!(s, Stmt::Comment(..)))
        .map(lower_modifier_stmt)
        .collect()
}

/// Lower a flat list of modifier body statements (assignments, conditionals, lets)
/// into a ModifierIrProgram. Unlike `lower_modifier_ir`, this does not expect
/// Always wrapper statements.
pub fn lower_modifier_body(statements: &[Stmt]) -> Result<ModifierIrProgram, IrLowerError> {
    Ok(ModifierIrProgram {
        statements: lower_modifier_block(statements)?,
    })
}

fn lower_modifier_stmt(stmt: &Stmt) -> Result<ModifierIrStmt, IrLowerError> {
    match stmt {
        Stmt::Assignment {
            target,
            property,
            value,
            ..
        } => {
            // Check if any segment is Indexed — if so, emit AssignIndexed.
            // The last segment (property) is always Static; for runtime-indexed
            // targets like `bars[i].color` we extract the Indexed segment's base
            // and compile the index expression for frame-time evaluation.
            let indexed_seg = target.iter().find(|s| matches!(s, TargetSegment::Indexed { .. }));
            if let Some(TargetSegment::Indexed { base, index }) = indexed_seg {
                Ok(ModifierIrStmt::AssignIndexed {
                    base: base.clone(),
                    index: compile_expr(index).ok_or(
                        IrLowerError::UnsupportedStatement(
                            "runtime index expression in target",
                        ),
                    )?,
                    property: property.clone(),
                    value: compile_modifier_expr(value),
                })
            } else {
                // All-static fast path: join static segments for the target key.
                let static_target: Vec<String> = target
                    .iter()
                    .map(|s| match s {
                        TargetSegment::Static(t) => t.clone(),
                        _ => unreachable!(), // filtered above
                    })
                    .collect();
                Ok(ModifierIrStmt::Assign {
                    target: static_target,
                    property: property.clone(),
                    value: compile_modifier_expr(value),
                })
            }
        },
        Stmt::LetDecl {
            name,
            value,
            is_pub: _,
            ..
        } => Ok(ModifierIrStmt::Let {
            name: name.clone(),
            value: compile_modifier_expr(value),
        }),
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => Ok(ModifierIrStmt::If {
            condition: compile_modifier_expr(condition),
            then_branch: lower_modifier_block(then_branch)?,
            else_branch: lower_modifier_block(else_branch.as_deref().unwrap_or(&[]))?,
        }),
        Stmt::Match {
            scrutinee,
            arms,
            ..
        } => {
            lower_match_stmt(scrutinee, arms)
        }
        Stmt::Comment(..) => Err(IrLowerError::UnsupportedStatement("comment")),
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            ..
        } => {
            let compiled_iterable = compile_expr(iterable).ok_or(
                IrLowerError::UnsupportedStatement(
                    "for loop with unsupported iterable expression",
                ),
            )?;
            Ok(ModifierIrStmt::For {
                var: var.clone(),
                index_var: index_var.clone(),
                iterable: compiled_iterable,
                body: lower_modifier_block(body)?,
            })
        }
        Stmt::Action(..) => Err(IrLowerError::UnsupportedStatement("action")),
        Stmt::ActorDecl { .. }
        | Stmt::Import { .. }
        | Stmt::Keyframe { .. }
        | Stmt::RelativeKeyframe { .. }
        | Stmt::Sequence { .. }
        | Stmt::Stagger { .. }
        | Stmt::Always { .. }
        | Stmt::ReactiveBinding { .. }
        | Stmt::ComponentDef(..)
        | Stmt::ComponentAction { .. }
        | Stmt::Config { .. }
        | Stmt::Scene { .. }
        | Stmt::Play { .. } => Err(IrLowerError::UnsupportedStatement("non-modifier statement")),
    }
}

/// Compile an AST expression into a modifier expression (compiled or unsupported).
pub fn compile_modifier_expr(expr: &Expr) -> ModifierExpr {
    compile_expr(expr)
        .map(ModifierExpr::Compiled)
        .unwrap_or_else(|| ModifierExpr::Unsupported(expr.clone()))
}

/// Compile an AST expression into a compiled IR expression, if supported.
pub fn compile_expr(expr: &Expr) -> Option<CompiledExpr> {
    match expr {
        Expr::Num(n) => Some(CompiledExpr::Const(Value::Num(*n))),
        Expr::Percent(n) => Some(CompiledExpr::Const(Value::Num(*n / 100.0))),
        Expr::Str(s) => Some(CompiledExpr::Const(Value::Str(s.clone()))),
        Expr::Bool(b) => Some(CompiledExpr::Const(Value::Bool(*b))),
        Expr::Null => Some(CompiledExpr::Const(Value::Num(0.0))),
        Expr::Ident(name) => Some(CompiledExpr::LoadEnv(name.clone())),
        Expr::Path(parts) => Some(CompiledExpr::LoadEnv(parts.join("."))),
        Expr::Tuple(items) => items
            .iter()
            .map(compile_expr)
            .collect::<Option<Vec<_>>>()
            .map(CompiledExpr::MakeVec),
        Expr::List(items) => items
            .iter()
            .map(compile_expr)
            .collect::<Option<Vec<_>>>()
            .map(CompiledExpr::MakeVec),
        Expr::Unary(op, expr) => Some(CompiledExpr::Unary(
            op.clone(),
            Box::new(compile_expr(expr)?),
        )),
        Expr::Binary(left, op, right) => Some(CompiledExpr::Binary(
            Box::new(compile_expr(left)?),
            op.clone(),
            Box::new(compile_expr(right)?),
        )),
        Expr::Conditional(cond, then_expr, else_expr) => Some(CompiledExpr::Select(
            Box::new(compile_expr(cond)?),
            Box::new(compile_expr(then_expr)?),
            Box::new(compile_expr(else_expr)?),
        )),
        Expr::Call(name, args) => {
            let builtin = match name.as_str() {
                "sin" => BuiltinFn::Sin,
                "cos" => BuiltinFn::Cos,
                "lerp" => BuiltinFn::Lerp,
                "format" => BuiltinFn::Format,
                "tan" => BuiltinFn::Tan,
                "sqrt" => BuiltinFn::Sqrt,
                "exp" => BuiltinFn::Exp,
                "log" => BuiltinFn::Log,
                "atan2" => BuiltinFn::Atan2,
                "clamp" => BuiltinFn::Clamp,
                "abs" => BuiltinFn::Abs,
                "min" => BuiltinFn::Min,
                "max" => BuiltinFn::Max,
                "floor" => BuiltinFn::Floor,
                "ceil" => BuiltinFn::Ceil,
                "deg" => BuiltinFn::Deg,
                "rad" => BuiltinFn::Rad,
                "list_swap" => BuiltinFn::ListSwap,
                "list_set" => BuiltinFn::ListSet,
                _ => return None,
            };
            Some(CompiledExpr::CallBuiltin(
                builtin,
                args.iter().map(compile_expr).collect::<Option<Vec<_>>>()?,
            ))
        }
        Expr::Index(container, index) => {
            let container = compile_expr(container)?;
            let index = compile_expr(index)?;
            Some(CompiledExpr::Index(Box::new(container), Box::new(index)))
        }
        Expr::Method(receiver, name, args) => {
            let receiver = compile_expr(receiver)?;
            let args: Vec<_> = args.iter().map(compile_expr).collect::<Option<Vec<_>>>()?;
            Some(CompiledExpr::Method(Box::new(receiver), name.clone(), args))
        }
        Expr::Closure(params, body) => {
            Some(CompiledExpr::Closure(params.clone(), body.clone()))
        }
        Expr::Match(scrutinee, arms) => {
            // Lower to nested Select expressions
            let scrutinee_compiled = compile_expr(scrutinee)?;
            // Build from the last arm backwards
            let default = arms.last().map(|(_pat, arm_expr)| compile_expr(arm_expr)).unwrap_or(Some(CompiledExpr::Const(Value::Num(0.0))))?;
            let mut result = default;
            for (pat, arm_expr) in arms.iter().rev().skip(if arms.last().map(|(p,_)| matches!(p, MatchPattern::Wildcard)).unwrap_or(false) { 1 } else { 0 }) {
                let arm = compile_expr(arm_expr)?;
                let condition = pattern_to_compiled_condition(&scrutinee_compiled, pat);
                result = CompiledExpr::Select(Box::new(condition), Box::new(arm), Box::new(result));
            }
            // If the last arm is wildcard, it's already the default; no extra select needed.
            // If not, wrap with a final wildcard check (treat as default anyway).
            Some(result)
        }
        Expr::Construct(name, properties) => {
            let fields: Option<Vec<_>> = properties
                .iter()
                .map(|p| compile_expr(&p.value).map(|v| (p.name.clone(), v)))
                .collect();
            fields.map(|f| CompiledExpr::Construct(name.clone(), f))
        }
    }
}

/// Lower a `Stmt::Match` to nested `ModifierIrStmt::If` statements.
/// Builds from the last arm backwards: wildcard → else, then chain each arm.
fn lower_match_stmt(scrutinee: &Expr, arms: &[(MatchPattern, Vec<Stmt>)]) -> Result<ModifierIrStmt, IrLowerError> {
    let compiled_scrutinee = compile_modifier_expr(scrutinee);
    if arms.is_empty() {
        return Ok(ModifierIrStmt::Noop);
    }

    // Start from the last arm and build the chain backwards.
    // If the last arm is a wildcard, it becomes the initial else branch.
    // Otherwise, the initial else branch is empty (ModifierIrStmt::Noop).
    let mut else_body: Vec<ModifierIrStmt> = Vec::new();
    
    // Determine the range to iterate in reverse
    let has_wildcard_last = arms.last().map(|(p, _)| matches!(p, MatchPattern::Wildcard)).unwrap_or(false);
    
    // If the last arm is wildcard, set it as the else body and skip it in the iteration
    let iter_arms: Vec<&(MatchPattern, Vec<Stmt>)> = if has_wildcard_last {
        let (_, wildcard_body) = arms.last().unwrap();
        else_body = lower_modifier_block(wildcard_body)?;
        arms[..arms.len() - 1].iter().rev().collect()
    } else {
        arms.iter().rev().collect()
    };

    let mut result: Vec<ModifierIrStmt> = else_body;
    for (pat, body) in iter_arms {
        let body_ir = lower_modifier_block(body)?;
        let condition = pattern_to_condition(&compiled_scrutinee, pat);
        result = vec![ModifierIrStmt::If {
            condition,
            then_branch: body_ir,
            else_branch: result,
        }];
    }

    Ok(result.into_iter().next().unwrap_or(ModifierIrStmt::Noop))
}

/// Convert a match pattern to a condition expression (CompiledExpr) that evaluates
/// to `1.0` (true) when the scrutinee matches the pattern.
fn pattern_to_compiled_condition(scrutinee: &CompiledExpr, pat: &MatchPattern) -> CompiledExpr {
    match pat {
        MatchPattern::Wildcard => CompiledExpr::Const(Value::Num(1.0)),
        MatchPattern::Num(n) => {
            let rhs = CompiledExpr::Const(Value::Num(*n));
            CompiledExpr::Binary(
                Box::new(scrutinee.clone()),
                BinaryOp::Eq,
                Box::new(rhs),
            )
        }
        MatchPattern::Str(s) => {
            let rhs = CompiledExpr::Const(Value::Str(s.clone()));
            CompiledExpr::Binary(
                Box::new(scrutinee.clone()),
                BinaryOp::Eq,
                Box::new(rhs),
            )
        }
        MatchPattern::Bool(b) => {
            let rhs = CompiledExpr::Const(Value::Bool(*b));
            CompiledExpr::Binary(
                Box::new(scrutinee.clone()),
                BinaryOp::Eq,
                Box::new(rhs),
            )
        }
        MatchPattern::Range(lo, hi) => {
            let lo_val = match lo.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return CompiledExpr::Const(Value::Num(0.0)),
            };
            let hi_val = match hi.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return CompiledExpr::Const(Value::Num(0.0)),
            };
            // scrutinee >= lo AND scrutinee <= hi
            let ge = CompiledExpr::Binary(
                Box::new(scrutinee.clone()),
                BinaryOp::Gte,
                Box::new(CompiledExpr::Const(Value::Num(lo_val))),
            );
            let le = CompiledExpr::Binary(
                Box::new(scrutinee.clone()),
                BinaryOp::Lte,
                Box::new(CompiledExpr::Const(Value::Num(hi_val))),
            );
            CompiledExpr::Binary(
                Box::new(ge),
                BinaryOp::And,
                Box::new(le),
            )
        }
        MatchPattern::Or(pats) => {
            if pats.is_empty() {
                return CompiledExpr::Const(Value::Num(0.0));
            }
            let mut result = pattern_to_compiled_condition(scrutinee, &pats[0]);
            for p in &pats[1..] {
                let cond = pattern_to_compiled_condition(scrutinee, p);
                result = CompiledExpr::Binary(
                    Box::new(result),
                    BinaryOp::Or,
                    Box::new(cond),
                );
            }
            result
        }
        MatchPattern::Tuple(pats) => {
            if pats.is_empty() {
                return CompiledExpr::Const(Value::Num(1.0));
            }
            // For tuples, generate nested Index + comparisons
            // scrutinee[0] == pat0 && scrutinee[1] == pat1 && ...
            let mut result = pattern_to_compiled_index_condition(scrutinee, &pats[0], 0);
            for (i, p) in pats[1..].iter().enumerate() {
                let cond = pattern_to_compiled_index_condition(scrutinee, p, i + 1);
                result = CompiledExpr::Binary(
                    Box::new(result),
                    BinaryOp::And,
                    Box::new(cond),
                );
            }
            result
        }
    }
}

/// Helper for tuple patterns: generate `scrutinee[i] == pat` condition.
fn pattern_to_compiled_index_condition(scrutinee: &CompiledExpr, pat: &MatchPattern, index: usize) -> CompiledExpr {
    let indexed = CompiledExpr::Index(
        Box::new(scrutinee.clone()),
        Box::new(CompiledExpr::Const(Value::Num(index as f64))),
    );
    match pat {
        MatchPattern::Wildcard => CompiledExpr::Const(Value::Num(1.0)),
        MatchPattern::Num(n) => {
            CompiledExpr::Binary(
                Box::new(indexed),
                BinaryOp::Eq,
                Box::new(CompiledExpr::Const(Value::Num(*n))),
            )
        }
        MatchPattern::Str(s) => {
            CompiledExpr::Binary(
                Box::new(indexed),
                BinaryOp::Eq,
                Box::new(CompiledExpr::Const(Value::Str(s.clone()))),
            )
        }
        MatchPattern::Bool(b) => {
            CompiledExpr::Binary(
                Box::new(indexed),
                BinaryOp::Eq,
                Box::new(CompiledExpr::Const(Value::Bool(*b))),
            )
        }
        // Nested or/range patterns inside tuples are not common but supported
        MatchPattern::Or(_pats) => pattern_to_compiled_condition(&indexed, pat),
        MatchPattern::Range(..) => pattern_to_compiled_condition(&indexed, pat),
        MatchPattern::Tuple(_) => {
            // Nested tuple: recurse with the indexed scrutinee
            pattern_to_compiled_condition(&indexed, pat)
        }
    }
}

/// Convert a match pattern to a condition `ModifierExpr` for use in `lower_match_stmt`.
fn pattern_to_condition(scrutinee: &ModifierExpr, pat: &MatchPattern) -> ModifierExpr {
    match scrutinee {
        ModifierExpr::Compiled(c) => {
            ModifierExpr::Compiled(pattern_to_compiled_condition(c, pat))
        }
        ModifierExpr::Unsupported(e) => {
            // If the scrutinee is unsupported, emit a catch-all condition
            // This means the match won't work correctly at runtime — log a warning.
            tracing::warn!(
                "Match scrutinee expression is unsupported in IR; match may not work correctly: {:?}",
                e
            );
            ModifierExpr::Compiled(CompiledExpr::Const(Value::Num(1.0)))
        }
    }
}