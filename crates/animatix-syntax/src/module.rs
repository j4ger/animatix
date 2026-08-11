//! Module system for the Animatix language: multi-file programs, imports,
//! component collection, and namespace resolution.

pub mod discovery;
mod expand;
mod inline_actions;
mod rewrite;

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

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        if matches!(component, std::path::Component::CurDir) {
            continue;
        }
        normalized.push(component.as_os_str());
    }
    normalized
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

/// Tracks loaded files, their imports, and the component registry.
pub struct ModuleGraph {
    files: HashMap<FileId, ParsedModule>,
    next_id: u32,
    paths: HashMap<PathBuf, FileId>,
    /// In-memory source overrides for paths that may not exist on disk.
    sources: HashMap<PathBuf, String>,
}

/// A component definition together with its source file and custom actions.
#[derive(Clone, Debug)]
pub struct ComponentEntry {
    /// The parsed component definition.
    pub definition: ComponentDef,
    /// Absolute path to the file that defined this component.
    pub source_path: PathBuf,
    /// Custom action templates defined inside this component: action_name → template.
    pub actions: HashMap<String, ActionTemplate>,
}

/// A component action template with parameter definitions and body.
#[derive(Clone, Debug)]
pub struct ActionTemplate {
    /// Parameter definitions for this action.
    pub params: Vec<ParamDef>,
    /// Body statements of the action.
    pub body: Vec<Stmt>,
}

/// Maps instance label → action_name → action template.
pub type InstanceActionRegistry = HashMap<String, HashMap<String, ActionTemplate>>;

/// A fully loaded program: top-level statements, component registry, and namespaces.
#[derive(Clone, Debug, Default)]
pub struct LoadedProgram {
    /// Top-level statements from the entry file and its transitive imports.
    pub statements: Vec<Stmt>,
    /// All components keyed by name, collected from the entry file and imports.
    pub components: HashMap<String, ComponentEntry>,
    /// Module-scoped actions: action_name → template.
    pub module_actions: HashMap<String, ActionTemplate>,
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
        let mut env = crate::typecheck::TypeEnv::new(&self.components, &self.module_actions)
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

    /// Expand component instances into concrete statements and inline custom actions.
    pub fn expand_components(&self) -> Vec<Stmt> {
        let (stmts, registry) = expand_statements(&self.statements, &self.components);
        inline_actions::inline_custom_actions(stmts, &registry, &self.module_actions)
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
    /// Scene definitions from this module, keyed by scene name.
    pub scenes: HashMap<String, SceneData>,
    /// Nested namespaces from this module's aliased imports.
    pub namespaces: HashMap<String, Namespace>,
}

/// Collects all `pub let` declarations from statements, recursing into
/// Keyframe, RelativeKeyframe, Sequence, and Stagger bodies.
fn collect_pub_lets(statements: &[Stmt]) -> HashMap<String, Expr> {
    let mut result = HashMap::new();
    collect_pub_lets_inner(statements, &mut result);
    result
}

fn collect_pub_type_aliases(statements: &[Stmt]) -> HashMap<String, TypeAnnotation> {
    let mut result = HashMap::new();
    for stmt in statements {
        if let Stmt::TypeAlias {
            is_pub: true,
            name,
            annotation,
            ..
        } = stmt
        {
            result.insert(name.clone(), annotation.clone());
        }
    }
    result
}

fn collect_pub_lets_inner(statements: &[Stmt], result: &mut HashMap<String, Expr>) {
    for stmt in statements {
        match stmt {
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                ..
            } => {
                if *is_pub {
                    result.insert(name.clone(), value.clone());
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. } => {
                collect_pub_lets_inner(body, result);
            },
            _ => {},
        }
    }
}

/// Resolve re-export expressions by looking up paths that reference imported modules.
///
/// If a `pub let` value is `c.accent` and `c` is an aliased import, this resolves
/// the path by looking up `accent` in `c`'s namespace exports.
fn resolve_exports(
    raw_exports: HashMap<String, Expr>,
    import_namespaces: &HashMap<String, &Namespace>,
) -> HashMap<String, Expr> {
    let mut resolved = HashMap::with_capacity(raw_exports.len());
    for (name, expr) in raw_exports {
        let resolved_expr = resolve_reexport_expr(&expr, import_namespaces);
        resolved.insert(name, resolved_expr);
    }
    resolved
}

fn resolve_reexport_expr(expr: &Expr, import_namespaces: &HashMap<String, &Namespace>) -> Expr {
    match expr {
        Expr::Path(segments) if !segments.is_empty() => {
            if let Some(ns) = import_namespaces.get(&segments[0]) {
                // This is a re-export path like `c.accent`
                // Try to resolve the remaining segments
                resolve_path_in_namespace(ns, &segments[1..], import_namespaces)
                    .unwrap_or_else(|| expr.clone())
            } else {
                expr.clone()
            }
        },
        _ => expr.clone(),
    }
}

fn resolve_path_in_namespace(
    ns: &Namespace,
    segments: &[String],
    import_namespaces: &HashMap<String, &Namespace>,
) -> Option<Expr> {
    if segments.is_empty() {
        return None;
    }
    if let Some(nested) = ns.namespaces.get(&segments[0]) {
        return resolve_path_in_namespace(nested, &segments[1..], import_namespaces);
    }
    if let Some(expr) = ns.exports.get(&segments[0]) {
        if segments.len() == 1 {
            // Fully resolved — but the value itself might be another re-export
            Some(resolve_reexport_expr(expr, import_namespaces))
        } else {
            // More segments to resolve — only possible if the value is a namespace path
            match expr {
                Expr::Path(inner_segments) if !inner_segments.is_empty() => {
                    if let Some(next_ns) = import_namespaces.get(&inner_segments[0]) {
                        resolve_path_in_namespace(next_ns, &inner_segments[1..], import_namespaces)
                    } else {
                        None
                    }
                },
                _ => None,
            }
        }
    } else {
        None
    }
}

struct ParsedModule {
    path: PathBuf,
    statements: Vec<Stmt>,
    imports: Vec<(String, Option<String>)>,
}

struct LoadResult {
    import_ids: Vec<FileId>,
}

/// Errors that can occur while loading or resolving modules.
#[derive(Debug)]
pub enum ModuleError {
    /// The requested file could not be found on disk.
    FileNotFound(PathBuf),
    /// One or more parse errors occurred while reading a file.
    ParseErrors(Vec<ParseError>),
    /// A circular import dependency was detected.
    CycleDetected(Vec<PathBuf>),
    /// The same component name was exported from two different files.
    DuplicateComponent {
        /// Name of the duplicated component.
        name: String,
        /// Path of the first file that defined it.
        first_path: PathBuf,
        /// Path of the second file that defined it.
        second_path: PathBuf,
    },
    /// An underlying I/O error occurred.
    IoError(std::io::Error),
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleError::FileNotFound(path) => {
                write!(f, "File not found: {}", path.display())
            },
            ModuleError::ParseErrors(errors) => {
                for (i, err) in errors.iter().enumerate() {
                    if i > 0 {
                        writeln!(f)?;
                    }
                    let location = if err.line > 0 && err.column > 0 {
                        format!("{}:{}", err.line, err.column)
                    } else {
                        String::new()
                    };
                    if location.is_empty() {
                        write!(f, "parse error: {}", err.message)?;
                    } else {
                        write!(f, "parse error at {}: {}", location, err.message)?;
                    }
                    if !err.expected.is_empty() {
                        write!(f, "\n  expected: {}", err.expected.join(", "))?;
                    }
                    if let Some(found) = &err.found {
                        write!(f, "\n  found: '{}'", found)?;
                    }
                    if !err.context.is_empty() {
                        write!(f, "\n  context: {}", err.context.join(" > "))?;
                    }
                }
                Ok(())
            },
            ModuleError::CycleDetected(paths) => {
                let cycle =
                    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(" -> ");
                write!(f, "Circular dependency detected: {}", cycle)
            },
            ModuleError::DuplicateComponent {
                name,
                first_path,
                second_path,
            } => write!(
                f,
                "Duplicate component export '{}' found in {} and {}",
                name,
                first_path.display(),
                second_path.display()
            ),
            ModuleError::IoError(e) => {
                write!(f, "IO error: {}", e)
            },
        }
    }
}

impl ModuleGraph {
    /// Create an empty `ModuleGraph`.
    pub fn new() -> Self {
        ModuleGraph {
            files: HashMap::new(),
            next_id: 0,
            paths: HashMap::new(),
            sources: HashMap::new(),
        }
    }

    /// Register an in-memory source for a path.
    ///
    /// Paths must be absolute. Imports resolve against the same source map, so
    /// multi-file programs can be loaded entirely without touching the disk.
    pub fn add_source(&mut self, path: PathBuf, source: impl Into<String>) {
        let path = normalize_path(&path);
        if let Ok(key) = self.path_key(&path) {
            if let Some(old_id) = self.paths.remove(&key) {
                self.files.remove(&old_id);
            }
        }
        self.sources.insert(path, source.into());
    }

    /// Remove a previously registered in-memory source, if any.
    pub fn remove_source(&mut self, path: &Path) {
        let path = normalize_path(path);
        if let Some(old_id) = self.paths.remove(&path) {
            self.files.remove(&old_id);
        }
        self.sources.remove(&path);
    }

    fn path_key(&self, path: &Path) -> Result<PathBuf, ModuleError> {
        let path = normalize_path(path);
        if self.sources.contains_key(&path) {
            Ok(path)
        } else {
            fs::canonicalize(&path).map_err(|_| ModuleError::FileNotFound(path))
        }
    }

    fn file_id_for_path(&self, path: &Path) -> Option<FileId> {
        self.path_key(path).ok().and_then(|key| self.paths.get(&key).copied())
    }

    fn alloc_id(&mut self) -> FileId {
        let id = FileId::new(self.next_id);
        self.next_id += 1;
        id
    }

    fn resolve_path(base_dir: &Path, import_path: &str) -> PathBuf {
        let trimmed = import_path.trim_matches('"');
        normalize_path(&base_dir.join(trimmed))
    }

    fn load_file(
        &mut self,
        path: &Path,
        visiting: &mut HashSet<PathBuf>,
    ) -> Result<LoadResult, ModuleError> {
        let key = self.path_key(path)?;

        if let Some(&id) = self.paths.get(&key) {
            let import_ids = self.collect_import_ids(id);
            return Ok(LoadResult { import_ids });
        }

        if visiting.contains(&key) {
            let cycle_path: Vec<PathBuf> = visiting
                .iter()
                .skip_while(|p| *p != &key)
                .chain(std::iter::once(&key))
                .cloned()
                .collect();
            return Err(ModuleError::CycleDetected(cycle_path));
        }

        visiting.insert(key.clone());

        let source = if let Some(source) = self.sources.get(&key) {
            source.clone()
        } else {
            fs::read_to_string(&key).map_err(|_| ModuleError::FileNotFound(key.clone()))?
        };

        let parsed = parse_canonical(&source);
        let parse_errors = parsed.parse_errors;

        if !parse_errors.is_empty() {
            return Err(ModuleError::ParseErrors(parse_errors));
        }

        let mut statements = parsed.statements.unwrap_or_default();

        // Convert byte spans captured during parsing into line/col spans.
        set_action_spans(&mut statements, &source);

        let imports = collect_imports(&statements);

        let id = self.alloc_id();
        self.paths.insert(key.clone(), id);

        let mut all_import_ids = Vec::new();

        for import in &imports {
            let import_path =
                Self::resolve_path(key.parent().unwrap_or(Path::new(".")), &import.0);

            let result = self.load_file(&import_path, visiting)?;
            let import_id = self
                .file_id_for_path(&import_path)
                .ok_or_else(|| ModuleError::FileNotFound(import_path.clone()))?;
            all_import_ids.push(import_id);
            all_import_ids.extend(result.import_ids);
        }

        let module = ParsedModule {
            path: key.clone(),
            statements,
            imports,
        };

        self.files.insert(id, module);

        visiting.remove(&key);

        Ok(LoadResult {
            import_ids: all_import_ids,
        })
    }

    fn collect_import_ids(&self, file_id: FileId) -> Vec<FileId> {
        if let Some(module) = self.files.get(&file_id) {
            module
                .imports
                .iter()
                .filter_map(|imp| {
                    let path =
                        Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
                    self.file_id_for_path(&path)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Load an entry file and return all flattened top-level statements.
    pub fn load_entry(&mut self, path: &Path) -> Result<Vec<Stmt>, ModuleError> {
        self.load_entry_with_source(path, None)
    }

    /// Load an entry file with optional source override and return all flattened top-level
    /// statements.
    pub fn load_entry_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<Vec<Stmt>, ModuleError> {
        let previous = self.sources.get(path).cloned();
        let inserted = if let Some(source) = source {
            self.add_source(path.to_path_buf(), source.to_string());
            true
        } else {
            false
        };

        let result = (|| {
            let mut visiting = HashSet::new();
            let key = self.path_key(path)?;

            self.load_file(path, &mut visiting)?;

            let entry_id = self
                .paths
                .get(&key)
                .copied()
                .ok_or_else(|| ModuleError::FileNotFound(path.to_path_buf()))?;

            let mut result = Vec::new();
            self.flatten_recursive(entry_id, &mut result, &mut Vec::new())?;

            Ok(result)
        })();

        if inserted {
            match previous {
                Some(previous) => self.add_source(path.to_path_buf(), previous),
                None => self.remove_source(path),
            }
        }
        result
    }

    /// Load an entry file and return a fully resolved `LoadedProgram`.
    pub fn load_program(&mut self, path: &Path) -> Result<LoadedProgram, ModuleError> {
        self.load_program_with_source(path, None)
    }

    /// Load an entry file with optional source override and return a fully resolved
    /// `LoadedProgram`.
    pub fn load_program_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<LoadedProgram, ModuleError> {
        let previous = self.sources.get(path).cloned();
        let inserted = if let Some(source) = source {
            self.add_source(path.to_path_buf(), source.to_string());
            true
        } else {
            false
        };

        let result = (|| {
            let mut visiting = HashSet::new();
            let key = self.path_key(path)?;

            self.load_file(path, &mut visiting)?;

            let entry_id = self
                .paths
                .get(&key)
                .copied()
                .ok_or_else(|| ModuleError::FileNotFound(path.to_path_buf()))?;

            let mut statements = Vec::new();
            self.flatten_recursive(entry_id, &mut statements, &mut Vec::new())?;

            let mut components = HashMap::new();
            self.collect_components_recursive(entry_id, entry_id, &mut components, &mut Vec::new())?;

            // Collect module-scoped actions from flattened statements
            let module_actions = Self::collect_module_actions(&statements);

            let mut namespaces = HashMap::new();
            // Collect namespaces from the entry file's direct aliased imports,
            // resolving re-exports transitively.
            if let Some(entry_module) = self.files.get(&entry_id) {
                for imp in &entry_module.imports {
                    if let Some(alias) = &imp.1 {
                        let import_path = Self::resolve_path(
                            entry_module.path.parent().unwrap_or(Path::new(".")),
                            &imp.0,
                        );
                        if let Some(import_id) = self.file_id_for_path(&import_path) {
                            let resolved_exports = self.collect_resolved_exports(import_id);
                            let resolved_type_exports = self.collect_resolved_type_exports(import_id);
                            let resolved_scenes = self.collect_resolved_scenes(import_id);
                            let resolved_namespaces = self.collect_resolved_namespaces(import_id);
                            namespaces.insert(
                                alias.clone(),
                                Namespace {
                                    exports: resolved_exports,
                                    type_exports: resolved_type_exports,
                                    scenes: resolved_scenes,
                                    namespaces: resolved_namespaces,
                                },
                            );
                        }
                    }
                }
            }

            Ok(LoadedProgram {
                statements,
                components,
                module_actions,
                namespaces,
            })
        })();

        if inserted {
            match previous {
                Some(previous) => self.add_source(path.to_path_buf(), previous),
                None => self.remove_source(path),
            }
        }
        result
    }

    /// Collect module-scoped actions from a list of statements.
    /// Walks into keyframe / relative-keyframe / sequence / stagger / always / drive / conditional
    /// / for bodies.
    #[doc(hidden)]
    pub fn collect_module_actions(stmts: &[Stmt]) -> HashMap<String, ActionTemplate> {
        let mut actions = HashMap::new();
        for stmt in stmts {
            Self::collect_module_actions_from_stmt(stmt, &mut actions);
        }
        actions
    }

    fn collect_module_actions_from_stmt(
        stmt: &Stmt,
        actions: &mut HashMap<String, ActionTemplate>,
    ) {
        match stmt {
            Stmt::ComponentAction {
                name, params, body, ..
            } => {
                actions.insert(
                    name.clone(),
                    ActionTemplate {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ForLoop { body, .. } => {
                for stmt in body {
                    Self::collect_module_actions_from_stmt(stmt, actions);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    Self::collect_module_actions_from_stmt(stmt, actions);
                }
                if let Some(else_body) = else_branch {
                    for stmt in else_body {
                        Self::collect_module_actions_from_stmt(stmt, actions);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for stmt in body {
                        Self::collect_module_actions_from_stmt(stmt, actions);
                    }
                }
            },
            _ => {},
        }
    }

    /// Collect resolved exports for a module, resolving re-export paths transitively.
    fn collect_resolved_exports(&self, file_id: FileId) -> HashMap<String, Expr> {
        let module = match self.files.get(&file_id) {
            Some(m) => m,
            None => return HashMap::new(),
        };

        let import_namespaces = self.collect_resolved_namespaces(file_id);

        let raw_exports = collect_pub_lets(&module.statements);
        let import_refs: HashMap<String, &Namespace> =
            import_namespaces.iter().map(|(k, v)| (k.clone(), v)).collect();
        resolve_exports(raw_exports, &import_refs)
    }

    /// Collect exported type aliases from a module.
    fn collect_resolved_type_exports(&self, file_id: FileId) -> HashMap<String, TypeAnnotation> {
        let Some(module) = self.files.get(&file_id) else {
            return HashMap::new();
        };
        collect_pub_type_aliases(&module.statements)
    }

    /// Collect nested namespaces for a module's aliased imports.
    fn collect_resolved_namespaces(&self, file_id: FileId) -> HashMap<String, Namespace> {
        let mut namespaces = HashMap::new();
        let Some(module) = self.files.get(&file_id) else {
            return namespaces;
        };

        for imp in &module.imports {
            let Some(alias) = &imp.1 else {
                continue;
            };
            let import_path =
                Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
            let Some(sub_id) = self.file_id_for_path(&import_path) else {
                continue;
            };
            namespaces.insert(
                alias.clone(),
                Namespace {
                    exports: self.collect_resolved_exports(sub_id),
                    type_exports: self.collect_resolved_type_exports(sub_id),
                    scenes: self.collect_resolved_scenes(sub_id),
                    namespaces: self.collect_resolved_namespaces(sub_id),
                },
            );
        }

        namespaces
    }

    /// Collect resolved scene data for a module, flattening the module's
    /// statements and extracting `Stmt::Scene` nodes with their file prelude.
    fn collect_resolved_scenes(&self, file_id: FileId) -> HashMap<String, SceneData> {
        let module = match self.files.get(&file_id) {
            Some(m) => m,
            None => return HashMap::new(),
        };

        // Flatten this module's statements (including non-aliased imports)
        let mut flat = Vec::new();
        let mut visited = Vec::new();
        // We inline the flatten logic here to avoid borrow conflicts with `self`.
        self.flatten_module_stmts(file_id, &mut flat, &mut visited);

        let mut scenes = collect_scenes_from_stmts(&flat);

        // Recursively collect scenes from aliased sub-imports.
        // These are available as "alias.SceneName" in the parent namespace.
        for imp in &module.imports {
            if let Some(alias) = &imp.1 {
                let import_path =
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
                if let Some(sub_id) = self.file_id_for_path(&import_path) {
                    let sub_scenes = self.collect_resolved_scenes(sub_id);
                    for (name, data) in sub_scenes {
                        // Prefix with alias to make it accessible as "alias.SceneName"
                        scenes.insert(format!("{}.{}", alias, name), data);
                    }
                }
            }
        }

        scenes
    }

    /// Flatten a module's statements into `result`, following non-aliased imports.
    /// Separate from `flatten_recursive` to allow calling from multiple contexts.
    fn flatten_module_stmts(
        &self,
        file_id: FileId,
        result: &mut Vec<Stmt>,
        visited: &mut Vec<FileId>,
    ) {
        if visited.contains(&file_id) {
            return;
        }
        visited.push(file_id);

        if let Some(module) = self.files.get(&file_id) {
            for imp in &module.imports {
                if imp.1.is_some() {
                    continue; // Aliased imports are not flattened
                }
                let import_path =
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
                if let Some(import_id) = self.file_id_for_path(&import_path) {
                    self.flatten_module_stmts(import_id, result, visited);
                }
            }

            for stmt in &module.statements {
                if let Some(stmt) = strip_imports(stmt) {
                    result.push(stmt);
                }
            }
        }
    }

    fn flatten_recursive(
        &self,
        file_id: FileId,
        result: &mut Vec<Stmt>,
        visited: &mut Vec<FileId>,
    ) -> Result<(), ModuleError> {
        if visited.contains(&file_id) {
            return Ok(());
        }
        visited.push(file_id);

        if let Some(module) = self.files.get(&file_id) {
            for imp in &module.imports {
                if imp.1.is_some() {
                    continue; // Aliased imports are not flattened
                }
                let import_path =
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
                if let Some(import_id) = self.file_id_for_path(&import_path) {
                    self.flatten_recursive(import_id, result, visited)?;
                }
            }

            for stmt in &module.statements {
                if let Some(stmt) = strip_imports(stmt) {
                    result.push(stmt);
                }
            }
        }

        Ok(())
    }

    fn collect_components_recursive(
        &self,
        file_id: FileId,
        entry_id: FileId,
        components: &mut HashMap<String, ComponentEntry>,
        visited: &mut Vec<FileId>,
    ) -> Result<(), ModuleError> {
        if visited.contains(&file_id) {
            return Ok(());
        }
        visited.push(file_id);

        if let Some(module) = self.files.get(&file_id) {
            for imp in &module.imports {
                let import_path =
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.0);
                if let Some(import_id) = self.file_id_for_path(&import_path) {
                    self.collect_components_recursive(import_id, entry_id, components, visited)?;
                }
            }

            for definition in collect_component_defs(&module.statements) {
                if file_id != entry_id && !definition.is_pub {
                    continue;
                }

                if let Some(existing) = components.get(&definition.name) {
                    return Err(ModuleError::DuplicateComponent {
                        name: definition.name.clone(),
                        first_path: existing.source_path.clone(),
                        second_path: module.path.clone(),
                    });
                }

                let actions = collect_component_actions(&definition);
                components.insert(
                    definition.name.clone(),
                    ComponentEntry {
                        definition,
                        source_path: module.path.clone(),
                        actions,
                    },
                );
            }
        }

        Ok(())
    }
}

impl Default for ModuleGraph {
    fn default() -> Self {
        Self::new()
    }
}
