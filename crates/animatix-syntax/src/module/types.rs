use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::helpers::ParsedModule;
use crate::ast::{ComponentDef, Expr, ParamDef, Property, Span, Stmt, TypeAnnotation};

/// Unique identifier for a loaded source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId(pub(super) u32);

/// Controls whether [`ModuleGraph`] may read a path from disk when no
/// in-memory source override exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SourceAccess {
    /// Read registered in-memory sources and fall back to the filesystem.
    #[default]
    DiskAndSources,
    /// Only read registered in-memory sources. Missing in-memory files are
    /// reported as `FileNotFound` without touching the filesystem.
    SourcesOnly,
}

/// Tracks loaded files, their imports, and the component registry.
#[derive(Clone, Debug)]
pub struct ModuleGraph {
    pub(super) files: HashMap<FileId, ParsedModule>,
    pub(super) next_id: u32,
    pub(super) paths: HashMap<PathBuf, FileId>,
    /// In-memory source overrides and normalized path identity.
    pub(super) sources: super::source_map::SourceMap,
    pub(super) source_access: SourceAccess,
}

/// A component definition together with its source file and timeline functions.
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    /// The parsed component definition.
    pub definition: ComponentDef,
    /// Absolute path to the file that defined this component.
    pub source_path: PathBuf,
    /// Public and private methods defined inside this component: fn_name → template.
    pub methods: HashMap<String, FnTemplate>,
}

/// A timeline-function template with parameter definitions and body.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FnTemplate {
    /// Parameter definitions for this function.
    pub params: Vec<ParamDef>,
    /// Optional declared result type, checked against inferred function results.
    pub return_type: Option<TypeAnnotation>,
    /// Body statements of the function.
    pub body: Vec<Stmt>,
    /// Whether the function is exported from its defining module.
    pub is_pub: bool,
    /// Source module where unqualified calls inside this body resolve.
    pub definition_path: PathBuf,
    /// Source span of the declaration.
    pub span: Option<Span>,
    /// Declaration-time index expression for an indexed component instance.
    /// Set only on instance-scoped component methods so their bodies can be
    /// rebound when invoked with a different array index.
    pub instance_index: Option<Expr>,
    /// Caller-scope identifiers referenced by this component instance's
    /// parameter bindings. Used only by instance-scoped methods; ordinary
    /// module functions leave this empty.
    pub component_captures: Vec<String>,
}

/// Maps instance label → fn_name → function template.
pub type InstanceFnRegistry = HashMap<String, HashMap<String, FnTemplate>>;

/// Fully expanded source statements together with per-instance component
/// methods needed by the engine's callable VM.
#[derive(Clone, Debug, Default)]
pub struct ExpandedProgram {
    /// Expanded source statements consumed by the timeline builder.
    pub statements: Vec<Stmt>,
    /// Methods for component instances outside explicit scene declarations.
    pub instance_methods: InstanceFnRegistry,
    /// Methods scoped to each explicitly declared scene, keyed by scene name.
    pub scene_methods: HashMap<String, InstanceFnRegistry>,
    /// Source-aware functions and module values after embedded plan expansion.
    /// Bare `expand_statements` calls have no module context and leave this unset.
    pub expanded_function_context: Option<FunctionContext>,
}

/// A module's lexical function environment used when executing a function.
#[derive(Clone, Debug, Default)]
pub struct FunctionModule {
    /// Source statements for this module, including private functions and values.
    pub statements: Vec<Stmt>,
    /// All functions declared locally in this source file.
    pub functions: HashMap<String, FnTemplate>,
    /// Functions visible without a namespace prefix from this source file.
    pub visible_functions: HashMap<String, FnTemplate>,
    /// Namespace aliases declared by this module.
    pub namespaces: HashMap<String, Namespace>,
    /// Component definitions visible from this source file, with their exact
    /// defining paths and method templates.
    pub components: HashMap<String, ComponentEntry>,
}

/// Lexical source data required by normal function evaluation.
#[derive(Clone, Debug, Default)]
pub struct FunctionContext {
    /// Function bindings visible to the entry program after unaliased imports.
    pub visible: Arc<HashMap<String, FnTemplate>>,
    /// Per-source-module lexical scopes, keyed by canonical source path.
    pub modules: Arc<HashMap<PathBuf, FunctionModule>>,
    /// Defining source module for scenes flattened into the entry program.
    pub scene_paths: Arc<HashMap<String, PathBuf>>,
}

impl FunctionContext {
    /// Expand one imported scene with the component scope of its defining file.
    ///
    /// The scene's prelude, configuration and body are expanded together so
    /// component declarations inside reusable plans receive their own method
    /// registries. A missing source module is an error rather than an empty
    /// component scope.
    pub fn expand_scene(&self, scene: &SceneData) -> Result<ExpandedProgram, ExpansionError> {
        let mut diagnostics = Vec::new();
        let module = self.modules.get(&scene.definition_path).ok_or_else(|| ExpansionError {
            diagnostics: vec![
                crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::MissingExpansionScope,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!(
                        "Missing defining module scope for scene '{}': {}",
                        scene.name,
                        scene.definition_path.display()
                    ),
                )
                .with_path(&scene.definition_path),
            ],
        })?;
        let mut statements = scene.file_prelude.clone();
        if !scene.config.is_empty() {
            statements.push(Stmt::Config {
                settings: scene.config.clone(),
                span: scene.span,
            });
        }
        statements.extend(scene.body.clone());
        let component_scopes = self
            .modules
            .iter()
            .map(|(path, module)| (path.clone(), module.components.clone()))
            .collect();
        let mut expanded = super::expand::expand_statements_with_component_scopes(
            &statements,
            &module.components,
            &component_scopes,
            &mut diagnostics,
        );
        expanded.expanded_function_context =
            Some(super::expand::expand_function_context(self, &mut diagnostics));
        super::expand::finish_expansion(expanded, diagnostics)
    }
}

/// A fully loaded program: top-level statements, component registry, and namespaces.
#[derive(Clone, Debug, Default)]
pub struct LoadedProgram {
    /// Top-level statements from the entry file and its transitive imports.
    pub statements: Vec<Stmt>,
    /// All components keyed by name, collected from the entry file and imports.
    pub components: HashMap<String, ComponentEntry>,
    /// Module-scoped functions: function_name → template.
    pub module_fns: HashMap<String, FnTemplate>,
    /// Lexical environments needed to evaluate functions in their defining modules.
    pub function_context: FunctionContext,
    /// Namespaces exported by aliased imports, keyed by alias name.
    pub namespaces: HashMap<String, Namespace>,
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
    /// Source module that owns this scene body and its lexical function scope.
    pub definition_path: PathBuf,
}

/// A namespace of exported values from an aliased import.
#[derive(Clone, Debug, Default)]
pub struct Namespace {
    /// Source module that owns this namespace's declarations.
    pub definition_path: PathBuf,
    /// Exported values keyed by name.
    pub exports: HashMap<String, Expr>,
    /// Exported type aliases keyed by name.
    pub type_exports: HashMap<String, TypeAnnotation>,
    /// Exported component definitions keyed by name.
    pub component_exports: HashMap<String, ComponentDef>,
    /// Public module functions keyed by their source name.
    pub function_exports: HashMap<String, FnTemplate>,
    /// Scene definitions from this module, keyed by scene name.
    pub scenes: HashMap<String, SceneData>,
    /// Nested namespaces from this module's aliased imports.
    pub namespaces: HashMap<String, Namespace>,
}

/// Fatal component expansion failure. No partial output can escape with an error.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpansionError {
    /// Structured errors, preserving their source paths and locations.
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
}

impl std::fmt::Display for ExpansionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", crate::diagnostics::format_diagnostic(diagnostic))?;
        }
        Ok(())
    }
}

impl std::error::Error for ExpansionError {}
