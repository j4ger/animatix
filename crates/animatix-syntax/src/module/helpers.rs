use super::*;

pub(super) fn collect_pub_lets(statements: &[Stmt]) -> HashMap<String, Expr> {
    let mut result = HashMap::new();
    collect_pub_lets_inner(statements, &mut result);
    result
}

pub(super) fn collect_pub_type_aliases(statements: &[Stmt]) -> HashMap<String, TypeAnnotation> {
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

pub(super) fn collect_pub_components(statements: &[Stmt]) -> HashMap<String, ComponentDef> {
    let mut result = HashMap::new();
    for stmt in statements {
        if let Stmt::ComponentDef(def, _) = stmt {
            if def.is_pub {
                result.insert(def.name.clone(), def.clone());
            }
        }
    }
    result
}

pub(super) fn collect_pub_lets_inner(statements: &[Stmt], result: &mut HashMap<String, Expr>) {
    for stmt in statements {
        match stmt {
            Stmt::LetDecl {
                is_pub: true,
                name,
                value,
                ..
            } => {
                result.insert(name.clone(), value.clone());
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
pub(super) fn resolve_exports(
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

pub(super) fn resolve_reexport_expr(
    expr: &Expr,
    import_namespaces: &HashMap<String, &Namespace>,
) -> Expr {
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

pub(super) fn resolve_path_in_namespace(
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

#[derive(Clone, Debug)]
pub(super) struct ParsedModule {
    pub(super) path: PathBuf,
    pub(super) statements: Vec<Stmt>,
    pub(super) imports: Vec<(String, Option<String>)>,
    pub(super) symbols: crate::symbol_table::SymbolTable,
}

pub(super) struct LoadResult {
    pub(super) import_ids: Vec<FileId>,
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
        }
    }
}
