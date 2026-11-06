//! Shared binary-operation evaluation.

use super::*;

/// Evaluate a binary operation on two runtime values.
pub fn eval_binary_op(left: Value, op: &BinaryOp, right: Value) -> Result<Value, EvalError> {
    // `and`/`or` are truthiness-based and accept both `Num` and `Bool` operands,
    // so handle them before the type-specific arithmetic arms below.
    match op {
        BinaryOp::And => {
            return Ok(Value::Num(if left.is_truthy() && right.is_truthy() {
                1.0
            } else {
                0.0
            }));
        },
        BinaryOp::Or => {
            return Ok(Value::Num(if left.is_truthy() || right.is_truthy() {
                1.0
            } else {
                0.0
            }));
        },
        _ => {},
    }

    match (left, right) {
        (Value::Num(l), Value::Num(r)) => eval_num_num(l, r, op),
        (Value::Vec2(l), Value::Vec2(r)) => eval_vec2_vec2(l, r, op),
        (Value::Vec3(l), Value::Vec3(r)) => eval_vec3_vec3(l, r, op),
        (Value::Color(l), Value::Color(r)) => eval_color_color(l, r, op),
        (Value::Vec2(l), Value::Num(r)) => eval_vec2_num(l, r, op),
        (Value::Num(l), Value::Vec2(r)) => eval_num_vec2(l, r, op),
        (Value::Vec3(l), Value::Num(r)) => eval_vec3_num(l, r, op),
        (Value::Num(l), Value::Vec3(r)) => eval_num_vec3(l, r, op),
        (Value::Color(l), Value::Num(r)) => eval_color_num(l, r, op),
        (Value::Num(l), Value::Color(r)) => eval_num_color(l, r, op),
        // Catch-all: Eq/Neq works on any pair via PartialEq;
        // everything else is a type error.
        (left, right) => {
            if *op == BinaryOp::Eq {
                Ok(Value::Num(if left == right { 1.0 } else { 0.0 }))
            } else if *op == BinaryOp::Neq {
                Ok(Value::Num(if left != right { 1.0 } else { 0.0 }))
            } else {
                Err(EvalError::TypeMismatch(format!(
                    "Unsupported operation {:?} for {:?} and {:?}",
                    op, left, right
                )))
            }
        },
    }
}

/// `Num` op `Num`: arithmetic plus the comparison family.
fn eval_num_num(l: f64, r: f64, op: &BinaryOp) -> Result<Value, EvalError> {
    Ok(Value::Num(match op {
        BinaryOp::Add => l + r,
        BinaryOp::Sub => l - r,
        BinaryOp::Mul => l * r,
        BinaryOp::Div => safe_div(l, r),
        BinaryOp::Mod => safe_rem(l, r),
        BinaryOp::Pow => l.powf(r),
        BinaryOp::Eq => {
            if l == r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Neq => {
            if l != r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Lt => {
            if l < r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Gt => {
            if l > r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Lte => {
            if l <= r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Gte => {
            if l >= r {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::And => {
            if l != 0.0 && r != 0.0 {
                1.0
            } else {
                0.0
            }
        },
        BinaryOp::Or => {
            if l != 0.0 || r != 0.0 {
                1.0
            } else {
                0.0
            }
        },
    }))
}

/// `Vec2` op `Vec2`: component-wise arithmetic.
fn eval_vec2_vec2(l: [f64; 2], r: [f64; 2], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec2([l[0] + r[0], l[1] + r[1]])),
        BinaryOp::Sub => Ok(Value::Vec2([l[0] - r[0], l[1] - r[1]])),
        BinaryOp::Mul => Ok(Value::Vec2([l[0] * r[0], l[1] * r[1]])),
        BinaryOp::Div => Ok(Value::Vec2([safe_div(l[0], r[0]), safe_div(l[1], r[1])])),
        BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l[0], r[0]), safe_rem(l[1], r[1])])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Vec2 and Vec2",
            op
        ))),
    }
}

/// `Vec3` op `Vec3`: component-wise arithmetic.
fn eval_vec3_vec3(l: [f64; 3], r: [f64; 3], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec3([l[0] + r[0], l[1] + r[1], l[2] + r[2]])),
        BinaryOp::Sub => Ok(Value::Vec3([l[0] - r[0], l[1] - r[1], l[2] - r[2]])),
        BinaryOp::Mul => Ok(Value::Vec3([l[0] * r[0], l[1] * r[1], l[2] * r[2]])),
        BinaryOp::Div => Ok(Value::Vec3([
            safe_div(l[0], r[0]),
            safe_div(l[1], r[1]),
            safe_div(l[2], r[2]),
        ])),
        BinaryOp::Mod => Ok(Value::Vec3([
            safe_rem(l[0], r[0]),
            safe_rem(l[1], r[1]),
            safe_rem(l[2], r[2]),
        ])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Vec3 and Vec3",
            op
        ))),
    }
}

/// `Color` op `Color`: component-wise arithmetic.
fn eval_color_color(l: [f64; 4], r: [f64; 4], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Color([l[0] + r[0], l[1] + r[1], l[2] + r[2], l[3] + r[3]])),
        BinaryOp::Sub => Ok(Value::Color([l[0] - r[0], l[1] - r[1], l[2] - r[2], l[3] - r[3]])),
        BinaryOp::Mul => Ok(Value::Color([l[0] * r[0], l[1] * r[1], l[2] * r[2], l[3] * r[3]])),
        BinaryOp::Div => Ok(Value::Color([
            safe_div(l[0], r[0]),
            safe_div(l[1], r[1]),
            safe_div(l[2], r[2]),
            safe_div(l[3], r[3]),
        ])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Color and Color",
            op
        ))),
    }
}

/// `Vec2` op `Num`: scalar broadcast.
fn eval_vec2_num(l: [f64; 2], r: f64, op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec2([l[0] + r, l[1] + r])),
        BinaryOp::Sub => Ok(Value::Vec2([l[0] - r, l[1] - r])),
        BinaryOp::Mul => Ok(Value::Vec2([l[0] * r, l[1] * r])),
        BinaryOp::Div => Ok(Value::Vec2([safe_div(l[0], r), safe_div(l[1], r)])),
        BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l[0], r), safe_rem(l[1], r)])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Vec2 and Num",
            op
        ))),
    }
}

/// `Num` op `Vec2`: scalar broadcast.
fn eval_num_vec2(l: f64, r: [f64; 2], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec2([l + r[0], l + r[1]])),
        BinaryOp::Sub => Ok(Value::Vec2([l - r[0], l - r[1]])),
        BinaryOp::Mul => Ok(Value::Vec2([l * r[0], l * r[1]])),
        BinaryOp::Div => Ok(Value::Vec2([safe_div(l, r[0]), safe_div(l, r[1])])),
        BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l, r[0]), safe_rem(l, r[1])])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Num and Vec2",
            op
        ))),
    }
}

/// `Vec3` op `Num`: scalar broadcast.
fn eval_vec3_num(l: [f64; 3], r: f64, op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec3([l[0] + r, l[1] + r, l[2] + r])),
        BinaryOp::Sub => Ok(Value::Vec3([l[0] - r, l[1] - r, l[2] - r])),
        BinaryOp::Mul => Ok(Value::Vec3([l[0] * r, l[1] * r, l[2] * r])),
        BinaryOp::Div => Ok(Value::Vec3([safe_div(l[0], r), safe_div(l[1], r), safe_div(l[2], r)])),
        BinaryOp::Mod => Ok(Value::Vec3([safe_rem(l[0], r), safe_rem(l[1], r), safe_rem(l[2], r)])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Vec3 and Num",
            op
        ))),
    }
}

/// `Num` op `Vec3`: scalar broadcast.
fn eval_num_vec3(l: f64, r: [f64; 3], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Vec3([l + r[0], l + r[1], l + r[2]])),
        BinaryOp::Sub => Ok(Value::Vec3([l - r[0], l - r[1], l - r[2]])),
        BinaryOp::Mul => Ok(Value::Vec3([l * r[0], l * r[1], l * r[2]])),
        BinaryOp::Div => Ok(Value::Vec3([safe_div(l, r[0]), safe_div(l, r[1]), safe_div(l, r[2])])),
        BinaryOp::Mod => Ok(Value::Vec3([safe_rem(l, r[0]), safe_rem(l, r[1]), safe_rem(l, r[2])])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Num and Vec3",
            op
        ))),
    }
}

/// `Color` op `Num`: scalar broadcast.
fn eval_color_num(l: [f64; 4], r: f64, op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Color([l[0] + r, l[1] + r, l[2] + r, l[3] + r])),
        BinaryOp::Sub => Ok(Value::Color([l[0] - r, l[1] - r, l[2] - r, l[3] - r])),
        BinaryOp::Mul => Ok(Value::Color([l[0] * r, l[1] * r, l[2] * r, l[3] * r])),
        BinaryOp::Div => Ok(Value::Color([
            safe_div(l[0], r),
            safe_div(l[1], r),
            safe_div(l[2], r),
            safe_div(l[3], r),
        ])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Color and Num",
            op
        ))),
    }
}

/// `Num` op `Color`: scalar broadcast.
fn eval_num_color(l: f64, r: [f64; 4], op: &BinaryOp) -> Result<Value, EvalError> {
    match op {
        BinaryOp::Add => Ok(Value::Color([l + r[0], l + r[1], l + r[2], l + r[3]])),
        BinaryOp::Sub => Ok(Value::Color([l - r[0], l - r[1], l - r[2], l - r[3]])),
        BinaryOp::Mul => Ok(Value::Color([l * r[0], l * r[1], l * r[2], l * r[3]])),
        BinaryOp::Div => Ok(Value::Color([
            safe_div(l, r[0]),
            safe_div(l, r[1]),
            safe_div(l, r[2]),
            safe_div(l, r[3]),
        ])),
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for Num and Color",
            op
        ))),
    }
}
