use super::*;

/// Runtime value type used in the timeline evaluation environment.
#[derive(Clone)]
pub enum Value {
    /// 64-bit floating-point number.
    Num(f64),
    /// UTF-8 string.
    Str(String),
    /// Boolean flag.
    Bool(bool),
    /// 2D vector (x, y).
    Vec2([f64; 2]),
    /// 3D vector (x, y, z).
    Vec3([f64; 3]),
    /// 4D vector (x, y, z, w).
    Vec4([f64; 4]),
    /// RGBA color with components in [0, 1].
    Color([f64; 4]),
    /// Ordered list of values.
    ///
    /// PF-6 round 10: shared as `Arc<[Value]>` — plot closures capture lists
    /// (an FFT sample array) and the per-sample-point inject/merge cycle
    /// deep-cloned the whole list per sample (85 KB/frame on `fft_explain`);
    /// with the `Arc` those clones are refcount bumps, same as the
    /// `VelloPath.path: Arc<BezPath>` memo.
    List(std::sync::Arc<[Value]>),
    /// Object(type_name, fields) — constructed value with named fields.
    Object(String, HashMap<String, Value>),
    /// Native Rust function callable from the runtime.
    #[allow(clippy::type_complexity)]
    NativeFn(Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>),
    /// User-defined closure (parameter names, compiled body, captured environment).
    Closure(Vec<String>, Box<CompiledExpr>, CapturedEnv),
    /// A reference to an actor property slot (e.g. produced by
    /// `&actor.prop`). Not a value: consuming it in arithmetic/format is an
    /// error; query functions (`is_animating`) accept it.
    /// A reference to an actor property slot (produced by `&actor.prop`).
    /// Not a value: arithmetic/format consumption is an error; query
    /// functions like `is_animating` accept it.
    PropRef {
        /// The actor label owning the property.
        label: String,
        /// The property name.
        prop: String,
    },
    /// User-declared pure function (`fn f(...) -> T`), evaluated at build time.
    UserFn {
        /// Function name.
        name: String,
        /// Parameter definitions (name + optional default).
        params: std::sync::Arc<Vec<crate::ast::ParamDef>>,
        /// Body statements; the tail expression (last non-return statement's
        /// expression) provides the return value when no `return` fires.
        body: std::sync::Arc<Vec<crate::ast::Stmt>>,
    },
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(n) => write!(f, "Num({})", n),
            Value::Str(s) => write!(f, "Str({:?})", s),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Vec2(v) => write!(f, "Vec2({:?})", v),
            Value::PropRef { label, prop } => write!(f, "PropRef({label}.{prop})"),
            Value::Vec3(v) => write!(f, "Vec3({:?})", v),
            Value::Vec4(v) => write!(f, "Vec4({:?})", v),
            Value::Color(c) => write!(f, "Color({:?})", c),
            Value::List(items) => write!(f, "List({:?})", items),
            Value::Object(name, fields) => write!(f, "{}({:?})", name, fields),
            Value::NativeFn(_) => write!(f, "<NativeFn>"),
            Value::Closure(args, _, _) => write!(f, "<Closure({:?})>", args),
            Value::UserFn { name, .. } => write!(f, "<UserFn({name})>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Vec2(a), Value::Vec2(b)) => a == b,
            (Value::Vec3(a), Value::Vec3(b)) => a == b,
            (Value::Vec4(a), Value::Vec4(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Object(name_a, fields_a), Value::Object(name_b, fields_b)) => {
                name_a == name_b && fields_a == fields_b
            },
            // Native functions and closures cannot be compared for equality
            _ => false,
        }
    }
}

impl Value {
    /// Extract the contained number, or `0.0` if the value is not a `Num`.
    pub fn as_num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => 0.0,
        }
    }

    /// Extract the contained string, or an empty string if the value is not a `Str`.
    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        }
    }

    /// Extract the contained boolean, or `false` if the value is not a `Bool`.
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    /// Returns whether this value is truthy in a conditional or boolean context.
    ///
    /// `Num(non-zero)` and `Bool(true)` are truthy; `Num(0)`, `Bool(false)`, and
    /// every other value type are falsy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Num(n) => *n != 0.0,
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    /// Extract the contained 2D vector, or `[0.0, 0.0]` if the value is not a `Vec2`.
    pub fn as_vec2(&self) -> [f64; 2] {
        match self {
            Value::Vec2(v) => *v,
            _ => [0.0, 0.0],
        }
    }

    /// Extract the contained 3D vector, or `[0.0, 0.0, 0.0]` if the value is not a `Vec3`.
    pub fn as_vec3(&self) -> [f64; 3] {
        match self {
            Value::Vec3(v) => *v,
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Extract the contained 4D vector, or `[0.0; 4]` if the value is not a `Vec4`.
    pub fn as_vec4(&self) -> [f64; 4] {
        match self {
            Value::Vec4(v) => *v,
            _ => [0.0, 0.0, 0.0, 0.0],
        }
    }

    /// Extract the contained color, or opaque black if the value is not a `Color`.
    pub fn as_color(&self) -> [f64; 4] {
        match self {
            Value::Color(c) => *c,
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    }

    /// Extract the contained list, or an empty vector if the value is not a `List`.
    pub fn as_list(&self) -> Vec<Value> {
        match self {
            Value::List(items) => items.to_vec(),
            _ => Vec::new(),
        }
    }

    /// Get a field value from an Object, or None if this is not an Object or field doesn't exist.
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Object(_, fields) => fields.get(name),
            _ => None,
        }
    }

    /// Set a field value on an Object, returning a new Object (immutable).
    /// If this value is not an Object, creates a new Object with the given field.
    pub fn with_field(&self, name: &str, value: Value) -> Value {
        match self {
            Value::Object(type_name, fields) => {
                let mut new_fields = fields.clone();
                new_fields.insert(name.to_string(), value);
                Value::Object(type_name.clone(), new_fields)
            },
            _ => Value::Object(String::new(), {
                let mut m = std::collections::HashMap::new();
                m.insert(name.to_string(), value);
                m
            }),
        }
    }
}

/// Cheap content-version identity for the build-time expression cache.
///
/// `(instance, version)` is part of the `evaluate_expr` memoization key: two
/// environments with equal stamps are guaranteed to be the same instance in an
/// unmutated state, so their contents are equal and cached values can be
/// shared. This replaces hashing every override entry on each evaluation,
/// which cost O(env size) per expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EnvStamp {
    pub(crate) instance: u32,
    pub(crate) version: u64,
}

pub(super) static ENV_INSTANCE_COUNTER: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1);
