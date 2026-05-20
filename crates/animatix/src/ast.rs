//! Core AST definitions for the Animatix language.

use std::fmt::Debug;

// ----------------------------------------------------------------------------
// 0. Source Spans (for editor-timeline sync)
// ----------------------------------------------------------------------------
    /// Source location span for AST nodes.
/// Used for editor navigation, diagnostics, and bidirectional sync.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

/// Byte-offset range into source text. Used during parsing before line/col conversion.
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
        let mut _in_start = true;

        for (i, ch) in source.char_indices() {
            if i == range.start {
                start_line = line;
                start_col = col;
                _in_start = false;
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

impl Expr {
    /// Returns true if this expression (or any sub-expression) references
    /// the given identifier name.
    pub fn references_ident(&self, name: &str) -> bool {
        match self {
            Expr::Ident(ident) => ident == name,
            Expr::Path(parts) => parts.iter().any(|p| p == name),
            Expr::Index(container, index) => {
                container.references_ident(name) || index.references_ident(name)
            }
            Expr::Tuple(items) => items.iter().any(|item| item.references_ident(name)),
            Expr::Binary(left, _, right) => {
                left.references_ident(name) || right.references_ident(name)
            }
            Expr::Unary(_, expr) => expr.references_ident(name),
            Expr::Call(_, args) => args.iter().any(|arg| arg.references_ident(name)),
            Expr::Method(receiver, _, args) => {
                receiver.references_ident(name)
                    || args.iter().any(|arg| arg.references_ident(name))
            }
            Expr::Closure(_, body) => body.references_ident(name),
            Expr::Conditional(cond, then_branch, else_branch) => {
                cond.references_ident(name)
                    || then_branch.references_ident(name)
                    || else_branch.references_ident(name)
            }
            Expr::Construct(_, props) => props.iter().any(|p| p.value.references_ident(name)),
            // Literals never reference an identifier
            _ => false,
        }
    }
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
    
    pub value_span: Option<ByteSpan>,
    /// Trailing line comment after this property, e.g. `size: (100, 200) // half-extents`.
    /// Only `//` line comments immediately following the property value are captured.
    /// Block comments (`/* */`) and comments inside expressions are rejected.
    pub trailing_comment: Option<String>,
}

impl Property {
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
    pub duration_ms: u64,
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
#[non_exhaustive]
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
        slot: String,
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
    Action(Action,  Option<Span>),

    // === Declarations ===
    /// Variable: let x = 0
    LetDecl {
        is_pub: bool,
        name: String,
        value: Expr,
        
        span: Option<Span>,
    },

    // === Actors/Nodes ===
    Text {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        
        span: Option<Span>,
    },

    Math {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        
        span: Option<Span>,
    },

    Code {
        label: Option<String>,
        props: Vec<Property>,
        modifiers: Vec<Modifier>,
        
        span: Option<Span>,
    },

    Svg {
        label: Option<String>,
        url: String,
        at: Option<Expr>,
        anchor: Option<Expr>,
        offset: Option<Expr>,
        scale: f32,
        
        span: Option<Span>,
    },

    Image {
        label: Option<String>,
        url: String,
        at: Option<Expr>,
        anchor: Option<Expr>,
        offset: Option<Expr>,
        size: Option<(f32, f32)>,
        
        span: Option<Span>,
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
        
        span: Option<Span>,
    },

    /// Import: import "path"
    Import {
        path: String,
        alias: Option<String>,
        
        span: Option<Span>,
    },

    /// Use: use container.{a, b}
    Use {
        path: String,
        items: Vec<String>,
        
        span: Option<Span>,
    },

    // === Timeline ===
    /// Keyframe: #2s { ... }
    /// Contains a body of statements/actions occurring at this time
    Keyframe {
        time: Time,
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    /// Relative Keyframe: #+1s { ... }
    RelativeKeyframe {
        offset: Time,
        body: Vec<Stmt>,
        
        span: Option<Span>,
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
        
        value_span: Option<ByteSpan>,
        
        span: Option<Span>,
    },

    /// Composition helper: sequence { ... }
    Sequence {
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    /// Composition helper: stagger [150ms] { ... }
    Stagger {
        modifiers: Vec<Modifier>,
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    // === Reactive Blocks ===
    /// Always: always { ... }
    Always {
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    /// Drive: drive actor { ... }
    /// Per-actor reactive block where assignments are implicitly scoped.
    Drive {
        label: String,
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    /// Reactive binding: actor.prop := expr
    /// Desugars to an always-assignment at build time.
    ReactiveBinding {
        target: Vec<String>,
        property: String,
        value: Expr,
        value_span: Option<ByteSpan>,
        
        span: Option<Span>,
    },

    // === Control Flow ===
    /// Conditional: if x > 0 { ... }
    Conditional {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
        
        span: Option<Span>,
    },

    /// Iteration: for item in items { ... }
    ForLoop {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    // === Component Definitions (in .actor.actx files) ===
    /// Component: Button(text: "Click") { ... }
    ComponentDef(ComponentDef,  Option<Span>),

    /// Component Action: action hover { ... }
    ComponentAction {
        name: String,
        params: Vec<ParamDef>,
        body: Vec<Stmt>,
        
        span: Option<Span>,
    },

    // === Configuration ===
    /// Config: @config { resolution: 1920x1080 }
    Config {
        settings: Vec<Property>,
        
        span: Option<Span>,
    },

    // === Multi-Scene Composition ===
    /// Scene declaration: # SceneName
    Scene {
        name: String,
        config: Vec<Property>,
        body: Vec<Stmt>,
        span: Option<Span>,
    },

    /// Play statement: play SceneName [transition, duration]
    Play {
        scene_name: String,
        transition: Option<Transition>,
        span: Option<Span>,
    },

    // === Comments ===
    Comment(String,  Option<Span>),
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
