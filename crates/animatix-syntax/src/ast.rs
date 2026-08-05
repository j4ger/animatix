//! Core AST definitions for the Animatix language.

use std::fmt::Debug;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ----------------------------------------------------------------------------
// 0. Source Spans (for editor-timeline sync)
// ----------------------------------------------------------------------------

/// Source location span for AST nodes.
/// Used for editor navigation, diagnostics, and bidirectional sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Span {
    /// Start line number (1-indexed).
    pub start_line: usize,
    /// Start column number (1-indexed).
    pub start_col: usize,
    /// End line number (1-indexed).
    pub end_line: usize,
    /// End column number (1-indexed).
    pub end_col: usize,
}

/// Byte-offset range into source text.
///
/// Used during parsing where only byte offsets are available (e.g. from chumsky
/// spans). Convert to [`Span`] via [`Span::from_byte_span`] for editor-facing
/// positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ByteSpan {
    /// Start byte offset into the source text.
    pub start: usize,
    /// End byte offset into the source text.
    pub end: usize,
}

impl Span {
    /// Create a new span with the given line and column bounds.
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Convert a `ByteSpan` to a `Span` given the source text.
    pub fn from_byte_span(source: &str, byte_span: ByteSpan) -> Self {
        Self::from_range(source, byte_span.start..byte_span.end)
    }

    /// Convert a byte-offset range to a `Span` given the source text.
    pub fn from_range(source: &str, range: std::ops::Range<usize>) -> Self {
        let mut line = 1;
        let mut col = 1;
        let mut start_line = 1;
        let mut start_col = 1;
        let mut end_line = 1;
        let mut end_col = 1;

        for (i, ch) in source.char_indices() {
            if i == range.start {
                start_line = line;
                start_col = col;
            }
            if i == range.end {
                end_line = line;
                end_col = col;
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }

        // Handle edge case where end is at EOF
        if range.end == source.len() {
            end_line = line;
            end_col = col;
        }

        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}

// ----------------------------------------------------------------------------
// 1. Expressions
// ----------------------------------------------------------------------------
// Represents any value-computing element: literals, math, logic, function calls.
// Used in property values, conditions, and reactive blocks.
// ----------------------------------------------------------------------------

/// Expression types for the Animatix language.
/// Represents any value-computing element: literals, math, logic, function
/// calls, and more. Used in property values, conditions, and reactive blocks.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Expr {
    /// Numeric literal (e.g. `42`, `3.14`).
    Num(f64),
    /// Percentage literal (e.g. `50%`).
    Percent(f64),
    /// String literal (e.g. `"hello"`).
    Str(String),
    /// Boolean literal (`true` or `false`).
    Bool(bool),
    /// Null literal.
    Null,

    // Identifiers & Access
    /// Variable identifier (e.g. `x`, `slider_value`).
    Ident(String),
    /// Path access for nested identifiers (e.g. `container.child`).
    Path(Vec<String>),
    /// Index or subscript expression (e.g. `items[0]`, `children[i]`).
    Index(Box<Expr>, Box<Expr>),

    // Collections
    /// Tuple/vector literal `(x, y)`. Fixed-size: Vec2, Vec4, Color, domains.
    /// Length 2-4 is inferred as the corresponding vector type at the type level.
    Tuple(Vec<Expr>),
    /// List literal `{a, b, c}`. Variadic/homogeneous array.
    /// Used for points, commands, levels, data, for-iterables, etc.
    List(Vec<Expr>),

    // Operators
    /// Binary operation (e.g. `x + y`, `a > b`).
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    /// Unary operation (e.g. `-x`, `!flag`).
    Unary(UnaryOp, Box<Expr>),

    // Functions & Methods
    /// Function call (e.g. `format("{}", x)`).
    Call(String, Vec<Expr>),
    /// Method call on a receiver (e.g. `graph.plot(func)`).
    Method(Box<Expr>, String, Vec<Expr>),
    /// Closure / lambda expression (e.g. `(x) => x ^ 2`).
    Closure(Vec<String>, Box<Expr>),

    // Conditionals
    /// Conditional expression (e.g. `if cond { a } else { b }`).
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),

    // Match
    /// Match expression (e.g. `match v { 0 => red, _ => white }`).
    /// First matching arm's value is returned; `_` wildcard arm is required.
    Match(Box<Expr>, Vec<(MatchPattern, Box<Expr>)>),

    // Type Construction (for inline morph targets)
    /// Type construction for inline morph targets (e.g. `Button, text: "OK"`).
    Construct(String, Vec<Property>),
}

impl Expr {
    /// Returns true if this expression (or any sub-expression) references
    /// the given identifier name.
    pub fn references_ident(&self, name: &str) -> bool {
        match self {
            Expr::Ident(ident) => ident == name,
            Expr::Path(parts) => parts.iter().any(|p| p == name),
            Expr::Index(container, index) => {
                container.references_ident(name) || index.references_ident(name)
            },
            Expr::Tuple(items) => items.iter().any(|item| item.references_ident(name)),
            Expr::List(items) => items.iter().any(|item| item.references_ident(name)),
            Expr::Binary(left, _, right) => {
                left.references_ident(name) || right.references_ident(name)
            },
            Expr::Unary(_, expr) => expr.references_ident(name),
            Expr::Call(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            Expr::Method(receiver, _, args) => {
                receiver.references_ident(name) || args.iter().any(|arg| arg.references_ident(name))
            },
            Expr::Closure(_, body) => body.references_ident(name),
            Expr::Conditional(cond, then_branch, else_branch) => {
                cond.references_ident(name)
                    || then_branch.references_ident(name)
                    || else_branch.references_ident(name)
            },
            Expr::Match(scrutinee, arms) => {
                if scrutinee.references_ident(name) {
                    return true;
                }
                for (_pattern, arm_expr) in arms {
                    if arm_expr.references_ident(name) {
                        return true;
                    }
                }
                false
            },
            Expr::Construct(_, props) => props.iter().any(|p| p.value.references_ident(name)),
            // Literals never reference an identifier
            _ => false,
        }
    }
}

/// Binary operators supported by the Animatix language.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum BinaryOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Division (`/`).
    Div,
    /// Modulo (`%`).
    Mod,
    /// Power (`^`).
    Pow,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Neq,
    /// Less than (`<`).
    Lt,
    /// Greater than (`>`).
    Gt,
    /// Less than or equal (`<=`).
    Lte,
    /// Greater than or equal (`>=`).
    Gte,
    /// Logical AND (`&&`).
    And,
    /// Logical OR (`||`).
    Or,
}

/// Unary operators supported by the Animatix language.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum UnaryOp {
    /// Negation (`-`).
    Neg,
    /// Logical NOT (`!`).
    Not,
}

// ----------------------------------------------------------------------------
// 2. Properties & Modifiers
// ----------------------------------------------------------------------------
// Key-value pairs used for actor configuration and action modifiers.
// ----------------------------------------------------------------------------

/// Key-value pair for actor configuration.
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
    pub targets: Vec<String>,
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
            TypeAnnotation::Vec4 => write!(f, "Vec4"),
            TypeAnnotation::Color => write!(f, "Color"),
            TypeAnnotation::Actor => write!(f, "Actor"),
            TypeAnnotation::Scene => write!(f, "Scene"),
            TypeAnnotation::List(inner) => write!(f, "List<{}>", inner),
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
// Distinguishes between anonymous items and labeled scene actors.
// ----------------------------------------------------------------------------

/// Item declared inside a container (Row, Col, Grid).
/// Distinguishes between anonymous items and labeled scene actors.
#[derive(Clone, Debug, PartialEq)]
pub enum InlineItem {
    /// Anonymous item that only exists within its container (e.g.
    /// `Button, text: "OK"`).
    Anonymous {
        /// Actor type name.
        ty: String,
        /// Properties for actor configuration.
        props: Vec<Property>,
        /// Modifiers applied to the item.
        modifiers: Vec<Modifier>,
        /// Nested child items.
        children: Vec<InlineItem>,
    },
    /// Labeled item added to the scene graph (e.g. `btn: Button, text:
    /// "OK"`).
    Labeled {
        /// Item label.
        label: String,
        /// Array index expression for programmatic actor generation.
        /// `None` for normal inline declarations.
        array_index: Option<Expr>,
        /// Actor type name.
        ty: String,
        /// Properties for actor configuration.
        props: Vec<Property>,
        /// Modifiers applied to the item.
        modifiers: Vec<Modifier>,
        /// Nested child items.
        children: Vec<InlineItem>,
    },
    /// For loop inside a container's children block (e.g. `for item, i in items { ... }`).
    /// Body items are generated inline during build.
    ForLoop {
        /// Loop variable pattern (single or tuple destructuring).
        var: LoopPattern,
        /// Optional index variable name.
        index_var: Option<String>,
        /// Iterable expression.
        iterable: Expr,
        /// Body inline items to repeat.
        body: Vec<InlineItem>,
    },
    /// `@slot` marker inside a container's children block.
    /// Default items (if any) are non-@slot sibling items in the same
    /// container.
    SlotMarker,
    /// `@slot` fill item.
    /// Maps to a container label inside the component body (e.g. `header {
    /// ... }` maps to the container named `header` that contains a `@slot`
    /// marker).
    SlotFill {
        /// Slot name.
        slot: String,
        /// Items filling this slot.
        items: Vec<InlineItem>,
    },
}

// ----------------------------------------------------------------------------
// 5. Statements
// ----------------------------------------------------------------------------
// The core logic units of the language. Includes declarations, timeline,
// reactive blocks, and control flow.
// ----------------------------------------------------------------------------

/// A single segment of an assignment/reactive-binding target path.
///
/// - `Static("bars__0")` — a pre-resolved label (e.g. `bars[0]` → `"bars__0"`).
/// - `Indexed { base: "bars", index: <expr> }` — a runtime-indexed segment (e.g. `bars[i]` where
///   `i` is a per-frame `let`).
#[derive(Clone, Debug, PartialEq)]
pub enum TargetSegment {
    /// A static label segment (e.g. `"container"`, `"bars__0"`).
    Static(String),
    /// A runtime-indexed segment (e.g. `"bars[i]"` where `i` is frame-time).
    Indexed {
        /// The base actor label.
        base: String,
        /// The runtime index expression.
        index: Box<Expr>,
    },
}

impl TargetSegment {
    /// If this is a Static segment, return the label string.
    /// For Indexed segments, returns `None`.
    pub fn as_static_str(&self) -> Option<&str> {
        match self {
            TargetSegment::Static(s) => Some(s.as_str()),
            TargetSegment::Indexed { .. } => None,
        }
    }

    /// Return the static string, panicking if this is an Indexed segment.
    /// Use only in build-time code where Indexed segments cannot appear.
    pub fn expect_static(&self) -> &str {
        match self {
            TargetSegment::Static(s) => s.as_str(),
            TargetSegment::Indexed { base, .. } => {
                panic!("expected static target segment, got Indexed(\"{}\")", base)
            },
        }
    }

    /// Extract the "label string" from the segment for diagnostic/checking purposes.
    /// For `Static(s)`, returns `Some(s)`; for `Indexed { base, .. }`, returns `Some(base)`.
    /// Never returns `None` — this exists as a convenience when both variants carry a label.
    pub fn label_str(&self) -> &str {
        match self {
            TargetSegment::Static(s) => s.as_str(),
            TargetSegment::Indexed { base, .. } => base.as_str(),
        }
    }
}

impl std::fmt::Display for TargetSegment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetSegment::Static(s) => write!(f, "{}", s),
            TargetSegment::Indexed { base, .. } => write!(f, "{}[<index>]", base),
        }
    }
}

/// Join static-only target segments with `"."`.
/// Panics if any segment is `Indexed`.
pub fn target_segments_static_key(target: &[TargetSegment]) -> String {
    target.iter().map(|s| s.expect_static()).collect::<Vec<&str>>().join(".")
}

/// Build an array-indexed actor label (e.g. `bars[0]` → `"bars__0"`).
pub fn array_actor_label(base: &str, n: usize) -> String {
    format!("{}__{}", base, n)
}

/// If `s` matches `^(.+)__\d+$`, return the base part (the prefix before `__`).
/// Otherwise returns `None`.
pub fn is_array_member_label(s: &str) -> Option<&str> {
    let (prefix, suffix) = s.rsplit_once("__")?;
    if suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty() {
        Some(prefix)
    } else {
        None
    }
}

/// Statement types for the Animatix language.
/// Core logic units including declarations, timeline, reactive blocks, and
/// control flow.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    // === Actions ===
    /// Action invocation: `move btn to (100, 100) [2s]`
    Action(Action, Option<Span>),

    // === Declarations ===
    /// Variable declaration: `let x = 0`
    LetDecl {
        /// Whether the variable is exported (`pub`).
        is_pub: bool,
        /// Variable name.
        name: String,
        /// Initial value expression.
        value: Expr,
        /// Source span for this declaration.
        span: Option<Span>,
    },

    // === Actors/Nodes ===
    /// Actor declaration: `btn: Button, text: "OK"`
    /// Also used for Text, Math, Code, Svg, Image declarations.
    /// Re-declaring an existing label triggers morphing logic in compiler.
    ActorDecl {
        /// Whether the actor is exported (`pub`).
        is_pub: bool,
        /// Whether the actor is anonymous (no label).
        is_anonymous: bool,
        /// Actor label.
        label: String,
        /// Array index expression for programmatic actor generation (e.g. `i` in `bars[i]: Rect`).
        /// `None` for normal actor declarations.
        array_index: Option<Expr>,
        /// Actor type name.
        ty: String,
        /// Properties for actor configuration.
        props: Vec<Property>,
        /// Modifiers applied to the actor.
        modifiers: Vec<Modifier>,
        /// Nested child items (for containers: `Row { A, B }`).
        children: Vec<InlineItem>,
        /// Source span for this declaration.
        span: Option<Span>,
    },

    /// Import declaration: `import "path"`
    Import {
        /// Import path.
        path: String,
        /// Optional import alias.
        alias: Option<String>,
        /// Source span for this declaration.
        span: Option<Span>,
    },

    // === Timeline ===
    /// Absolute keyframe: `#2s { ... }`
    /// Contains a body of statements/actions occurring at this time.
    Keyframe {
        /// Keyframe time.
        time: Time,
        /// Body statements at this keyframe.
        body: Vec<Stmt>,
        /// Source span for this keyframe.
        span: Option<Span>,
    },

    /// Relative keyframe: `#+1s { ... }`
    RelativeKeyframe {
        /// Time offset from the previous keyframe.
        offset: Time,
        /// Body statements at this keyframe.
        body: Vec<Stmt>,
        /// Source span for this keyframe.
        span: Option<Span>,
    },

    // === Assignments ===
    /// Property assignment: `btn.color = red`
    Assignment {
        /// Label path segments (e.g. `[Static("container"), Static("child")]`).
        /// When a segment has a runtime index, use `Indexed { base, index }`
        /// (e.g. `bars[i].color = red` → `[Indexed { base: "bars", index: i }, Static("color")]`).
        target: Vec<TargetSegment>,
        /// Property name.
        property: String,
        /// Assigned value expression.
        value: Expr,
        /// Modifiers applied to the assignment.
        modifiers: Vec<Modifier>,
        /// Optional easing curve for animated assignments.
        easing: Option<crate::easing::Easing>,
        /// Byte-offset span of the value expression within the source text.
        /// Used for surgical source edits (writing back to .amx file).
        value_span: Option<ByteSpan>,
        /// Source span for this assignment.
        span: Option<Span>,
    },

    /// Sequence composition block: `sequence { ... }`
    Sequence {
        /// Body statements in the sequence.
        body: Vec<Stmt>,
        /// Source span for this sequence.
        span: Option<Span>,
    },

    /// Stagger composition block: `stagger [150ms] { ... }`
    Stagger {
        /// Stagger modifiers (e.g. interval duration).
        modifiers: Vec<Modifier>,
        /// Body statements to stagger.
        body: Vec<Stmt>,
        /// Source span for this stagger.
        span: Option<Span>,
    },

    // === Reactive Blocks ===
    /// Always block: `always { ... }`
    Always {
        /// Body statements in the always block.
        body: Vec<Stmt>,
        /// Source span for this block.
        span: Option<Span>,
    },

    /// Reactive binding: `actor.prop := expr`
    /// Desugars to an always-assignment at build time.
    ReactiveBinding {
        /// Label path segments.
        target: Vec<TargetSegment>,
        /// Property name.
        property: String,
        /// Bound value expression.
        value: Expr,
        /// Byte-offset span of the value expression.
        value_span: Option<ByteSpan>,
        /// Source span for this binding.
        span: Option<Span>,
    },

    // === Control Flow ===
    /// Conditional block: `if x > 0 { ... }`
    Conditional {
        /// Condition expression.
        condition: Expr,
        /// Then-branch statements.
        then_branch: Vec<Stmt>,
        /// Optional else-branch statements.
        else_branch: Option<Vec<Stmt>>,
        /// Source span for this conditional.
        span: Option<Span>,
    },

    /// Match block: `match e { (0, i, j) => { swap a, b }, _ => {} }`
    /// The scrutinee is evaluated at build time (or frame time if inside `always`);
    /// the first matching arm's body is executed. The `_` wildcard arm is required.
    Match {
        /// Scrutinee expression.
        scrutinee: Expr,
        /// List of (pattern, body_statements) arms.
        arms: Vec<(MatchPattern, Vec<Stmt>)>,
        /// Source span for this match.
        span: Option<Span>,
    },

    /// For loop: `for item in items { ... }` or `for item, i in items { ... }`
    ForLoop {
        /// Loop variable pattern (single or tuple destructuring).
        var: LoopPattern,
        /// Optional index variable name (e.g. `i` in `for item, i in items`).
        index_var: Option<String>,
        /// Iterable expression.
        iterable: Expr,
        /// Body statements in the loop.
        body: Vec<Stmt>,
        /// Source span for this loop.
        span: Option<Span>,
    },

    // === Component Definitions (in .actor.actx files) ===
    /// Component definition: `Button(text: "Click") { ... }`
    ComponentDef(ComponentDef, Option<Span>),

    /// Component action definition: `action hover { ... }`
    ComponentAction {
        /// Action name.
        name: String,
        /// Parameter definitions.
        params: Vec<ParamDef>,
        /// Body statements in the action.
        body: Vec<Stmt>,
        /// Source span for this action.
        span: Option<Span>,
    },

    // === Configuration ===
    /// Config block: `@config { resolution: 1920x1080 }`
    Config {
        /// Configuration settings as properties.
        settings: Vec<Property>,
        /// Source span for this config block.
        span: Option<Span>,
    },

    // === Multi-Scene Composition ===
    /// Scene declaration: `# SceneName`
    Scene {
        /// Scene name.
        name: String,
        /// Scene configuration properties.
        config: Vec<Property>,
        /// Body statements in the scene.
        body: Vec<Stmt>,
        /// Source span for this scene.
        span: Option<Span>,
    },

    /// Play statement: `play SceneName [transition, duration]`
    Play {
        /// Scene name to play.
        scene_name: String,
        /// Optional scene transition.
        transition: Option<Transition>,
        /// Source span for this play statement.
        span: Option<Span>,
    },

    // === Comments ===
    /// Standalone comment statement.
    Comment(String, Option<Span>),
}
