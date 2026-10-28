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
    /// `SymbolTable::infer_expr_type` used to construct a fresh environment per
    /// call; a declaration-heavy file did that dozens of times per keystroke
    /// rebuild. The cache lives and dies with the table, and every mutator
    /// that changes labels/components/namespaces invalidates it.
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
    /// The 1-based line number of the declaration.
    pub line: usize,
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
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
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
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
    /// The 1-based column number of the declaration.
    pub col: usize,
    /// Full source span (line/col range) for precise source write-back.
    pub span: Option<Span>,
}

/// Map an effect parameter's declared value kind to its analysis type.
pub(crate) fn effect_param_type(kind: crate::schema::PropertyValueKind) -> Option<typing::Type> {
    use crate::schema::PropertyValueKind as K;
    match kind {
        K::F32 | K::U32 => Some(typing::Type::Num),
        K::Bool => Some(typing::Type::Bool),
        K::Vec2 => Some(typing::Type::Vec2),
        K::Vec4 => Some(typing::Type::Vec4),
        _ => None,
    }
}

/// Build a type-env component signature from parameter declarations.
///
/// Shared by [`SymbolTable::type_env`] (annotations only, `infer_default`
/// returns `None`) and the type checker (annotation first, inferred default
/// second), so the two layers cannot disagree about a component parameter's
/// type.
pub(crate) fn component_signature<'a>(
    params: impl Iterator<Item = (&'a str, Option<&'a TypeAnnotation>, Option<&'a Expr>)>,
    infer_default: &mut dyn FnMut(&Expr) -> Option<typing::Type>,
) -> typing::ComponentSignature {
    let mut signature = typing::ComponentSignature::default();
    for (name, annotation, default) in params {
        if let Some(annotation) = annotation {
            signature
                .params
                .insert(name.to_string(), typing::Type::from_annotation(annotation));
        } else if let Some(default) = default {
            if let Some(ty) = infer_default(default) {
                signature.params.insert(name.to_string(), ty);
            }
        }
    }
    signature
}

impl SymbolTable {
    /// `true` when `ty` names a built-in or manifest-declared effect.
    pub fn is_effect_type(&self, ty: &str) -> bool {
        self.effect_names.contains(ty)
    }

    /// Declared type of an inline child label, if collected.
    pub fn inline_child_type(&self, label: &str) -> Option<&str> {
        self.inline_child_types.get(label).map(String::as_str)
    }

    /// Get the capability flags for an actor type (built-in or extension).
    pub fn caps_of(&self, ty: &str) -> Option<animatix_core::caps::ActorCaps> {
        animatix_std::caps_for_type(ty)
    }

    /// True when `ty` is a container or effect compositing scope.
    pub fn is_container_type(&self, ty: &str) -> bool {
        self.caps_of(ty).is_some_and(|c| c.is_container || c.is_effect_scope())
    }

    /// Retrieve author-facing documentation for this type.
    pub fn doc_of(&self, ty: &str) -> Option<&str> {
        animatix_std::catalog_lookup(ty).and_then(|info| {
            if info.doc.is_empty() {
                None
            } else {
                Some(info.doc)
            }
        })
    }
}

impl SymbolTable {
    /// Build a symbol table from one parsed AST in two passes.
    ///
    /// Pass 1 collects declarations (labels, components, scenes, aliases);
    /// pass 2 resolves the occurrences that can only be classified once every
    /// declaration exists (`Callout` targets, unused-label references). This is
    /// the entry point consumers should use; calling [`Self::build_from_ast`]
    /// and [`Self::collect_references`] by hand is the same work spelled out.
    pub fn build(stmts: &[Stmt]) -> Self {
        let mut table = Self::build_from_ast(stmts);
        table.collect_references(stmts);
        table
    }

    /// Drop the memoized type environment after a mutation that changes the
    /// symbols it was built from.
    fn invalidate_type_env(&mut self) {
        self.type_env_cache = std::sync::OnceLock::new();
    }

    /// Build a symbol-aware type environment from the current table.
    ///
    /// Cached for the table's lifetime: inference queries outnumber table
    /// mutations by a wide margin (every property value in a semantic check is
    /// one query), and rebuilding the environment per query made the analyzer's
    /// per-keystroke path allocate the whole vocabulary repeatedly.
    pub fn type_env(&self) -> &typing::TypeEnv {
        self.type_env_cache.get_or_init(|| self.build_type_env())
    }

    fn build_type_env(&self) -> typing::TypeEnv {
        let mut env = typing::TypeEnv::with_stdlib();
        for (name, info) in &self.labels {
            if let Some(inferred) = &info.inferred_type {
                match inferred {
                    typing::Type::Component(component) => {
                        env.declare_component_instance(name, component);
                    },
                    typing::Type::Actor(actor_ty) => {
                        env.declare_actor(name, actor_ty);
                    },
                    _ => {
                        env.bind(name, inferred.clone());
                    },
                }
            } else if let Some(ty) = &info.ty {
                if self.components.contains_key(ty) {
                    env.declare_component_instance(name, ty);
                } else {
                    env.declare_actor(name, ty);
                }
            }
        }
        for (name, info) in &self.components {
            let signature = component_signature(
                info.params
                    .iter()
                    .map(|param| (param.name.as_str(), param.param_type.as_ref(), None)),
                &mut |_default| None,
            );
            env.register_component(name, signature);
        }
        for (alias, namespace) in &self.namespaces {
            env.register_namespace(alias, namespace_type_from_symbols(namespace));
        }
        env
    }

    /// Infer an expression type using this table's symbols.
    pub fn infer_expr_type(&self, expr: &Expr) -> typing::Type {
        typing::infer_expr_type(expr, self.type_env())
    }

    /// Infer against a fresh snapshot of the table.
    ///
    /// The declaration pass runs before the table is complete, so it must not
    /// populate (or read) the memoized environment: a `let` later in the file
    /// has to see the bindings collected before it, and caching mid-collection
    /// would freeze the environment before those bindings exist.
    pub(super) fn infer_expr_type_snapshot(&self, expr: &Expr) -> typing::Type {
        typing::infer_expr_type(expr, &self.build_type_env())
    }

    /// Resolve a type annotation against the built-in type aliases.
    fn typed_resolve_annotation(&self, annotation: &TypeAnnotation) -> typing::Type {
        typing::TypeEnv::with_stdlib().resolve_annotation(annotation)
    }
}

fn namespace_type_from_symbols(table: &SymbolTable) -> typing::NamespaceType {
    let mut values = HashMap::new();
    for (name, info) in &table.labels {
        let ty = match &info.inferred_type {
            Some(ty) => ty.clone(),
            None => match &info.ty {
                Some(actor_ty) if table.components.contains_key(actor_ty) => {
                    typing::Type::Component(actor_ty.clone())
                },
                Some(actor_ty) => typing::Type::Actor(actor_ty.clone()),
                None => typing::Type::Any,
            },
        };
        values.insert(name.clone(), typing::NamespaceType::Value(ty));
    }
    for (alias, namespace) in &table.namespaces {
        values.insert(alias.clone(), namespace_type_from_symbols(namespace));
    }
    typing::NamespaceType::Namespace(values)
}

mod collect;
mod refs;
mod table;
#[cfg(test)]
mod tests;
