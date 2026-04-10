use crate::timeline::env::{Environment, EvalError, Value};
use crate::ast::{BinaryOp, Expr, Time};

/// Represents a runtime value produced by evaluating an expression.
pub fn evaluate_expr(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Num(0.0)),

        Expr::Ident(name) => {
            env.get(name).ok_or_else(|| EvalError::UndefinedVariable(name.clone()))
        }

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
                Ok(Value::Str(format!("{:?}", items)))
            }
        }

        Expr::Call(func, args) => evaluate_call(func, args, env),

        Expr::Binary(left, op, right) => {
            let l_val = evaluate_expr(left, env)?;
            let r_val = evaluate_expr(right, env)?;
            
            match (l_val.clone(), r_val.clone()) {
                (Value::Num(l), Value::Num(r)) => {
                    Ok(Value::Num(match op {
                        BinaryOp::Add => l + r,
                        BinaryOp::Sub => l - r,
                        BinaryOp::Mul => l * r,
                        BinaryOp::Div if r != 0.0 => l / r,
                        BinaryOp::Div => 0.0,
                        BinaryOp::Mod if r != 0.0 => l % r,
                        BinaryOp::Mod => 0.0,
                        BinaryOp::Pow => l.powf(r),
                        BinaryOp::Eq => if l == r { 1.0 } else { 0.0 },
                        BinaryOp::Neq => if l != r { 1.0 } else { 0.0 },
                        BinaryOp::Lt => if l < r { 1.0 } else { 0.0 },
                        BinaryOp::Gt => if l > r { 1.0 } else { 0.0 },
                        BinaryOp::Lte => if l <= r { 1.0 } else { 0.0 },
                        BinaryOp::Gte => if l >= r { 1.0 } else { 0.0 },
                        BinaryOp::And => if l != 0.0 && r != 0.0 { 1.0 } else { 0.0 },
                        BinaryOp::Or => if l != 0.0 || r != 0.0 { 1.0 } else { 0.0 },
                    }))
                }
                (Value::Vec2(l), Value::Vec2(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec2([l[0] + r[0], l[1] + r[1]])),
                        BinaryOp::Sub => Ok(Value::Vec2([l[0] - r[0], l[1] - r[1]])),
                        BinaryOp::Mul => Ok(Value::Vec2([l[0] * r[0], l[1] * r[1]])),
                        BinaryOp::Div => Ok(Value::Vec2([l[0] / r[0], l[1] / r[1]])),
                        BinaryOp::Mod => Ok(Value::Vec2([l[0] % r[0], l[1] % r[1]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Vec2 and Vec2", op))),
                    }
                }
                (Value::Vec3(l), Value::Vec3(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec3([l[0] + r[0], l[1] + r[1], l[2] + r[2]])),
                        BinaryOp::Sub => Ok(Value::Vec3([l[0] - r[0], l[1] - r[1], l[2] - r[2]])),
                        BinaryOp::Mul => Ok(Value::Vec3([l[0] * r[0], l[1] * r[1], l[2] * r[2]])),
                        BinaryOp::Div => Ok(Value::Vec3([l[0] / r[0], l[1] / r[1], l[2] / r[2]])),
                        BinaryOp::Mod => Ok(Value::Vec3([l[0] % r[0], l[1] % r[1], l[2] % r[2]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Vec3 and Vec3", op))),
                    }
                }
                (Value::Color(l), Value::Color(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Color([l[0] + r[0], l[1] + r[1], l[2] + r[2], l[3] + r[3]])),
                        BinaryOp::Sub => Ok(Value::Color([l[0] - r[0], l[1] - r[1], l[2] - r[2], l[3] - r[3]])),
                        BinaryOp::Mul => Ok(Value::Color([l[0] * r[0], l[1] * r[1], l[2] * r[2], l[3] * r[3]])),
                        BinaryOp::Div => Ok(Value::Color([l[0] / r[0], l[1] / r[1], l[2] / r[2], l[3] / r[3]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Color and Color", op))),
                    }
                }
                (Value::Vec2(l), Value::Num(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec2([l[0] + r, l[1] + r])),
                        BinaryOp::Sub => Ok(Value::Vec2([l[0] - r, l[1] - r])),
                        BinaryOp::Mul => Ok(Value::Vec2([l[0] * r, l[1] * r])),
                        BinaryOp::Div => Ok(Value::Vec2([l[0] / r, l[1] / r])),
                        BinaryOp::Mod => Ok(Value::Vec2([l[0] % r, l[1] % r])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Vec2 and Num", op))),
                    }
                }
                (Value::Num(l), Value::Vec2(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec2([l + r[0], l + r[1]])),
                        BinaryOp::Sub => Ok(Value::Vec2([l - r[0], l - r[1]])),
                        BinaryOp::Mul => Ok(Value::Vec2([l * r[0], l * r[1]])),
                        BinaryOp::Div => Ok(Value::Vec2([l / r[0], l / r[1]])),
                        BinaryOp::Mod => Ok(Value::Vec2([l % r[0], l % r[1]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Num and Vec2", op))),
                    }
                }
                (Value::Vec3(l), Value::Num(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec3([l[0] + r, l[1] + r, l[2] + r])),
                        BinaryOp::Sub => Ok(Value::Vec3([l[0] - r, l[1] - r, l[2] - r])),
                        BinaryOp::Mul => Ok(Value::Vec3([l[0] * r, l[1] * r, l[2] * r])),
                        BinaryOp::Div => Ok(Value::Vec3([l[0] / r, l[1] / r, l[2] / r])),
                        BinaryOp::Mod => Ok(Value::Vec3([l[0] % r, l[1] % r, l[2] % r])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Vec3 and Num", op))),
                    }
                }
                (Value::Num(l), Value::Vec3(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Vec3([l + r[0], l + r[1], l + r[2]])),
                        BinaryOp::Sub => Ok(Value::Vec3([l - r[0], l - r[1], l - r[2]])),
                        BinaryOp::Mul => Ok(Value::Vec3([l * r[0], l * r[1], l * r[2]])),
                        BinaryOp::Div => Ok(Value::Vec3([l / r[0], l / r[1], l / r[2]])),
                        BinaryOp::Mod => Ok(Value::Vec3([l % r[0], l % r[1], l % r[2]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Num and Vec3", op))),
                    }
                }
                (Value::Color(l), Value::Num(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Color([l[0] + r, l[1] + r, l[2] + r, l[3] + r])),
                        BinaryOp::Sub => Ok(Value::Color([l[0] - r, l[1] - r, l[2] - r, l[3] - r])),
                        BinaryOp::Mul => Ok(Value::Color([l[0] * r, l[1] * r, l[2] * r, l[3] * r])),
                        BinaryOp::Div => Ok(Value::Color([l[0] / r, l[1] / r, l[2] / r, l[3] / r])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Color and Num", op))),
                    }
                }
                (Value::Num(l), Value::Color(r)) => {
                    match op {
                        BinaryOp::Add => Ok(Value::Color([l + r[0], l + r[1], l + r[2], l + r[3]])),
                        BinaryOp::Sub => Ok(Value::Color([l - r[0], l - r[1], l - r[2], l - r[3]])),
                        BinaryOp::Mul => Ok(Value::Color([l * r[0], l * r[1], l * r[2], l * r[3]])),
                        BinaryOp::Div => Ok(Value::Color([l / r[0], l / r[1], l / r[2], l / r[3]])),
                        _ => Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} for Num and Color", op))),
                    }
                }
                _ => {
                    // Fallback to old behavior for anything else just in case? Or type mismatch
                    // Let's just treat as nums for == and != as a fallback or return TypeMismatch
                    if *op == BinaryOp::Eq {
                        Ok(Value::Num(if l_val == r_val { 1.0 } else { 0.0 }))
                    } else if *op == BinaryOp::Neq {
                        Ok(Value::Num(if l_val != r_val { 1.0 } else { 0.0 }))
                    } else {
                        Err(EvalError::TypeMismatch(format!("Unsupported operation {:?} between {:?} and {:?}", op, l_val, r_val)))
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

        Expr::Method(_, _, _) | Expr::Path(_) | Expr::Index(_, _) | Expr::Construct(_, _) => {
            Ok(Value::Num(0.0))
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
        if let Value::NativeFn(native_func) = val {
            let mut arg_values = Vec::new();
            for arg in args {
                arg_values.push(evaluate_expr(arg, env)?);
            }
            native_func(&arg_values, env)
        } else {
            Err(EvalError::NotCallable(func.to_string()))
        }
    } else {
        Err(EvalError::UndefinedVariable(func.to_string()))
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
        Value::NativeFn(_) => "<NativeFn>".to_string(),
    }
}

pub fn parse_color(expr: &Expr) -> [f32; 4] {
    if let Expr::Ident(name) = expr {
        match name.as_str() {
            "red" => [1.0, 0.0, 0.0, 1.0],
            "green" => [0.0, 1.0, 0.0, 1.0],
            "blue" => [0.0, 0.0, 1.0, 1.0],
            "black" => [0.0, 0.0, 0.0, 1.0],
            "white" => [1.0, 1.0, 1.0, 1.0],
            "yellow" => [1.0, 1.0, 0.0, 1.0],
            _ => [0.8, 0.8, 0.8, 1.0],
        }
    } else {
        [0.8, 0.8, 0.8, 1.0]
    }
}

pub fn time_to_ms(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
    }
}
