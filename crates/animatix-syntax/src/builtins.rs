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

/// Built-in actor/scene primitive type names.
pub const TYPES: &[&str] = &[
    // Shapes
    "Rect",
    "Ellipse",
    "Line",
    "Arrow",
    "Polygon",
    "Path",
    // Text
    "Text",
    "Code",
    "Math",
    "Typst",
    // Media
    "Image",
    "Svg",
    "Audio",
    // Plots
    "Graph",
    "PlotCurve",
    "VectorField",
    "Heatmap",
    "ContourSet",
    "NumberPlane",
    "BarChart",
    // Containers
    "Row",
    "Col",
    "Grid",
    "Stack",
    "Group",
    "Mask",
    "Filter",
    // Equation / Fragment
    "Equation",
    "Fragment",
    // Annotations
    "Callout",
    "Legend",
    // Built-in component (handled by the component system)
    "Button",
];

/// Built-in action verbs.
pub const ACTIONS: &[&str] = &[
    "fade-in",
    "draw-in",
    "wipe-in",
    "reveal-in",
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
pub const MATH_FUNCTIONS: &[&str] = &[
    "abs",
    "clamp",
    "ceil",
    "cos",
    "deg",
    "exp",
    "factorial",
    "floor",
    "lerp",
    "log",
    "max",
    "min",
    "rad",
    "rand",
    "seeded_rand",
    "sin",
    "sqrt",
    "sum",
    "tan",
];

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
        "fade-out" => "Fade out to transparent.",
        "wipe-out" => "Wipe out to edge.",
        "reveal-out" => "Reveal out (reverse draw).",
        "draw-out" => "Draw out (reverse handwriting).",
        "move" => "Move to position: `move target to (x, y)`",
        "shift" => "Shift by offset: `shift target by (dx, dy)`",
        "rotate" => "Rotate: `rotate target by 90`",
        "scale" => "Scale: `scale target to 2`",
        "persist" => "Mark actor(s) to carry into the next scene: `persist actor1, actor2`",
        "remove" => "Fade out and stop persisting: `remove actor [500ms]`",
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
