use super::*;

/// Item declared inside a container (Row, Col, Grid): anonymous or labeled.
#[derive(Clone, Debug, PartialEq)]
pub enum InlineItem {
    /// Anonymous item existing only within its container (e.g. `Button, text: "OK"`).
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
    /// Labeled item added to the scene graph (e.g. `btn: Button, text: "OK"`).
    Labeled {
        /// Item label.
        label: String,
        /// Array index expression for programmatic generation; `None` for normal items.
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
    SlotMarker,
    /// `@slot` fill item: maps to a container label inside the component body.
    SlotFill {
        /// Slot name.
        slot: String,
        /// Items filling this slot.
        items: Vec<InlineItem>,
    },
}

// ── Statements ───────────────────────────────────────────────────────────────

/// A single segment of an assignment/reactive-binding target path: `Static(label)`
/// for a pre-resolved label, `Indexed { base, index }` for a runtime index.
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
    /// If this is a Static segment, return the label string; otherwise `None`.
    pub fn as_static_str(&self) -> Option<&str> {
        match self {
            TargetSegment::Static(s) => Some(s.as_str()),
            TargetSegment::Indexed { .. } => None,
        }
    }

    /// Return the static string, panicking on an Indexed segment. Build-time only.
    pub fn expect_static(&self) -> &str {
        match self {
            TargetSegment::Static(s) => s.as_str(),
            TargetSegment::Indexed { base, .. } => {
                panic!("expected static target segment, got Indexed(\"{}\")", base)
            },
        }
    }

    /// Extract the label string for diagnostics: `Static(s)` or `Indexed { base, .. }`.
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

/// Build an array-indexed actor label (e.g. `bars[0]` → `"bars__0"`).
pub fn array_actor_label(base: &str, n: usize) -> String {
    format!("{}__{}", base, n)
}

/// If `s` matches `^(.+)__\d+$`, return the prefix before `__`.
pub fn is_array_member_label(s: &str) -> Option<&str> {
    let (prefix, suffix) = s.rsplit_once("__")?;
    if suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty() {
        Some(prefix)
    } else {
        None
    }
}

/// Statement types for the Animatix language.
///
/// Traversal is centralized in [`crate::visit`]: implement
/// [`AstVisitor`](crate::visit::AstVisitor) for read-only walks and
/// [`AstFolder`](crate::visit::AstFolder) for rewrites instead of matching every
/// variant by hand. [`crate::walk::recurse_stmt_bodies`] enumerates the
/// body-bearing variants for non-visitor callers.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// Action invocation: `move btn to (100, 100) [2s]`
    Action(Action, Option<Span>),

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

    /// Actor declaration: `btn: Button, text: "OK"` (also Text, Math, Code, Svg,
    /// Image); re-declaring a label triggers morphing.
    ActorDecl {
        /// Whether the actor is exported (`pub`).
        is_pub: bool,
        /// Whether the actor is anonymous (no label).
        is_anonymous: bool,
        /// Actor label.
        label: String,
        /// Array index expression for programmatic generation (e.g. `i` in
        /// `bars[i]: Rect`); `None` for normal declarations.
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

    /// Type alias declaration: `type LegendMode = Bool | Str`
    TypeAlias {
        /// Whether the alias is exported.
        is_pub: bool,
        /// Alias name.
        name: String,
        /// Aliased type annotation.
        annotation: TypeAnnotation,
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

    /// Absolute keyframe: `#2s { ... }`
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

    /// Property assignment: `btn.color = red`
    Assignment {
        /// Label path segments; a runtime index uses `Indexed { base, index }`.
        target: Vec<TargetSegment>,
        /// Property name.
        property: String,
        /// Assigned value expression.
        value: Expr,
        /// Modifiers applied to the assignment.
        modifiers: Vec<Modifier>,
        /// Optional easing curve for animated assignments.
        easing: Option<crate::easing::Easing>,
        /// Byte-offset span of the value expression, used for surgical source edits.
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

    /// Always block: `always { ... }`
    Always {
        /// Body statements in the always block.
        body: Vec<Stmt>,
        /// Source span for this block.
        span: Option<Span>,
    },

    /// Reactive binding: `actor.prop := expr`, desugared to an always-assignment.
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

    /// Match block: `match e { (0, i, j) => { swap a, b }, _ => {} }`. The
    /// scrutinee runs at build time (or frame time inside `always`); `_` is required.
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
        /// Timing modifiers, currently `step`: advance the build-time clock per iteration.
        modifiers: Vec<Modifier>,
        /// Source span for this loop.
        span: Option<Span>,
    },

    /// Component definition: `Button(text: "Click") { ... }`
    ComponentDef(ComponentDef, Option<Span>),

    /// Function declaration: `fn name(params) -> Type? { body }`.
    ///
    /// A `return_type` makes it pure (callable from expressions); without one it
    /// is a timeline function that may emit events and receives `self`.
    FnDecl {
        /// Whether the function is exported (`pub fn`) for cross-file import.
        is_pub: bool,
        /// Function name.
        name: String,
        /// Parameter definitions.
        params: Vec<ParamDef>,
        /// Return type annotation; `Some` marks a pure function.
        return_type: Option<TypeAnnotation>,
        /// Body statements.
        body: Vec<Stmt>,
        /// Source span for this function.
        span: Option<Span>,
    },

    /// A scoped statement block from timeline-function expansion; local `let`
    /// bindings stay local and shadowed caller bindings are restored on exit.
    Block {
        /// Body statements.
        body: Vec<Stmt>,
        /// Build-time specialization metadata for an indexed component method.
        instance_scope: Option<InstanceScope>,
        /// Source span for this block.
        span: Option<Span>,
    },

    /// `return expr` inside a pure function body; unwinds to the caller.
    Return {
        /// Optional value expression; `return` alone returns nothing.
        value: Option<Expr>,
        /// Source span for this return statement.
        span: Option<Span>,
    },

    /// A bare expression statement; a final expression in a pure function body is
    /// the return value (Rust-style tail expression), rejected elsewhere.
    Expr(Expr, Option<Span>),

    /// Config block: `@config { resolution: 1920x1080 }`
    Config {
        /// Configuration settings as properties.
        settings: Vec<Property>,
        /// Source span for this config block.
        span: Option<Span>,
    },

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

    /// Step or pause marker: `#step 1` or `pause`
    Step {
        /// Optional step index.
        index: Option<u64>,
        /// Source span for this step marker.
        span: Option<Span>,
    },

    /// Standalone comment statement.
    Comment(String, Option<Span>),
}

/// Build-time context attached to an indexed component-method call block.
/// The engine evaluates `index` after the leading `prologue_len` statements,
/// then specializes source-root property reads in the remaining body.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct InstanceScope {
    /// Source component path used by method-body property reads.
    pub label: String,
    /// Invocation index expression, evaluated in the block's build scope.
    pub index: Expr,
    /// Path segment position that receives the array index.
    pub index_position: usize,
    /// Number of leading statements to build before resolving `index`.
    pub prologue_len: usize,
}
