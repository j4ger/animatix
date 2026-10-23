//! Module system for the Animatix language: multi-file programs, imports,
//! component collection, and namespace resolution.

pub mod discovery;
pub(crate) mod expand;
mod inline_actions;
mod rewrite;
pub mod source_map;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use discovery::{
    collect_component_actions, collect_component_defs, collect_imports, collect_scenes_from_stmts,
    strip_imports,
};
use expand::expand_statements;

use crate::ast::{
    Action, ComponentDef, Expr, InlineItem, MatchPattern, Modifier, ParamDef, Property, Span, Stmt,
    TargetSegment, TypeAnnotation,
};
use crate::parser::{ParseError, parse_canonical};
use crate::walk::walk_stmts_mut;

/// Walk the AST and convert `Action.byte_span` into `Stmt::Action` line/col spans.
fn set_action_spans(stmts: &mut [Stmt], source: &str) {
    walk_stmts_mut(stmts, &mut |stmt| {
        if let Stmt::Action(action, span) = stmt {
            if let Some(byte_span) = action.byte_span {
                *span = Some(crate::ast::Span::from_byte_span(source, byte_span));
            }
        }
    });
}

/// Unique identifier for a loaded source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId(u32);

impl FileId {
    /// Create a new `FileId` from a raw index.
    pub fn new(id: u32) -> Self {
        FileId(id)
    }
    /// Return the underlying index.
    pub fn index(&self) -> u32 {
        self.0
    }
}

/// Controls whether [`ModuleGraph`] may read a path from disk when no
/// in-memory source override exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceAccess {
    /// Read registered in-memory sources and fall back to the filesystem.
    #[default]
    DiskAndSources,
    /// Only read registered in-memory sources. Missing in-memory files are
    /// reported as `FileNotFound` without touching disk (analyzer/LSP mode).
    SourcesOnly,
}

/// Tracks loaded files, their imports, and the component registry.
#[derive(Clone, Debug)]
pub struct ModuleGraph {
    files: HashMap<FileId, ParsedModule>,
    next_id: u32,
    paths: HashMap<PathBuf, FileId>,
    /// In-memory source overrides and normalized path identity.
    sources: source_map::SourceMap,
    source_access: SourceAccess,
}

/// A component definition together with its source file and timeline functions.
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    /// The parsed component definition.
    pub definition: ComponentDef,
    /// Absolute path to the file that defined this component.
    pub source_path: PathBuf,
    /// Timeline-function templates defined inside this component: fn_name → template.
    pub actions: HashMap<String, FnTemplate>,
}

/// A timeline-function template with parameter definitions and body.
#[derive(Clone, Debug)]
pub struct FnTemplate {
    /// Parameter definitions for this function.
    pub params: Vec<ParamDef>,
    /// Return type annotation; `Some` marks a pure function that is evaluated
    /// at runtime instead of being expanded.
    pub return_type: Option<crate::ast::TypeAnnotation>,
    /// Body statements of the function.
    pub body: Vec<Stmt>,
}

/// Maps instance label → fn_name → function template.
pub type InstanceFnRegistry = HashMap<String, HashMap<String, FnTemplate>>;

/// A fully loaded program: top-level statements, component registry, and namespaces.
#[derive(Clone, Debug, Default)]
pub struct LoadedProgram {
    /// Top-level statements from the entry file and its transitive imports.
    pub statements: Vec<Stmt>,
    /// All components keyed by name, collected from the entry file and imports.
    pub components: HashMap<String, ComponentEntry>,
    /// Module-scoped actions: action_name → template.
    pub module_fns: HashMap<String, FnTemplate>,
    /// Namespaces exported by aliased imports, keyed by alias name.
    pub namespaces: HashMap<String, Namespace>,
}

impl LoadedProgram {
    /// Run the gradual type checker on the program's statements.
    ///
    /// Validates component instantiation properties against parameter type
    /// annotations. Returns diagnostics for any type mismatches found.
    /// Unannotated parameters accept any value.
    pub fn typecheck(&mut self) -> Vec<crate::diagnostics::Diagnostic> {
        let strict_types = self.extract_strict_types();
        let mut env = crate::typecheck::TypeEnv::new(&self.components, &self.module_fns)
            .with_strict_types(strict_types);
        env.register_module_aliases(&self.namespaces);
        env.check_statements(&self.statements)
    }

    /// Extract `strict_types` config value from prelude config statements.
    fn extract_strict_types(&self) -> bool {
        for stmt in &self.statements {
            if let Stmt::Config { settings, .. } = stmt {
                for setting in settings {
                    if setting.name == "strict_types" {
                        return match &setting.value {
                            crate::ast::Expr::Bool(b) => *b,
                            crate::ast::Expr::Str(s) => s.parse().unwrap_or(false),
                            _ => false,
                        };
                    }
                }
            }
        }
        false
    }

    /// Expand component instances into concrete statements and expand
    /// timeline-function calls. Expansion diagnostics (recursion cycles,
    /// statement-level pure-function calls) are appended to `diagnostics`.
    pub fn expand_components(&self, diagnostics: &mut Vec<String>) -> Vec<Stmt> {
        let (stmts, registry) = expand_statements(&self.statements, &self.components);
        inline_actions::expand_fn_calls(stmts, &registry, &self.module_fns, diagnostics)
    }
}

/// Raw scene data from an imported module, stored in a namespace.
/// Used to build cross-file scene timelines at composition time.
#[derive(Clone, Debug)]
pub struct SceneData {
    /// Scene name from the `# SceneName` declaration.
    pub name: String,
    /// Scene-level config properties (e.g. colorscheme, duration).
    pub config: Vec<Property>,
    /// Scene body statements (keyframes, actors, actions, etc.).
    pub body: Vec<Stmt>,
    /// Top-level statements before the first scene in the source file.
    /// Provides shared context (components, pub lets, config) needed
    /// to build the scene's timeline.
    pub file_prelude: Vec<Stmt>,
    /// Source span of the scene declaration.
    pub span: Option<Span>,
}

/// A namespace of exported values from an aliased import.
#[derive(Clone, Debug, Default)]
pub struct Namespace {
    /// Exported values keyed by name.
    pub exports: HashMap<String, Expr>,
    /// Exported type aliases keyed by name.
    pub type_exports: HashMap<String, TypeAnnotation>,
    /// Exported component definitions keyed by name.
    pub component_exports: HashMap<String, ComponentDef>,
    /// Scene definitions from this module, keyed by scene name.
    pub scenes: HashMap<String, SceneData>,
    /// Nested namespaces from this module's aliased imports.
    pub namespaces: HashMap<String, Namespace>,
}

/// Collects all `pub let` declarations from statements, recursing into
/// Keyframe, RelativeKeyframe, Sequence, and Stagger bodies.
use helpers::ParsedModule;

mod graph;
mod helpers;
#[cfg(test)]
mod tests;

pub use helpers::ModuleError;
