mod discovery;
mod expand;
mod rewrite;

use crate::ast::{
    Action, ComponentDef, Expr, Import, InlineItem, Modifier, ParamDef, Property, Stmt,
};
use crate::parser::parser;
use chumsky::Parser;
use discovery::{collect_component_defs, collect_imports, strip_imports};
use expand::expand_statements;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId(u32);

impl FileId {
    pub fn new(id: u32) -> Self {
        FileId(id)
    }
    pub fn index(&self) -> u32 {
        self.0
    }
}

pub struct ModuleGraph {
    files: HashMap<FileId, ParsedModule>,
    next_id: u32,
    paths: HashMap<PathBuf, FileId>,
}

#[derive(Clone, Debug)]
pub struct ComponentEntry {
    pub definition: ComponentDef,
    pub source_path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct LoadedProgram {
    pub statements: Vec<Stmt>,
    pub components: HashMap<String, ComponentEntry>,
    pub namespaces: HashMap<String, Namespace>,
}

impl LoadedProgram {
    pub fn expand_components(&self) -> Vec<Stmt> {
        expand_statements(&self.statements, &self.components)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Namespace {
    pub exports: HashMap<String, Expr>,
}

/// Collects all `pub let` declarations from statements, recursing into
/// Keyframe, RelativeKeyframe, Sequence, and Stagger bodies.
fn collect_pub_lets(statements: &[Stmt]) -> HashMap<String, Expr> {
    let mut result = HashMap::new();
    collect_pub_lets_inner(statements, &mut result);
    result
}

fn collect_pub_lets_inner(statements: &[Stmt], result: &mut HashMap<String, Expr>) {
    for stmt in statements {
        match stmt {
            Stmt::LetDecl { is_pub, name, value, .. } => {
                if *is_pub {
                    result.insert(name.clone(), value.clone());
                }
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. } => {
                collect_pub_lets_inner(body, result);
            }
            _ => {}
        }
    }
}

struct ParsedModule {
    path: PathBuf,
    statements: Vec<Stmt>,
    imports: Vec<Import>,
}

struct SourceOverride<'a> {
    path: &'a Path,
    source: &'a str,
}

struct LoadResult {
    import_ids: Vec<FileId>,
}

#[derive(Debug)]
pub enum ModuleError {
    FileNotFound(PathBuf),
    ParseError(String),
    CycleDetected(Vec<PathBuf>),
    DuplicateComponent {
        name: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    UnfilledSlot {
        component: String,
        slot_name: String,
    },
    IoError(std::io::Error),
}

impl fmt::Display for ModuleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleError::FileNotFound(path) => {
                write!(f, "File not found: {}", path.display())
            }
            ModuleError::ParseError(msg) => {
                write!(f, "Parse error: {}", msg)
            }
            ModuleError::CycleDetected(paths) => {
                let cycle = paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" -> ");
                write!(f, "Circular dependency detected: {}", cycle)
            }
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
            ModuleError::UnfilledSlot {
                component,
                slot_name,
            } => {
                write!(
                    f,
                    "Unfilled slot: component '{}' requires slot '{}' but no fill provided and no defaults exist",
                    component, slot_name
                )
            }
            ModuleError::IoError(e) => {
                write!(f, "IO error: {}", e)
            }
        }
    }
}

impl ModuleGraph {
    pub fn new() -> Self {
        ModuleGraph {
            files: HashMap::new(),
            next_id: 0,
            paths: HashMap::new(),
        }
    }

    fn alloc_id(&mut self) -> FileId {
        let id = FileId::new(self.next_id);
        self.next_id += 1;
        id
    }

    fn resolve_path(base_dir: &Path, import_path: &str) -> PathBuf {
        let trimmed = import_path.trim_matches('"');
        base_dir.join(trimmed)
    }

    fn load_file(
        &mut self,
        path: &Path,
        visiting: &mut HashSet<PathBuf>,
        source_override: Option<&SourceOverride<'_>>,
    ) -> Result<LoadResult, ModuleError> {
        let canonical = fs::canonicalize(path).map_err(ModuleError::IoError)?;

        if let Some(&id) = self.paths.get(&canonical) {
            let import_ids = self.collect_import_ids(id);
            return Ok(LoadResult { import_ids });
        }

        if visiting.contains(&canonical) {
            let cycle_path: Vec<PathBuf> = visiting
                .iter()
                .skip_while(|p| *p != &canonical)
                .chain(std::iter::once(&canonical))
                .cloned()
                .collect();
            return Err(ModuleError::CycleDetected(cycle_path));
        }

        visiting.insert(canonical.clone());

        let source = source_override
            .filter(|override_source| override_source.path == canonical.as_path())
            .map(|override_source| override_source.source.to_owned())
            .unwrap_or(fs::read_to_string(&canonical).map_err(ModuleError::IoError)?);

        let (statements, parse_errors) = parser().parse(&source).into_output_errors();

        if !parse_errors.is_empty() {
            let errors: Vec<String> = parse_errors.iter().map(|e| format!("{:?}", e)).collect();
            return Err(ModuleError::ParseError(errors.join("\n")));
        }

        let statements = statements.unwrap_or_default();

        let imports = collect_imports(&statements);

        let id = self.alloc_id();
        self.paths.insert(canonical.clone(), id);

        let mut all_import_ids = Vec::new();

        for import in &imports {
            let import_path =
                Self::resolve_path(canonical.parent().unwrap_or(Path::new(".")), &import.path);

            let result = self.load_file(&import_path, visiting, source_override)?;
            all_import_ids.push(
                self.paths
                    .get(&fs::canonicalize(&import_path).map_err(ModuleError::IoError)?)
                    .copied()
                    .unwrap(),
            );
            all_import_ids.extend(result.import_ids);
        }

        let module = ParsedModule {
            path: canonical.clone(),
            statements,
            imports,
        };

        self.files.insert(id, module);

        visiting.remove(&canonical);

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
                    let path = Self::resolve_path(
                        module.path.parent().unwrap_or(Path::new(".")),
                        &imp.path,
                    );
                    fs::canonicalize(&path)
                        .ok()
                        .and_then(|p| self.paths.get(&p).copied())
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn load_entry(&mut self, path: &Path) -> Result<Vec<Stmt>, ModuleError> {
        self.load_entry_with_source(path, None)
    }

    pub fn load_entry_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<Vec<Stmt>, ModuleError> {
        let mut visiting = HashSet::new();
        let canonical = fs::canonicalize(path).map_err(ModuleError::IoError)?;
        let source_override = source.map(|source| SourceOverride {
            path: canonical.as_path(),
            source,
        });

        self.load_file(path, &mut visiting, source_override.as_ref())?;

        let entry_id = self
            .paths
            .get(&canonical)
            .copied()
            .ok_or_else(|| ModuleError::FileNotFound(path.to_path_buf()))?;

        let mut result = Vec::new();
        self.flatten_recursive(entry_id, &mut result, &mut Vec::new())?;

        Ok(result)
    }

    pub fn load_program(&mut self, path: &Path) -> Result<LoadedProgram, ModuleError> {
        self.load_program_with_source(path, None)
    }

    pub fn load_program_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<LoadedProgram, ModuleError> {
        let mut visiting = HashSet::new();
        let canonical = fs::canonicalize(path).map_err(ModuleError::IoError)?;
        let source_override = source.map(|source| SourceOverride {
            path: canonical.as_path(),
            source,
        });

        self.load_file(path, &mut visiting, source_override.as_ref())?;

        let entry_id = self
            .paths
            .get(&canonical)
            .copied()
            .ok_or_else(|| ModuleError::FileNotFound(path.to_path_buf()))?;

        let mut statements = Vec::new();
        self.flatten_recursive(entry_id, &mut statements, &mut Vec::new())?;

        let mut components = HashMap::new();
        self.collect_components_recursive(entry_id, entry_id, &mut components, &mut Vec::new())?;

        let mut namespaces = HashMap::new();
        // Only collect namespaces from the entry file's direct aliased imports
        if let Some(entry_module) = self.files.get(&entry_id) {
            for imp in &entry_module.imports {
                if let Some(alias) = &imp.alias {
                    let import_path = Self::resolve_path(
                        entry_module.path.parent().unwrap_or(Path::new(".")),
                        &imp.path,
                    );
                    if let Some(imported_module) = fs::canonicalize(&import_path)
                        .ok()
                        .and_then(|p| self.paths.get(&p).copied())
                        .and_then(|import_id| self.files.get(&import_id))
                    {
                        let exports = collect_pub_lets(&imported_module.statements);
                        namespaces.insert(alias.clone(), Namespace { exports });
                    }
                }
            }
        }

        Ok(LoadedProgram {
            statements,
            components,
            namespaces,
        })
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
                if imp.alias.is_some() {
                    continue; // Aliased imports are not flattened
                }
                let import_path =
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.path);
                if let Some(import_id) = fs::canonicalize(&import_path)
                    .ok()
                    .and_then(|p| self.paths.get(&p).copied())
                {
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
                    Self::resolve_path(module.path.parent().unwrap_or(Path::new(".")), &imp.path);
                if let Some(import_id) = fs::canonicalize(&import_path)
                    .ok()
                    .and_then(|p| self.paths.get(&p).copied())
                {
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

                components.insert(
                    definition.name.clone(),
                    ComponentEntry {
                        definition,
                        source_path: module.path.clone(),
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
