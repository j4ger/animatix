use animatix::ast::Expr;
use animatix::timeline::env::{load_standard_library, Environment, Value};

use crate::app::panels::PropertyValue;

/// Verify that an expression round-trips correctly.
pub fn validate_roundtrip(expr: &Expr, expected: &PropertyValue) -> Result<(), String> {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    let value = animatix::timeline::utils::evaluate_expr(expr, &env)
        .map_err(|e| format!("Evaluation failed: {}", e))?;

    let expected_value = property_value_to_runtime(expected)?;

    if values_match(&value, &expected_value) {
        Ok(())
    } else {
        Err(format!("evaluated to {:?}, expected {:?}", value, expected_value))
    }
}

fn property_value_to_runtime(value: &PropertyValue) -> Result<Value, String> {
    Ok(match value {
        PropertyValue::Vec2(v) => Value::Vec2([v[0] as f64, v[1] as f64]),
        PropertyValue::Float(v) => Value::Num(*v as f64),
        PropertyValue::Color(v) => Value::Color([
            v[0] as f64,
            v[1] as f64,
            v[2] as f64,
            v[3] as f64,
        ]),
        PropertyValue::Text(s) => Value::Str(s.clone()),
        PropertyValue::StringList(items) => {
            Value::List(items.iter().cloned().map(Value::Str).collect())
        }
    })
}

fn values_match(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Num(a), Value::Num(b)) => (a - b).abs() < 0.000_001,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Vec2(a), Value::Vec2(b)) => a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001),
        (Value::Vec3(a), Value::Vec3(b)) => a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001),
        (Value::Vec4(a), Value::Vec4(b)) => a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001),
        (Value::Color(a), Value::Color(b)) => a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001),
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_match(x, y))
        }
        (Value::Object(name_a, fields_a), Value::Object(name_b, fields_b)) => {
            name_a == name_b
                && fields_a.len() == fields_b.len()
                && fields_a
                    .iter()
                    .all(|(k, v)| fields_b.get(k).is_some_and(|other| values_match(v, other)))
        }
        _ => actual == expected,
    }
}
