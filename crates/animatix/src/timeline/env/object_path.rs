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

// ---------------------------------------------------------------------------
// Serde for Value
// ---------------------------------------------------------------------------
//
// `Value::NativeFn` holds a Rust `Arc<dyn Fn>` which is not serializable.
// It is serialized as a unit variant `"NativeFn"` and deserialized back as an
// error variant — NativeFn values never appear in `CapturedEnv` (stdlib is
// not captured), so round-trip deserialization of `NativeFn` is not needed.
//
// `Value::Closure` is serialized with its argument list, compiled body, and
// captured `CapturedEnv`.

#[cfg(feature = "serde")]
mod serde_impl {
    use serde::{Deserializer, Serializer, de};

    use super::*;

    /// Wire representation: a plain serializable enum mirroring `Value`.
    /// `NativeFn` round-trips as a unit (error on deserialize).
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "Value")]
    enum ValueWire {
        Num(f64),
        Str(String),
        Bool(bool),
        Vec2([f64; 2]),
        Vec3([f64; 3]),
        Vec4([f64; 4]),
        Color([f64; 4]),
        List(Vec<ValueWire>),
        Object(String, std::collections::HashMap<String, ValueWire>),
        /// Serialized as unit; cannot be deserialized back to a live function.
        NativeFn,
        Closure(
            Vec<String>,
            Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
            CapturedEnv,
        ),
    }

    impl From<&Value> for ValueWire {
        fn from(v: &Value) -> Self {
            match v {
                Value::Num(n) => ValueWire::Num(*n),
                Value::Str(s) => ValueWire::Str(s.clone()),
                Value::Bool(b) => ValueWire::Bool(*b),
                Value::Vec2(v) => ValueWire::Vec2(*v),
                Value::Vec3(v) => ValueWire::Vec3(*v),
                Value::Vec4(v) => ValueWire::Vec4(*v),
                Value::Color(c) => ValueWire::Color(*c),
                Value::List(l) => ValueWire::List(l.iter().map(ValueWire::from).collect()),
                Value::Object(name, fields) => ValueWire::Object(
                    name.clone(),
                    fields.iter().map(|(k, v)| (k.clone(), ValueWire::from(v))).collect(),
                ),
                Value::NativeFn(_) => ValueWire::NativeFn, // stdlib; not captured
                // Live references and user functions are not serializable;
                // like `NativeFn` they round-trip as an unresolvable unit.
                Value::PropRef { .. } | Value::UserFn { .. } => ValueWire::NativeFn,
                Value::Closure(args, body, env) => {
                    ValueWire::Closure(args.clone(), body.clone(), env.clone())
                },
            }
        }
    }

    impl TryFrom<ValueWire> for Value {
        type Error = String;
        fn try_from(w: ValueWire) -> Result<Self, String> {
            Ok(match w {
                ValueWire::Num(n) => Value::Num(n),
                ValueWire::Str(s) => Value::Str(s),
                ValueWire::Bool(b) => Value::Bool(b),
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
                ValueWire::NativeFn => {
                    return Err("NativeFn is not deserializable; \
                                stdlib functions are re-provided at runtime"
                        .into());
                },
                ValueWire::Closure(args, body, env) => Value::Closure(args, body, env),
            })
        }
    }

    impl Serialize for Value {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            ValueWire::from(self).serialize(s)
        }
    }

    impl<'de> Deserialize<'de> for Value {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            let wire = ValueWire::deserialize(d)?;
            Value::try_from(wire).map_err(de::Error::custom)
        }
    }
}
