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

use super::DocumentSession;
use crate::error::GuiError;

/// Result of loading and parsing a program.
pub struct LoadedProgramResult {
    pub raw_statements: Vec<Stmt>,
    pub expanded:
        Result<animatix_syntax::module::ExpandedProgram, animatix_syntax::module::ExpansionError>,
    pub namespaces: HashMap<String, Namespace>,
    pub type_diagnostics: Vec<Diagnostic>,
    pub components: HashMap<String, ComponentEntry>,
    pub module_fns: HashMap<String, FnTemplate>,
    pub function_context: animatix_syntax::module::FunctionContext,
}

impl DocumentSession {
    /// Test-only: the GUI loads through [`RebuildWorker`](super::rebuild::RebuildWorker) instead.
    #[cfg(test)]
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
            waveform_identity: super::audio::fresh_waveform_identity(),
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
            last_expansion_hash: 0,
            cached_expanded: None,
            cached_module_graph: None,
            cached_module_sources: HashMap::new(),
        }
    }

    /// Create a session from in-memory source text without loading from disk.
    /// Test-only: the GUI builds through the rebuild worker instead.
    #[cfg(test)]
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

    /// Set the current plugin context generation.
    pub fn set_plugin_epoch(&mut self, plugin_epoch: u64) {
        self.plugin_epoch = plugin_epoch;
    }
}

impl DocumentSession {
    /// Load the program, returning a structured result.
    /// Raw statements are the parsed statements before component expansion.
    /// Reuses parsed imports and expansion only while their source content is unchanged.
    pub(super) fn load_program(&mut self) -> Result<LoadedProgramResult, ModuleError> {
        let mut graph = self.cached_module_graph.take().unwrap_or_default();
        if self
            .cached_module_sources
            .iter()
            .any(|(path, previous)| fs::read_to_string(path).as_ref().ok() != Some(previous))
        {
            // A cached graph intentionally owns its parsed imports. Reopen it
            // when disk content changes so new function definitions reach both
            // the expansion hash and the compiled lexical function context.
            graph = animatix_syntax::module::ModuleGraph::new();
        }
        let mut program =
            graph.load_program_with_source(&self.file_path, Some(&self.source_text))?;
        let type_diagnostics =
            program.typecheck_with_callables(self.extension_context.callable_descriptors());

        // Expansion caches complete function scopes, including plan-valued
        // module lets. Hash every source AST so an imported helper edit cannot
        // leave an older compiled function context attached to the statements.
        let expansion_hash = {
            let mut hasher = DefaultHasher::new();
            format!("{:?}", program.statements).hash(&mut hasher);
            let mut modules = program.function_context.modules.iter().collect::<Vec<_>>();
            modules.sort_by_key(|(path, _)| *path);
            for (path, module) in modules {
                path.hash(&mut hasher);
                format!("{:?}", module.statements).hash(&mut hasher);
            }
            hasher.finish()
        };

        let expansion = if expansion_hash == self.last_expansion_hash {
            if let Some(expanded) = self.cached_expanded.clone() {
                expanded
            } else {
                let expanded = program.expand_components();
                self.cached_expanded = Some(expanded.clone());
                self.last_expansion_hash = expansion_hash;
                expanded
            }
        } else {
            let expanded = program.expand_components();
            self.cached_expanded = Some(expanded.clone());
            self.last_expansion_hash = expansion_hash;
            expanded
        };

        let expanded = expansion;

        let entry = fs::canonicalize(&self.file_path).unwrap_or_else(|_| self.file_path.clone());
        let imports = program
            .function_context
            .modules
            .keys()
            .filter(|path| **path != entry)
            .map(|path| fs::read_to_string(path).map(|source| (path.clone(), source)))
            .collect::<Result<HashMap<_, _>, _>>();
        match imports {
            Ok(sources) => {
                self.cached_module_sources = sources;
                self.cached_module_graph = Some(graph);
            },
            Err(_) => {
                // A file disappeared during the rebuild; the next rebuild must
                // read disk again and report it rather than reusing this graph.
                self.cached_module_sources.clear();
            },
        }
        Ok(LoadedProgramResult {
            raw_statements: program.statements,
            expanded,
            namespaces: program.namespaces,
            type_diagnostics,
            components: program.components,
            module_fns: program.module_fns,
            function_context: program.function_context,
        })
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
        if let Some(ctx_mut) = std::sync::Arc::get_mut(&mut ctx)
            && let Err(err) = loader.install_all(ctx_mut)
        {
            tracing::warn!(
                "Failed to install document plugins for {}: {}",
                file_path.display(),
                err
            );
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
