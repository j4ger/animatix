use crate::ast::{Expr, Stmt};
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
        } => Ok(ModifierIrStmt::Assign {
            target: target.clone(),
            property: property.clone(),
            value: compile_modifier_expr(value),
        }),
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
        Expr::Closure(_, _) | Expr::Construct(_, _) => None,
    }
}