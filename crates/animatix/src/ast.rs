// ============================================================================
// Animatix Abstract Syntax Tree (AST) Definitions
// File: src/parser/ast.rs
// ============================================================================
// This module defines the core data structures representing the Animatix language.
// The AST is designed to be declarative, capturing scene states, timelines,
// and reactive logic without imperative implementation details.
// ============================================================================

use std::fmt::Debug;

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

#[derive(Clone, Debug, PartialEq)]
pub struct Property {
    pub name: String,
    pub value: Expr,
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
pub enum LoopKind {
    Infinite,      // loop { ... }
    Bounded(Time), // loop 5s { ... }
    Count(u32),    // loop 3 times { ... }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoopCommand {
    Stop,
    Pause,
    Resume,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LifecycleEvent {
    Appear,
    Disappear,
    Hover,
    Click,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamDef {
    pub name: String,
    pub param_type: Option<String>, // Optional type hint
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDef {
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
        children: Vec<InlineItem>,
    },
    // Labeled: btn: Button, text: "OK" (Added to scene graph)
    Labeled {
        label: String,
        ty: String,
        props: Vec<Property>,
        children: Vec<InlineItem>,
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
        name: String,
        value: Expr,
    },

    /// Actor: btn: Button, text: "OK"
    /// Re-declaring an existing label triggers morphing logic in compiler
    ActorDecl {
        label: String,
        ty: String,
        props: Vec<Property>,
        children: Vec<InlineItem>, // For containers: Row { A, B }
    },

    /// Import: import Button from "path"
    Import {
        name: String,
        path: String,
    },

    /// Use: use container.{a, b}
    Use {
        path: String,
        items: Vec<String>,
    },

    // === Timeline ===
    /// Keyframe: #2s { ... }
    /// Contains a body of statements/actions occurring at this time
    Keyframe {
        time: Time,
        body: Vec<Stmt>,
    },

    /// Relative Keyframe: #+1s { ... }
    RelativeKeyframe {
        offset: Time,
        body: Vec<Stmt>,
    },

    // === Assignments ===
    /// Property: btn.color = red
    Assignment {
        target: String, // Can be path: container.child
        property: String,
        value: Expr,
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

    // === Loops ===
    /// Loop: loop 3 times { ... }
    Loop {
        kind: LoopKind,
        label: Option<String>,
        body: Vec<Stmt>,
    },

    /// Loop Control: stop job
    LoopControl {
        command: LoopCommand,
        label: String,
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

    /// Lifecycle Hook: on appear { ... }
    LifecycleHook {
        event: LifecycleEvent,
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
    pub name: String,
    pub path: String,
    pub items: Option<Vec<String>>, // None = import all, Some = specific items
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
    // Constructor placeholder
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
