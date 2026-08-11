use crate::ast::BinaryOp;
use crate::timeline::{Environment, EvalError, Value};

/// Evaluate a method call on a receiver value.
pub(crate) fn eval_method(
    receiver: Value,
    name: &str,
    args: &[Value],
    env: &Environment,
) -> Result<Value, EvalError> {
    crate::timeline::utils::eval_method_dispatch(receiver, name, args, env)
}

pub(crate) fn apply_binary_op(
    left: Value,
    op: &BinaryOp,
    right: Value,
) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_binary_op(left, op, right)
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
        _ => Value::List(values),
    }
}

// Builtin wrappers retained for the bytecode VM. The implementations all
// delegate to the shared expression evaluator.
pub(crate) fn eval_sin(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("sin", args)
}

pub(crate) fn eval_cos(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("cos", args)
}

pub(crate) fn eval_lerp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("lerp", args)
}

pub(crate) fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("format", args)
}

pub(crate) fn eval_tan(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("tan", args)
}

pub(crate) fn eval_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("sqrt", args)
}

pub(crate) fn eval_exp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("exp", args)
}

pub(crate) fn eval_log(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("ln", args)
}

pub(crate) fn eval_atan2(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("atan2", args)
}

pub(crate) fn eval_clamp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("clamp", args)
}

pub(crate) fn eval_abs(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("abs", args)
}

pub(crate) fn eval_min(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("min", args)
}

pub(crate) fn eval_max(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("max", args)
}

pub(crate) fn eval_floor(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("floor", args)
}

pub(crate) fn eval_ceil(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("ceil", args)
}

pub(crate) fn eval_deg(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("deg", args)
}

pub(crate) fn eval_rad(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("rad", args)
}
