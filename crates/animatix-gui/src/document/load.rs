//! Program loading: file reads, module-graph expansion, and plugin manifests.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use animatix::timeline::{SceneDimensions, TimelineIndex};
use animatix_analyzer::ExtensionManifest;
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use animatix_syntax::module::{ComponentEntry, FnTemplate, ModuleError, Namespace};
use animatix_syntax::source_index::SourceIndex;

use super::DocumentSession;
use crate::error::GuiError;

/// Result of loading and parsing a program.
pub struct LoadedProgramResult {
    pub raw_statements: Vec<Stmt>,
    pub expanded_statements: Vec<Stmt>,
    pub namespaces: HashMap<String, Namespace>,
    pub type_diagnostics: Vec<Diagnostic>,
    pub components: HashMap<String, ComponentEntry>,
    pub module_fns: HashMap<String, FnTemplate>,
}

impl DocumentSession {
    pub fn load(file_path: PathBuf) -> Result<Self, GuiError> {
        let source_text = fs::read_to_string(&file_path).map_err(|err| GuiError::Io {
            path: file_path.clone(),
            source: err,
        })?;

        let (extension_context, extension_manifest) = document_extensions(&file_path);
        let mut document =
            Self::from_parts(file_path, source_text, extension_context, extension_manifest);

        if let Err(e) = document.rebuild() {
            tracing::warn!("Initial document rebuild failed: {}", e);
        }
        Ok(document)
    }

    /// Load a document using a caller-provided plugin context/manifest.
    pub fn load_with_extension(
        file_path: PathBuf,
        extension_context: std::sync::Arc<animatix::extension_context::ExtensionContext>,
        extension_manifest: ExtensionManifest,
        workspace_root: Option<PathBuf>,
    ) -> Result<Self, GuiError> {
        let source_text = fs::read_to_string(&file_path).map_err(|err| GuiError::Io {
            path: file_path.clone(),
            source: err,
        })?;
        let mut document =
            Self::from_parts(file_path, source_text, extension_context, extension_manifest);
        document.workspace_root = workspace_root;
        if let Err(e) = document.rebuild() {
            tracing::warn!("Initial document rebuild failed: {}", e);
        }
        Ok(document)
    }

    fn from_parts(
        file_path: PathBuf,
        source_text: String,
        extension_context: std::sync::Arc<animatix::extension_context::ExtensionContext>,
        extension_manifest: ExtensionManifest,
    ) -> Self {
        Self {
            file_path,
            source_text,
            raw_statements: None,
            expanded_statements: None,
            namespaces: HashMap::new(),
            source_index: None,
            timeline: None,
            composition: None,
            active_scene: None,
            diagnostics: Vec::new(),
            last_rebuild_error: None,
            is_dirty: false,
            duration_s: 5.0,
            scene_dimensions: SceneDimensions::default(),
            extension_context,
            extension_manifest,
            workspace_root: None,
            plugin_epoch: 0,
            timeline_index: TimelineIndex::default(),
            keyframe_lines: Vec::new(),
            components: HashMap::new(),
            module_fns: HashMap::new(),
            last_source_hash: 0,
            last_component_hash: 0,
            cached_expanded: None,
            cached_module_graph: None,
        }
    }

    /// Create a session from in-memory source text without loading from disk.
    /// This is used by the background rebuild worker.
    pub fn from_source(file_path: PathBuf, source_text: String) -> Result<Self, GuiError> {
        let (extension_context, extension_manifest) = document_extensions(&file_path);
        Ok(Self::from_parts(file_path, source_text, extension_context, extension_manifest))
    }

    /// Create a session from in-memory source text with a shared plugin
    /// context/manifest, so background rebuilds do not load plugins again.
    pub fn from_source_with_extension(
        file_path: PathBuf,
        source_text: String,
        extension_context: std::sync::Arc<animatix::extension_context::ExtensionContext>,
        extension_manifest: ExtensionManifest,
        workspace_root: Option<PathBuf>,
    ) -> Result<Self, GuiError> {
        let mut session =
            Self::from_parts(file_path, source_text, extension_context, extension_manifest);
        session.workspace_root = workspace_root;
        Ok(session)
    }

    pub fn from_error(file_path: PathBuf) -> Self {
        let (extension_context, extension_manifest) = document_extensions(&file_path);
        Self::from_parts(file_path, String::new(), extension_context, extension_manifest)
    }

    /// Replace the extension context/manifest after a plugin reload.
    pub fn set_extension_context(
        &mut self,
        extension_context: std::sync::Arc<animatix::extension_context::ExtensionContext>,
        extension_manifest: ExtensionManifest,
        plugin_epoch: u64,
    ) {
        self.extension_context = extension_context;
        self.extension_manifest = extension_manifest;
        self.plugin_epoch = plugin_epoch;
    }

    /// Replace the workspace root used for relative asset resolution.
    pub fn set_workspace_root(&mut self, workspace_root: Option<PathBuf>) {
        self.workspace_root = workspace_root;
    }

    /// Set the current plugin context generation.
    pub fn set_plugin_epoch(&mut self, plugin_epoch: u64) {
        self.plugin_epoch = plugin_epoch;
    }
}

impl DocumentSession {
    /// Rebuild just the source index from the current source text.
    ///
    /// Used after external source changes (typing, hot-reload) when the AST is
    /// not already available. Inspector edits mutate the AST directly and build
    /// the index from the mutated AST without re-parsing.
    pub fn rebuild_source_index(&mut self) {
        if let Ok(result) = self.load_program() {
            self.source_index = Some(SourceIndex::build(&result.raw_statements));
        }
    }
}

impl DocumentSession {
    /// Load the program, returning a structured result.
    /// Raw statements are the parsed statements before component expansion.
    /// Uses cached ModuleGraph to avoid re-reading unchanged imports.
    /// Skips component expansion when the component registry hasn't changed.
    pub(super) fn load_program(&mut self) -> Result<LoadedProgramResult, ModuleError> {
        let mut graph = self.cached_module_graph.take().unwrap_or_default();
        let mut program =
            graph.load_program_with_source(&self.file_path, Some(&self.source_text))?;
        let mut type_diagnostics = program.typecheck();

        // Hash the component registry to detect changes.
        let component_hash = {
            let mut hasher = DefaultHasher::new();
            let mut names: Vec<&String> = program.components.keys().collect();
            names.sort();
            for name in names {
                name.hash(&mut hasher);
                let entry = &program.components[name];
                format!("{:?}", entry.definition).hash(&mut hasher);
            }
            hasher.finish()
        };

        // Skip expansion if components haven't changed and we have cached results.
        let expanded_statements = if component_hash == self.last_component_hash {
            if let Some(expanded) = self.cached_expanded.clone() {
                expanded
            } else {
                let mut expansion_errors = Vec::new();
                let expanded = program.expand_components(&mut expansion_errors);
                self.cached_expanded = Some(expanded.clone());
                self.last_component_hash = component_hash;
                Self::push_expansion_diagnostics(&mut type_diagnostics, expansion_errors);
                expanded
            }
        } else {
            let mut expansion_errors = Vec::new();
            let expanded = program.expand_components(&mut expansion_errors);
            self.cached_expanded = Some(expanded.clone());
            self.last_component_hash = component_hash;
            Self::push_expansion_diagnostics(&mut type_diagnostics, expansion_errors);
            expanded
        };

        // Cache the graph for next rebuild (preserves parsed imports)
        self.cached_module_graph = Some(graph);
        Ok(LoadedProgramResult {
            raw_statements: program.statements,
            expanded_statements,
            namespaces: program.namespaces,
            type_diagnostics,
            components: program.components,
            module_fns: program.module_fns,
        })
    }

    /// Convert timeline-function expansion errors (recursion cycles, statement-level
    /// pure-function calls) into build diagnostics attached to the document.
    fn push_expansion_diagnostics(
        diagnostics: &mut Vec<animatix_syntax::diagnostics::Diagnostic>,
        errors: Vec<String>,
    ) {
        for error in errors {
            diagnostics.push(animatix_syntax::diagnostics::Diagnostic::error(
                animatix_syntax::diagnostics::DiagnosticCode::UnknownAction,
                animatix_syntax::diagnostics::DiagnosticPhase::Build,
                error,
            ));
        }
    }
}

/// Convert a `ModuleError` into a vector of `Diagnostic`s.
/// Parse errors are preserved as individual diagnostics with location info
/// via `ParseError::to_diagnostic()`.
/// Other module errors become a single `SourceLoadFailure` diagnostic.
pub(super) fn diagnostics_from_module_error(
    err: &ModuleError,
    file_path: &Path,
) -> Vec<Diagnostic> {
    match err {
        ModuleError::ParseErrors(parse_errors) => {
            parse_errors.iter().map(|e| e.to_diagnostic().with_path(file_path)).collect()
        },
        _ => vec![
            Diagnostic::error(
                DiagnosticCode::SourceLoadFailure,
                DiagnosticPhase::Parse,
                err.to_string(),
            )
            .with_path(file_path),
        ],
    }
}

fn document_extensions(
    file_path: &Path,
) -> (std::sync::Arc<animatix::extension_context::ExtensionContext>, ExtensionManifest) {
    let manifests = load_extension_manifests(file_path);
    let extension_manifest = ExtensionManifest::merge(&manifests);
    let mut ctx = std::sync::Arc::new(animatix::extension_context::ExtensionContext::new());

    #[cfg(feature = "plugin-loading")]
    {
        use animatix::extension_plugin::{NativePlugin, PluginLoader};

        let mut loader = PluginLoader::new();
        for manifest in &manifests {
            let Some(library) = manifest.library.as_deref() else {
                continue;
            };
            let library_path = file_path.parent().unwrap_or_else(|| Path::new(".")).join(library);
            match NativePlugin::load(&library_path) {
                Ok(plugin) => loader.register(Box::new(plugin)),
                Err(err) => {
                    tracing::warn!(
                        "Failed to load plugin library {} for {}: {}",
                        library_path.display(),
                        file_path.display(),
                        err
                    );
                },
            }
        }
        if let Some(ctx_mut) = std::sync::Arc::get_mut(&mut ctx) {
            if let Err(err) = loader.install_all(ctx_mut) {
                tracing::warn!(
                    "Failed to install document plugins for {}: {}",
                    file_path.display(),
                    err
                );
            }
        }
    }

    (ctx, extension_manifest)
}

/// Analyzer-only manifest metadata for a document's directory.
pub(crate) fn extension_manifest_for_path(file_path: &Path) -> ExtensionManifest {
    ExtensionManifest::merge(&load_extension_manifests(file_path))
}

fn load_extension_manifests(file_path: &Path) -> Vec<ExtensionManifest> {
    let Some(dir) = file_path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let file_name = path.file_name().map(|name| name.to_string_lossy().into_owned())?;
            if !file_name.ends_with(".amx-plugin.toml") {
                return None;
            }
            let source = fs::read_to_string(&path).ok()?;
            ExtensionManifest::from_toml(&source).ok()
        })
        .collect()
}
