use crate::ast::{Import, Stmt};
use crate::parser::parser;
use chumsky::Parser;
use std::collections::HashMap;
use std::collections::HashSet;
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

struct ParsedModule {
    id: FileId,
    path: PathBuf,
    statements: Vec<Stmt>,
    imports: Vec<Import>,
}

struct LoadResult {
    statements: Vec<Stmt>,
    import_ids: Vec<FileId>,
}

#[derive(Debug)]
pub enum ModuleError {
    FileNotFound(PathBuf),
    ParseError(String),
    CycleDetected(Vec<PathBuf>),
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
        if trimmed.starts_with("./") || trimmed.starts_with("../") {
            base_dir.join(trimmed)
        } else {
            base_dir.join(trimmed)
        }
    }

    fn load_file(
        &mut self,
        path: &Path,
        visiting: &mut HashSet<PathBuf>,
    ) -> Result<LoadResult, ModuleError> {
        let canonical = fs::canonicalize(path).map_err(ModuleError::IoError)?;

        if let Some(&id) = self.paths.get(&canonical) {
            let import_ids = self.collect_import_ids(id);
            return Ok(LoadResult {
                statements: self
                    .files
                    .get(&id)
                    .map(|m| m.statements.clone())
                    .unwrap_or_default(),
                import_ids,
            });
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

        let source = fs::read_to_string(&canonical).map_err(ModuleError::IoError)?;

        let (statements, parse_errors) = parser().parse(&source).into_output_errors();

        if !parse_errors.is_empty() {
            let errors: Vec<String> = parse_errors.iter().map(|e| format!("{:?}", e)).collect();
            return Err(ModuleError::ParseError(errors.join("\n")));
        }

        let statements = statements.unwrap_or_default();

        let imports: Vec<Import> = statements
            .iter()
            .filter_map(|s| {
                if let Stmt::Import { path } = s {
                    Some(Import { path: path.clone() })
                } else {
                    None
                }
            })
            .collect();

        let id = self.alloc_id();
        self.paths.insert(canonical.clone(), id);

        let mut all_import_ids = Vec::new();

        for import in &imports {
            let import_path =
                Self::resolve_path(canonical.parent().unwrap_or(Path::new(".")), &import.path);

            let result = self.load_file(&import_path, visiting)?;
            all_import_ids.push(
                self.paths
                    .get(&fs::canonicalize(&import_path).map_err(ModuleError::IoError)?)
                    .copied()
                    .unwrap(),
            );
            all_import_ids.extend(result.import_ids);
        }

        let module = ParsedModule {
            id,
            path: canonical.clone(),
            statements,
            imports,
        };

        self.files.insert(id, module);

        visiting.remove(&canonical);

        Ok(LoadResult {
            statements: self
                .files
                .get(&id)
                .map(|m| m.statements.clone())
                .unwrap_or_default(),
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
        let mut visiting = HashSet::new();
        self.load_file(path, &mut visiting)?;

        let canonical = fs::canonicalize(path).map_err(ModuleError::IoError)?;
        let entry_id = self
            .paths
            .get(&canonical)
            .copied()
            .ok_or_else(|| ModuleError::FileNotFound(path.to_path_buf()))?;

        let mut result = Vec::new();
        self.flatten_recursive(entry_id, &mut result, &mut Vec::new())?;

        Ok(result)
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
                if !matches!(stmt, Stmt::Import { .. }) {
                    result.push(stmt.clone());
                }
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
