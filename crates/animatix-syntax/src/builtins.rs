//! Single registry of built-in names, type signatures, and documentation.
//!
//! Runtime implementations live in the `animatix` crate, but their names,
//! static types, and documentation are defined here so the tokenizer, type
//! checker, symbol table, completions, hover, diagnostics, and highlighting
//! cannot drift.

use crate::typing::Type;

/// Structural keywords recognized by the tokenizer and parser.
pub const KEYWORDS: &[&str] = &[
    "config",
    "import",
    "as",
    "let",
    "pub",
    "type",
    "component",
    "fn",
    "return",
    "sequence",
    "stagger",
    "always",
    "for",
    "in",
    "if",
    "else",
    "match",
    "play",
];

/// Reserved words that are lexed as keywords but rejected as identifiers.
pub const RESERVED_KEYWORDS: &[&str] = &["loop", "yield", "stop", "pause", "resume"];

/// Built-in actor/scene primitive type names, derived from the shared
/// primitive spec table plus component-system types. Adding a primitive needs
/// no edit here — its `builtin_primitive_specs()` row is the single source.
pub fn types() -> &'static [String] {
    use std::sync::OnceLock;
    static TYPES: OnceLock<Vec<String>> = OnceLock::new();
    TYPES.get_or_init(|| {
        let mut names: Vec<String> = crate::schema::builtin_primitive_specs()
            .iter()
            .map(|spec| spec.type_name.clone())
            .collect();
        // Built-in component, handled by the component system rather than a
        // primitive.
        names.push("Button".to_string());
        names.sort();
        names
    })
}

/// Built-in action verbs.
pub const ACTIONS: &[&str] = &[
    "fade-in",
    "draw-in",
    "wipe-in",
    "reveal-in",
    "settle-in",
    "pop-in",
    "fade-out",
    "wipe-out",
    "reveal-out",
    "draw-out",
    "move",
    "shift",
    "rotate",
    "scale",
    "shake",
    "pulse",
    "bounce",
    "highlight",
    "unhighlight",
    "persist",
    "remove",
    "swap",
    "reorder",
];

/// Built-in functions that construct a color value.
pub const COLOR_CONSTRUCTOR_FUNCTIONS: &[&str] = &["rgb", "rgba", "hsv", "hsl", "hsla"];

/// Built-in scalar/math functions.
///
/// This list is the language's declaration of what `eval_shared` implements:
/// the analyzer exempts these names from `unresolved-variable`, and
/// [`function_return_type`] types the call. A builtin missing here works at
/// runtime and is still wrong — the editor flags it and completion never
/// offers it. Checked against `eval_shared`'s dispatch in batch 3, which found
/// eleven such names.
pub const MATH_FUNCTIONS: &[&str] = &[
    "abs",
    "atan2",
    "clamp",
    "ceil",
    "cos",
    "curve_at",
    "curve_smooth",
    "deg",
    "deg_to_rad",
    "exp",
    "factorial",
    "fbm",
    "floor",
    "fract",
    "hypot",
    "lerp",
    "ln",
    "log",
    "max",
    "min",
    "noise",
    "noise2",
    "pow",
    "rad",
    "rad_to_deg",
    "rand",
    "rem",
    "round",
    "seeded_fbm",
    "seeded_noise",
    "seeded_noise2",
    "seeded_rand",
    "signum",
    "sin",
    "sqrt",
    "step",
    "sum",
    "tan",
];

/// Built-in list helpers. Separate from [`MATH_FUNCTIONS`] because they do not
/// return a number: `list_set`/`list_swap` return a new list, and typing them
/// as `Num` would make the type checker reject the assignment the docs show.
pub const LIST_FUNCTIONS: &[&str] = &["list_set", "list_swap"];

/// Built-in string-formatting function.
pub const FORMAT_FUNCTIONS: &[&str] = &["format"];

/// Built-in colorscheme namespaces whose two-segment paths are colors.
pub const COLOR_NAMESPACES: &[&str] = &["accent", "text", "surface", "stroke"];

/// Named color literals accepted by the runtime and static type layer.
pub const COLOR_NAMES: &[&str] = &[
    "red", "RED", "green", "GREEN", "blue", "BLUE", "black", "BLACK", "white", "WHITE", "yellow",
    "YELLOW", "orange", "ORANGE",
];

/// Return the static return type of a built-in function, if known.
pub fn function_return_type(name: &str) -> Option<Type> {
    if COLOR_CONSTRUCTOR_FUNCTIONS.contains(&name) {
        Some(Type::Color)
    } else if FORMAT_FUNCTIONS.contains(&name) {
        Some(Type::Str)
    } else if MATH_FUNCTIONS.contains(&name) {
        Some(Type::Num)
    } else if LIST_FUNCTIONS.contains(&name) {
        // `Any` inner on purpose: the helpers preserve whatever the list
        // held, and claiming `Num` would reject a list of strings.
        Some(Type::List(Box::new(Type::Any)))
    } else {
        None
    }
}

/// Named color literal values in RGBA order.
pub fn named_color_rgba(name: &str) -> Option<[f64; 4]> {
    match name {
        "red" | "RED" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" | "GREEN" => Some([0.0, 1.0, 0.0, 1.0]),
        "blue" | "BLUE" => Some([0.0, 0.0, 1.0, 1.0]),
        "black" | "BLACK" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" | "WHITE" => Some([1.0, 1.0, 1.0, 1.0]),
        "yellow" | "YELLOW" => Some([1.0, 1.0, 0.0, 1.0]),
        "orange" | "ORANGE" => Some([1.0, 0.65, 0.0, 1.0]),
        _ => None,
    }
}

/// Documentation for a built-in type.
pub fn type_documentation(name: &str) -> &'static str {
    match name {
        "Text" => "Text element with content and styling properties.",
        "Code" => "Code block with syntax highlighting.",
        "Svg" => "SVG image element.",
        "Image" => "Raster image element.",
        "Rect" => "Rectangle shape with fill and stroke.",
        "Ellipse" => "Ellipse, circle, arc, or dot shape.",
        "Line" => "Line segment or arrow with optional head.",
        "Polygon" => "Polygon or regular polygon shape.",
        "Path" => "SVG path element.",
        "Graph" => "Function graph.",
        "PlotCurve" => "Plot curve with configurable sampling kind.",
        "Button" => "Interactive button element.",
        _ => "Unknown type.",
    }
}

/// Documentation for a built-in action.
pub fn action_documentation(name: &str) -> &'static str {
    match name {
        "fade-in" => "Fade in from transparent.",
        "draw-in" => "Draw in (like handwriting).",
        "wipe-in" => "Wipe in from edge.",
        "settle-in" => "Fade in while easing up from a slightly smaller scale.",
        "pop-in" => "Fade in and scale up with a back overshoot.",
        "fade-out" => "Fade out to transparent.",
        "wipe-out" => "Wipe out to edge.",
        "reveal-out" => "Reveal out (reverse draw).",
        "draw-out" => "Draw out (reverse handwriting).",
        "move" => "Move to position: `move target [to: (x, y), 1s]`",
        "shift" => "Shift by offset: `shift target [by: (dx, dy), 1s]`",
        "rotate" => "Rotate: `rotate target [by: 1.5708, 1s]`",
        "scale" => "Scale: `scale target [by: 2, 1s]`",
        "persist" => {
            "Mark actor(s) to carry into the next scene: `persist actor1, actor2`. `persist camera` also hands over the camera transform."
        },
        "remove" => {
            "Fade out and stop persisting: `remove actor [500ms]`. `remove camera` stops the camera carry (no fade)."
        },
        _ => "Unknown action.",
    }
}

/// Documentation for a keyword.
pub fn keyword_documentation(name: &str) -> &'static str {
    match name {
        "let" => "Declare a variable: `let name = value`",
        "import" => "Import another file: `import \"path\"`",
        "always" => "Reactive block that runs continuously.",
        "if" => "Conditional: `if condition { ... }`",
        "else" => "Else branch: `if ... { } else { }`",
        "for" => "Loop: `for item in collection { ... }`",
        "in" => "Used in for loops.",
        "pub" => "Make visible to other files.",
        "component" => "Define a reusable component.",
        "sequence" => "Run actions in sequence.",
        "stagger" => "Stagger actions with delay.",
        _ => "Keyword.",
    }
}
