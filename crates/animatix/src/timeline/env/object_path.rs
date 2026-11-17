use super::*;

pub(super) fn update_object_path(
    current: Value,
    path: &[&str],
    field: &str,
    value: Value,
) -> Option<Value> {
    match current {
        Value::Object(name, mut fields) => {
            if path.is_empty() {
                fields.insert(field.to_string(), value);
                return Some(Value::Object(name, fields));
            }
            let child = fields.get(path[0])?.clone();
            let updated = update_object_path(child, &path[1..], field, value)?;
            fields.insert(path[0].to_string(), updated);
            Some(Value::Object(name, fields))
        },
        _ => None,
    }
}

// Serde for Value. Registered callables retain their canonical descriptor but
// relink the Rust callback from the host environment after deserialization.
// Bare native callbacks, source functions, live actor references, plans, and
// property references are build/runtime values and fail serialization rather
// than being written as a misleading placeholder.

#[cfg(feature = "serde")]
mod serde_impl {
    use serde::{Deserializer, Serializer, de};

    use super::*;

    /// Wire representation: a serializable enum mirroring persistent values.
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "Value")]
    enum ValueWire {
        Null,
        Num(f64),
        Str(String),
        Bool(bool),
        Duration(f64),
        Unit,
        Easing(crate::easing::Easing),
        Vec2([f64; 2]),
        Vec3([f64; 3]),
        Vec4([f64; 4]),
        Color([f64; 4]),
        List(Vec<ValueWire>),
        Object(String, std::collections::HashMap<String, ValueWire>),
        /// Callable contract; the host-owned Rust callback is resolved later.
        RegisteredFn {
            name: String,
            descriptor: animatix_core::callable::CallableDescriptor,
        },
        Closure(
            Vec<String>,
            Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
            CapturedEnv,
        ),
    }

    impl TryFrom<&Value> for ValueWire {
        type Error = String;

        fn try_from(v: &Value) -> Result<Self, String> {
            Ok(match v {
                Value::Null => ValueWire::Null,
                Value::Num(n) => ValueWire::Num(*n),
                Value::Str(s) => ValueWire::Str(s.clone()),
                Value::Bool(b) => ValueWire::Bool(*b),
                Value::Duration(seconds) => ValueWire::Duration(*seconds),
                Value::Unit => ValueWire::Unit,
                Value::Easing(easing) => ValueWire::Easing(*easing),
                Value::Vec2(v) => ValueWire::Vec2(*v),
                Value::Vec3(v) => ValueWire::Vec3(*v),
                Value::Vec4(v) => ValueWire::Vec4(*v),
                Value::Color(c) => ValueWire::Color(*c),
                Value::List(values) => ValueWire::List(
                    values.iter().map(ValueWire::try_from).collect::<Result<_, _>>()?,
                ),
                Value::Object(name, fields) => ValueWire::Object(
                    name.clone(),
                    fields
                        .iter()
                        .map(|(key, value)| Ok((key.clone(), ValueWire::try_from(value)?)))
                        .collect::<Result<_, String>>()?,
                ),
                Value::RegisteredFn {
                    name, descriptor, ..
                } => ValueWire::RegisteredFn {
                    name: name.clone(),
                    descriptor: descriptor.as_ref().clone(),
                },
                Value::NativeFn(_) => {
                    return Err("unregistered native callbacks cannot be serialized".into());
                },
                Value::PropRef { label, prop } => {
                    return Err(format!(
                        "property reference '{label}.{prop}' cannot be serialized"
                    ));
                },
                Value::ActorRef(reference) => {
                    return Err(format!(
                        "actor reference '{}' cannot be serialized",
                        reference.label()
                    ));
                },
                Value::TimelinePlan(_) => {
                    return Err("timeline plans cannot be serialized as runtime values".into());
                },
                Value::UserFn { name, .. } => {
                    return Err(format!("source function '{name}' cannot be serialized"));
                },
                Value::Closure(args, body, env) => {
                    ValueWire::Closure(args.clone(), body.clone(), env.clone())
                },
            })
        }
    }

    impl TryFrom<ValueWire> for Value {
        type Error = String;
        fn try_from(w: ValueWire) -> Result<Self, String> {
            Ok(match w {
                ValueWire::Null => Value::Null,
                ValueWire::Num(n) => Value::Num(n),
                ValueWire::Str(s) => Value::Str(s),
                ValueWire::Bool(b) => Value::Bool(b),
                ValueWire::Duration(seconds) => Value::Duration(seconds),
                ValueWire::Unit => Value::Unit,
                ValueWire::Easing(easing) => Value::Easing(easing),
                ValueWire::Vec2(v) => Value::Vec2(v),
                ValueWire::Vec3(v) => Value::Vec3(v),
                ValueWire::Vec4(v) => Value::Vec4(v),
                ValueWire::Color(c) => Value::Color(c),
                ValueWire::List(l) => {
                    let items: Vec<Value> =
                        l.into_iter().map(Value::try_from).collect::<Result<_, _>>()?;
                    Value::List(items.into())
                },
                ValueWire::Object(name, fields) => Value::Object(
                    name,
                    fields
                        .into_iter()
                        .map(|(k, v)| Value::try_from(v).map(|v| (k, v)))
                        .collect::<Result<_, _>>()?,
                ),
                ValueWire::RegisteredFn { name, descriptor } => {
                    if descriptor.qualified_name() != name {
                        return Err(format!(
                            "registered callable name '{name}' does not match descriptor '{}'",
                            descriptor.qualified_name()
                        ));
                    }
                    Value::RegisteredFn {
                        name,
                        descriptor: std::sync::Arc::new(descriptor),
                        native: None,
                    }
                },
                ValueWire::Closure(args, body, env) => Value::Closure(args, body, env),
            })
        }
    }

    impl Serialize for Value {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let wire = ValueWire::try_from(self).map_err(serde::ser::Error::custom)?;
            wire.serialize(s)
        }
    }

    impl<'de> Deserialize<'de> for Value {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let wire = ValueWire::deserialize(d)?;
            Value::try_from(wire).map_err(de::Error::custom)
        }
    }
}
