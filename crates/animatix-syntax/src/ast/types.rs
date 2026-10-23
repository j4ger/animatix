use super::*;

/// Note: `name` may contain dots for nested keys (e.g., "scene.background",
/// "border.width").
/// The parser handles dot-separated paths; dots are preserved as-is in the name
/// string.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Property {
    /// Property name (may contain dots for nested keys).
    pub name: String,
    /// Property value expression.
    pub value: Expr,
    /// Byte-offset span of the value expression within the source text.
    /// Used for surgical source edits (writing back to .amx file).
    pub value_span: Option<ByteSpan>,
    /// Trailing line comment after this property, e.g.
    /// `size: (100, 200) // half-extents`.
    /// Only `//` line comments immediately following the property value are
    /// captured.
    /// Block comments (`/* */`) and comments inside expressions are rejected.
    pub trailing_comment: Option<String>,
}

impl Property {
    /// Create a new property with the given name and value.
    pub fn new(name: impl Into<String>, value: Expr) -> Self {
        Self {
            name: name.into(),
            value,
            value_span: None,
            trailing_comment: None,
        }
    }
}

impl Default for Property {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: Expr::Null,
            value_span: None,
            trailing_comment: None,
        }
    }
}

/// Modifier applied to an action (e.g. duration `2s`, `ease: bounce`).
#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    /// Optional modifier name (e.g. `ease` in `ease: bounce`).
    pub name: Option<String>,
    /// Modifier value expression.
    pub value: Expr,
}

/// Action invocation (e.g. `move btn to (100, 100) [2s]`).
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// Action verb (e.g. `appear`, `move`, `fade-out`).
    pub verb: String,
    /// Target labels the action applies to (e.g. `["btn"]`, `["A", "B",
    /// "C"]`).
    ///
    /// An indexed target like `bars[j]` stores the base projection `"bars"`
    /// here and the index expression in [`Action::target_index`]; the runtime
    /// resolves it to `"bars__N"` against the build environment before
    /// dispatching the action.
    pub targets: Vec<String>,
    /// Optional leaf index expression per target, aligned with `targets`.
    ///
    /// `None` for plain targets, `Some(expr)` for `name[expr]` targets. Only
    /// the last path segment may carry an index; the parser rejects others.
    pub target_index: Vec<Option<Expr>>,
    /// Positional arguments (e.g. `to (100, 100)`).
    pub args: Vec<Expr>,
    /// Named modifiers (e.g. `[2s, ease: bounce]`).
    pub modifiers: Vec<Modifier>,
    /// Byte-offset span of the action declaration in the source text.
    /// Used for diagnostic location reporting.
    pub byte_span: Option<ByteSpan>,
}

// ----------------------------------------------------------------------------
// 2b. Transition Types (Multi-Scene Composition)
// ----------------------------------------------------------------------------

/// Scene transition descriptor for the `play` statement.
#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    /// Transition ID from the registry (e.g. "fade", "wipe-left").
    pub id: String,
    /// Duration of the transition in milliseconds.
    pub duration_ms: u64,
    /// Easing curve for the transition.
    pub easing: crate::easing::Easing,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            id: "cut".into(),
            duration_ms: 0,
            easing: crate::easing::Easing::Linear,
        }
    }
}

// ----------------------------------------------------------------------------
// 3. Supporting Types
// ----------------------------------------------------------------------------
// Enums and structs used throughout the AST for specific domains.
// ----------------------------------------------------------------------------

/// Time value used for keyframes and durations.
#[derive(Clone, Debug, PartialEq)]
pub enum Time {
    /// Time in seconds (e.g. `2s`, `2.5`).
    Seconds(f64),
    /// Time in milliseconds (e.g. `500ms`).
    Milliseconds(u64),
    /// Time in beats (e.g. `2b`), resolved by the scene's `config { bpm: … }`.
    Beats(f64),
    /// Speech cue/marker reference: `#track@marker + offset`
    SpeechMarker {
        /// Audio track label (e.g. `voice`).
        track: String,
        /// Named marker identifier within the transcript.
        marker: String,
        /// Relative time offset in milliseconds.
        offset_ms: i64,
    },
    /// Speech word reference: `#track["word", occurrence] + offset`
    SpeechWord {
        /// Audio track label (e.g. `voice`).
        track: String,
        /// Spoken word to search for in the transcript.
        word: String,
        /// 1-indexed occurrence count for duplicate words.
        occurrence: usize,
        /// Relative time offset in milliseconds.
        offset_ms: i64,
    },
}

/// Tempo assumed where a beat stamp has to be resolved without a scene config
/// (formatting, outline views, the analyzer). A scene that means a different
/// tempo declares `config { bpm: … }`.
pub const DEFAULT_BPM: f64 = 120.0;

/// Seconds per beat at `bpm`.
pub fn beat_seconds(bpm: f64) -> f64 {
    if bpm > 0.0 {
        60.0 / bpm
    } else {
        60.0 / DEFAULT_BPM
    }
}

/// Type annotation for parameters.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeAnnotation {
    /// 64-bit floating-point number.
    Num,
    /// UTF-8 string.
    Str,
    /// Boolean flag.
    Bool,
    /// 2D vector.
    Vec2,
    /// 3D vector.
    Vec3,
    /// 4D vector.
    Vec4,
    /// RGBA color.
    Color,
    /// Actor label reference.
    Actor,
    /// Scene name reference.
    Scene,
    /// Homogeneous list.
    List(Box<TypeAnnotation>),
    /// Heterogeneous tuple, e.g. `Tuple<Num, Str>`.
    Tuple(Vec<TypeAnnotation>),
    /// Function type, e.g. `Fn(Num, Num) => Num`.
    Function {
        /// Function parameter types.
        params: Vec<TypeAnnotation>,
        /// Function return type.
        ret: Box<TypeAnnotation>,
    },
    /// Union of accepted types, e.g. `Bool | Str`.
    Union(Vec<TypeAnnotation>),
    /// Named enum choices, e.g. `Enum(left, right)`.
    Enum(Vec<String>),
    /// Named type alias reference.
    Alias(String),
    /// Unannotated — accepts any value.
    Any,
}

impl std::fmt::Display for TypeAnnotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeAnnotation::Num => write!(f, "Num"),
            TypeAnnotation::Str => write!(f, "Str"),
            TypeAnnotation::Bool => write!(f, "Bool"),
            TypeAnnotation::Vec2 => write!(f, "Vec2"),
            TypeAnnotation::Vec3 => write!(f, "Vec3"),
            TypeAnnotation::Vec4 => write!(f, "Vec4"),
            TypeAnnotation::Color => write!(f, "Color"),
            TypeAnnotation::Actor => write!(f, "Actor"),
            TypeAnnotation::Scene => write!(f, "Scene"),
            TypeAnnotation::List(inner) => write!(f, "List<{}>", inner),
            TypeAnnotation::Tuple(items) => {
                let inner = items.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                write!(f, "Tuple<{inner}>")
            },
            TypeAnnotation::Function { params, ret } => {
                let inner = params.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
                write!(f, "Fn({inner}) => {ret}")
            },
            TypeAnnotation::Union(types) => {
                let inner = types.iter().map(ToString::to_string).collect::<Vec<_>>().join(" | ");
                write!(f, "{inner}")
            },
            TypeAnnotation::Enum(variants) => {
                write!(f, "Enum({})", variants.join(", "))
            },
            TypeAnnotation::Alias(name) => write!(f, "{name}"),
            TypeAnnotation::Any => write!(f, "Any"),
        }
    }
}

/// Parameter definition for component parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamDef {
    /// Parameter name.
    pub name: String,
    /// Optional type annotation (e.g. `Num`, `Vec2`).
    pub param_type: Option<TypeAnnotation>,
    /// Default value expression, if any.
    pub default: Option<Expr>,
}

/// Component definition (used in `.actor.actx` files).
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDef {
    /// Whether the component is exported (`pub`).
    pub is_pub: bool,
    /// Component name.
    pub name: String,
    /// Parameter definitions.
    pub params: Vec<ParamDef>,
    /// Body statements.
    pub body: Vec<Stmt>,
}

/// Loop variable binding pattern for `for` loops.
/// Supports single variable or tuple destructuring.
#[derive(Debug, Clone, PartialEq)]
pub enum LoopPattern {
    /// Single variable: `for x in items { ... }`
    Single(String),
    /// Tuple destructuring: `for (x, y) in items { ... }`
    Tuple(Vec<String>),
}

/// Pattern for `match` arms.
/// Supports a subset of Rust patterns: literals, ranges, or-patterns,
/// tuple patterns, and the required wildcard `_`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MatchPattern {
    /// Wildcard pattern `_` — matches any value.
    Wildcard,
    /// Numeric literal pattern (e.g. `0`, `3.14`).
    Num(f64),
    /// String literal pattern (e.g. `"hello"`).
    Str(String),
    /// Boolean literal pattern (`true` or `false`).
    Bool(bool),
    /// Range pattern `a..=b` (inclusive).
    Range(Box<MatchPattern>, Box<MatchPattern>),
    /// Or-pattern `pat1 | pat2 | ...`.
    Or(Vec<MatchPattern>),
    /// Tuple pattern `(pat1, pat2, ...)`.
    Tuple(Vec<MatchPattern>),
}

impl std::fmt::Display for LoopPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoopPattern::Single(name) => write!(f, "{}", name),
            LoopPattern::Tuple(names) => {
                write!(f, "({})", names.join(", "))
            },
        }
    }
}

// ----------------------------------------------------------------------------
// 4. Container Items (Inline Children)
// ----------------------------------------------------------------------------
// Represents items declared inside containers (Row, Col, Grid).
