use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::UnaryOp;
use crate::timeline::eval_shared::{eval_binary_op, eval_builtin_fn};
use crate::timeline::modifier_runtime::ir::{BuiltinFn, CompiledExpr};
use crate::timeline::{Timeline, Value};

pub(super) struct Folder<'a> {
    timeline: &'a Timeline,
    shadowed: HashSet<String>,
    pub(super) guards: HashMap<String, Value>,
}

impl<'a> Folder<'a> {
    pub(super) fn new(timeline: &'a Timeline, shadowed: HashSet<String>) -> Self {
        Self {
            timeline,
            shadowed,
            guards: HashMap::new(),
        }
    }

    pub(super) fn expression(
        &mut self,
        expr: &CompiledExpr,
        locals: &HashMap<String, Value>,
    ) -> CompiledExpr {
        match expr {
            CompiledExpr::LoadEnv(name) => locals
                .get(name)
                .cloned()
                .or_else(|| self.global(name))
                .map_or_else(|| expr.clone(), CompiledExpr::Const),
            CompiledExpr::Unary(op, inner) => {
                let inner = self.expression(inner, locals);
                match (op, &inner) {
                    (UnaryOp::Neg, CompiledExpr::Const(Value::Num(number))) => {
                        CompiledExpr::Const(Value::Num(-number))
                    },
                    (UnaryOp::Not, CompiledExpr::Const(value)) => {
                        CompiledExpr::Const(Value::Bool(!value.is_truthy()))
                    },
                    _ => CompiledExpr::Unary(op.clone(), Box::new(inner)),
                }
            },
            CompiledExpr::Binary(left, op, right) => {
                let left = self.expression(left, locals);
                let right = self.expression(right, locals);
                if let (CompiledExpr::Const(l), CompiledExpr::Const(r)) = (&left, &right)
                    && let Ok(value) = eval_binary_op(l.clone(), op, r.clone())
                {
                    return CompiledExpr::Const(value);
                }
                CompiledExpr::Binary(Box::new(left), op.clone(), Box::new(right))
            },
            CompiledExpr::Select(condition, yes, no) => {
                let condition = self.expression(condition, locals);
                if let CompiledExpr::Const(value) = &condition {
                    return self.expression(if value.is_truthy() { yes } else { no }, locals);
                }
                CompiledExpr::Select(
                    Box::new(condition),
                    Box::new(self.expression(yes, locals)),
                    Box::new(self.expression(no, locals)),
                )
            },
            CompiledExpr::MakeList(items) => {
                let items: Vec<_> =
                    items.iter().map(|item| self.expression(item, locals)).collect();
                if let Some(values) = constants(&items) {
                    return CompiledExpr::Const(Value::List(values.into()));
                }
                CompiledExpr::MakeList(items)
            },
            CompiledExpr::MakeVec(items) => CompiledExpr::MakeVec(
                items.iter().map(|item| self.expression(item, locals)).collect(),
            ),
            CompiledExpr::Field(base, field) => {
                let base = self.expression(base, locals);
                if let CompiledExpr::Const(value) = &base
                    && let Some(value) = value.read_field(field)
                {
                    return CompiledExpr::Const(value);
                }
                CompiledExpr::Field(Box::new(base), field.clone())
            },
            CompiledExpr::CallBuiltin(builtin, args) => {
                let args: Vec<_> = args.iter().map(|arg| self.expression(arg, locals)).collect();
                if let Some(name) = pure_builtin(builtin)
                    && let Some(values) = constants(&args)
                    && values.iter().all(|value| matches!(value, Value::Num(_)))
                    && !locals.contains_key(name)
                    && !self.shadowed.contains(name)
                    && let Some(binding) = self.timeline.env_base.get(name)
                    && crate::timeline::builtins::is_standard_binding(name, Some(binding))
                    && let Ok(value) = eval_builtin_fn(name, &values)
                {
                    self.guards.entry(name.to_owned()).or_insert_with(|| binding.clone());
                    return CompiledExpr::Const(value);
                }
                CompiledExpr::CallBuiltin(builtin.clone(), args)
            },
            CompiledExpr::CallNamed(name, args) => CompiledExpr::CallNamed(
                name.clone(),
                args.iter()
                    .map(|(name, arg)| (name.clone(), self.expression(arg, locals)))
                    .collect(),
            ),
            _ => expr.clone(), // Geometry, nested lexical scopes and dynamic callables stay live.
        }
    }

    fn global(&mut self, name: &str) -> Option<Value> {
        if matches!(name, "t" | "scene_width" | "scene_height")
            || name.contains('.')
            || self.shadowed.contains(name)
        {
            return None;
        }
        let value = self.timeline.env_base.get(name)?;
        if !is_constant(value) {
            return None;
        }
        self.guards.entry(name.to_owned()).or_insert_with(|| value.clone());
        Some(value.clone())
    }
}

fn constants(exprs: &[CompiledExpr]) -> Option<Vec<Value>> {
    exprs
        .iter()
        .map(|expr| match expr {
            CompiledExpr::Const(value) => Some(value.clone()),
            _ => None, // A dynamic operand is evaluated in the VM.
        })
        .collect()
}

fn pure_builtin(builtin: &BuiltinFn) -> Option<&'static str> {
    Some(match builtin {
        BuiltinFn::Sin => "sin",
        BuiltinFn::Cos => "cos",
        BuiltinFn::Sqrt => "sqrt",
        BuiltinFn::Abs => "abs",
        BuiltinFn::Exp => "exp",
        BuiltinFn::Min => "min",
        BuiltinFn::Max => "max",
        BuiltinFn::Pow => "pow",
        _ => return None, // Extend only for functions with audited numeric semantics.
    })
}

fn is_constant(value: &Value) -> bool {
    match value {
        Value::Num(_) | Value::Bool(_) | Value::Str(_) => true,
        Value::List(items) => items.iter().all(is_constant),
        _ => false, // Actor references and captures can have scene-dependent behavior.
    }
}

pub(super) fn same_value(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Num(a), Value::Num(b)) => a.to_bits() == b.to_bits(),
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            Arc::ptr_eq(a, b)
                || (a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| same_value(a, b)))
        },
        (Value::NativeFn(a), Value::NativeFn(b)) => Arc::ptr_eq(a, b),
        _ => false, // Unexpected types restore normal execution.
    }
}
