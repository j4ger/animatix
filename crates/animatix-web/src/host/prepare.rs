//! Parse and close imports without building a timeline or acquiring a GPU.

use std::{cell::RefCell, collections::HashMap, path::PathBuf, sync::Arc};

use animatix_syntax::ast::Stmt;
use animatix_syntax::module::{ModuleError, ModuleGraph, Namespace, SourceAccess};
use animatix_syntax::visit::AstFolder;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{ENTRY_PATH, bundled_library};
use crate::dto::DiagnosticDto;

/// An asset's canonical fetch URL is also its key in the prepared asset cache.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AssetRequirement {
    pub id: String,
    pub url: String,
    pub kind: String,
}

/// Conservative text inputs used by the font catalog resolver.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct FontRequirements {
    pub text: String,
    pub families: Vec<String>,
    pub weights: Vec<f32>,
    pub styles: Vec<String>,
    pub dynamic: bool,
    /// Rich markup may request emphasis faces or Fira Math internally even
    /// when the corresponding style/font is not explicit in source text.
    pub rich_text: bool,
    /// Prepared transcript resources whose cues can be displayed by Captions.
    pub transcript_ids: Vec<String>,
}

/// Preparation diagnostics and dependencies; no GPU or font bytes are needed.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct AnalysisDto {
    pub ok: bool,
    pub diagnostics: Vec<DiagnosticDto>,
    pub missing_imports: Vec<String>,
    pub required_features: u32,
    pub fonts: FontRequirements,
    pub assets: Vec<AssetRequirement>,
}

#[derive(Default)]
pub(super) struct PreparedSyntax {
    pub expanded: Vec<Stmt>,
    pub namespaces: HashMap<String, Namespace>,
    diagnostics: Vec<DiagnosticDto>,
}

thread_local! {
    // Syntax identity excludes vars/quality/fonts. A bounded source cache lets
    // slider updates reuse parsed/expanded ASTs while rechecking dependencies.
    static SYNTAX: RefCell<HashMap<[u8;32], Arc<PreparedSyntax>>> = RefCell::new(HashMap::new());
}

/// Immutable source snapshot reused by every quality/font-aware build.
#[derive(Clone)]
pub struct PreparedProgram {
    pub(super) syntax: Arc<PreparedSyntax>,
    pub(super) vars: Vec<(String, String)>,
    pub analysis: AnalysisDto,
}

/// Prepare source, registered modules and overrides as one immutable operation.
pub fn prepare_document(
    source: &str,
    modules: &[(PathBuf, String)],
    vars: &[(String, String)],
    source_url: Option<&str>,
) -> PreparedProgram {
    let key = syntax_key(source, modules, source_url);
    let cached = SYNTAX.with(|cache| cache.borrow().get(&key).cloned());
    let syntax = match cached {
        Some(syntax) => syntax,
        None => match prepare_syntax(source, modules, source_url) {
            Ok(syntax) => {
                let syntax = Arc::new(syntax);
                SYNTAX.with(|cache| {
                    let mut cache = cache.borrow_mut();
                    if cache.len() >= 32 {
                        cache.retain(|_, syntax| Arc::strong_count(syntax) > 1);
                    }
                    if cache.len() < 32 {
                        cache.insert(key, syntax.clone());
                    }
                });
                syntax
            },
            Err(analysis) => {
                return PreparedProgram {
                    syntax: Arc::new(PreparedSyntax::default()),
                    vars: vars.to_vec(),
                    analysis: *analysis,
                };
            },
        },
    };
    let mut analysis = AnalysisDto {
        diagnostics: syntax.diagnostics.clone(),
        ..Default::default()
    };
    super::requirements::collect(&syntax.expanded, &syntax.namespaces, vars, &mut analysis);
    analysis.ok = !analysis.diagnostics.iter().any(|diagnostic| diagnostic.severity == "error");
    PreparedProgram {
        syntax,
        vars: vars.to_vec(),
        analysis,
    }
}

fn syntax_key(source: &str, modules: &[(PathBuf, String)], source_url: Option<&str>) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut feed = |text: &str| {
        hash.update((text.len() as u64).to_le_bytes());
        hash.update(text);
    };
    feed(source_url.unwrap_or(""));
    feed(source);
    let mut modules: Vec<_> = modules.iter().collect();
    modules.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, source) in modules {
        feed(&path.to_string_lossy());
        feed(source);
    }
    hash.finalize().into()
}

fn prepare_syntax(
    source: &str,
    modules: &[(PathBuf, String)],
    source_url: Option<&str>,
) -> Result<PreparedSyntax, Box<AnalysisDto>> {
    let path = PathBuf::from(ENTRY_PATH);
    let mut graph = graph(source, modules);
    let mut analysis = AnalysisDto::default();
    let mut program =
        graph.load_program(&path).map_err(|error| module_error(error, &mut analysis))?;
    analysis
        .diagnostics
        .extend(program.typecheck().iter().map(DiagnosticDto::from_diagnostic));
    if let Some(url) = source_url {
        graph.map_loaded_statements(
            |path, statements| match super::resources::ResourceOrigin::new(url, path) {
                Ok(mut origin) => *statements = origin.fold_stmts(std::mem::take(statements)),
                Err(error) => analysis.diagnostics.push(error_diagnostic(
                    "resource-url",
                    &error.to_string(),
                    None,
                )),
            },
        );
        program = graph.load_program(&path).map_err(|error| module_error(error, &mut analysis))?;
    }
    let mut errors = Vec::new();
    let expanded = program.expand_components(&mut errors);
    for error in errors {
        analysis.diagnostics.push(error_diagnostic("expansion-error", &error, None));
    }
    Ok(PreparedSyntax {
        expanded,
        namespaces: program.namespaces,
        diagnostics: analysis.diagnostics,
    })
}

fn graph(source: &str, modules: &[(PathBuf, String)]) -> ModuleGraph {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.add_source(PathBuf::from(ENTRY_PATH), source);
    for (path, text) in bundled_library() {
        graph.add_source(PathBuf::from(path), *text);
        if let Some(name) = path.strip_prefix("../lib/") {
            graph.add_source(PathBuf::from(format!("../../web/demos/lib/{name}")), *text);
        }
    }
    for (path, text) in modules {
        graph.add_source(path.clone(), text.clone());
    }
    graph
}

fn module_error(error: ModuleError, analysis: &mut AnalysisDto) -> Box<AnalysisDto> {
    if let ModuleError::FileNotFound(path) = error {
        analysis.diagnostics.push(error_diagnostic(
            "module-error",
            &format!("module not found: {}", path.display()),
            None,
        ));
        analysis.missing_imports.push(path.display().to_string());
    } else {
        analysis
            .diagnostics
            .push(error_diagnostic("module-error", &error.to_string(), None));
    }
    Box::new(analysis.clone())
}

pub(super) fn error_diagnostic(code: &str, message: &str, subject: Option<&str>) -> DiagnosticDto {
    DiagnosticDto {
        severity: "error".to_string(),
        code: code.to_string(),
        message: message.to_string(),
        subject: subject.map(str::to_owned),
        line: None,
        column: None,
        span: None,
    }
}
