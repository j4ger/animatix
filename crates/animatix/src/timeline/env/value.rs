use super::*;

/// Lexical component methods available while evaluating one method body.
/// The scope stores source templates, never runtime callable values, so nested
/// methods do not form an `Arc` cycle.
#[derive(Clone)]
pub struct ComponentMethodScope {
    /// Component instance root that owns these methods.
    pub(crate) source_label: String,
    /// All methods on the component instance, including private helpers.
    pub(crate) methods: Arc<HashMap<String, crate::module::FnTemplate>>,
    /// Names of caller values used by each method's component parameter
    /// bindings. These are the only call-site locals a component method may
    /// capture across the module-lexical function boundary.
    pub(crate) component_captures: Arc<HashMap<String, Vec<String>>>,
    /// Values of component parameter bindings captured when the actor is
    /// built. These preserve expressions such as `width: w` after the lexical
    /// caller scope is gone at frame time.
    pub(crate) captured_values: Arc<HashMap<String, HashMap<String, Value>>>,
    /// Symbolic declaration-time array index attached to these methods.
    pub(crate) instance_index: Option<crate::ast::Expr>,
    /// Actor selected by the method call, including an indexed instance.
    pub(crate) bound_actor: Option<crate::timeline::plan_value::ActorRef>,
    /// Concrete value of the declaration index variable for this receiver.
    pub(crate) bound_index: Option<(String, usize)>,
}

/// Runtime value type used in the timeline evaluation environment.
#[derive(Clone)]
pub enum Value {
    /// Absence of a value. Distinct from unit, which is a function result.
    Null,
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
    /// Shared as `Arc<[Value]>`: plot closures capture large lists (an FFT
    /// sample array), and the per-sample-point inject/merge cycle would
    /// otherwise deep-clone the whole list; with the `Arc` those clones are
    /// refcount bumps, matching the `VelloPath.path: Arc<BezPath>` memo.
    List(std::sync::Arc<[Value]>),
    /// Object(type_name, fields) — constructed value with named fields.
    Object(String, HashMap<String, Value>),
    /// Native Rust function callable from the runtime.
    #[allow(clippy::type_complexity)]
    NativeFn(Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>),
    /// First-class function value retaining its canonical signature when
    /// stored under an alias or passed as a parameter.
    RegisteredFn {
        /// Canonical callable path used for dispatch (for example `std::sin`).
        name: String,
        /// Shared public parameter, phase and return contract.
        descriptor: Arc<animatix_core::callable::CallableDescriptor>,
        /// Native callback, when this callable is implemented in Rust.
        #[allow(clippy::type_complexity)]
        native:
            Option<Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>>,
    },
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
    /// A typed duration in canonical seconds. It is intentionally distinct
    /// from `Num` so time and ordinary arithmetic cannot be confused.
    Duration(f64),
    /// The explicit no-result value of a unit-returning function.
    Unit,
    /// A typed easing value returned by an easing constructor.
    Easing(crate::easing::Easing),
    /// An actor identity passed by reference into a timeline plan.
    ActorRef(crate::timeline::plan_value::ActorRef),
    /// An inert timeline plan; it is lowered only when mounted in a Timeline.
    TimelinePlan(Arc<crate::timeline::plan_value::TimelinePlan>),
    /// User-declared pure function (`fn f(...) -> T`), evaluated at build time.
    UserFn {
        /// Function name.
        name: String,
        /// Parameter definitions (name + optional default).
        params: std::sync::Arc<Vec<crate::ast::ParamDef>>,
        /// Optional return type assertion.
        return_type: Option<crate::ast::TypeAnnotation>,
        /// Body statements; the tail expression (last non-return statement's
        /// expression) provides the return value when no `return` fires.
        body: std::sync::Arc<Vec<crate::ast::Stmt>>,
        /// Loaded module graph used to resolve private helpers and values.
        function_context: Option<std::sync::Arc<crate::module::FunctionContext>>,
        /// Source module that owns this function's lexical environment.
        definition_path: Option<std::path::PathBuf>,
        /// Component method scope, present only for component-bound methods.
        component_scope: Option<Arc<ComponentMethodScope>>,
    },
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "Null"),
            Value::Num(n) => write!(f, "Num({})", n),
            Value::Str(s) => write!(f, "Str({:?})", s),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Vec2(v) => write!(f, "Vec2({:?})", v),
            Value::PropRef { label, prop } => write!(f, "PropRef({label}.{prop})"),
            Value::Duration(seconds) => write!(f, "Duration({seconds}s)"),
            Value::Unit => write!(f, "Unit"),
            Value::Easing(easing) => write!(f, "Easing({easing:?})"),
            Value::ActorRef(actor) => write!(f, "ActorRef({})", actor.label()),
            Value::TimelinePlan(plan) => write!(f, "<TimelinePlan({})>", plan.id()),
            Value::Vec3(v) => write!(f, "Vec3({:?})", v),
            Value::Vec4(v) => write!(f, "Vec4({:?})", v),
            Value::Color(c) => write!(f, "Color({:?})", c),
            Value::List(items) => write!(f, "List({:?})", items),
            Value::Object(name, fields) => write!(f, "{}({:?})", name, fields),
            Value::NativeFn(_) => write!(f, "<NativeFn>"),
            Value::RegisteredFn { name, .. } => write!(f, "<Callable({name})>"),
            Value::Closure(args, _, _) => write!(f, "<Closure({:?})>", args),
            Value::UserFn { name, .. } => write!(f, "<UserFn({name})>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
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
            (Value::Duration(a), Value::Duration(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Easing(a), Value::Easing(b)) => a == b,
            (Value::ActorRef(a), Value::ActorRef(b)) => a == b,
            (Value::TimelinePlan(a), Value::TimelinePlan(b)) => a == b,
            // Native functions and closures cannot be compared for equality
            (Value::RegisteredFn { .. }, _) | (_, Value::RegisteredFn { .. }) => false,
            _ => false,
        }
    }
}

impl Value {
    /// Project numbers and booleans to a numeric scalar (true is 1, false is 0).
    /// Other kinds project to zero; callable type validation remains strict.
    pub fn as_num(&self) -> f64 {
        match self {
            Value::Null => 0.0,
            Value::Num(n) => *n,
            Value::Bool(true) => 1.0,
            Value::Bool(false) => 0.0,
            _ => 0.0,
        }
    }

    /// Extract the contained string, or an empty string if the value is not a `Str`.
    pub fn as_str(&self) -> String {
        match self {
            Value::Null => String::new(),
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
            Value::Null => false,
            Value::Num(n) => *n != 0.0,
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    /// Extract the contained 2D vector, or `[0.0, 0.0]` if the value is not a `Vec2`.
    pub fn as_vec2(&self) -> [f64; 2] {
        match self {
            Value::Null => [0.0, 0.0],
            Value::Vec2(v) => *v,
            _ => [0.0, 0.0],
        }
    }

    /// Extract the contained 3D vector, or `[0.0, 0.0, 0.0]` if the value is not a `Vec3`.
    pub fn as_vec3(&self) -> [f64; 3] {
        match self {
            Value::Null => [0.0; 3],
            Value::Vec3(v) => *v,
            _ => [0.0, 0.0, 0.0],
        }
    }

    /// Extract the contained 4D vector, or `[0.0; 4]` if the value is not a `Vec4`.
    pub fn as_vec4(&self) -> [f64; 4] {
        match self {
            Value::Null => [0.0; 4],
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

    /// Safe extraction to `[f64; 2]` without fallback.
    pub fn to_vec2(&self) -> Option<[f64; 2]> {
        match self {
            Value::Vec2(v) => Some(*v),
            _ => None,
        }
    }

    /// Safe extraction to `[f32; 2]` without fallback.
    pub fn to_f32_vec2(&self) -> Option<[f32; 2]> {
        match self {
            Value::Vec2([x, y]) => Some([*x as f32, *y as f32]),
            _ => None,
        }
    }

    /// Safe extraction to `[f32; 4]` without fallback (from `Vec4` or `Color`).
    pub fn to_f32_vec4(&self) -> Option<[f32; 4]> {
        match self {
            Value::Vec4([r, g, b, a]) | Value::Color([r, g, b, a]) => {
                Some([*r as f32, *g as f32, *b as f32, *a as f32])
            },
            _ => None,
        }
    }

    /// Extract a typed numeric vector if this value is a `List` of `Num`s,
    /// returning `None` if this is not a list or any element is not a number.
    pub fn to_f64_vec(&self) -> Option<Vec<f64>> {
        match self {
            Value::List(items) => items
                .iter()
                .map(|item| match item {
                    Value::Num(n) => Some(*n),
                    _ => None,
                })
                .collect(),
            _ => None,
        }
    }

    /// Get a field value from an Object, or None if this is not an Object or field doesn't exist.
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        match self {
            Value::Object(_, fields) => fields.get(name),
            _ => None,
        }
    }

    /// Read a stored field or a computed standard-library member.
    pub(crate) fn read_field(&self, name: &str) -> Option<Value> {
        match (self, name) {
            (Self::List(items), "length") => Some(Self::Num(items.len() as f64)),
            (Self::Str(text), "length") => Some(Self::Num(text.len() as f64)),
            (Self::Vec2([x, _]), "x") => Some(Self::Num(*x)),
            (Self::Vec2([_, y]), "y") => Some(Self::Num(*y)),
            _ => self.get_field(name).cloned(),
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
/// shared. Cheaper than hashing every override entry on each evaluation, which
/// would cost O(env size) per expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EnvStamp {
    pub(crate) instance: u32,
    pub(crate) version: u64,
}

pub(super) static ENV_INSTANCE_COUNTER: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(1);
