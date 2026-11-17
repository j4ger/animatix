//! Author-visible contracts shared by standard, user and extension functions.

mod arguments;

pub use arguments::{ArgumentParameter, ArgumentSlot, bind_arguments};

/// A type in a callable contract, independent of parser and engine internals.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallableType {
    /// Gradually typed value.
    Any,
    /// Floating-point number.
    Num,
    /// Boolean.
    Bool,
    /// UTF-8 string.
    Str,
    /// Two numeric coordinates.
    Vec2,
    /// Three numeric coordinates.
    Vec3,
    /// Four numeric coordinates.
    Vec4,
    /// RGBA color.
    Color,
    /// A duration, expressed in seconds internally.
    Duration,
    /// An immutable build-time scene-content plan.
    Timeline,
    /// No result.
    Unit,
    /// A reference to an actor, never an actor label string.
    Actor,
    /// A scene-host reference, distinct from an embeddable timeline value.
    Scene,
    /// An easing curve, including parameterized curves.
    Easing,
    /// A homogeneous collection.
    List(Box<CallableType>),
    /// A fixed heterogeneous collection.
    Tuple(Vec<CallableType>),
    /// One of several explicitly accepted types.
    Union(Vec<CallableType>),
    /// A finite set of bare symbolic choices.
    Enum(Vec<String>),
    /// A callable value with named parameters.
    Function {
        /// Ordered parameter names and types.
        params: Vec<(String, CallableType)>,
        /// Result type.
        ret: Box<CallableType>,
    },
}

/// Whether a function can execute while sampling a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CallablePhase {
    /// Construction and compilation only.
    Build,
    /// Pure calculation in either construction or frame evaluation.
    BuildAndFrame,
}

/// A literal contract default; user expression defaults stay in their definition.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DefaultValue {
    /// Numeric default.
    Num(f64),
    /// Boolean default.
    Bool(bool),
    /// String default.
    Str(String),
    /// Duration default in seconds.
    Duration(f64),
    /// A symbolic enum/default value resolved in the expected type.
    Ident(String),
}

/// One ordered parameter in a function signature.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CallableParam {
    /// Source name used by named calls.
    pub name: String,
    /// Accepted value type.
    pub ty: CallableType,
    /// Literal default, when present.
    pub default: Option<DefaultValue>,
    /// May be omitted without supplying a literal default.
    pub optional: bool,
    /// Collects trailing positional arguments into a list.
    pub variadic: bool,
    /// Parameter documentation.
    pub help: String,
}

impl CallableParam {
    /// Construct a required parameter.
    pub fn required(name: impl Into<String>, ty: CallableType) -> Self {
        Self {
            name: name.into(),
            ty,
            default: None,
            optional: false,
            variadic: false,
            help: String::new(),
        }
    }

    /// Construct a parameter that can be omitted.
    pub fn optional(name: impl Into<String>, ty: CallableType) -> Self {
        Self {
            optional: true,
            ..Self::required(name, ty)
        }
    }

    /// Attach a literal default.
    pub fn with_default(mut self, value: DefaultValue) -> Self {
        self.default = Some(value);
        self.optional = true;
        self
    }
}

/// The single author-visible signature of a function, regardless of its origin.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CallableDescriptor {
    /// Unqualified source name.
    pub name: String,
    /// Owning namespace; standard functions use `std`.
    pub namespace: String,
    /// Ordered parameter contracts.
    pub params: Vec<CallableParam>,
    /// Result type, never an execution-permission switch.
    pub return_type: CallableType,
    /// Execution-stage capability.
    pub phase: CallablePhase,
    /// One-line documentation.
    pub help: String,
    /// Documentation/completion grouping.
    pub category: String,
}

impl CallableDescriptor {
    /// Canonical qualified source name.
    pub fn qualified_name(&self) -> String {
        if self.namespace.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.namespace, self.name)
        }
    }
}

impl std::fmt::Display for CallableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use CallableType::*;
        match self {
            Any => f.write_str("Any"),
            Num => f.write_str("Num"),
            Bool => f.write_str("Bool"),
            Str => f.write_str("Str"),
            Vec2 => f.write_str("Vec2"),
            Vec3 => f.write_str("Vec3"),
            Vec4 => f.write_str("Vec4"),
            Color => f.write_str("Color"),
            Duration => f.write_str("Duration"),
            Timeline => f.write_str("Timeline"),
            Unit => f.write_str("Unit"),
            Actor => f.write_str("Actor"),
            Scene => f.write_str("Scene"),
            Easing => f.write_str("Easing"),
            List(ty) => write!(f, "List<{ty}>"),
            Tuple(types) => {
                f.write_str("Tuple<")?;
                fmt_callable_types(f, types, ", ")?;
                f.write_str(">")
            },
            Union(types) => fmt_callable_types(f, types, " | "),
            Enum(values) => write!(f, "Enum({})", values.join(", ")),
            Function { params, ret } => {
                f.write_str("Fn(")?;
                for (index, (_, ty)) in params.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ") => {ret}")
            },
        }
    }
}

fn fmt_callable_types(
    f: &mut std::fmt::Formatter<'_>,
    types: &[CallableType],
    separator: &str,
) -> std::fmt::Result {
    for (index, ty) in types.iter().enumerate() {
        if index > 0 {
            f.write_str(separator)?;
        }
        write!(f, "{ty}")?;
    }
    Ok(())
}

impl CallableDescriptor {
    /// Check contract invariants before installing a standard or extension callable.
    pub fn validate(&self) -> Result<(), String> {
        fn identifier(name: &str) -> bool {
            let mut chars = name.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        if !identifier(&self.name)
            || self.namespace.is_empty()
            || !self.namespace.split("::").all(identifier)
        {
            return Err("callables require valid namespace and function identifiers".into());
        }
        let mut names = std::collections::HashSet::new();
        for (index, param) in self.params.iter().enumerate() {
            if !identifier(&param.name) || !names.insert(&param.name) {
                return Err("callable parameters require unique identifier names".into());
            }
            if param.variadic && (index + 1 != self.params.len() || param.default.is_some()) {
                return Err("a variadic parameter must be last and have no default".into());
            }
            if let Some(default) = &param.default
                && !default_matches(default, &param.ty)
            {
                return Err(format!("default for '{}' does not match {}", param.name, param.ty));
            }
        }
        if contains_timeline(&self.return_type) && self.phase != CallablePhase::Build {
            return Err("timeline construction is build-only".into());
        }
        Ok(())
    }
}

fn contains_timeline(ty: &CallableType) -> bool {
    match ty {
        CallableType::Timeline => true,
        CallableType::List(item) => contains_timeline(item),
        CallableType::Tuple(items) | CallableType::Union(items) => {
            items.iter().any(contains_timeline)
        },
        _ => false, // Other types do not contain a constructed timeline value.
    }
}

fn default_matches(value: &DefaultValue, ty: &CallableType) -> bool {
    use CallableType as T;
    if let T::Union(types) = ty {
        return types.iter().any(|ty| default_matches(value, ty));
    }
    let finite = match value {
        DefaultValue::Num(value) | DefaultValue::Duration(value) => value.is_finite(),
        _ => true, // Non-numeric literals have no finite-number constraint.
    };
    finite
        && match (value, ty) {
            (_, T::Any) => true,
            (DefaultValue::Num(_), T::Num)
            | (DefaultValue::Duration(_), T::Duration)
            | (DefaultValue::Bool(_), T::Bool)
            | (DefaultValue::Str(_), T::Str) => true,
            (DefaultValue::Ident(value) | DefaultValue::Str(value), T::Enum(variants)) => {
                variants.contains(value)
            },
            (DefaultValue::Ident(_), T::Easing) => true,
            _ => false, // Literal defaults cannot construct actors, lists, functions or plans.
        }
}
