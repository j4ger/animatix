//! The backend and its server lifecycle.
//!
//! `LanguageServer` is implemented exactly once here: lifecycle events stay in
//! this module and each request method delegates to its handler family.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix_analyzer::{
    Analyzer, ExtensionManifest, Workspace, discover_manifest_sources, fingerprint_sources,
};
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use crate::handlers::{actions, formatting, navigation, rename, symbols};
use crate::paths::{has_amx_extension, is_manifest_file_name, uri_to_path};

pub(crate) const SEMANTIC_TOKEN_TYPES: &[&str] = &[
    "keyword",
    "type",
    "string",
    "number",
    "comment",
    "operator",
    "variable",
    "property",
    "parameter",
    "function",
    "action",
    "label",
    "boolean",
    "punctuation",
    "wildcard",
    "importalias",
];

/// The LSP server backend.
pub(crate) struct Backend {
    pub(crate) client: Client,
    /// Analyzer instances per document URI.
    /// `Analyzer` is not `Clone` — we hold it behind a mutex and call query
    /// methods while the lock is held (queries are fast).
    pub(crate) analyzers: Mutex<HashMap<String, Analyzer>>,
    /// Cached workspace for cross-file analysis.
    /// Rebuilt when files are opened, changed, or closed.
    pub(crate) cached_workspace: Mutex<Option<Arc<Workspace>>>,
    /// Analyzer-only extension manifests loaded from the document workspace.
    manifests: Mutex<Vec<ExtensionManifest>>,
    /// Per-directory manifest cache, so keystroke-level updates do not rescan
    /// the directory and re-parse every `*.amx-plugin.toml` on disk.
    manifest_cache: Mutex<HashMap<PathBuf, ManifestCacheEntry>>,
}

/// Cached manifest discovery for one directory, keyed by content fingerprint.
struct ManifestCacheEntry {
    /// Combined fingerprint of the manifests found in the directory.
    fingerprint: u64,
    /// The merged manifests that fingerprint represents.
    manifests: Vec<ExtensionManifest>,
}

impl Backend {
    pub(crate) fn new(client: Client) -> Self {
        Self {
            client,
            analyzers: Mutex::new(HashMap::new()),
            cached_workspace: Mutex::new(None),
            manifests: Mutex::new(Vec::new()),
            manifest_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Reload `*.amx-plugin.toml` manifests for a document's directory.
    ///
    /// Discovery is fingerprint-cached: a redisplay of the same directory with
    /// unchanged manifest content reuses the parsed manifests. The fingerprint
    /// still requires reading the manifest files (their content is part of it),
    /// so this is called only when manifests could actually have changed —
    /// document open, save, or a watched-file notification — never on an
    /// ordinary keystroke.
    async fn refresh_manifests(&self, path: Option<&Path>) {
        let Some(dir) = path.and_then(Path::parent) else {
            *self.manifests.lock().await = Vec::new();
            return;
        };
        let dir = dir.to_path_buf();

        let sources = discover_manifest_sources(Some(&dir), None, &[]);
        let fingerprint = fingerprint_sources(&sources);

        let mut cache = self.manifest_cache.lock().await;
        if cache.get(&dir).is_some_and(|entry| entry.fingerprint == fingerprint) {
            let cached = cache[&dir].manifests.clone();
            drop(cache);
            *self.manifests.lock().await = cached;
            return;
        }
        let manifests: Vec<ExtensionManifest> =
            sources.into_iter().map(|source| source.manifest).collect();
        cache.insert(
            dir,
            ManifestCacheEntry {
                fingerprint,
                manifests: manifests.clone(),
            },
        );
        drop(cache);
        *self.manifests.lock().await = manifests;
    }

    /// Update the analyzer for a document. Rebuilds workspace if needed.
    ///
    /// Manifest refresh is *not* part of this path: it reads the filesystem,
    /// and this runs on every keystroke. Call [`Backend::reload_manifests`]
    /// first from the events where manifests could have changed.
    async fn update_analyzer(&self, uri: String, text: String) {
        let path = uri_to_path(&uri);
        let manifest = ExtensionManifest::merge(&self.manifests.lock().await);
        let is_new;
        {
            let mut analyzers = self.analyzers.lock().await;
            is_new = !analyzers.contains_key(&uri);
            let analyzer = analyzers.entry(uri.clone()).or_insert_with(|| {
                Analyzer::new_with_path(&text, path.clone())
                    .with_extension_manifest(manifest.clone())
            });
            analyzer.update(&text);
            analyzer.set_extension_manifest(manifest);
            // Populate the import-symbol cache once per opened document so
            // diagnostics see imported `pub component` types. Later edits
            // reuse the cache (see Analyzer::merge_import_symbols).
            if is_new {
                analyzer.merge_import_symbols();
            }
        }

        if is_new {
            self.rebuild_workspace().await;
        } else {
            self.update_workspace_file(&uri, &text).await;
        }
    }

    /// Refresh manifests from disk, then apply them to every open document.
    ///
    /// Call this from the events where a manifest can plausibly have changed —
    /// document open, save, or an external file change — not per keystroke.
    async fn reload_manifests(&self, path: Option<&Path>) {
        let before = self.manifests.lock().await.clone();
        self.refresh_manifests(path).await;
        let after = self.manifests.lock().await.clone();
        if before == after {
            return;
        }
        let merged = ExtensionManifest::merge(&after);
        let mut analyzers = self.analyzers.lock().await;
        for analyzer in analyzers.values_mut() {
            analyzer.set_extension_manifest(merged.clone());
        }
    }

    /// Remove an analyzer for a closed document.
    async fn remove_analyzer(&self, uri: &str) {
        {
            let mut analyzers = self.analyzers.lock().await;
            analyzers.remove(uri);
        }
        self.rebuild_workspace().await;
    }

    /// Build a workspace from all open documents.
    /// Full rebuild — use when files are opened or closed.
    async fn rebuild_workspace(&self) {
        let workspace = {
            let analyzers = self.analyzers.lock().await;
            if analyzers.len() <= 1 {
                None
            } else {
                let mut workspace = Workspace::new();
                for (_uri, analyzer) in analyzers.iter() {
                    if let Some(path) = analyzer.path() {
                        workspace.add_file(path.to_path_buf(), analyzer.source());
                    }
                }
                Some(Arc::new(workspace))
            }
        };
        let mut cached = self.cached_workspace.lock().await;
        *cached = workspace;
    }

    /// Incrementally update a single file in the cached workspace.
    /// Much faster than full rebuild for keystroke-level changes.
    async fn update_workspace_file(&self, uri: &str, source: &str) {
        let cached = self.cached_workspace.lock().await;
        if let Some(workspace) = cached.as_ref() {
            let mut workspace = Workspace::clone(workspace);
            drop(cached);

            if let Some(path) = uri_to_path(uri) {
                workspace.add_file(path, source);
                let mut cached = self.cached_workspace.lock().await;
                *cached = Some(Arc::new(workspace));
            }
        }
    }

    /// Republish diagnostics for every open document: after a save or an
    /// external change to an imported file, both the changed file and the
    /// files importing it may have different diagnostics.
    async fn republish_all_diagnostics(&self) {
        let uris: Vec<String> = self.analyzers.lock().await.keys().cloned().collect();
        for open_uri in uris {
            self.publish_diagnostics(&open_uri).await;
        }
    }

    /// Publish diagnostics for a document to the LSP client.
    async fn publish_diagnostics(&self, uri: &str) {
        let diagnostics = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(uri) else {
                return;
            };
            analyzer.diagnostics()
        };

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| {
                let severity = match d.severity {
                    animatix_analyzer::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    animatix_analyzer::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                    animatix_analyzer::DiagnosticSeverity::Info => DiagnosticSeverity::INFORMATION,
                    animatix_analyzer::DiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
                };

                Diagnostic {
                    range: Range {
                        start: Position::new(d.line as u32, d.col as u32),
                        end: Position::new(d.end_line as u32, d.end_col as u32),
                    },
                    severity: Some(severity),
                    code: d.code.map(NumberOrString::String),
                    source: Some("animatix".to_string()),
                    message: d.message,
                    related_information: None,
                    tags: None,
                    code_description: None,
                    data: None,
                }
            })
            .collect();

        if let Ok(url) = Url::parse(uri) {
            self.client.publish_diagnostics(url, lsp_diagnostics, None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        // Receive did_save: the natural point to re-resolve
                        // imports from disk (imported files may have changed
                        // since this document was opened).
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        ":".to_string(),
                        ".".to_string(),
                        " ".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        // Only spelling fixes for now; the kind hint lets
                        // clients group them under "Quick Fix".
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::SOURCE,
                        ]),
                        resolve_provider: Some(false),
                        ..Default::default()
                    },
                )),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                // Watch `.amx` sources and plugin manifests so a change to an
                // imported module or an extension manifest on disk (not open
                // in the editor) refreshes the affected documents.
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: None,
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: SEMANTIC_TOKEN_TYPES
                                    .iter()
                                    .map(|s| SemanticTokenType::from(*s))
                                    .collect(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Animatix LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text;
        // Opening a document is a natural point to pick up manifests on disk.
        self.reload_manifests(uri_to_path(&uri).as_deref()).await;
        self.update_analyzer(uri.clone(), text).await;
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().next() {
            // Keystroke path: no filesystem work beyond re-parsing the buffer.
            self.update_analyzer(uri.clone(), change.text).await;
            self.publish_diagnostics(&uri).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let saved_uri = params.text_document.uri;
        // A save is the natural point to re-resolve imports from disk:
        // imported files may have changed since each document was opened.
        // merge_import_symbols refreshes the cached import table and rebuilds.
        // Manifests are refreshed too — a save often follows adding or editing
        // a plugin manifest in the same directory.
        self.reload_manifests(uri_to_path(saved_uri.as_ref()).as_deref()).await;
        {
            let mut analyzers = self.analyzers.lock().await;
            for analyzer in analyzers.values_mut() {
                analyzer.merge_import_symbols();
            }
        }
        self.republish_all_diagnostics().await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        // An imported file changed outside the editor: re-resolve every open
        // document's import symbols from disk, then republish, so the
        // diagnostics of the importing documents track the new content.
        let changed: Vec<PathBuf> = params
            .changes
            .iter()
            .filter_map(|change| uri_to_path(change.uri.as_ref()))
            .collect();
        if changed.is_empty() {
            return;
        }
        let manifest_changed = changed
            .iter()
            .any(|path| is_manifest_file_name(path) || !has_amx_extension(path));

        // A watched manifest (or a directory-level change) must refresh the
        // cached manifests before the analyzers are rebuilt.
        if manifest_changed {
            for path in &changed {
                self.reload_manifests(Some(path.as_path())).await;
            }
        }
        // Import resolution must be re-run whenever a source file changed; it
        // is a no-op for a manifest-only change, but the diagnostics still
        // need republishing because the extension symbols just changed.
        if changed.iter().any(|path| has_amx_extension(path)) {
            let mut analyzers = self.analyzers.lock().await;
            for analyzer in analyzers.values_mut() {
                analyzer.merge_import_symbols();
            }
        }
        self.republish_all_diagnostics().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.remove_analyzer(&uri).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        symbols::semantic_tokens_full(self, params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        navigation::completion(self, params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        navigation::hover(self, params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        navigation::goto_definition(self, params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        symbols::document_symbol(self, params).await
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        symbols::symbol(self, params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        navigation::references(self, params).await
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        actions::code_action(self, params).await
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        rename::prepare_rename(self, params).await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        rename::rename(self, params).await
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        formatting::formatting(self, params).await
    }
}
