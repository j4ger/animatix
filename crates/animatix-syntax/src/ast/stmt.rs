use super::*;

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
        /// Timing modifiers, currently `step`: advance the build-time clock by
        /// `step` per iteration so loop-emitted events land on distinct times.
        modifiers: Vec<Modifier>,
        /// Source span for this loop.
        span: Option<Span>,
    },

    // === Component Definitions ===
    /// Component definition: `Button(text: "Click") { ... }`
    ComponentDef(ComponentDef, Option<Span>),

    // === Function Declarations ===
    /// Function declaration: `fn name(params) -> Type? { body }`.
    ///
    /// A `return_type` makes the function pure (computation only, no timeline
    /// operations; callable from expressions and `always`). Without a return
    /// type it is a timeline function that may emit events; `self` is
    /// implicitly available and bound when invoked target-style.
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

    /// A scoped statement block, generated by timeline-function call
    /// expansion. Local `let` bindings inside the block are removed when the
    /// block exits, so function-local state never leaks into the scene.
    Block {
        /// Body statements.
        body: Vec<Stmt>,
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

    /// A bare expression statement. Inside a pure function body the final
    /// expression is the return value (Rust-style tail expression); elsewhere
    /// the checker rejects it.
    Expr(Expr, Option<Span>),

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

    // === Interactive Step / Pause Markers ===
    /// Step or pause marker: `#step 1` or `pause`
    Step {
        /// Optional step index.
        index: Option<u64>,
        /// Source span for this step marker.
        span: Option<Span>,
    },

    // === Comments ===
    /// Standalone comment statement.
    Comment(String, Option<Span>),
}
