use crate::ast::{BinaryOp, Expr, Time};

/// Represents a runtime value produced by evaluating an expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    /// 2-element tuple used for positions, sizes, etc.
    Tuple2([f64; 2]),
}

impl Value {
    /// Extract as f64, returning 0.0 on type mismatch.
    pub fn as_num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => 0.0,
        }
    }

    /// Extract as String, returning empty string on type mismatch.
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        }
    }

    /// Extract as [f64; 2], returning [0.0, 0.0] on type mismatch.
    pub fn as_tuple2(&self) -> [f64; 2] {
        match self {
            Value::Tuple2(t) => *t,
            _ => [0.0, 0.0],
        }
    }
}

/// Evaluate an expression down to a runtime `Value`.
/// Handles `sin`, `cos`, and `format` (string interpolation) calls.
pub fn evaluate_expr(expr: &Expr) -> Value {
    match expr {
        Expr::Num(n) => Value::Num(*n),
        Expr::Str(s) => Value::Str(s.clone()),
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Null => Value::Num(0.0),

        Expr::Ident(name) => {
            // Resolve known constants
            match name.as_str() {
                "PI" => Value::Num(std::f64::consts::PI),
                "E" => Value::Num(std::f64::consts::E),
                "TAU" => Value::Num(std::f64::consts::TAU),
                _ => Value::Num(0.0),
            }
        }

        Expr::Tuple(items) => {
            if items.len() == 2 {
                let x = evaluate_expr(&items[0]).as_num();
                let y = evaluate_expr(&items[1]).as_num();
                Value::Tuple2([x, y])
            } else {
                Value::Str(format!("{:?}", items))
            }
        }

        Expr::Call(func, args) => evaluate_call(func, args),

        Expr::Binary(left, op, right) => {
            let l = evaluate_expr(left).as_num();
            let r = evaluate_expr(right).as_num();
            Value::Num(match op {
                BinaryOp::Add => l + r,
                BinaryOp::Sub => l - r,
                BinaryOp::Mul => l * r,
                BinaryOp::Div if r != 0.0 => l / r,
                BinaryOp::Div => 0.0,
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
            })
        }

        Expr::Unary(op, inner) => {
            let v = evaluate_expr(inner).as_num();
            Value::Num(match op {
                crate::ast::UnaryOp::Neg => -v,
                crate::ast::UnaryOp::Not => {
                    if v == 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            })
        }

        Expr::Conditional(cond, then_branch, else_branch) => {
            if evaluate_expr(cond).as_num() != 0.0 {
                evaluate_expr(then_branch)
            } else {
                evaluate_expr(else_branch)
            }
        }

        Expr::Method(_, _, _) | Expr::Path(_) | Expr::Index(_, _) | Expr::Construct(_, _) => {
            Value::Num(0.0)
        }
    }
}

/// Evaluate a function call (`sin`, `cos`, `format`).
fn evaluate_call(func: &str, args: &[Expr]) -> Value {
    match func {
        "sin" => {
            if args.len() == 1 {
                Value::Num(evaluate_expr(&args[0]).as_num().sin())
            } else {
                Value::Num(0.0)
            }
        }
        "cos" => {
            if args.len() == 1 {
                Value::Num(evaluate_expr(&args[0]).as_num().cos())
            } else {
                Value::Num(0.0)
            }
        }
        "format" => {
            // format("template {}", arg1, arg2)
            if args.is_empty() {
                return Value::Str(String::new());
            }
            let template = evaluate_expr(&args[0]).as_str();
            let mut result = String::new();
            let mut placeholder_idx = 0;
            let mut chars = template.chars().peekable();
            let arg_values: Vec<Value> = args[1..].iter().map(evaluate_expr).collect();

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
            Value::Str(result)
        }
        _ => Value::Num(0.0),
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
        Value::Tuple2(t) => format!("({}, {})", t[0], t[1]),
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
