use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum EvalError {
    UndefinedVariable(String),
    TypeMismatch(String),
    NotCallable(String),
    UnsupportedMethod(String),
    UnsupportedIndex,
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

#[derive(Clone)]
pub enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    Vec2([f64; 2]),
    Vec3([f64; 3]),
    Vec4([f64; 4]),
    Color([f64; 4]),
    List(Vec<Value>),
    /// Object(type_name, fields) — constructed value with named fields
    Object(String, HashMap<String, Value>),
    #[allow(clippy::type_complexity)]
    NativeFn(Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>),
    Closure(Vec<String>, Box<crate::ast::Expr>),
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
            Value::Closure(args, _) => write!(f, "<Closure({:?})>", args),
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
    pub fn as_num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => 0.0,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    pub fn as_vec2(&self) -> [f64; 2] {
        match self {
            Value::Vec2(v) => *v,
            _ => [0.0, 0.0],
        }
    }

    pub fn as_vec3(&self) -> [f64; 3] {
        match self {
            Value::Vec3(v) => *v,
            _ => [0.0, 0.0, 0.0],
        }
    }

    pub fn as_vec4(&self) -> [f64; 4] {
        match self {
            Value::Vec4(v) => *v,
            _ => [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn as_color(&self) -> [f64; 4] {
        match self {
            Value::Color(c) => *c,
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    }

    pub fn as_list(&self) -> Vec<Value> {
        match self {
            Value::List(items) => items.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Environment {
    values: HashMap<String, Value>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            values: HashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Environment {
            values: HashMap::with_capacity(capacity),
        }
    }

    pub fn set(&mut self, name: &str, value: Value) {
        self.values.insert(name.to_string(), value);
    }

    /// Extend this environment with all values from another.
    pub fn extend_from(&mut self, other: &Environment) {
        for (k, v) in &other.values {
            self.values.insert(k.clone(), v.clone());
        }
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.values.get(name).cloned()
    }

    pub fn all_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.values.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}
