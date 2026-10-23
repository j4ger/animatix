use super::helpers::*;
use super::*;

impl ModuleGraph {
    /// Create an empty `ModuleGraph`.
    pub fn new() -> Self {
        ModuleGraph {
            files: HashMap::new(),
            next_id: 0,
            paths: HashMap::new(),
            sources: source_map::SourceMap::new(),
            source_access: SourceAccess::DiskAndSources,
        }
    }

    /// Configure whether paths may be read from disk.
    ///
    /// Analyzer/LSP workspaces should use [`SourceAccess::SourcesOnly`] so
    /// virtual documents never resolve through the filesystem.
    pub fn with_source_access(mut self, source_access: SourceAccess) -> Self {
        self.source_access = source_access;
        self
    }

    /// Return the current source access policy.
    pub fn source_access(&self) -> SourceAccess {
        self.source_access
    }

    /// Register an in-memory source for a path.
    ///
    /// Paths must be absolute. Imports resolve against the same source map, so
    /// multi-file programs can be loaded entirely without touching the disk.
    ///
    /// Updating a source invalidates all parsed modules. Parent modules can
    /// cache a `FileId` for a now-replaced import, so conservatively clearing
    /// the cache avoids silently loading stale imports.
    pub fn add_source(&mut self, path: PathBuf, source: impl Into<String>) {
        self.sources.add_source(path, source.into());
        self.files.clear();
        self.paths.clear();
    }

    /// Remove a previously registered in-memory source, if any.
    ///
    /// Like [`Self::add_source`], this clears parsed modules so cached parents
    /// cannot keep pointing at the removed import.
    pub fn remove_source(&mut self, path: &Path) {
        self.sources.remove_source(path);
        self.files.clear();
        self.paths.clear();
    }

    /// Return the in-memory source for a registered path, if any.
    pub fn source(&self, path: &Path) -> Option<&str> {
        self.sources.get(path)
    }

    /// Insert or replace a source without clearing unrelated parsed modules.
    ///
    /// Only the replaced path is invalidated. This is the right granularity for
    /// temporary source overrides and analyzer workspace updates, where other
    /// imported files remain unchanged.
    pub fn upsert_source(&mut self, path: PathBuf, source: impl Into<String>) {
        self.set_source_for_path(path, source.into());
    }

    /// Remove a source override without clearing unrelated parsed modules.
    pub fn remove_source_for_path(&mut self, path: &Path) {
        self.clear_source_for_path(path);
    }

    /// Return the parsed AST for a registered path, if any.
    pub fn file_ast(&self, path: &Path) -> Option<&[Stmt]> {
        let id = self.file_id_for_path(path)?;
        self.files.get(&id).map(|module| module.statements.as_slice())
    }

    /// Return the imports for a registered path, if any.
    pub fn file_imports(&self, path: &Path) -> Option<&[(String, Option<String>)]> {
        let id = self.file_id_for_path(path)?;
        self.files.get(&id).map(|module| module.imports.as_slice())
    }

    /// Return the extracted symbol table for a registered path, if any.
    pub fn file_symbols(&self, path: &Path) -> Option<crate::symbol_table::SymbolTable> {
        let id = self.file_id_for_path(path)?;
        self.files.get(&id).map(|module| module.symbols.clone())
    }

    /// Resolve imports for a file and return a merged symbol table containing
    /// local symbols plus exported symbols from imported files.
    ///
    /// This mirrors the analyzer's virtual workspace semantics: aliased imports
    /// become namespaces, direct imports are flattened, and missing imports are
    /// skipped instead of failing the whole workspace.
    pub fn resolve_symbols(&self, path: &Path) -> crate::symbol_table::SymbolTable {
        let mut visited = HashSet::new();
        self.resolve_symbols_inner(path, &mut visited)
    }

    fn resolve_symbols_inner(
        &self,
        path: &Path,
        visited: &mut HashSet<PathBuf>,
    ) -> crate::symbol_table::SymbolTable {
        let mut merged = self.file_symbols(path).unwrap_or_default();
        if !visited.insert(source_map::normalize_path(path)) {
            return merged;
        }

        if let Some(module_path) = self
            .file_id_for_path(path)
            .and_then(|id| self.files.get(&id))
            .map(|module| module.path.clone())
        {
            if let Some(imports) = self.file_imports(&module_path) {
                for (import_path, alias) in imports {
                    let import_path = Self::resolve_path(&module_path, import_path);
                    let resolved = self.resolve_symbols_inner(&import_path, visited);
                    if let Some(alias) = alias {
                        // Aliased import: store only pub exports under the namespace.
                        // Nested scenes are also exposed as `alias.SceneName`,
                        // matching the runtime namespace scene registry.
                        let exported = resolved.exported_namespace();
                        for (scene_name, info) in &exported.scenes {
                            merged
                                .scenes
                                .entry(format!("{alias}.{scene_name}"))
                                .or_insert_with(|| info.clone());
                        }
                        merged.namespaces.insert(alias.clone(), exported);
                    } else {
                        // Direct import: merge all symbols (backward-compatible flatten).
                        merged.merge(&resolved);
                    }
                }
            }
        }

        visited.remove(&source_map::normalize_path(path));
        merged
    }

    /// Insert or replace a source without clearing unrelated parsed modules.
    ///
    /// Only the replaced path is invalidated. This is the right granularity for
    /// temporary source overrides, where other imported files remain unchanged.
    /// Load and cache a single file without resolving its imports.
    ///
    /// Analyzer workspaces use this to keep local symbols available when an
    /// import is missing or intentionally absent. A later `load_program` call
    /// resolves imports normally, reusing this cached module.
    pub fn load_file_standalone(&mut self, path: &Path) -> Result<(), ModuleError> {
        let key = self.path_key(path)?;
        if self.paths.contains_key(&key) {
            return Ok(());
        }

        let source = self
            .sources
            .get(&key)
            .map(str::to_owned)
            .ok_or_else(|| ModuleError::FileNotFound(key.clone()))?;
        let parsed = parse_canonical(&source);
        if !parsed.parse_errors.is_empty() {
            return Err(ModuleError::ParseErrors(parsed.parse_errors));
        }
        let mut statements = parsed.statements.unwrap_or_default();
        set_action_spans(&mut statements, &source);
        let imports = collect_imports(&statements);
        let id = self.alloc_id();
        let symbols = {
            let mut symbols = crate::symbol_table::SymbolTable::build_from_ast(&statements);
            symbols.collect_references(&statements);
            symbols
        };
        self.paths.insert(key.clone(), id);
        self.files.insert(
            id,
            ParsedModule {
                path: key,
                statements,
                imports,
                symbols,
            },
        );
        Ok(())
    }

    fn set_source_for_path(&mut self, path: PathBuf, source: String) {
        self.sources.add_source(path.clone(), source);
        let path = source_map::normalize_path(&path);
        self.invalidate_file_and_dependents(&path);
    }

    /// Remove a source override without clearing unrelated parsed modules.
    fn clear_source_for_path(&mut self, path: &Path) {
        self.sources.remove_source(path);
        let path = source_map::normalize_path(path);
        self.invalidate_file_and_dependents(&path);
    }

    /// Invalidate a path plus every parsed module that (transitively) imports
    /// it, so parents cannot keep using a stale version of the replaced file.
    fn invalidate_file_and_dependents(&mut self, changed: &Path) {
        let mut removed = HashSet::from([changed.to_path_buf()]);
        loop {
            let mut found = false;
            for module in self.files.values() {
                if removed.contains(&module.path) {
                    continue;
                }
                let imports_changed = module.imports.iter().any(|(import, _)| {
                    let target = Self::resolve_path(&module.path, import);
                    removed.contains(&target)
                });
                if imports_changed {
                    removed.insert(module.path.clone());
                    found = true;
                }
            }
            if !found {
                break;
            }
        }
        for path in removed {
            if let Some(old_id) = self.paths.remove(&path) {
                self.files.remove(&old_id);
            }
        }
    }

    fn path_key(&self, path: &Path) -> Result<PathBuf, ModuleError> {
        let normalized = source_map::normalize_path(path);
        if self.sources.contains(&normalized) {
            return Ok(normalized);
        }
        match self.source_access {
            SourceAccess::SourcesOnly => Err(ModuleError::FileNotFound(normalized)),
            SourceAccess::DiskAndSources => self.sources.path_key(path).map_err(|err| match err {
                source_map::SourceMapError::FileNotFound(path) => ModuleError::FileNotFound(path),
            }),
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

    fn resolve_path(base_file: &Path, import_path: &str) -> PathBuf {
        source_map::resolve_import(base_file, import_path)
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
            source.to_owned()
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
            let import_path = Self::resolve_path(&key, &import.0);

            let result = self.load_file(&import_path, visiting)?;
            let import_id = self
                .file_id_for_path(&import_path)
                .ok_or_else(|| ModuleError::FileNotFound(import_path.clone()))?;
            all_import_ids.push(import_id);
            all_import_ids.extend(result.import_ids);
        }

        let module = ParsedModule {
            path: key.clone(),
            statements: statements.clone(),
            imports: imports.clone(),
            symbols: {
                let mut symbols = crate::symbol_table::SymbolTable::build_from_ast(&statements);
                symbols.collect_references(&statements);
                symbols
            },
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
                    let path = Self::resolve_path(&module.path, &imp.0);
                    self.file_id_for_path(&path)
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Load an entry file and return all flattened top-level statements.
    pub fn load_entry(&mut self, path: &Path) -> Result<Vec<Stmt>, ModuleError> {
        self.load_entry_inner(path)
    }

    fn load_entry_inner(&mut self, path: &Path) -> Result<Vec<Stmt>, ModuleError> {
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
    }

    /// Run a loading operation with a temporary in-memory source override.
    ///
    /// The previous source for `path` is restored afterwards, and a source that
    /// did not exist before is removed, on both success and error paths.
    pub fn with_source<T>(
        &mut self,
        path: &Path,
        source: &str,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.sources.get(path).map(str::to_owned);
        self.set_source_for_path(path.to_path_buf(), source.to_string());
        let result = f(self);
        match previous {
            Some(previous) => self.set_source_for_path(path.to_path_buf(), previous),
            None => self.clear_source_for_path(path),
        }
        result
    }

    /// Load an entry file with optional source override and return all flattened top-level
    /// statements.
    pub fn load_entry_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<Vec<Stmt>, ModuleError> {
        match source {
            Some(source) => self.with_source(path, source, |graph| graph.load_entry_inner(path)),
            None => self.load_entry(path),
        }
    }

    /// Load an entry file and return a fully resolved `LoadedProgram`.
    pub fn load_program(&mut self, path: &Path) -> Result<LoadedProgram, ModuleError> {
        self.load_program_inner(path)
    }

    fn load_program_inner(&mut self, path: &Path) -> Result<LoadedProgram, ModuleError> {
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
        let module_fns = Self::collect_module_fns(&statements);

        let mut namespaces = HashMap::new();
        // Collect namespaces from the entry file's direct aliased imports,
        // resolving re-exports transitively.
        if let Some(entry_module) = self.files.get(&entry_id) {
            for imp in &entry_module.imports {
                if let Some(alias) = &imp.1 {
                    let import_path = Self::resolve_path(&entry_module.path, &imp.0);
                    if let Some(import_id) = self.file_id_for_path(&import_path) {
                        let resolved_exports = self.collect_resolved_exports(import_id);
                        let resolved_type_exports = self.collect_resolved_type_exports(import_id);
                        let resolved_component_exports =
                            self.collect_resolved_component_exports(import_id);
                        let resolved_scenes = self.collect_resolved_scenes(import_id);
                        let resolved_namespaces = self.collect_resolved_namespaces(import_id);
                        namespaces.insert(
                            alias.clone(),
                            Namespace {
                                exports: resolved_exports,
                                type_exports: resolved_type_exports,
                                component_exports: resolved_component_exports,
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
            module_fns,
            namespaces,
        })
    }

    /// Load an entry file with optional source override and return a fully resolved
    /// `LoadedProgram`.
    pub fn load_program_with_source(
        &mut self,
        path: &Path,
        source: Option<&str>,
    ) -> Result<LoadedProgram, ModuleError> {
        match source {
            Some(source) => self.with_source(path, source, |graph| graph.load_program_inner(path)),
            None => self.load_program(path),
        }
    }

    /// Collect module-scoped actions from a list of statements.
    /// Walks into keyframe / relative-keyframe / sequence / stagger / always / drive / conditional
    /// / for bodies.
    #[doc(hidden)]
    pub fn collect_module_fns(stmts: &[Stmt]) -> HashMap<String, FnTemplate> {
        let mut actions = HashMap::new();
        for stmt in stmts {
            Self::collect_module_fns_from_stmt(stmt, &mut actions);
        }
        actions
    }

    fn collect_module_fns_from_stmt(stmt: &Stmt, actions: &mut HashMap<String, FnTemplate>) {
        match stmt {
            Stmt::FnDecl {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                actions.insert(
                    name.clone(),
                    FnTemplate {
                        params: params.clone(),
                        return_type: return_type.clone(),
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
                    Self::collect_module_fns_from_stmt(stmt, actions);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    Self::collect_module_fns_from_stmt(stmt, actions);
                }
                if let Some(else_body) = else_branch {
                    for stmt in else_body {
                        Self::collect_module_fns_from_stmt(stmt, actions);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for stmt in body {
                        Self::collect_module_fns_from_stmt(stmt, actions);
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

    /// Collect exported component definitions from a module.
    fn collect_resolved_component_exports(&self, file_id: FileId) -> HashMap<String, ComponentDef> {
        let Some(module) = self.files.get(&file_id) else {
            return HashMap::new();
        };
        collect_pub_components(&module.statements)
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
            let import_path = Self::resolve_path(&module.path, &imp.0);
            let Some(sub_id) = self.file_id_for_path(&import_path) else {
                continue;
            };
            namespaces.insert(
                alias.clone(),
                Namespace {
                    exports: self.collect_resolved_exports(sub_id),
                    type_exports: self.collect_resolved_type_exports(sub_id),
                    component_exports: self.collect_resolved_component_exports(sub_id),
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
                let import_path = Self::resolve_path(&module.path, &imp.0);
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
                let import_path = Self::resolve_path(&module.path, &imp.0);
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
                let import_path = Self::resolve_path(&module.path, &imp.0);
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
                let import_path = Self::resolve_path(&module.path, &imp.0);
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
