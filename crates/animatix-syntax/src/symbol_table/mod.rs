//! Symbol table extraction from the AST.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::to_source::ToSource;
use crate::typing;

/// Extracted symbols from a source file.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    /// All labels defined in the file (actor labels, let bindings).
    pub labels: HashMap<String, LabelInfo>,
    /// Built-in types: Text, Math, Ellipse, etc.
    pub types: HashSet<String>,
    /// Components defined in this file.
    pub components: HashMap<String, ComponentInfo>,
    /// Scenes defined in this file.
    pub scenes: HashMap<String, SceneInfo>,
    /// Properties available per type: "Text" → ["content", "position", ...].
    pub properties: HashMap<String, Vec<String>>,
    /// Expected types per property: ("Text", "font_size") → `typing::Type::Num`.
    pub property_types: HashMap<(String, String), typing::Type>,
    /// Keywords and built-in actions.
    pub keywords: HashSet<String>,
    /// Built-in action verbs (e.g., "fade-in", "move", "rotate").
    pub actions: HashSet<String>,
    /// Extension expression function names.
    pub functions: HashSet<String>,
    /// Extension service names.
    pub services: HashSet<String>,
    /// Imports declared in this file.
    pub imports: Vec<ImportInfo>,
    /// Labels referenced in actions/assignments (for unused label detection).
    pub referenced_labels: HashSet<String>,
    /// Labels of array-indexed actors (e.g., `bars[i]: Rect`).
    /// Targets like `bars__0`, `bars__1` are considered defined.
    pub array_labels: HashSet<String>,
    /// Labels declared inside component templates, which are not scene actors.
    pub component_internal_labels: HashSet<String>,
    /// Namespaced symbols from aliased imports (e.g., "foo" → SymbolTable).
    pub namespaces: HashMap<String, SymbolTable>,
    /// Named type aliases declared in this file: name → annotation.
    pub type_aliases: HashMap<String, TypeAnnotation>,
    /// Built-in and manifest-declared effect type names. Effect stages
    /// (`pix: Pixelate`) validate against these instead of primitive
    /// property tables.
    pub effect_names: HashSet<String>,
    /// Declared type of inline child labels (`pix: Pixelate` inside a
    /// container body). Container children are collected before the full
    /// `LabelInfo.ty` pipeline, so their declared type lives here.
    pub inline_child_types: HashMap<String, String>,
    /// Component definitions collected in the same declaration pass as
    /// [`Self::components`]. Consumers that need the body (the analyzer's
    /// type-check registry) read it here instead of walking the AST again.
    pub component_defs: HashMap<String, crate::ast::ComponentDef>,
    /// Memoized type environment, rebuilt lazily after the table mutates.
    ///
    /// Without the cache every `SymbolTable::infer_expr_type` call rebuilds the
    /// environment, and a declaration-heavy file makes dozens of those per
    /// keystroke rebuild. The cache lives and dies with the table, and every
    /// mutator that changes labels/components/namespaces invalidates it.
    pub(crate) type_env_cache: std::sync::OnceLock<typing::TypeEnv>,
}

/// Information about an import declaration.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The import path (e.g., `"lib.amx"`).
    pub path: String,
    /// Optional alias for the import.
    pub alias: Option<String>,
    /// Full source span of the import statement.
    pub span: Option<Span>,
}

/// Information about a labeled entity.
#[derive(Debug, Clone)]
pub struct LabelInfo {
    /// The name of the label.
    pub name: String,
    /// The kind of label (actor, let binding, etc.).
    pub kind: LabelKind,
    /// The 1-based line number of the declaration; `0` until
    /// `Analyzer::enrich_positions` fills it from the token stream.
    pub line: usize,
    /// The 1-based UTF-16 column number of the declaration (LSP position);
    /// `0` until `Analyzer::enrich_positions` fills it from the token stream.
    pub col: usize,
    /// Full source span in character-column units for precise source
    /// write-back and GUI navigation.
    pub span: Option<Span>,
    /// The type of the actor (e.g., "Text", "Button"), if applicable.
    pub ty: Option<String>,
    /// Plot runtime-parameter names (property names matching identifiers in
    /// the actor's `func` closure), populated for plot-family actors. Lets
    /// the analyzer skip the unknown-property info for `curve.freq = ...`
    /// assignments on parameters declared on the same actor.
    pub plot_params: Vec<String>,
    /// Inferred expression type for variables and actors.
    pub inferred_type: Option<typing::Type>,
    /// Whether the declaration is exported with `pub`.
    pub is_pub: bool,
}

/// The kind of labeled entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelKind {
    /// Actor declaration: `label: Type { ... }`
    Actor,
    /// Let binding: `let name = value`
    Let,
    /// For loop variable: `for item in ...`
    For,
    /// Always block: `always { ... }`
    Always,
    /// Component definition: `component Name { ... }`
    Component,
}

/// Information about a component definition.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    /// The component name.
    pub name: String,
    /// The list of parameters for this component.
    pub params: Vec<ParamInfo>,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based UTF-16 column number of the declaration (LSP position).
    pub col: usize,
    /// Full source span in character-column units for precise source
    /// write-back and GUI navigation.
    pub span: Option<Span>,
    /// Whether the component is exported with `pub`.
    pub is_pub: bool,
}

/// Information about a component parameter.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// The parameter name.
    pub name: String,
    /// The type annotation, if any (e.g. `Num`, `Vec2`).
    pub param_type: Option<TypeAnnotation>,
    /// The default value, if any, as a source string.
    pub default: Option<String>,
}

/// Information about a scene declaration.
#[derive(Debug, Clone)]
pub struct SceneInfo {
    /// The scene name.
    pub name: String,
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based UTF-16 column number of the declaration (LSP position).
    pub col: usize,
    /// Full source span in character-column units for precise source
    /// write-back and GUI navigation.
    pub span: Option<Span>,
}

mod api;
mod collect;
mod env;
mod refs;
mod table;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use types::{component_signature, effect_param_type};
