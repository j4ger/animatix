//! LSP server for the Animatix DSL.
//!
//! This binary provides language intelligence (completions, diagnostics, hover, go-to-definition)
//! to external editors like VS Code and Neovim via the Language Server Protocol.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use animatix_analyzer::{
    Analyzer, ExtensionManifest, Workspace, discover_manifest_sources, fingerprint_sources,
};
use animatix_syntax::token::LineIndex;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const SEMANTIC_TOKEN_TYPES: &[&str] = &[
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
struct Backend {
    client: Client,
    /// Analyzer instances per document URI.
    /// `Analyzer` is not `Clone` — we hold it behind a mutex and call query
    /// methods while the lock is held (queries are fast).
    analyzers: Mutex<HashMap<String, Analyzer>>,
    /// Cached workspace for cross-file analysis.
    /// Rebuilt when files are opened, changed, or closed.
    cached_workspace: Mutex<Option<Arc<Workspace>>>,
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
    fn new(client: Client) -> Self {
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

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let data = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            let roles = analyzer.token_roles();
            build_semantic_tokens(analyzer.source(), &roles)
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
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

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;

        let items = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(Some(CompletionResponse::Array(vec![])));
            };
            analyzer.completions_at(position.line as usize, position.character as usize)
        };

        let lsp_items: Vec<CompletionItem> = items
            .into_iter()
            .map(|item| {
                let kind = match item.kind {
                    animatix_analyzer::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                    animatix_analyzer::CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
                    animatix_analyzer::CompletionKind::Property => CompletionItemKind::PROPERTY,
                    animatix_analyzer::CompletionKind::Label => CompletionItemKind::VARIABLE,
                    animatix_analyzer::CompletionKind::Action => CompletionItemKind::FUNCTION,
                    animatix_analyzer::CompletionKind::Value => CompletionItemKind::VALUE,
                    animatix_analyzer::CompletionKind::Snippet => CompletionItemKind::SNIPPET,
                };

                CompletionItem {
                    label: item.label,
                    kind: Some(kind),
                    detail: item.detail,
                    documentation: item.documentation.map(Documentation::String),
                    insert_text: item.insert_text,
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        let hover_info = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            analyzer.hover_at(position.line as usize, position.character as usize)
        };

        Ok(hover_info.map(|info| {
            let range = info.range.map(|(sl, sc, el, ec)| {
                Range::new(Position::new(sl as u32, sc as u32), Position::new(el as u32, ec as u32))
            });

            Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info.contents,
                }),
                range,
            }
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        let location = {
            let analyzers = self.analyzers.lock().await;
            let workspace = self.cached_workspace.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            analyzer.definition_at(
                workspace.as_deref(),
                position.line as usize,
                position.character as usize,
            )
        };

        Ok(location.map(|loc| {
            let target_uri =
                loc.file.as_deref().and_then(path_to_uri).unwrap_or_else(|| {
                    params.text_document_position_params.text_document.uri.clone()
                });

            GotoDefinitionResponse::Scalar(Location {
                uri: target_uri,
                range: Range::new(
                    Position::new(loc.line as u32, loc.col as u32),
                    Position::new(loc.line as u32, loc.col as u32),
                ),
            })
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        let symbols = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            analyzer.document_symbols()
        };

        let lsp_symbols: Vec<SymbolInformation> = symbols
            .into_iter()
            .map(|sym| {
                let kind = match sym.kind {
                    animatix_analyzer::SymbolKind::Actor => SymbolKind::VARIABLE,
                    animatix_analyzer::SymbolKind::Variable => SymbolKind::VARIABLE,
                    animatix_analyzer::SymbolKind::Component => SymbolKind::CLASS,
                    animatix_analyzer::SymbolKind::Block => SymbolKind::NAMESPACE,
                };

                #[allow(deprecated)]
                SymbolInformation {
                    name: sym.name,
                    kind,
                    location: Location {
                        uri: params.text_document.uri.clone(),
                        range: Range::new(
                            Position::new(sym.line as u32, sym.col as u32),
                            Position::new(sym.line as u32, sym.col as u32),
                        ),
                    },
                    tags: None,
                    deprecated: None,
                    container_name: None,
                }
            })
            .collect();

        Ok(Some(DocumentSymbolResponse::Flat(lsp_symbols)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_lowercase();
        let analyzers = self.analyzers.lock().await;
        let mut all_symbols = Vec::new();

        for (uri, analyzer) in analyzers.iter() {
            let symbols = analyzer.document_symbols();
            for sym in symbols {
                if !query.is_empty() && !sym.name.to_lowercase().contains(&query) {
                    continue;
                }
                let kind = match sym.kind {
                    animatix_analyzer::SymbolKind::Actor => SymbolKind::VARIABLE,
                    animatix_analyzer::SymbolKind::Variable => SymbolKind::VARIABLE,
                    animatix_analyzer::SymbolKind::Component => SymbolKind::CLASS,
                    animatix_analyzer::SymbolKind::Block => SymbolKind::NAMESPACE,
                };

                #[allow(deprecated)]
                all_symbols.push(SymbolInformation {
                    name: sym.name,
                    kind,
                    location: Location {
                        uri: Url::parse(uri).unwrap_or_else(|_| {
                            path_to_uri(uri)
                                .unwrap_or_else(|| Url::parse("file:///unknown").unwrap())
                        }),
                        range: Range::new(
                            Position::new(sym.line as u32, sym.col as u32),
                            Position::new(sym.line as u32, sym.col as u32),
                        ),
                    },
                    tags: None,
                    deprecated: None,
                    container_name: None,
                });
            }
        }

        if all_symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_symbols))
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;

        let analyzers = self.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };

        let symbol_name = analyzer.symbol_at(position.line as usize, position.character as usize);

        let Some(symbol_name) = symbol_name else {
            return Ok(None);
        };

        // Search for references across all workspace files. The originating
        // file uses cursor-based scope resolution; other files fall back to
        // name lookup because they do not share the selected binding.
        let mut locations = Vec::new();

        for (file_uri, file_analyzer) in analyzers.iter() {
            let refs = if file_uri == &uri {
                file_analyzer
                    .find_references_at(position.line as usize, position.character as usize)
            } else {
                file_analyzer.find_references(&symbol_name)
            };
            for (start_line, start_col, end_line, end_col) in refs {
                if let Ok(uri) = Url::parse(file_uri) {
                    locations.push(Location {
                        uri,
                        range: Range::new(
                            Position::new(start_line as u32, start_col as u32),
                            Position::new(end_line as u32, end_col as u32),
                        ),
                    });
                }
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.to_string();
        let requested = params.range;

        let analyzers = self.analyzers.lock().await;
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };

        let mut actions: Vec<CodeActionOrCommand> = Vec::new();
        let url = match Url::parse(&uri) {
            Ok(url) => url,
            Err(_) => return Ok(None),
        };

        // Diagnostics are computed once per request and reused by every
        // action family below: each pass is a full re-check (~0.4ms on a
        // large file), and the cleanup actions need the same set.
        let diagnostics = analyzer.diagnostics();

        for diagnostic in diagnostics.iter() {
            // Only act on diagnostics the client actually asked about: the
            // request carries the visible range, and offering a fix for an
            // off-screen problem would apply an edit the user never saw.
            if !range_contains(requested, diagnostic) {
                continue;
            }
            let Some(code) = diagnostic.code.as_deref() else {
                continue;
            };

            match code {
                "undefined-label" => {
                    if let Some(misspelled) = misspelled_name(diagnostic) {
                        let candidates = animatix_analyzer::suggest_label_names(
                            analyzer.symbols(),
                            &misspelled,
                            3,
                        );
                        // A single unambiguous correction is the preferred fix;
                        // several candidates leave the choice to the author.
                        let unambiguous = candidates.len() == 1;
                        for candidate in candidates {
                            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                title: format!("Replace with '{}'", candidate.name),
                                kind: Some(CodeActionKind::QUICKFIX),
                                diagnostics: None,
                                edit: Some(replace_range_edit(&url, diagnostic, &candidate.name)),
                                command: None,
                                is_preferred: unambiguous.then_some(true),
                                disabled: None,
                                data: None,
                            }));
                        }
                    }
                    if let Some(name) = undefined_label_name(diagnostic) {
                        if let Some(action) = declare_actor_action(analyzer.source(), &url, &name) {
                            actions.push(CodeActionOrCommand::CodeAction(action));
                        }
                    }
                },
                "unknown-action" => {
                    if let Some(misspelled) = misspelled_name(diagnostic) {
                        let candidates = animatix_analyzer::suggest_action_names(
                            analyzer.symbols(),
                            &misspelled,
                            3,
                        );
                        let unambiguous = candidates.len() == 1;
                        for candidate in candidates {
                            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                title: format!("Replace with '{}'", candidate.name),
                                kind: Some(CodeActionKind::QUICKFIX),
                                diagnostics: None,
                                edit: Some(replace_range_edit(&url, diagnostic, &candidate.name)),
                                command: None,
                                is_preferred: unambiguous.then_some(true),
                                disabled: None,
                                data: None,
                            }));
                        }
                    }
                },
                "unused-label" => {
                    if let Some(name) = unused_name(diagnostic) {
                        if let Some(action) = remove_declaration_action(analyzer, &url, &name) {
                            actions.push(CodeActionOrCommand::CodeAction(action));
                        }
                    }
                },
                // Other codes have no quick fix to offer yet.
                _ => {},
            }
        }

        // A file-wide cleanup is offered when the document has several unused
        // top-level declarations: deleting them one at a time is tedious on
        // content like a dashboard with dozens of retired actors.
        if let Some(action) = remove_all_unused_action(analyzer, &diagnostics, &url) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
        // A cleanup spanning several open documents gets its own action, so
        // the user does not have to run the per-file one in each tab.
        if let Some(action) = remove_all_unused_workspace_action(&analyzers) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri.to_string();
        let position = params.position;

        let range = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            match analyzer.rename_at(position.line as usize, position.character as usize) {
                Ok(target) => target.range,
                // Refusing here is what makes the editor show "cannot rename"
                // instead of letting the user type a name that would be
                // rejected at apply time.
                Err(_) => return Ok(None),
            }
        };

        Ok(Some(PrepareRenameResponse::Range(Range::new(
            Position::new(range.0 as u32, range.1 as u32),
            Position::new(range.2 as u32, range.3 as u32),
        ))))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        if let Err(reason) = animatix_analyzer::validate_new_name(&new_name) {
            // Rejecting without an edit surfaces the reason through the client
            // rather than writing a name the analyzer would then flag.
            self.client
                .show_message(MessageType::ERROR, format!("Rename failed: {reason}"))
                .await;
            return Ok(None);
        }

        let analyzers = self.analyzers.lock().await;

        // Resolve the target in the originating file: its scope-aware
        // references are authoritative.
        let Some(analyzer) = analyzers.get(&uri) else {
            return Ok(None);
        };
        let target = match analyzer.rename_at(position.line as usize, position.character as usize) {
            Ok(target) => target,
            Err(reason) => {
                drop(analyzers);
                self.client
                    .show_message(
                        MessageType::ERROR,
                        format!("Rename failed: {}", reason.message()),
                    )
                    .await;
                return Ok(None);
            },
        };

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();

        // The originating file uses cursor-based scope resolution. Other files
        // fall back to name lookup, with one safety guard: a file that
        // *declares its own* binding of the same name keeps it — those
        // occurrences are its own symbol, not the renamed one, and rewriting
        // them would be a silent wrong edit. Files that merely reference an
        // imported name (no declaration of their own) are renamed.
        let workspace = self.cached_workspace.lock().await;
        let target_path = uri_to_path(&uri);
        for (file_uri, file_analyzer) in analyzers.iter() {
            let ranges = if file_uri == &uri {
                target.references.clone()
            } else {
                let file_path = uri_to_path(file_uri);
                // Only rewrite another file's occurrences when that file
                // reaches the renamed module through its imports (directly or
                // via a re-exporting intermediate): a same-named identifier
                // elsewhere is a different symbol, and rewriting it on name
                // coincidence is a silent wrong edit.
                let imports_target = match (workspace.as_deref(), &target_path, &file_path) {
                    (Some(workspace), Some(target_path), Some(file_path)) => {
                        workspace.imports_transitively(file_path, target_path)
                    },
                    // Without a workspace (single-file session) there is no
                    // cross-file set to be wrong about.
                    _ => false,
                };
                if !imports_target
                    || file_analyzer
                        .occurrences()
                        .iter()
                        .any(|o| o.declaration && o.name == target.name)
                {
                    continue;
                }
                file_analyzer.find_references(&target.name)
            };
            if ranges.is_empty() {
                continue;
            }
            let Ok(url) = Url::parse(file_uri) else {
                continue;
            };
            let edits = ranges
                .into_iter()
                .map(|(start_line, start_col, end_line, end_col)| TextEdit {
                    range: Range::new(
                        Position::new(start_line as u32, start_col as u32),
                        Position::new(end_line as u32, end_col as u32),
                    ),
                    new_text: new_name.clone(),
                })
                .collect();
            changes.insert(url, edits);
        }

        if changes.is_empty() {
            return Ok(None);
        }

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri.to_string();

        let (source, formatted) = {
            let analyzers = self.analyzers.lock().await;
            let Some(analyzer) = analyzers.get(&uri) else {
                return Ok(None);
            };
            let source = analyzer.source();
            let stmts = match analyzer.ast() {
                Some(stmts) => stmts,
                None => return Ok(None),
            };
            let fmt = animatix_syntax::formatter::Formatter::default();
            let formatted = fmt.format(stmts);
            if source == formatted {
                return Ok(None);
            }
            (source.to_string(), formatted)
        };

        // Replace the entire document
        let lines: Vec<&str> = source.lines().collect();
        let last_line = lines.len().saturating_sub(1) as u32;
        let last_char = lines.last().map(|l| l.len()).unwrap_or(0) as u32;

        Ok(Some(vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(last_line, last_char)),
            new_text: formatted,
        }]))
    }
}

/// Build LSP semantic token deltas from analyzer token roles.
fn build_semantic_tokens(
    source: &str,
    roles: &[(usize, usize, &'static str)],
) -> Vec<SemanticToken> {
    let line_index = LineIndex::new(source);
    let mut data = Vec::with_capacity(roles.len());
    let mut prev_line = 0u32;
    let mut prev_col = 0u32;

    for &(start, end, role) in roles {
        let (line, col) = line_index.byte_to_line_col_utf16(start);
        let (_, end_col) = line_index.byte_to_line_col_utf16(end);
        let delta_line = line as u32 - prev_line;
        let delta_start = if delta_line == 0 {
            col as u32 - prev_col
        } else {
            col as u32
        };
        let length = (end_col - col) as u32;
        let token_type = role_index(role);

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        });

        prev_line = line as u32;
        prev_col = col as u32;
    }

    data
}

fn role_index(role: &str) -> u32 {
    SEMANTIC_TOKEN_TYPES
        .iter()
        .position(|name| *name == role)
        .map(|idx| idx as u32)
        .unwrap_or(6) // fall back to variable for unknown roles
}

/// True when `diagnostic`'s range falls inside `range`.
///
/// A client asks for actions over the range the user is looking at; offering
/// an edit outside it would apply a change the user never saw.
fn range_contains(range: Range, diagnostic: &animatix_analyzer::Diagnostic) -> bool {
    let start = Position::new(diagnostic.line as u32, diagnostic.col as u32);
    let end = Position::new(diagnostic.end_line as u32, diagnostic.end_col as u32);
    position_le(range.start, start) && position_le(end, range.end)
}

fn position_le(a: Position, b: Position) -> bool {
    (a.line, a.character) <= (b.line, b.character)
}

/// A single-range replace edit wrapped in a `WorkspaceEdit`.
fn replace_range_edit(
    url: &Url,
    diagnostic: &animatix_analyzer::Diagnostic,
    new_text: &str,
) -> WorkspaceEdit {
    let edit = TextEdit {
        range: Range::new(
            Position::new(diagnostic.line as u32, diagnostic.col as u32),
            Position::new(diagnostic.end_line as u32, diagnostic.end_col as u32),
        ),
        new_text: new_text.to_string(),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Extract the offending name from a spelling-related diagnostic message.
///
/// The analyzer phrases these as `Undefined label: NAME` and `Unknown action:
/// NAME`; anything else yields `None` so unrelated diagnostics are skipped.
fn misspelled_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    const PREFIXES: [&str; 2] = ["Undefined label: ", "Unknown action: "];
    for prefix in PREFIXES {
        if let Some(rest) = diagnostic.message.strip_prefix(prefix) {
            let name = rest.trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract the name from an `undefined-label` diagnostic.
fn undefined_label_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    diagnostic
        .message
        .strip_prefix("Undefined label: ")
        .map(|rest| rest.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Extract the name from an `unused-label` diagnostic (`Unused actor: 'x'`,
/// `Unused binding: 'x'`, …).
fn unused_name(diagnostic: &animatix_analyzer::Diagnostic) -> Option<String> {
    for kind in ["actor", "binding", "label", "component"] {
        let prefix = format!("Unused {kind}: '");
        if let Some(rest) = diagnostic.message.strip_prefix(prefix.as_str()) {
            return rest.strip_suffix('\'').map(str::to_string);
        }
    }
    None
}

/// Build the "declare the missing actor" quick fix.
///
/// Inserts the declaration before the first keyframe (or `always` block, or
/// end of file) so it precedes every use the author is reaching for.
fn declare_actor_action(source: &str, url: &Url, name: &str) -> Option<CodeAction> {
    let line = animatix_analyzer::declaration_insertion_line(source);
    let edit = TextEdit {
        range: Range::new(Position::new(line as u32, 0), Position::new(line as u32, 0)),
        new_text: animatix_analyzer::missing_actor_statement(name),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    Some(CodeAction {
        title: format!("Declare actor '{name}'"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

/// Build the "remove the unused declaration" quick fix.
///
/// The removal range covers the whole statement (brace-matched), and is only
/// offered for actors and `let` bindings — unused components and other kinds
/// may be intentional scaffolding a quick fix should not delete.
fn remove_declaration_action(
    analyzer: &animatix_analyzer::Analyzer,
    url: &Url,
    name: &str,
) -> Option<CodeAction> {
    let decl_byte = animatix_analyzer::first_declaration_byte(analyzer.occurrences(), name)?;
    // Only top-level declarations are safe to remove: an inline child (an
    // effect stage inside a Filter, an actor inside a Row) renders as part of
    // its parent even when its label is never referenced, so deleting it
    // would silently change the output.
    if !animatix_analyzer::is_top_level_position(analyzer.source(), decl_byte) {
        return None;
    }
    let (start_byte, end_byte) =
        animatix_analyzer::statement_removal_range(analyzer.source(), decl_byte)?;
    let (start_line, start_col) =
        animatix_syntax::token::byte_to_line_col(analyzer.source(), start_byte);
    let (end_line, end_col) = animatix_syntax::token::byte_to_line_col(analyzer.source(), end_byte);
    let edit = TextEdit {
        range: Range::new(
            Position::new(start_line as u32, start_col as u32),
            Position::new(end_line as u32, end_col as u32),
        ),
        new_text: String::new(),
    };
    let mut changes = HashMap::new();
    changes.insert(url.clone(), vec![edit]);
    Some(CodeAction {
        title: format!("Remove unused '{name}'"),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    })
}

/// Build the file-wide "remove all unused declarations" action.
///
/// Only offered when more than one top-level declaration is unused, and only
/// for the removals the analyzer confirms are safe (top-level statements with
/// a determinable extent). Edits are emitted in reverse source order so a
/// client applying them sequentially does not shift the later ranges.
/// Removal edits for one document's unused top-level declarations.
///
/// Empty when the document has fewer than two removable declarations.
fn unused_removal_edits(
    analyzer: &animatix_analyzer::Analyzer,
    diagnostics: &[animatix_analyzer::Diagnostic],
) -> Vec<TextEdit> {
    let names: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.code.as_deref() == Some("unused-label"))
        .filter_map(unused_name)
        .collect();
    if names.len() < 2 {
        return Vec::new();
    }
    let mut ranges =
        animatix_analyzer::batch_removal_ranges(analyzer.occurrences(), analyzer.source(), &names);
    if ranges.len() < 2 {
        return Vec::new();
    }
    // Reverse order: each edit's range stays valid as earlier ones are applied.
    ranges.sort_unstable_by(|a, b| b.0.cmp(&a.0));

    let source = analyzer.source();
    ranges
        .into_iter()
        .map(|(start, end)| {
            let (start_line, start_col) = animatix_syntax::token::byte_to_line_col(source, start);
            let (end_line, end_col) = animatix_syntax::token::byte_to_line_col(source, end);
            TextEdit {
                range: Range::new(
                    Position::new(start_line as u32, start_col as u32),
                    Position::new(end_line as u32, end_col as u32),
                ),
                new_text: String::new(),
            }
        })
        .collect()
}

/// Build the "remove all unused declarations" action for one document.
fn remove_all_unused_action(
    analyzer: &animatix_analyzer::Analyzer,
    diagnostics: &[animatix_analyzer::Diagnostic],
    url: &Url,
) -> Option<CodeAction> {
    let edits = unused_removal_edits(analyzer, diagnostics);
    if edits.is_empty() {
        return None;
    }
    let count = edits.len();
    Some(source_action(
        format!("Remove all {count} unused declarations"),
        single_file_edit(url, edits),
    ))
}

/// Build the workspace-wide "remove all unused declarations" action.
///
/// Offered only when the cleanup spans more than one document, so it does not
/// duplicate the per-file action in the common single-file case.
fn remove_all_unused_workspace_action(
    analyzers: &HashMap<String, animatix_analyzer::Analyzer>,
) -> Option<CodeAction> {
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let mut total = 0usize;
    for (uri, analyzer) in analyzers {
        let document_diagnostics = analyzer.diagnostics();
        let edits = unused_removal_edits(analyzer, &document_diagnostics);
        if edits.is_empty() {
            continue;
        }
        let Ok(url) = Url::parse(uri) else {
            continue;
        };
        total += edits.len();
        changes.insert(url, edits);
    }
    if changes.len() < 2 {
        return None;
    }
    Some(source_action(
        format!("Remove all {total} unused declarations in {} files", changes.len()),
        WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        },
    ))
}

/// Wrap an edit in a `source`-kind code action.
fn source_action(title: String, edit: WorkspaceEdit) -> CodeAction {
    CodeAction {
        title,
        // A source action so it is reachable from the file-level menu rather
        // than only from a diagnostic's lightbulb.
        kind: Some(CodeActionKind::SOURCE),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred: None,
        disabled: None,
        data: None,
    }
}

/// A `WorkspaceEdit` touching one document.
fn single_file_edit(url: &Url, edits: Vec<TextEdit>) -> WorkspaceEdit {
    let mut changes = HashMap::new();
    changes.insert(url.clone(), edits);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Convert a file:// URI to a PathBuf.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    url.to_file_path().ok()
}

fn path_to_uri(path: &str) -> Option<Url> {
    Url::parse(&format!("file://{path}")).ok()
}

/// True when a path names an Animatix source file.
fn has_amx_extension(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "amx")
}

/// True when a path names an `*.amx-plugin.toml` extension manifest.
fn is_manifest_file_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".amx-plugin.toml"))
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_to_path_strips_file_prefix() {
        assert_eq!(
            uri_to_path("file:///home/user/project/main.amx"),
            Some(PathBuf::from("/home/user/project/main.amx"))
        );
    }

    #[test]
    fn uri_to_path_returns_none_for_non_file_uri() {
        assert_eq!(uri_to_path("http://example.com/file.amx"), None);
    }

    #[test]
    fn uri_to_path_handles_empty_string() {
        assert_eq!(uri_to_path(""), None);
    }

    #[test]
    fn manifest_and_source_paths_are_classified() {
        assert!(is_manifest_file_name(Path::new("/p/demo.amx-plugin.toml")));
        assert!(!is_manifest_file_name(Path::new("/p/demo.amx")));
        assert!(!is_manifest_file_name(Path::new("/p/plain.toml")));

        assert!(has_amx_extension(Path::new("/p/main.amx")));
        assert!(!has_amx_extension(Path::new("/p/demo.amx-plugin.toml")));
        assert!(!has_amx_extension(Path::new("/p/notes.txt")));
    }

    #[test]
    fn loads_amx_plugin_manifests_from_directory() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-lsp-manifest-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("demo.amx-plugin.toml"), "[[primitives]]\ntype_name = \"Gauge\"\n")
            .expect("write manifest");
        std::fs::write(dir.join("ignored.toml"), "").expect("write ignored file");

        let sources = discover_manifest_sources(Some(&dir), None, &[]);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].manifest.primitives[0].type_name, "Gauge");

        std::fs::remove_dir_all(&dir).ok();
    }
}
