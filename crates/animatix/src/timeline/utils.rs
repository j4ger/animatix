//! Expression evaluator for the animation timeline.
//!
//! Uses an [`Environment`] (`Rc<RefCell<HashMap>>`) for shared mutable scope.
//! Built-ins (sin, cos, lerp, rand, format) resolve through the environment.
//! Closures evaluate against a clone of the caller environment with parameter bindings added.

use crate::ast::{BinaryOp, Expr, Time};
use crate::timeline::env::{Environment, EvalError, Value};

/// Represents a runtime value produced by evaluating an expression.
pub fn evaluate_expr(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Percent(n) => Ok(Value::Num(*n / 100.0)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Num(0.0)),

        Expr::Ident(name) => env
            .get(name)
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),

        Expr::Tuple(items) => {
            if items.len() == 2 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                Ok(Value::Vec2([x, y]))
            } else if items.len() == 3 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                let z = evaluate_expr(&items[2], env)?.as_num();
                Ok(Value::Vec3([x, y, z]))
            } else if items.len() == 4 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                let z = evaluate_expr(&items[2], env)?.as_num();
                let w = evaluate_expr(&items[3], env)?.as_num();
                Ok(Value::Vec4([x, y, z, w]))
            } else {
                // Arbitrary-length tuples become lists
                let values: Result<Vec<Value>, EvalError> =
                    items.iter().map(|item| evaluate_expr(item, env)).collect();
                Ok(Value::List(values?))
            }
        }

        Expr::Call(func, args) => evaluate_call(func, args, env),

        Expr::Binary(left, op, right) => {
            let l_val = evaluate_expr(left, env)?;
            let r_val = evaluate_expr(right, env)?;

            match (l_val.clone(), r_val.clone()) {
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
                _ => {
                    if *op == BinaryOp::Eq {
                        Ok(Value::Num(if l_val == r_val { 1.0 } else { 0.0 }))
                    } else if *op == BinaryOp::Neq {
                        Ok(Value::Num(if l_val != r_val { 1.0 } else { 0.0 }))
                    } else {
                        Err(EvalError::TypeMismatch(format!(
                            "Unsupported operation {:?} between {:?} and {:?}",
                            op, l_val, r_val
                        )))
                    }
                }
            }
        }

        Expr::Unary(op, inner) => {
            let v = evaluate_expr(inner, env)?.as_num();
            Ok(Value::Num(match op {
                crate::ast::UnaryOp::Neg => -v,
                crate::ast::UnaryOp::Not => {
                    if v == 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            }))
        }

        Expr::Conditional(cond, then_branch, else_branch) => {
            if evaluate_expr(cond, env)?.as_num() != 0.0 {
                evaluate_expr(then_branch, env)
            } else {
                evaluate_expr(else_branch, env)
            }
        }

        // Closures capture by cloning the call-time environment (not a lexical snapshot).
        Expr::Closure(args, body) => Ok(Value::Closure(args.clone(), body.clone())),

        Expr::Path(parts) => {
            let dotted = parts.join(".");
            env.get(&dotted)
                .ok_or_else(|| EvalError::UndefinedVariable(dotted))
        }

        Expr::Method(receiver, name, args) => {
            let receiver_val = evaluate_expr(receiver, env)?;
            evaluate_method(receiver_val, name, args, env)
        }

        Expr::Index(container, index) => {
            let container_val = evaluate_expr(container, env)?;
            let index_val = evaluate_expr(index, env)?;
            let idx = index_val.as_num() as usize;
            match container_val {
                Value::List(items) => items
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for list of length {}",
                        idx,
                        items.len()
                    ))),
                Value::Str(s) => s
                    .chars()
                    .nth(idx)
                    .map(|c| Value::Str(c.to_string()))
                    .ok_or_else(|| EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for string of length {}",
                        idx,
                        s.len()
                    ))),
                Value::Vec2(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec2",
                        idx
                    ))),
                },
                Value::Vec3(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    2 => Ok(Value::Num(v[2])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec3",
                        idx
                    ))),
                },
                Value::Vec4(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    2 => Ok(Value::Num(v[2])),
                    3 => Ok(Value::Num(v[3])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec4",
                        idx
                    ))),
                },
                Value::Color(c) => match idx {
                    0 => Ok(Value::Num(c[0])),
                    1 => Ok(Value::Num(c[1])),
                    2 => Ok(Value::Num(c[2])),
                    3 => Ok(Value::Num(c[3])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Color",
                        idx
                    ))),
                },
                other => Err(EvalError::TypeMismatch(format!(
                    "Cannot index into {:?}",
                    other
                ))),
            }
        }

        Expr::Construct(name, properties) => {
            let mut fields = std::collections::HashMap::new();
            for prop in properties {
                let value = evaluate_expr(&prop.value, env)?;
                fields.insert(prop.name.clone(), value);
            }
            Ok(Value::Object(name.clone(), fields))
        }
    }
}

/// Evaluate a function call.
fn evaluate_call(func: &str, args: &[Expr], env: &Environment) -> Result<Value, EvalError> {
    if func == "format" {
        // format("template {}", arg1, arg2)
        if args.is_empty() {
            return Ok(Value::Str(String::new()));
        }
        let template = evaluate_expr(&args[0], env)?.as_str();
        let mut result = String::new();
        let mut placeholder_idx = 0;
        let mut chars = template.chars().peekable();

        let mut arg_values = Vec::new();
        for arg in &args[1..] {
            arg_values.push(evaluate_expr(arg, env)?);
        }

        while let Some(ch) = chars.next() {
            if ch == '{' {
                if chars.peek() == Some(&'}') {
                    chars.next(); // consume '}'
                    if placeholder_idx < arg_values.len() {
                        result.push_str(&format_value(&arg_values[placeholder_idx]));
                    }
                    placeholder_idx += 1;
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }
        return Ok(Value::Str(result));
    }

    // Look up the function in the environment
    if let Some(val) = env.get(func) {
        match val {
            Value::NativeFn(native_func) => {
                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(evaluate_expr(arg, env)?);
                }
                native_func(&arg_values, env)
            }
            // Closures capture by cloning the call-time environment.
            // Parameters are bound into the cloned environment.
            // The body evaluates against the extended clone.
            Value::Closure(params, body) => {
                if args.len() != params.len() {
                    return Err(EvalError::TypeMismatch(format!(
                        "Closure '{}' expects {} arguments, got {}",
                        func,
                        params.len(),
                        args.len()
                    )));
                }

                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(evaluate_expr(arg, env)?);
                }

                let mut child_env = env.clone();
                for (param, val) in params.iter().zip(arg_values) {
                    child_env.set(param, val);
                }

                evaluate_expr(&body, &child_env)
            }
            _ => Err(EvalError::NotCallable(func.to_string())),
        }
    } else {
        Err(EvalError::UndefinedVariable(func.to_string()))
    }
}

/// Evaluate a method call on a receiver value.
fn evaluate_method(
    receiver: Value,
    name: &str,
    args: &[Expr],
    env: &Environment,
) -> Result<Value, EvalError> {
    match (receiver, name) {
        (Value::Str(s), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(s.len() as f64))
        }
        (Value::Str(s), "split") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.split(delim) takes exactly 1 argument".to_string(),
                ));
            }
            let delim = evaluate_expr(&args[0], env)?.as_str();
            let parts: Vec<Value> = s
                .split(&delim)
                .map(|part| Value::Str(part.to_string()))
                .collect();
            Ok(Value::List(parts))
        }
        (Value::Str(s), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.contains(substr) takes exactly 1 argument".to_string(),
                ));
            }
            let substr = evaluate_expr(&args[0], env)?.as_str();
            Ok(Value::Num(if s.contains(&substr) { 1.0 } else { 0.0 }))
        }
        (Value::Str(s), "trim") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.trim() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Str(s.trim().to_string()))
        }
        (Value::Str(s), "starts_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.starts_with(prefix) takes exactly 1 argument".to_string(),
                ));
            }
            let prefix = evaluate_expr(&args[0], env)?.as_str();
            Ok(Value::Num(if s.starts_with(&prefix) { 1.0 } else { 0.0 }))
        }
        (Value::Str(s), "ends_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.ends_with(suffix) takes exactly 1 argument".to_string(),
                ));
            }
            let suffix = evaluate_expr(&args[0], env)?.as_str();
            Ok(Value::Num(if s.ends_with(&suffix) { 1.0 } else { 0.0 }))
        }
        (Value::List(items), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "List.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(items.len() as f64))
        }
        (Value::List(items), "get") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "List.get(index) takes exactly 1 argument".to_string(),
                ));
            }
            let idx = evaluate_expr(&args[0], env)?.as_num() as usize;
            items
                .get(idx)
                .cloned()
                .ok_or_else(|| {
                    EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for list of length {}",
                        idx,
                        items.len()
                    ))
                })
        }
        (Value::List(items), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "List.contains(item) takes exactly 1 argument".to_string(),
                ));
            }
            let item = evaluate_expr(&args[0], env)?;
            Ok(Value::Num(if items.contains(&item) { 1.0 } else { 0.0 }))
        }
        (Value::Num(n), "abs") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.abs() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.abs()))
        }
        (Value::Num(n), "floor") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.floor() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.floor()))
        }
        (Value::Num(n), "ceil") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.ceil() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.ceil()))
        }
        (Value::Num(n), "round") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.round() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.round()))
        }
        (receiver, name) => Err(EvalError::UnsupportedMethod(format!(
            "{}.{}()",
            format_value(&receiver),
            name
        ))),
    }
}

/// Format a single Value into its display string.
fn format_value(value: &Value) -> String {
    match value {
        Value::Num(n) => {
            if *n == n.floor() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Vec2(t) => format!("({}, {})", t[0], t[1]),
        Value::Vec3(t) => format!("({}, {}, {})", t[0], t[1], t[2]),
        Value::Vec4(t) => format!("({}, {}, {}, {})", t[0], t[1], t[2], t[3]),
        Value::Color(c) => format!("rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
        Value::List(items) => format!("{:?}", items),
        Value::Object(name, fields) => format!("{}({:?})", name, fields),
        Value::NativeFn(_) => "<NativeFn>".to_string(),
        Value::Closure(args, _) => format!("<Closure({:?})>", args),
    }
}

pub fn parse_color(expr: &Expr) -> [f32; 4] {
    parse_color_in_env(expr, &Environment::new())
}

fn named_color(name: &str) -> Option<[f32; 4]> {
    match name {
        "red" | "RED" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" | "GREEN" => Some([0.0, 1.0, 0.0, 1.0]),
        "blue" | "BLUE" => Some([0.0, 0.0, 1.0, 1.0]),
        "black" | "BLACK" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" | "WHITE" => Some([1.0, 1.0, 1.0, 1.0]),
        "yellow" | "YELLOW" => Some([1.0, 1.0, 0.0, 1.0]),
        "orange" | "ORANGE" => Some([1.0, 0.65, 0.0, 1.0]),
        _ => None,
    }
}

fn color_from_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Color([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec4([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec3([r, g, b]) => Some([r as f32, g as f32, b as f32, 1.0]),
        _ => None,
    }
}

pub fn resolve_color_in_env(expr: &Expr, env: &Environment) -> Result<Option<[f32; 4]>, EvalError> {
    if let Expr::Ident(name) = expr
        && let Some(color) = named_color(name)
    {
        return Ok(Some(color));
    }

    evaluate_expr(expr, env).map(color_from_value)
}

pub fn parse_color_in_env(expr: &Expr, env: &Environment) -> [f32; 4] {
    resolve_color_in_env(expr, env)
        .ok()
        .flatten()
        .unwrap_or([0.8, 0.8, 0.8, 1.0])
}

pub fn time_to_ms(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOp;

    #[test]
    fn test_evaluate_closure() {
        let mut env = Environment::new();
        let closure = Value::Closure(
            vec!["x".to_string()],
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            )),
        );
        env.set("f", closure);

        let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
        let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

        assert_eq!(result, Value::Num(8.0));
    }

    #[test]
    fn test_evaluate_method_string_length() {
        let mut env = Environment::new();
        env.set("text", Value::Str("hello".to_string()));
        let expr = Expr::Method(
            Box::new(Expr::Ident("text".to_string())),
            "length".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 5.0);
    }

    #[test]
    fn test_evaluate_method_string_split() {
        let mut env = Environment::new();
        env.set("text", Value::Str("a,b,c".to_string()));
        let expr = Expr::Method(
            Box::new(Expr::Ident("text".to_string())),
            "split".to_string(),
            vec![Expr::Str(",".to_string())],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        let list = result.as_list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].as_str(), "a");
        assert_eq!(list[1].as_str(), "b");
        assert_eq!(list[2].as_str(), "c");
    }

    #[test]
    fn test_evaluate_method_list_length() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(1.0), Value::Num(2.0)]));
        let expr = Expr::Method(
            Box::new(Expr::Ident("items".to_string())),
            "length".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 2.0);
    }

    #[test]
    fn test_evaluate_method_list_get() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)]));
        let expr = Expr::Method(
            Box::new(Expr::Ident("items".to_string())),
            "get".to_string(),
            vec![Expr::Num(2.0)],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 30.0);
    }

    #[test]
    fn test_evaluate_method_num_abs() {
        let mut env = Environment::new();
        env.set("x", Value::Num(-42.5));
        let expr = Expr::Method(
            Box::new(Expr::Ident("x".to_string())),
            "abs".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 42.5);
    }

    #[test]
    fn test_evaluate_method_unsupported() {
        let env = Environment::new();
        let expr = Expr::Method(
            Box::new(Expr::Ident("graph".to_string())),
            "plot".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_index_on_list() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("items".to_string())),
            Box::new(Expr::Num(1.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 20.0);
    }

    #[test]
    fn test_evaluate_index_on_vec2() {
        let mut env = Environment::new();
        env.set("pos", Value::Vec2([100.0, 200.0]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("pos".to_string())),
            Box::new(Expr::Num(0.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 100.0);
    }

    #[test]
    fn test_evaluate_index_on_string() {
        let mut env = Environment::new();
        env.set("text", Value::Str("hello".to_string()));
        let expr = Expr::Index(
            Box::new(Expr::Ident("text".to_string())),
            Box::new(Expr::Num(1.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_str(), "e");
    }

    #[test]
    fn test_evaluate_index_out_of_bounds() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0)]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("items".to_string())),
            Box::new(Expr::Num(5.0)),
        );

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_construct_creates_object() {
        let env = Environment::new();
        let expr = Expr::Construct(
            "Point".to_string(),
            vec![
                crate::ast::Property {
                    name: "x".to_string(),
                    value: Expr::Num(10.0),
                    value_span: None,
                    trailing_comment: None,
                },
                crate::ast::Property {
                    name: "y".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        match result {
            Value::Object(name, fields) => {
                assert_eq!(name, "Point");
                assert_eq!(fields.get("x").unwrap().as_num(), 10.0);
                assert_eq!(fields.get("y").unwrap().as_num(), 20.0);
            }
            other => panic!("Expected Object, got: {:?}", other),
        }
    }

    #[test]
    fn test_evaluate_closure_uses_call_time_environment() {
        let mut env = Environment::new();
        env.set(
            "f",
            Value::Closure(
                vec!["x".to_string()],
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Add,
                    Box::new(Expr::Ident("y".to_string())),
                )),
            ),
        );
        env.set("y", Value::Num(3.0));
        env.set("y", Value::Num(10.0));

        let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
        let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

        assert_eq!(result, Value::Num(14.0));
    }

    #[test]
    fn test_evaluate_path_uses_flat_dotted_lookup_key() {
        let mut env = Environment::new();
        env.set("node.at.x", Value::Num(320.0));

        let expr = Expr::Path(vec!["node".to_string(), "at".to_string(), "x".to_string()]);
        let result = evaluate_expr(&expr, &env).expect("path lookup should succeed");

        assert_eq!(result, Value::Num(320.0));
    }
}
