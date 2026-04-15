use crate::ast::{
    Action, ComponentDef, Expr, Import, InlineItem, Modifier, ParamDef, Property, Stmt,
};
use crate::parser::parser;
use chumsky::Parser;
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
}

impl LoadedProgram {
    pub fn expand_components(&self) -> Vec<Stmt> {
        expand_statements(&self.statements, &self.components)
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

        Ok(LoadedProgram {
            statements,
            components,
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

fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> Vec<Stmt> {
    let mut expanded = Vec::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded);
    }
    expanded
}

fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
) {
    match stmt {
        Stmt::Keyframe { time, body } => output.push(Stmt::Keyframe {
            time: time.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::RelativeKeyframe { offset, body } => output.push(Stmt::RelativeKeyframe {
            offset: offset.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::Always { body } => output.push(Stmt::Always {
            body: expand_statements(body, components),
        }),
        Stmt::LabeledAlways { label, body } => output.push(Stmt::LabeledAlways {
            label: label.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
        } => output.push(Stmt::Conditional {
            condition: condition.clone(),
            then_branch: expand_statements(then_branch, components),
            else_branch: else_branch
                .as_ref()
                .map(|branch| expand_statements(branch, components)),
        }),
        Stmt::ForLoop {
            var,
            iterable,
            body,
        } => output.push(Stmt::ForLoop {
            var: var.clone(),
            iterable: iterable.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::ComponentAction { name, params, body } => output.push(Stmt::ComponentAction {
            name: name.clone(),
            params: params.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::ComponentDef(_) => {}
        Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers: _,
            children: _,
            ..
        } => {
            if let Some(component) = components.get(ty) {
                output.extend(expand_component_instance(
                    label, props, component, components,
                ));
            } else {
                output.push(stmt.clone());
            }
        }
        _ => output.push(stmt.clone()),
    }
}

fn expand_component_instance(
    instance_label: &str,
    instance_props: &[Property],
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
) -> Vec<Stmt> {
    let bindings = component_bindings(&component.definition.params, instance_props);
    let root_label = first_labeled_stmt(&component.definition.body);
    let known_labels = collect_labels(&component.definition.body);

    let rewritten = component
        .definition
        .body
        .iter()
        .map(|stmt| {
            rewrite_stmt(
                stmt,
                instance_label,
                root_label.as_deref(),
                &known_labels,
                &bindings,
            )
        })
        .collect::<Vec<_>>();

    expand_statements(&rewritten, components)
}

fn component_bindings(params: &[ParamDef], instance_props: &[Property]) -> HashMap<String, Expr> {
    let mut bindings = HashMap::new();

    for param in params {
        if let Some(default) = &param.default {
            bindings.insert(param.name.clone(), default.clone());
        }
    }

    for prop in instance_props {
        bindings.insert(prop.name.clone(), prop.value.clone());
    }

    bindings
}

fn first_labeled_stmt(body: &[Stmt]) -> Option<String> {
    for stmt in body {
        match stmt {
            Stmt::Text {
                label: Some(label), ..
            }
            | Stmt::Math {
                label: Some(label), ..
            }
            | Stmt::Code {
                label: Some(label), ..
            }
            | Stmt::ActorDecl { label, .. } => return Some(label.clone()),
            Stmt::Svg {
                label: Some(label), ..
            }
            | Stmt::Image {
                label: Some(label), ..
            } => return Some(label.clone()),
            _ => {}
        }
    }
    None
}

fn collect_labels(body: &[Stmt]) -> HashSet<String> {
    let mut labels = HashSet::new();
    for stmt in body {
        collect_stmt_labels(stmt, &mut labels);
    }
    labels
}

fn collect_stmt_labels(stmt: &Stmt, labels: &mut HashSet<String>) {
    match stmt {
        Stmt::Text {
            label: Some(label), ..
        }
        | Stmt::Math {
            label: Some(label), ..
        }
        | Stmt::Code {
            label: Some(label), ..
        }
        | Stmt::ActorDecl { label, .. } => {
            labels.insert(label.clone());
        }
        Stmt::Svg {
            label: Some(label), ..
        }
        | Stmt::Image {
            label: Some(label), ..
        } => {
            labels.insert(label.clone());
        }
        Stmt::LabeledAlways { label, body } => {
            labels.insert(label.clone());
            for stmt in body {
                collect_stmt_labels(stmt, labels);
            }
        }
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body }
        | Stmt::ComponentAction { body, .. }
        | Stmt::ForLoop { body, .. } => {
            for stmt in body {
                collect_stmt_labels(stmt, labels);
            }
        }
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_stmt_labels(stmt, labels);
            }
            if let Some(else_branch) = else_branch {
                for stmt in else_branch {
                    collect_stmt_labels(stmt, labels);
                }
            }
        }
        _ => {}
    }
}

fn rewrite_stmt(
    stmt: &Stmt,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Stmt {
    match stmt {
        Stmt::Text {
            label,
            props,
            modifiers,
        } => Stmt::Text {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
        },
        Stmt::Math {
            label,
            props,
            modifiers,
        } => Stmt::Math {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
        },
        Stmt::Code {
            label,
            props,
            modifiers,
        } => Stmt::Code {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
        },
        Stmt::Svg {
            label,
            url,
            at,
            scale,
        } => Stmt::Svg {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            url: url.clone(),
            at: *at,
            scale: *scale,
        },
        Stmt::Image {
            label,
            url,
            at,
            size,
        } => Stmt::Image {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            url: url.clone(),
            at: *at,
            size: *size,
        },
        Stmt::ActorDecl {
            is_pub,
            label,
            ty,
            props,
            modifiers,
            children,
        } => Stmt::ActorDecl {
            is_pub: *is_pub,
            label: rewrite_label(label, prefix, root_label, known_labels),
            ty: ty.clone(),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            children: rewrite_inline_items(children, prefix, root_label, known_labels, bindings),
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
        } => Stmt::Assignment {
            target: rewrite_label_path(target, prefix, root_label, known_labels),
            property: property.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
        },
        Stmt::Sequence { body } => Stmt::Sequence {
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::Stagger { modifiers, body } => Stmt::Stagger {
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::Action(action) => Stmt::Action(Action {
            verb: action.verb.clone(),
            targets: action
                .targets
                .iter()
                .map(|target| rewrite_label_ref(target, prefix, root_label, known_labels))
                .collect(),
            args: action
                .args
                .iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
            modifiers: rewrite_modifiers(
                &action.modifiers,
                prefix,
                root_label,
                known_labels,
                bindings,
            ),
        }),
        Stmt::LetDecl { name, value } => Stmt::LetDecl {
            name: name.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
        },
        Stmt::Keyframe { time, body } => Stmt::Keyframe {
            time: time.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::RelativeKeyframe { offset, body } => Stmt::RelativeKeyframe {
            offset: offset.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::Always { body } => Stmt::Always {
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::LabeledAlways { label, body } => Stmt::LabeledAlways {
            label: rewrite_label(label, prefix, root_label, known_labels),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
        } => Stmt::Conditional {
            condition: rewrite_expr(condition, prefix, root_label, known_labels, bindings),
            then_branch: then_branch
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            else_branch: else_branch.as_ref().map(|branch| {
                branch
                    .iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            }),
        },
        Stmt::ForLoop {
            var,
            iterable,
            body,
        } => Stmt::ForLoop {
            var: var.clone(),
            iterable: rewrite_expr(iterable, prefix, root_label, known_labels, bindings),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::ComponentDef(definition) => Stmt::ComponentDef(ComponentDef {
            is_pub: definition.is_pub,
            name: definition.name.clone(),
            params: definition.params.clone(),
            body: definition
                .body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        }),
        Stmt::ComponentAction { name, params, body } => Stmt::ComponentAction {
            name: name.clone(),
            params: params.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        },
        Stmt::Config { settings } => Stmt::Config {
            settings: rewrite_properties(settings, prefix, root_label, known_labels, bindings),
        },
        Stmt::Import { path } => Stmt::Import { path: path.clone() },
        Stmt::Use { path, items } => Stmt::Use {
            path: path.clone(),
            items: items.clone(),
        },
        Stmt::Comment(comment) => Stmt::Comment(comment.clone()),
    }
}

fn rewrite_inline_items(
    items: &[InlineItem],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<InlineItem> {
    items
        .iter()
        .map(|item| match item {
            InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Anonymous {
                ty: ty.clone(),
                props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
                modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
                children: rewrite_inline_items(
                    children,
                    prefix,
                    root_label,
                    known_labels,
                    bindings,
                ),
            },
            InlineItem::Labeled {
                label,
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Labeled {
                label: rewrite_label(label, prefix, root_label, known_labels),
                ty: ty.clone(),
                props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
                modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
                children: rewrite_inline_items(
                    children,
                    prefix,
                    root_label,
                    known_labels,
                    bindings,
                ),
            },
        })
        .collect()
}

fn rewrite_properties(
    props: &[Property],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<Property> {
    props
        .iter()
        .map(|prop| Property {
            name: prop.name.clone(),
            value: rewrite_expr(&prop.value, prefix, root_label, known_labels, bindings),
        })
        .collect()
}

fn rewrite_modifiers(
    modifiers: &[Modifier],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<Modifier> {
    modifiers
        .iter()
        .map(|modifier| Modifier {
            name: modifier.name.clone(),
            value: rewrite_expr(&modifier.value, prefix, root_label, known_labels, bindings),
        })
        .collect()
}

fn rewrite_expr(
    expr: &Expr,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Expr {
    match expr {
        Expr::Ident(name) => bindings.get(name).cloned().unwrap_or_else(|| {
            Expr::Ident(rewrite_label_ref(name, prefix, root_label, known_labels))
        }),
        Expr::Path(parts) => {
            if let Some(bound) = parts.first().and_then(|part| bindings.get(part)) {
                if parts.len() == 1 {
                    return bound.clone();
                }

                let remaining = &parts[1..];
                return match bound {
                    Expr::Ident(name) => {
                        let mut path = split_rewritten_label(name);
                        path.extend(remaining.iter().cloned());
                        Expr::Path(path)
                    }
                    Expr::Path(path) => {
                        let mut path = path.clone();
                        path.extend(remaining.iter().cloned());
                        Expr::Path(path)
                    }
                    other => other.clone(),
                };
            }

            if let Some((first, rest)) = parts.split_first() {
                let mut rewritten = split_rewritten_label(&rewrite_label_ref(
                    first,
                    prefix,
                    root_label,
                    known_labels,
                ));
                rewritten.extend(rest.iter().cloned());
                Expr::Path(rewritten)
            } else {
                Expr::Path(parts.clone())
            }
        }
        Expr::Index(target, index) => Expr::Index(
            Box::new(rewrite_expr(
                target,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                index,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Tuple(items) => Expr::Tuple(
            items
                .iter()
                .map(|item| rewrite_expr(item, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Binary(lhs, op, rhs) => Expr::Binary(
            Box::new(rewrite_expr(
                lhs,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            op.clone(),
            Box::new(rewrite_expr(
                rhs,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Unary(op, value) => Expr::Unary(
            op.clone(),
            Box::new(rewrite_expr(
                value,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Method(target, name, args) => Expr::Method(
            Box::new(rewrite_expr(
                target,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Closure(params, body) => Expr::Closure(
            params.clone(),
            Box::new(rewrite_expr(
                body,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Conditional(condition, then_expr, else_expr) => Expr::Conditional(
            Box::new(rewrite_expr(
                condition,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                then_expr,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                else_expr,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Construct(name, props) => Expr::Construct(
            name.clone(),
            rewrite_properties(props, prefix, root_label, known_labels, bindings),
        ),
        Expr::Num(value) => Expr::Num(*value),
        Expr::Percent(value) => Expr::Percent(*value),
        Expr::Str(value) => Expr::Str(value.clone()),
        Expr::Bool(value) => Expr::Bool(*value),
        Expr::Null => Expr::Null,
    }
}

fn rewrite_label(
    label: &str,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> String {
    if root_label == Some(label) {
        prefix.to_string()
    } else if known_labels.contains(label) {
        format!("{}.{}", prefix, label)
    } else {
        label.to_string()
    }
}

fn rewrite_label_ref(
    label: &str,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> String {
    if label == "scene" {
        label.to_string()
    } else {
        rewrite_label(label, prefix, root_label, known_labels)
    }
}

fn rewrite_label_path(
    parts: &[String],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> Vec<String> {
    let Some((first, rest)) = parts.split_first() else {
        return Vec::new();
    };

    let mut rewritten =
        split_rewritten_label(&rewrite_label_ref(first, prefix, root_label, known_labels));
    rewritten.extend(rest.iter().cloned());
    rewritten
}

fn split_rewritten_label(label: &str) -> Vec<String> {
    label.split('.').map(str::to_string).collect()
}

fn collect_imports(statements: &[Stmt]) -> Vec<Import> {
    let mut imports = Vec::new();
    for stmt in statements {
        collect_imports_from_stmt(stmt, &mut imports);
    }
    imports
}

fn collect_imports_from_stmt(stmt: &Stmt, imports: &mut Vec<Import>) {
    match stmt {
        Stmt::Import { path } => imports.push(Import { path: path.clone() }),
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body } => {
            for stmt in body {
                collect_imports_from_stmt(stmt, imports);
            }
        }
        _ => {}
    }
}

fn strip_imports(stmt: &Stmt) -> Option<Stmt> {
    match stmt {
        Stmt::Import { .. } => None,
        Stmt::Keyframe { time, body } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Keyframe {
                    time: time.clone(),
                    body,
                })
            }
        }
        Stmt::RelativeKeyframe { offset, body } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::RelativeKeyframe {
                    offset: offset.clone(),
                    body,
                })
            }
        }
        Stmt::Sequence { body } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Sequence { body })
            }
        }
        Stmt::Stagger { modifiers, body } => {
            let body = body.iter().filter_map(strip_imports).collect::<Vec<_>>();
            if body.is_empty() {
                None
            } else {
                Some(Stmt::Stagger {
                    modifiers: modifiers.clone(),
                    body,
                })
            }
        }
        _ => Some(stmt.clone()),
    }
}

fn collect_component_defs(statements: &[Stmt]) -> Vec<ComponentDef> {
    let mut definitions = Vec::new();
    for stmt in statements {
        collect_component_defs_from_stmt(stmt, &mut definitions);
    }
    definitions
}

fn collect_component_defs_from_stmt(stmt: &Stmt, definitions: &mut Vec<ComponentDef>) {
    match stmt {
        Stmt::ComponentDef(definition) => definitions.push(definition.clone()),
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body }
        | Stmt::Stagger { body, .. } => {
            for stmt in body {
                collect_component_defs_from_stmt(stmt, definitions);
            }
        }
        _ => {}
    }
}
