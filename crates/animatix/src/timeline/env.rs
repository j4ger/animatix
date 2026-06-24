use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Error produced during expression evaluation in the timeline environment.
#[derive(Debug, Clone)]
pub enum EvalError {
    /// Referenced variable does not exist in the environment.
    UndefinedVariable(String),
    /// Value type does not match the expected type for an operation.
    TypeMismatch(String),
    /// Attempted to call a non-function value.
    NotCallable(String),
    /// Method name is not supported on the target type.
    UnsupportedMethod(String),
    /// Indexing operation is not supported.
    UnsupportedIndex,
    /// Language construct is not supported at runtime.
    UnsupportedConstruct(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UndefinedVariable(v) => write!(f, "Undefined variable: {}", v),
            EvalError::TypeMismatch(e) => write!(f, "Type mismatch: {}", e),
            EvalError::NotCallable(n) => write!(f, "Not callable: {}", n),
            EvalError::UnsupportedMethod(name) => {
                write!(f, "Unsupported runtime method call: {}", name)
            }
            EvalError::UnsupportedIndex => {
                write!(f, "Unsupported runtime index expression")
            }
            EvalError::UnsupportedConstruct(name) => {
                write!(f, "Unsupported runtime construct expression: {}", name)
            }
        }
    }
}

impl std::error::Error for EvalError {}

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
    List(Vec<Value>),
    /// Object(type_name, fields) — constructed value with named fields.
    Object(String, HashMap<String, Value>),
    /// Native Rust function callable from the runtime.
    #[allow(clippy::type_complexity)]
    NativeFn(Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>),
    /// User-defined closure (parameter names, body expression, captured environment).
    Closure(Vec<String>, Box<crate::ast::Expr>, HashMap<String, Value>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(n) => write!(f, "Num({})", n),
            Value::Str(s) => write!(f, "Str({:?})", s),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Vec2(v) => write!(f, "Vec2({:?})", v),
            Value::Vec3(v) => write!(f, "Vec3({:?})", v),
            Value::Vec4(v) => write!(f, "Vec4({:?})", v),
            Value::Color(c) => write!(f, "Color({:?})", c),
            Value::List(items) => write!(f, "List({:?})", items),
            Value::Object(name, fields) => write!(f, "{}({:?})", name, fields),
            Value::NativeFn(_) => write!(f, "<NativeFn>"),
            Value::Closure(args, _, _) => write!(f, "<Closure({:?})>", args),
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
            }
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
            Value::List(items) => items.clone(),
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
            }
            _ => Value::Object(String::new(), {
                let mut m = std::collections::HashMap::new();
                m.insert(name.to_string(), value);
                m
            }),
        }
    }
}

/// Variable environment for expression evaluation.
///
/// Uses an override layer over an optional shared base to avoid copying
/// large stdlib maps on every frame.
#[derive(Clone)]
pub struct Environment {
    pub(crate) overrides: HashMap<String, Value>,
    /// P2.22: Shared base layer. `get()` checks overrides first, then falls back
    /// to base. This avoids copying ~90 stdlib entries on every [`Timeline::build_frame_env`].
    pub(crate) base: Option<Arc<HashMap<String, Value>>>,
    /// P6.2+: Dual-variable binding overlay for plot sampling.
    /// Avoids cloning the entire overrides HashMap for every sample point.
    /// Two slots allow setting both `x` and `y` for scalar/vector field evaluation.
    pub(crate) bindings: [Option<(String, Value)>; 2],
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    /// Create an empty environment with no base layer.
    pub fn new() -> Self {
        Environment {
            overrides: HashMap::new(),
            base: None,
            bindings: [None, None],
        }
    }

    /// Create an empty environment with the given initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Environment {
            overrides: HashMap::with_capacity(capacity),
            base: None,
            bindings: [None, None],
        }
    }

    /// Create an environment with a shared base layer.
    pub fn with_base(base: Arc<HashMap<String, Value>>) -> Self {
        Environment {
            overrides: HashMap::new(),
            base: Some(base),
            bindings: [None, None],
        }
    }

    /// Insert or overwrite a variable in the override layer.
    pub fn set(&mut self, name: &str, value: Value) {
        self.overrides.insert(name.to_string(), value);
    }

    /// Extend this environment with all values from another.
    /// If other has a base, we adopt it (or merge if we already have one).
    pub fn extend_from(&mut self, other: &Environment) {
        for (k, v) in &other.overrides {
            self.overrides.insert(k.clone(), v.clone());
        }
        if let Some(ref other_base) = other.base {
            if self.base.is_none() {
                self.base = Some(Arc::clone(other_base));
            } else {
                // Merge other_base entries into our overrides (shadowing our base)
                for (k, v) in other_base.iter() {
                    if !self.overrides.contains_key(k) {
                        self.overrides.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    /// Look up a variable by name, returning a clone.
    /// Checks bindings → overrides → base, in that order.
    pub fn get(&self, name: &str) -> Option<Value> {
        for binding in self.bindings.iter().flatten() {
            if binding.0 == name {
                return Some(binding.1.clone());
            }
        }
        self.overrides.get(name).cloned().or_else(|| {
            self.base.as_ref().and_then(|b| b.get(name).cloned())
        })
    }

    /// Look up a variable by name, returning a reference (zero-copy).
    /// Checks bindings → overrides → base, in that order.
    pub fn get_ref(&self, name: &str) -> Option<&Value> {
        for binding in self.bindings.iter().flatten() {
            if binding.0 == name {
                return Some(&binding.1);
            }
        }
        self.overrides.get(name).or_else(|| {
            self.base.as_ref().and_then(|b| b.get(name))
        })
    }

    /// Set a variable binding overlay. Uses the first available slot, or
    /// replaces an existing binding with the same name.
    pub fn set_binding(&mut self, name: &str, value: Value) {
        // Replace existing binding with same name, or fill first empty slot
        for slot in self.bindings.iter_mut() {
            match slot {
                Some((existing_name, _)) if existing_name == name => {
                    *slot = Some((name.to_string(), value));
                    return;
                }
                None => {
                    *slot = Some((name.to_string(), value));
                    return;
                }
                _ => {}
            }
        }
        // Both slots occupied — replace the first one
        self.bindings[0] = Some((name.to_string(), value));
    }

    /// Clear all variable binding overlays.
    pub fn clear_bindings(&mut self) {
        self.bindings = [None, None];
    }

    /// Return all variable names defined in this environment, sorted.
    pub fn all_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.overrides.keys().cloned().collect();
        if let Some(ref base) = self.base {
            for k in base.keys() {
                if !self.overrides.contains_key(k) {
                    keys.push(k.clone());
                }
            }
        }
        keys.sort();
        keys
    }

    /// Total number of distinct variables (overrides + base, minus overlap).
    pub fn len(&self) -> usize {
        let base_count = self.base.as_ref().map(|b| b.len()).unwrap_or(0);
        let overlap = self.base.as_ref().map(|b| {
            self.overrides.keys().filter(|k| b.contains_key(*k)).count()
        }).unwrap_or(0);
        self.overrides.len() + base_count - overlap
    }

    /// Returns true if no variables are defined in either layer.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty() && self.base.as_ref().map(|b| b.is_empty()).unwrap_or(true)
    }
}
