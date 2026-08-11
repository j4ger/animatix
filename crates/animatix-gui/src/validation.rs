use animatix::timeline::env::{Environment, Value};
use animatix::timeline::load_standard_library;
use animatix_syntax::ast::Expr;

use crate::app::panels::PropertyValue;

/// Verify that an expression round-trips correctly.
pub(crate) fn validate_roundtrip(expr: &Expr, expected: &PropertyValue) -> Result<(), String> {
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
        PropertyValue::F32(v) => Value::Num(*v as f64),
        PropertyValue::U32(v) => Value::Num(*v as f64),
        PropertyValue::Bool(v) => Value::Bool(*v),
        PropertyValue::Color(v) | PropertyValue::Vec4(v) => {
            Value::Color([v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64])
        },
        PropertyValue::String(s) => Value::Str(s.clone()),
        PropertyValue::Enum(s) => Value::Str(s.clone()),
        PropertyValue::StringList(items) => {
            Value::List(items.iter().cloned().map(Value::Str).collect())
        },
        PropertyValue::PointList(points) => {
            Value::List(points.iter().map(|&[x, y]| Value::Vec2([x as f64, y as f64])).collect())
        },
        PropertyValue::Transform(matrix) => {
            Value::List(matrix.iter().map(|v| Value::Num(*v as f64)).collect())
        },
        PropertyValue::Variant { value, .. } => property_value_to_runtime(value)?,
        other => return Err(format!("unsupported property value {other:?}")),
    })
}

fn values_match(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Num(a), Value::Num(b)) => (a - b).abs() < 0.000_001,
        (Value::Str(a), Value::Str(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Vec2(a), Value::Vec2(b)) => {
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001)
        },
        (Value::Vec3(a), Value::Vec3(b)) => {
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001)
        },
        (Value::Vec4(a), Value::Vec4(b)) => {
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001)
        },
        (Value::Color(a), Value::Color(b)) => {
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.000_001)
        },
        (Value::List(a), Value::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_match(x, y))
        },
        (Value::Object(name_a, fields_a), Value::Object(name_b, fields_b)) => {
            name_a == name_b
                && fields_a.len() == fields_b.len()
                && fields_a
                    .iter()
                    .all(|(k, v)| fields_b.get(k).is_some_and(|other| values_match(v, other)))
        },
        _ => actual == expected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u32_transform_and_variant_convert_to_runtime_values() {
        assert_eq!(property_value_to_runtime(&PropertyValue::U32(7)).unwrap(), Value::Num(7.0));
        assert_eq!(
            property_value_to_runtime(&PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 2.0, 3.0]))
                .unwrap(),
            Value::List(vec![
                Value::Num(1.0),
                Value::Num(0.0),
                Value::Num(0.0),
                Value::Num(1.0),
                Value::Num(2.0),
                Value::Num(3.0),
            ])
        );
        assert_eq!(
            property_value_to_runtime(&PropertyValue::Variant {
                name: "small".to_string(),
                value: Box::new(PropertyValue::F32(0.5)),
            })
            .unwrap(),
            Value::Num(0.5)
        );
    }

    #[test]
    fn unsupported_values_return_explicit_error() {
        let err = property_value_to_runtime(&PropertyValue::CommandList("M 0 0".to_string()))
            .unwrap_err();
        assert!(err.contains("unsupported"), "error should be explicit: {err}");
    }
}
