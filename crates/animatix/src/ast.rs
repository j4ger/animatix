//! Core AST definitions for the Animatix language.

use std::fmt::Debug;

// ----------------------------------------------------------------------------
// 0. Source Spans (for editor-timeline sync)
// ----------------------------------------------------------------------------
/// Source location span for AST nodes.
/// Used for editor navigation and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Byte-offset range into source text. Used for surgical source edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    pub fn line(&self) -> usize {
        self.start_line
    }
}

// ----------------------------------------------------------------------------
// 1. Expressions
// ----------------------------------------------------------------------------
// Represents any value-computing element: literals, math, logic, function calls.
// Used in property values, conditions, and reactive blocks.
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    // Literals
    Num(f64),
    Percent(f64),
    Str(String),
    Bool(bool),
    Null,

    // Identifiers & Access
    Ident(String),               // Variable name: x, slider_value
    Path(Vec<String>),           // Nested access: container.child
    Index(Box<Expr>, Box<Expr>), // Array/Index: items[0], children[i]

    // Collections
    Tuple(Vec<Expr>), // Coordinates/Arrays: (x, y), {a, b, c}

    // Operators
    Binary(Box<Expr>, BinaryOp, Box<Expr>), // x + y, a > b
    Unary(UnaryOp, Box<Expr>),              // -x, !flag

    // Functions & Methods
    Call(String, Vec<Expr>),              // format("{}", x)
    Method(Box<Expr>, String, Vec<Expr>), // graph.plot(func)
    Closure(Vec<String>, Box<Expr>),      // (x) => x ^ 2

    // Conditionals
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>), // if cond { a } else { b }

    // Type Construction (for inline morph targets)
    Construct(String, Vec<Property>), // Button, text: "OK"
}

#[derive(Clone, Debug, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

// ----------------------------------------------------------------------------
// 2. Properties & Modifiers
// ----------------------------------------------------------------------------
// Key-value pairs used for actor configuration and action modifiers.
// ----------------------------------------------------------------------------

/// Key-value pair for actor configuration.
/// Note: `name` may contain dots for nested keys (e.g., "scene.background", "border.width").
/// The parser handles dot-separated paths; dots are preserved as-is in the name string.
#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Expr,
    /// Byte-offset span of the value expression within the source text.
    /// Used for surgical source edits (writing back to .amx file).
    #[doc(hidden)]
    pub value_span: Option<ByteSpan>,
    /// Trailing line comment after this property, e.g. `size: (100, 200) // half-extents`.
    /// Only `//` line comments immediately following the property value are captured.
    /// Block comments (`/* */`) and comments inside expressions are rejected.
    pub trailing_comment: Option<String>,
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

#[derive(Clone, Debug, PartialEq)]
pub struct Modifier {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub verb: String,             // e.g., "appear", "move", "fade-out"
    pub targets: Vec<String>,     // e.g., ["btn"], ["A", "B", "C"]
    pub args: Vec<Expr>,          // e.g., to (100, 100)
    pub modifiers: Vec<Modifier>, // e.g., [2s, ease: bounce]
}

// ----------------------------------------------------------------------------
// 3. Supporting Types
// ----------------------------------------------------------------------------
// Enums and structs used throughout the AST for specific domains.
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Time {
    Seconds(f64),      // 2s, 2.5
    Milliseconds(u64), // 500ms
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamDef {
    pub name: String,
    pub param_type: Option<String>, // Optional type hint
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDef {
    pub is_pub: bool,
    pub name: String,
    pub params: Vec<ParamDef>,
    pub body: Vec<Stmt>,
}

// ----------------------------------------------------------------------------
// 4. Container Items (Inline Children)
// ----------------------------------------------------------------------------
// Represents items declared inside containers (Row, Col, Grid).
// Distinguishes between anonymous items and labeled scene actors.
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum InlineItem {
    // Anonymous: Button, text: "OK" (Only exists within container)
    Anonymous {
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
    },
    // Labeled: btn: Button, text: "OK" (Added to scene graph)
    Labeled {
        label: String,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>,
    },
    ///
    /// @slot marker inside a container's children block.
    /// Default items (if any) are non-@slot sibling items in the same container.
    SlotMarker,
    /// Filled slot content from a component instantiation site.
    /// Maps to a container label inside the component body (e.g. `header { ... }` maps to the
    /// container named `header` that contains a `@slot` marker).
    SlotFill {
        slot_name: String,
        items: Vec<InlineItem>,
    },
}

// ----------------------------------------------------------------------------
// 5. Statements
// ----------------------------------------------------------------------------
// The core logic units of the language. Includes declarations, timeline,
// reactive blocks, and control flow.
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    // === Actions ===
    /// Action invocation: move btn to (100, 100) [2s]
    Action(Action),

    // === Declarations ===
    /// Variable: let x = 0
    LetDecl {
        is_pub: bool,
        name: String,
        value: Expr,
    },

    // === Actors/Nodes ===
    Text {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
    },

    Math {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
    },

    Code {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
    },

    Svg {
        label: Option<String>,
        url: String,
        at: Option<Expr>,
        anchor: Option<Expr>,
        offset: Option<Expr>,
        scale: f32,
    },

    Image {
        label: Option<String>,
        url: String,
        at: Option<Expr>,
        anchor: Option<Expr>,
        offset: Option<Expr>,
        size: Option<(f32, f32)>,
    },

    /// Actor: btn: Button, text: "OK"
    /// Re-declaring an existing label triggers morphing logic in compiler
    ActorDecl {
        is_pub: bool,
        label: String,
        ty: String,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        children: Vec<InlineItem>, // For containers: Row { A, B }
    },

    /// Import: import "path"
    Import {
        path: String,
        alias: Option<String>,
    },

    /// Use: use container.{a, b}
    Use {
        path: String,
        items: Vec<String>,
    },

    // === Timeline ===
    /// Keyframe: #2s { ... }
    /// Contains a body of statements/actions occurring at this time
    /// Note: `span` tracks source location for editor-timeline sync feature
    Keyframe {
        time: Time,
        body: Vec<Stmt>,
        #[doc(hidden)]
        span: Option<Span>, // Reserved for future editor-timeline sync
    },

    /// Relative Keyframe: #+1s { ... }
    /// Note: `span` tracks source location for editor-timeline sync feature
    RelativeKeyframe {
        offset: Time,
        body: Vec<Stmt>,
        #[doc(hidden)]
        span: Option<Span>, // Reserved for future editor-timeline sync
    },

    // === Assignments ===
    /// Property: btn.color = red
    Assignment {
        target: Vec<String>, // Label path segments, e.g. ["container", "child"]
        property: String,
        value: Expr,
        modifiers: Vec<Modifier>,
        /// Byte-offset span of the value expression within the source text.
        /// Used for surgical source edits (writing back to .amx file).
        #[doc(hidden)]
        value_span: Option<ByteSpan>,
    },

    /// Composition helper: sequence { ... }
    Sequence {
        body: Vec<Stmt>,
    },

    /// Composition helper: stagger [150ms] { ... }
    Stagger {
        modifiers: Vec<Modifier>,
        body: Vec<Stmt>,
    },

    // === Reactive Blocks ===
    /// Always: always { ... }
    Always {
        body: Vec<Stmt>,
    },

    /// Labeled Always: job: always { ... }
    LabeledAlways {
        label: String,
        body: Vec<Stmt>,
    },

    // === Control Flow ===
    /// Conditional: if x > 0 { ... }
    Conditional {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },

    /// Iteration: for item in items { ... }
    ForLoop {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },

    // === Component Definitions (in .actor.actx files) ===
    /// Component: Button(text: "Click") { ... }
    ComponentDef(ComponentDef),

    /// Component Action: action hover { ... }
    ComponentAction {
        name: String,
        params: Vec<ParamDef>,
        body: Vec<Stmt>,
    },

    // === Configuration ===
    /// Config: @config { resolution: 1920x1080 }
    Config {
        settings: Vec<Property>,
    },

    // === Comments ===
    Comment(String),
}

// ----------------------------------------------------------------------------
// 6. File & Module Structure
// ----------------------------------------------------------------------------
// Top-level structure representing a complete .actx file.
// ----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum FileType {
    Scene,   // .actx
    Actor,   // .actor.actx
    Library, // .lib.actx
}

#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub file_type: FileType,
    pub config: Option<Vec<Property>>,
    pub imports: Vec<Import>,
    pub declarations: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimatixFile {
    pub path: String,
    pub file_type: FileType,
    pub config: Vec<Property>,
    pub imports: Vec<Import>,
    pub components: Vec<ComponentDef>,
    pub statements: Vec<Stmt>,
}

impl AnimatixFile {
    pub fn new(path: &str, file_type: FileType) -> Self {
        Self {
            path: path.to_string(),
            file_type,
            config: Vec::new(),
            imports: Vec::new(),
            components: Vec::new(),
            statements: Vec::new(),
        }
    }
}
