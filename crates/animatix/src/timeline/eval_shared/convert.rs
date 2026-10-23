//! Value coercion helpers.

use super::*;

/// Try to extract a numeric value, returning an error on type mismatch.
pub fn value_to_f64(value: &Value) -> Result<f64, EvalError> {
    match value {
        Value::Num(n) => Ok(*n),
        other => Err(EvalError::TypeMismatch(format!("Expected number, got {:?}", other))),
    }
}

/// Try to extract a Vec2 value, returning an error on type mismatch.
pub fn value_to_vec2(value: &Value) -> Result<[f64; 2], EvalError> {
    match value {
        Value::Vec2(v) => Ok(*v),
        other => Err(EvalError::TypeMismatch(format!("Expected Vec2, got {:?}", other))),
    }
}

/// Try to extract a Vec3 value, returning an error on type mismatch.
pub fn value_to_vec3(value: &Value) -> Result<[f64; 3], EvalError> {
    match value {
        Value::Vec3(v) => Ok(*v),
        other => Err(EvalError::TypeMismatch(format!("Expected Vec3, got {:?}", other))),
    }
}

/// Try to extract a Color value, returning an error on type mismatch.
pub fn value_to_color(value: &Value) -> Result<[f64; 4], EvalError> {
    match value {
        Value::Color(c) => Ok(*c),
        other => Err(EvalError::TypeMismatch(format!("Expected Color, got {:?}", other))),
    }
}

/// Try to extract a List value, returning an error on type mismatch.
pub fn value_to_list(value: &Value) -> Result<Vec<Value>, EvalError> {
    match value {
        Value::List(items) => Ok(items.to_vec()),
        other => Err(EvalError::TypeMismatch(format!("Expected List, got {:?}", other))),
    }
}
