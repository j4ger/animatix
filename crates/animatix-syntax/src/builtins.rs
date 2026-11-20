//! Single registry of built-in names, type signatures, and documentation.
//!
//! Runtime implementations live in the `animatix` crate, but their names,
//! static types, and documentation are defined here so the tokenizer, type
//! checker, symbol table, completions, hover, diagnostics, and highlighting
//! cannot drift.

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
    "hex_color",
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

/// Built-in colorscheme namespaces whose two-segment paths are colors.
pub const COLOR_NAMESPACES: &[&str] = &["accent", "text", "surface", "stroke"];

/// Named color literals accepted by the runtime and static type layer.
pub const COLOR_NAMES: &[&str] = &[
    "red", "RED", "green", "GREEN", "blue", "BLUE", "black", "BLACK", "white", "WHITE", "yellow",
    "YELLOW", "orange", "ORANGE",
];

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
        "hex_color" => {
            "Construct a Color from six or eight quoted hexadecimal digits, such as `hex_color(\"fff1d6\")`."
        },
        _ => "Keyword.",
    }
}
