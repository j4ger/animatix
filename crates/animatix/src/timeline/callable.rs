//! Runtime binding and validation for descriptor-backed callables.

use animatix_core::callable::{CallableDescriptor, CallableParam, CallableType, DefaultValue};

use crate::timeline::env::{EvalError, Value};

/// Bind already evaluated arguments into descriptor order.
pub(crate) fn bind_descriptor_arguments(
    name: &str,
    params: &[CallableParam],
    arguments: Vec<(Option<String>, Value)>,
) -> Result<Vec<Value>, EvalError> {
    use animatix_core::callable::{ArgumentParameter, ArgumentSlot, bind_arguments};
    let parameters = params
        .iter()
        .map(|param| ArgumentParameter {
            name: &param.name,
            variadic: param.variadic,
        })
        .collect::<Vec<_>>();
    let slots = bind_arguments(name, &parameters, arguments).map_err(EvalError::TypeMismatch)?;
    params
        .iter()
        .zip(slots)
        .map(|(param, slot)| {
            let subject = format!("parameter '{}' of '{name}'", param.name);
            let value = match slot {
                ArgumentSlot::Variadic(values) => {
                    let values = values
                        .into_iter()
                        .map(|value| coerce_value(&param.ty, value, &subject))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(Value::List(values.into()));
                },
                ArgumentSlot::Single(Some(value)) => value,
                ArgumentSlot::Single(None) => {
                    if let Some(default) = &param.default {
                        default_value(default, &param.ty)
                    } else if param.optional {
                        return Ok(Value::Null);
                    } else {
                        return Err(EvalError::TypeMismatch(format!(
                            "call to '{name}' is missing required parameter '{}'",
                            param.name
                        )));
                    }
                },
            };
            coerce_value(&param.ty, value, &subject)
        })
        .collect()
}

/// Validate a function result against the callable's declared result type.
pub(crate) fn validate_return_value(
    descriptor: &CallableDescriptor,
    value: &Value,
) -> Result<(), EvalError> {
    validate_value(
        &descriptor.return_type,
        value,
        &format!("return value of '{}'", descriptor.qualified_name()),
    )
}

/// Convert a source type annotation to the runtime callable type vocabulary.
pub(crate) fn type_annotation_to_callable(annotation: &crate::ast::TypeAnnotation) -> CallableType {
    use crate::ast::TypeAnnotation as A;
    match annotation {
        A::Num => CallableType::Num,
        A::Duration => CallableType::Duration,
        A::Timeline => CallableType::Timeline,
        A::Unit => CallableType::Unit,
        A::Str => CallableType::Str,
        A::Bool => CallableType::Bool,
        A::Vec2 => CallableType::Vec2,
        A::Vec3 => CallableType::Vec3,
        A::Vec4 => CallableType::Vec4,
        A::Color => CallableType::Color,
        A::Easing => CallableType::Easing,
        A::Actor => CallableType::Actor,
        A::Scene => CallableType::Scene,
        A::List(inner) => CallableType::List(Box::new(type_annotation_to_callable(inner))),
        A::Tuple(items) => {
            CallableType::Tuple(items.iter().map(type_annotation_to_callable).collect())
        },
        A::Function { params, ret } => CallableType::Function {
            params: params
                .iter()
                .enumerate()
                .map(|(index, param)| (format!("arg{index}"), type_annotation_to_callable(param)))
                .collect(),
            ret: Box::new(type_annotation_to_callable(ret)),
        },
        A::Union(items) => {
            CallableType::Union(items.iter().map(type_annotation_to_callable).collect())
        },
        A::Enum(variants) => CallableType::Enum(variants.clone()),
        A::Alias(_) | A::Any => CallableType::Any,
    }
}

/// Check a runtime value against the public callable type vocabulary.
pub(crate) fn validate_value(
    expected: &CallableType,
    value: &Value,
    subject: &str,
) -> Result<(), EvalError> {
    if matches!(expected, CallableType::Any) {
        return Ok(());
    }
    let valid = match expected {
        CallableType::Any => true,
        CallableType::Num => matches!(value, Value::Num(_)),
        CallableType::Bool => matches!(value, Value::Bool(_)),
        CallableType::Str => matches!(value, Value::Str(_)),
        CallableType::Vec2 => matches!(value, Value::Vec2(_)),
        CallableType::Vec3 => matches!(value, Value::Vec3(_)),
        CallableType::Vec4 => matches!(value, Value::Vec4(_) | Value::Color(_)),
        CallableType::Color => matches!(value, Value::Color(_) | Value::Vec4(_)),
        CallableType::Duration => matches!(value, Value::Duration(_)),
        CallableType::Timeline => matches!(value, Value::TimelinePlan(_)),
        CallableType::Unit => matches!(value, Value::Unit),
        CallableType::Actor => matches!(value, Value::ActorRef(_)),
        CallableType::Scene => matches!(value, Value::Object(name, _) if name == "Scene"),
        CallableType::Easing => matches!(value, Value::Easing(_)),
        CallableType::List(inner) => match value {
            Value::List(items) => items.iter().all(|item| value_matches(inner, item)),
            _ => false,
        },
        CallableType::Tuple(types) => {
            let values = tuple_values(value);
            values.is_some_and(|values| {
                values.len() == types.len()
                    && types.iter().zip(&values).all(|(ty, item)| value_matches(ty, item))
            })
        },
        CallableType::Union(types) => types.iter().any(|ty| value_matches(ty, value)),
        CallableType::Enum(variants) => {
            matches!(value, Value::Str(name) if variants.iter().any(|variant| variant == name))
        },
        CallableType::Function { .. } => {
            matches!(
                value,
                Value::NativeFn(_)
                    | Value::RegisteredFn { .. }
                    | Value::UserFn { .. }
                    | Value::Closure(..)
            )
        },
    };
    if valid {
        Ok(())
    } else {
        Err(EvalError::TypeMismatch(format!("{subject} expects {expected}, got {value:?}")))
    }
}

/// Convert values across the schema's Color/Vec4-compatible boundary, then
/// validate every argument using the same runtime contract as return values.
pub(crate) fn coerce_value(
    expected: &CallableType,
    value: Value,
    subject: &str,
) -> Result<Value, EvalError> {
    match (expected, value) {
        (CallableType::Color, Value::Vec4(channels)) => Ok(Value::Color(channels)),
        (CallableType::Vec4, Value::Color(channels)) => Ok(Value::Vec4(channels)),
        (CallableType::List(inner), Value::List(items)) => {
            let items = items
                .iter()
                .cloned()
                .map(|item| coerce_value(inner, item, subject))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(items.into()))
        },
        (CallableType::Union(types), value) => {
            if types.iter().any(|ty| validate_value(ty, &value, subject).is_ok()) {
                return Ok(value);
            }
            for ty in types {
                if let Ok(value) = coerce_value(ty, value.clone(), subject) {
                    return Ok(value);
                }
            }
            validate_value(expected, &value, subject)?;
            Ok(value)
        },
        (_, value) => {
            validate_value(expected, &value, subject)?;
            Ok(value)
        },
    }
}

fn value_matches(expected: &CallableType, value: &Value) -> bool {
    validate_value(expected, value, "value").is_ok()
}

fn tuple_values(value: &Value) -> Option<Vec<Value>> {
    match value {
        Value::Vec2(values) => Some(values.iter().copied().map(Value::Num).collect()),
        Value::Vec3(values) => Some(values.iter().copied().map(Value::Num).collect()),
        Value::Vec4(values) => Some(values.iter().copied().map(Value::Num).collect()),
        Value::Color(values) => Some(values.iter().copied().map(Value::Num).collect()),
        Value::List(values) => Some(values.to_vec()),
        _ => None,
    }
}

fn default_value(value: &DefaultValue, expected: &CallableType) -> Value {
    match value {
        DefaultValue::Num(value) => Value::Num(*value),
        DefaultValue::Bool(value) => Value::Bool(*value),
        DefaultValue::Str(value) => Value::Str(value.clone()),
        DefaultValue::Duration(seconds) => Value::Duration(*seconds),
        DefaultValue::Ident(value) => match expected {
            CallableType::Easing => crate::easing::parse_easing_name(value)
                .map(Value::Easing)
                .unwrap_or_else(|| Value::Str(value.clone())),
            _ => Value::Str(value.clone()),
        },
    }
}
