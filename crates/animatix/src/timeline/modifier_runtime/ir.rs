use crate::ast::{BinaryOp, Expr, Stmt, UnaryOp};
use crate::timeline::{Environment, EvalError, Value};
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum BuiltinFn {
    Sin,
    Cos,
    Lerp,
    Format,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompiledExpr {
    Const(Value),
    LoadEnv(String),
    MakeVec(Vec<CompiledExpr>),
    Unary(UnaryOp, Box<CompiledExpr>),
    Binary(Box<CompiledExpr>, BinaryOp, Box<CompiledExpr>),
    Select(Box<CompiledExpr>, Box<CompiledExpr>, Box<CompiledExpr>),
    CallBuiltin(BuiltinFn, Vec<CompiledExpr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifierExpr {
    Compiled(CompiledExpr),
    Unsupported(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ModifierIrStmt {
    Assign {
        target: Vec<String>,
        property: String,
        value: ModifierExpr,
    },
    Let {
        name: String,
        value: ModifierExpr,
    },
    If {
        condition: ModifierExpr,
        then_branch: Vec<ModifierIrStmt>,
        else_branch: Vec<ModifierIrStmt>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModifierIrProgram {
    pub statements: Vec<ModifierIrStmt>,
}

pub type ModifierOverrides = HashMap<String, HashMap<String, Value>>;

#[derive(Clone, Debug, PartialEq)]
pub enum IrLowerError {
    UnsupportedStatement(&'static str),
}

impl fmt::Display for IrLowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrLowerError::UnsupportedStatement(kind) => {
                write!(f, "Unsupported IR statement: {kind}")
            }
        }
    }
}

impl std::error::Error for IrLowerError {}

pub fn lower_modifier_ir(program: &[Stmt]) -> Result<ModifierIrProgram, IrLowerError> {
    let mut statements = Vec::new();
    for stmt in program {
        lower_modifier_roots(stmt, &mut statements)?;
    }
    Ok(ModifierIrProgram { statements })
}

fn lower_modifier_roots(stmt: &Stmt, output: &mut Vec<ModifierIrStmt>) -> Result<(), IrLowerError> {
    match stmt {
        Stmt::Always { body } | Stmt::LabeledAlways { body, .. } => {
            output.extend(lower_modifier_block(body)?);
        }
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            for stmt in body {
                lower_modifier_roots(stmt, output)?;
            }
        }
        Stmt::Comment(_) => {}
        _ => {}
    }
    Ok(())
}

pub fn lower_modifier_block(body: &[Stmt]) -> Result<Vec<ModifierIrStmt>, IrLowerError> {
    body.iter().map(lower_modifier_stmt).collect()
}

/// Lower a flat list of modifier body statements (assignments, conditionals, lets)
/// into a ModifierIrProgram. Unlike `lower_modifier_ir`, this does not expect
/// Always/LabeledAlways wrapper statements.
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
        Stmt::LetDecl { name, value, is_pub: _ } => Ok(ModifierIrStmt::Let {
            name: name.clone(),
            value: compile_modifier_expr(value),
        }),
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
        } => Ok(ModifierIrStmt::If {
            condition: compile_modifier_expr(condition),
            then_branch: lower_modifier_block(then_branch)?,
            else_branch: lower_modifier_block(else_branch.as_deref().unwrap_or(&[]))?,
        }),
        Stmt::Comment(_) => Err(IrLowerError::UnsupportedStatement("comment")),
        Stmt::ForLoop { .. } => Err(IrLowerError::UnsupportedStatement("for loop")),
        Stmt::Action(_) => Err(IrLowerError::UnsupportedStatement("action")),
        Stmt::Text { .. }
        | Stmt::Math { .. }
        | Stmt::Code { .. }
        | Stmt::Svg { .. }
        | Stmt::Image { .. }
        | Stmt::ActorDecl { .. }
        | Stmt::Import { .. }
        | Stmt::Use { .. }
        | Stmt::Keyframe { .. }
        | Stmt::RelativeKeyframe { .. }
        | Stmt::Sequence { .. }
        | Stmt::Stagger { .. }
        | Stmt::Always { .. }
        | Stmt::LabeledAlways { .. }
        | Stmt::ComponentDef(_)
        | Stmt::ComponentAction { .. }
        | Stmt::Config { .. } => Err(IrLowerError::UnsupportedStatement("non-modifier statement")),
    }
}

pub fn compile_modifier_expr(expr: &Expr) -> ModifierExpr {
    compile_expr(expr)
        .map(ModifierExpr::Compiled)
        .unwrap_or_else(|| ModifierExpr::Unsupported(expr.clone()))
}

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
                _ => return None,
            };
            Some(CompiledExpr::CallBuiltin(
                builtin,
                args.iter().map(compile_expr).collect::<Option<Vec<_>>>()?,
            ))
        }
        Expr::Closure(_, _) | Expr::Method(_, _, _) | Expr::Index(_, _) | Expr::Construct(_, _) => {
            None
        }
    }
}

pub fn evaluate_modifier_expr(expr: &ModifierExpr, env: &Environment) -> Result<Value, EvalError> {
    match expr {
        ModifierExpr::Compiled(expr) => evaluate_compiled_expr(expr, env),
        ModifierExpr::Unsupported(expr) => crate::timeline::evaluate_expr(expr, env),
    }
}

pub fn execute_modifier_ir<F>(
    program: &ModifierIrProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    mut refresh_env: F,
) -> Result<(), EvalError>
where
    F: FnMut(&mut Environment, &ModifierOverrides),
{
    for stmt in &program.statements {
        execute_modifier_stmt(stmt, frame_env, overrides, &mut refresh_env)?;
    }
    Ok(())
}

fn execute_modifier_stmt<F>(
    stmt: &ModifierIrStmt,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    refresh_env: &mut F,
) -> Result<(), EvalError>
where
    F: FnMut(&mut Environment, &ModifierOverrides),
{
    match stmt {
        ModifierIrStmt::Assign {
            target,
            property,
            value,
        } => {
            let val = evaluate_modifier_expr(value, frame_env)?;
            overrides
                .entry(target.join("."))
                .or_default()
                .insert(property.clone(), val);
            refresh_env(frame_env, overrides);
            Ok(())
        }
        ModifierIrStmt::Let { name, value } => {
            let val = evaluate_modifier_expr(value, frame_env)?;
            frame_env.set(name, val);
            Ok(())
        }
        ModifierIrStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = evaluate_modifier_expr(condition, frame_env)?;
            let branch = if cond.as_num() != 0.0 {
                then_branch
            } else {
                else_branch
            };
            for stmt in branch {
                execute_modifier_stmt(stmt, frame_env, overrides, refresh_env)?;
            }
            Ok(())
        }
    }
}

pub fn evaluate_compiled_expr(expr: &CompiledExpr, env: &Environment) -> Result<Value, EvalError> {
    match expr {
        CompiledExpr::Const(value) => Ok(value.clone()),
        CompiledExpr::LoadEnv(name) => env
            .get(name)
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),
        CompiledExpr::MakeVec(items) => {
            let values = items
                .iter()
                .map(|item| evaluate_compiled_expr(item, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(make_vec_value(values))
        }
        CompiledExpr::Unary(op, expr) => {
            let value = evaluate_compiled_expr(expr, env)?;
            match op {
                UnaryOp::Neg => Ok(Value::Num(-value.as_num())),
                UnaryOp::Not => Ok(Value::Num(if value.as_num() == 0.0 { 1.0 } else { 0.0 })),
            }
        }
        CompiledExpr::Binary(left, op, right) => {
            let left = evaluate_compiled_expr(left, env)?;
            let right = evaluate_compiled_expr(right, env)?;
            apply_binary_op(left, op, right)
        }
        CompiledExpr::Select(condition, then_expr, else_expr) => {
            let cond = evaluate_compiled_expr(condition, env)?;
            if cond.as_num() != 0.0 {
                evaluate_compiled_expr(then_expr, env)
            } else {
                evaluate_compiled_expr(else_expr, env)
            }
        }
        CompiledExpr::CallBuiltin(builtin, args) => {
            let args = args
                .iter()
                .map(|arg| evaluate_compiled_expr(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            match builtin {
                BuiltinFn::Sin => eval_sin(&args),
                BuiltinFn::Cos => eval_cos(&args),
                BuiltinFn::Lerp => eval_lerp(&args),
                BuiltinFn::Format => eval_format(&args),
            }
        }
    }
}

pub(crate) fn eval_sin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "sin expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().sin()))
}

pub(crate) fn eval_cos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "cos expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().cos()))
}

pub(crate) fn eval_lerp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(
            "lerp expects 3 arguments".to_string(),
        ));
    }
    Ok(Value::Num(
        args[0].as_num() + (args[1].as_num() - args[0].as_num()) * args[2].as_num(),
    ))
}

pub(crate) fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
    let Some((template, rest)) = args.split_first() else {
        return Ok(Value::Str(String::new()));
    };
    let mut output = template.as_str();
    for arg in rest {
        let replacement = match arg {
            Value::Num(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Vec2(v) => format!("({}, {})", v[0], v[1]),
            Value::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
            Value::Vec4(v) | Value::Color(v) => {
                format!("({}, {}, {}, {})", v[0], v[1], v[2], v[3])
            }
            Value::NativeFn(_) => "<NativeFn>".to_string(),
            Value::Closure(_, _) => "<Closure>".to_string(),
        };
        output = output.replacen("{}", &replacement, 1);
    }
    Ok(Value::Str(output))
}

pub(crate) fn apply_binary_op(
    left: Value,
    op: &BinaryOp,
    right: Value,
) -> Result<Value, EvalError> {
    match (left.clone(), right.clone()) {
        (Value::Num(l), Value::Num(r)) => Ok(Value::Num(match op {
            BinaryOp::Add => l + r,
            BinaryOp::Sub => l - r,
            BinaryOp::Mul => l * r,
            BinaryOp::Div if r != 0.0 => l / r,
            BinaryOp::Div => 0.0,
            BinaryOp::Mod if r != 0.0 => l % r,
            BinaryOp::Mod => 0.0,
            BinaryOp::Pow => l.powf(r),
            BinaryOp::Eq => {
                if l == r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Neq => {
                if l != r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Lt => {
                if l < r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Gt => {
                if l > r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Lte => {
                if l <= r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Gte => {
                if l >= r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::And => {
                if l != 0.0 && r != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Or => {
                if l != 0.0 || r != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        })),
        (Value::Vec2(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l[0] + r[0], l[1] + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l[0] - r[0], l[1] - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l[0] * r[0], l[1] * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([l[0] / r[0], l[1] / r[1]])),
            BinaryOp::Mod => Ok(Value::Vec2([l[0] % r[0], l[1] % r[1]])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec2 and Vec2",
                op
            ))),
        },
        (Value::Vec3(l), Value::Vec3(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l[0] + r[0], l[1] + r[1], l[2] + r[2]])),
            BinaryOp::Sub => Ok(Value::Vec3([l[0] - r[0], l[1] - r[1], l[2] - r[2]])),
            BinaryOp::Mul => Ok(Value::Vec3([l[0] * r[0], l[1] * r[1], l[2] * r[2]])),
            BinaryOp::Div => Ok(Value::Vec3([l[0] / r[0], l[1] / r[1], l[2] / r[2]])),
            BinaryOp::Mod => Ok(Value::Vec3([l[0] % r[0], l[1] % r[1], l[2] % r[2]])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec3 and Vec3",
                op
            ))),
        },
        (Value::Color(l), Value::Color(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([
                l[0] + r[0],
                l[1] + r[1],
                l[2] + r[2],
                l[3] + r[3],
            ])),
            BinaryOp::Sub => Ok(Value::Color([
                l[0] - r[0],
                l[1] - r[1],
                l[2] - r[2],
                l[3] - r[3],
            ])),
            BinaryOp::Mul => Ok(Value::Color([
                l[0] * r[0],
                l[1] * r[1],
                l[2] * r[2],
                l[3] * r[3],
            ])),
            BinaryOp::Div => Ok(Value::Color([
                l[0] / r[0],
                l[1] / r[1],
                l[2] / r[2],
                l[3] / r[3],
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Color and Color",
                op
            ))),
        },
        (Value::Vec2(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l[0] + r, l[1] + r])),
            BinaryOp::Sub => Ok(Value::Vec2([l[0] - r, l[1] - r])),
            BinaryOp::Mul => Ok(Value::Vec2([l[0] * r, l[1] * r])),
            BinaryOp::Div => Ok(Value::Vec2([l[0] / r, l[1] / r])),
            BinaryOp::Mod => Ok(Value::Vec2([l[0] % r, l[1] % r])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec2 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l + r[0], l + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l - r[0], l - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l * r[0], l * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([l / r[0], l / r[1]])),
            BinaryOp::Mod => Ok(Value::Vec2([l % r[0], l % r[1]])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Vec2",
                op
            ))),
        },
        (Value::Vec3(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l[0] + r, l[1] + r, l[2] + r])),
            BinaryOp::Sub => Ok(Value::Vec3([l[0] - r, l[1] - r, l[2] - r])),
            BinaryOp::Mul => Ok(Value::Vec3([l[0] * r, l[1] * r, l[2] * r])),
            BinaryOp::Div => Ok(Value::Vec3([l[0] / r, l[1] / r, l[2] / r])),
            BinaryOp::Mod => Ok(Value::Vec3([l[0] % r, l[1] % r, l[2] % r])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec3 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec3(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l + r[0], l + r[1], l + r[2]])),
            BinaryOp::Sub => Ok(Value::Vec3([l - r[0], l - r[1], l - r[2]])),
            BinaryOp::Mul => Ok(Value::Vec3([l * r[0], l * r[1], l * r[2]])),
            BinaryOp::Div => Ok(Value::Vec3([l / r[0], l / r[1], l / r[2]])),
            BinaryOp::Mod => Ok(Value::Vec3([l % r[0], l % r[1], l % r[2]])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Vec3",
                op
            ))),
        },
        (Value::Color(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([l[0] + r, l[1] + r, l[2] + r, l[3] + r])),
            BinaryOp::Sub => Ok(Value::Color([l[0] - r, l[1] - r, l[2] - r, l[3] - r])),
            BinaryOp::Mul => Ok(Value::Color([l[0] * r, l[1] * r, l[2] * r, l[3] * r])),
            BinaryOp::Div => Ok(Value::Color([l[0] / r, l[1] / r, l[2] / r, l[3] / r])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Color and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Color(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([l + r[0], l + r[1], l + r[2], l + r[3]])),
            BinaryOp::Sub => Ok(Value::Color([l - r[0], l - r[1], l - r[2], l - r[3]])),
            BinaryOp::Mul => Ok(Value::Color([l * r[0], l * r[1], l * r[2], l * r[3]])),
            BinaryOp::Div => Ok(Value::Color([l / r[0], l / r[1], l / r[2], l / r[3]])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Color",
                op
            ))),
        },
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for {:?} and {:?}",
            op, left, right
        ))),
    }
}

impl fmt::Display for ModifierIrProgram {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for stmt in &self.statements {
            writeln!(f, "{}", DisplayStmt(stmt))?;
        }
        Ok(())
    }
}

pub(crate) fn make_vec_value(values: Vec<Value>) -> Value {
    match values.len() {
        2 => Value::Vec2([values[0].as_num(), values[1].as_num()]),
        3 => Value::Vec3([values[0].as_num(), values[1].as_num(), values[2].as_num()]),
        4 => Value::Vec4([
            values[0].as_num(),
            values[1].as_num(),
            values[2].as_num(),
            values[3].as_num(),
        ]),
        _ => Value::Str(format!("{:?}", values)),
    }
}

struct DisplayStmt<'a>(&'a ModifierIrStmt);

impl fmt::Display for DisplayStmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ModifierIrStmt::Assign {
                target,
                property,
                value,
            } => write!(
                f,
                "assign {}.{} = {}",
                target.join("."),
                property,
                DisplayExpr(value)
            ),
            ModifierIrStmt::Let { name, value } => {
                write!(f, "let {} = {}", name, DisplayExpr(value))
            }
            ModifierIrStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "if {} {{ ", DisplayExpr(condition))?;
                for (idx, stmt) in then_branch.iter().enumerate() {
                    if idx > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", DisplayStmt(stmt))?;
                }
                write!(f, " }}")?;
                if !else_branch.is_empty() {
                    write!(f, " else {{ ")?;
                    for (idx, stmt) in else_branch.iter().enumerate() {
                        if idx > 0 {
                            write!(f, "; ")?;
                        }
                        write!(f, "{}", DisplayStmt(stmt))?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
        }
    }
}

struct DisplayExpr<'a>(&'a ModifierExpr);

impl fmt::Display for DisplayExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ModifierExpr::Compiled(expr) => write!(f, "{}", DisplayCompiledExpr(expr)),
            ModifierExpr::Unsupported(expr) => write!(f, "unsupported({expr:?})"),
        }
    }
}

struct DisplayCompiledExpr<'a>(&'a CompiledExpr);

impl fmt::Display for DisplayCompiledExpr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            CompiledExpr::Const(value) => write!(f, "const({value:?})"),
            CompiledExpr::LoadEnv(name) => write!(f, "load({name})"),
            CompiledExpr::MakeVec(items) => {
                write!(f, "vec(")?;
                for (idx, item) in items.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayCompiledExpr(item))?;
                }
                write!(f, ")")
            }
            CompiledExpr::Unary(op, expr) => write!(f, "({op:?} {})", DisplayCompiledExpr(expr)),
            CompiledExpr::Binary(left, op, right) => write!(
                f,
                "({} {op:?} {})",
                DisplayCompiledExpr(left),
                DisplayCompiledExpr(right)
            ),
            CompiledExpr::Select(cond, then_expr, else_expr) => write!(
                f,
                "if {} then {} else {}",
                DisplayCompiledExpr(cond),
                DisplayCompiledExpr(then_expr),
                DisplayCompiledExpr(else_expr)
            ),
            CompiledExpr::CallBuiltin(name, args) => {
                write!(f, "{name:?}(")?;
                for (idx, arg) in args.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", DisplayCompiledExpr(arg))?;
                }
                write!(f, ")")
            }
        }
    }
}
