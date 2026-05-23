//! LSP server for the Animatix DSL.
//!
//! This binary provides language intelligence (completions, diagnostics, hover, go-to-definition)
//! to external editors like VS Code and Neovim via the Language Server Protocol.

use animatix_analyzer::{Analyzer, Workspace};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// The LSP server backend.
struct Backend {
    client: Client,
    /// Analyzer instances per document URI.
    analyzers: Arc<Mutex<HashMap<String, Analyzer>>>,
    /// Cached workspace for cross-file analysis.
    /// Rebuilt incrementally when files change.
    cached_workspace: Mutex<Option<Arc<Workspace>>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            analyzers: Arc::new(Mutex::new(HashMap::new())),
            cached_workspace: Mutex::new(None),
        }
    }

    /// Get or create an analyzer for a document.
    async fn get_analyzer(&self, uri: &str) -> Analyzer {
        let analyzers = self.analyzers.lock().await;
        analyzers.get(uri).cloned().unwrap_or_else(|| Analyzer::new(""))
    }

    /// Build a workspace from all open documents and attach it to each analyzer.
    /// Full rebuild — use when files are opened or closed.
    async fn rebuild_workspace(&self) {
        let mut analyzers = self.analyzers.lock().await;
        if analyzers.len() <= 1 {
            // Clear cached workspace when dropping below 2 files
            let mut cached = self.cached_workspace.lock().await;
            *cached = None;
            // Also clear workspace from remaining analyzer
            for (_, analyzer) in analyzers.iter_mut() {
                analyzer.set_workspace(Arc::new(Workspace::new()));
            }
            return;
        }

        // Build workspace from all open documents
        let mut workspace = Workspace::new();
        for (_uri, analyzer) in analyzers.iter() {
            if let Some(path) = analyzer.path() {
                workspace.add_file(path.to_path_buf(), analyzer.source());
            }
        }

        let workspace_arc = Arc::new(workspace);

        // Attach workspace to each analyzer
        for (_, analyzer) in analyzers.iter_mut() {
            analyzer.set_workspace(Arc::clone(&workspace_arc));
        }

        // Cache the workspace
        let mut cached = self.cached_workspace.lock().await;
        *cached = Some(workspace_arc);
    }

    /// Incrementally update a single file in the cached workspace.
    /// Much faster than full rebuild for keystroke-level changes.
    async fn update_workspace_file(&self, uri: &str, source: &str) {
        let cached = self.cached_workspace.lock().await;
        if let Some(workspace) = cached.as_ref() {
            // We have a cached workspace — update it incrementally
            let mut workspace = Workspace::clone(workspace);
            drop(cached);

            if let Some(path) = uri_to_path(uri) {
                workspace.add_file(path, source);
                let workspace_arc = Arc::new(workspace);

                // Update all analyzers with the new workspace
                let mut analyzers = self.analyzers.lock().await;
                for (_, analyzer) in analyzers.iter_mut() {
                    analyzer.set_workspace(Arc::clone(&workspace_arc));
                }

                // Update cache
                let mut cached = self.cached_workspace.lock().await;
                *cached = Some(workspace_arc);
            }
        }
        // If no cached workspace, do nothing — full rebuild will happen on file open
    }

    /// Update the analyzer for a document.
    async fn update_analyzer(&self, uri: String, text: String) {
        let mut analyzers = self.analyzers.lock().await;
        let path = uri_to_path(&uri);
        let is_new = !analyzers.contains_key(&uri);
        let analyzer = analyzers
            .entry(uri.clone())
            .or_insert_with(|| Analyzer::new_with_path(&text, path.clone()));
        analyzer.update(&text);
        drop(analyzers);

        if is_new {
            // Full rebuild when a new file is opened
            self.rebuild_workspace().await;
        } else {
            // Incremental update for keystroke-level changes
            self.update_workspace_file(&uri, &text).await;
        }
    }

    /// Remove an analyzer for a closed document.
    async fn remove_analyzer(&self, uri: &str) {
        let mut analyzers = self.analyzers.lock().await;
        analyzers.remove(uri);
        drop(analyzers);

        // Rebuild workspace after removal
        self.rebuild_workspace().await;
    }



    /// Publish diagnostics for a document to the LSP client.
    async fn publish_diagnostics(&self, uri: &str) {
        let analyzer = self.get_analyzer(uri).await;
        let diagnostics = analyzer.diagnostics();

        let lsp_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| {
                let severity = match d.severity {
                    animatix_analyzer::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
                    animatix_analyzer::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
                    animatix_analyzer::DiagnosticSeverity::Info => {
                        DiagnosticSeverity::INFORMATION
                    }
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
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
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
        self.update_analyzer(uri.clone(), text).await;
        self.publish_diagnostics(&uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_analyzer(uri.clone(), change.text).await;
            self.publish_diagnostics(&uri).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.remove_analyzer(&uri).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;

        let analyzer = self.get_analyzer(&uri).await;
        let items = analyzer.completions_at(position.line as usize, position.character as usize);

        let lsp_items: Vec<CompletionItem> = items.into_iter().map(|item| {
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
        }).collect();

        Ok(Some(CompletionResponse::Array(lsp_items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        let analyzer = self.get_analyzer(&uri).await;
        let hover_info = analyzer.hover_at(position.line as usize, position.character as usize);

        Ok(hover_info.map(|info| {
            let range = info.range.map(|(sl, sc, el, ec)| {
                Range::new(
                    Position::new(sl as u32, sc as u32),
                    Position::new(el as u32, ec as u32),
                )
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

        let analyzer = self.get_analyzer(&uri).await;
        let location = analyzer.definition_at(position.line as usize, position.character as usize);

        Ok(location.map(|loc| {
            let target_uri = loc.file.map(|f| {
                Url::parse(&format!("file://{}", f)).unwrap_or_else(|_| {
                    Url::parse("file:///unknown").unwrap()
                })
            }).unwrap_or_else(|| {
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
        let analyzer = self.get_analyzer(&uri).await;
        let symbols = analyzer.document_symbols();

        let lsp_symbols: Vec<SymbolInformation> = symbols.into_iter().map(|sym| {
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
        }).collect();

        Ok(Some(DocumentSymbolResponse::Flat(lsp_symbols)))
    }

    async fn symbol(
        &self,
        _params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let analyzers = self.analyzers.lock().await;
        let mut all_symbols = Vec::new();

        for (uri, analyzer) in analyzers.iter() {
            let symbols = analyzer.document_symbols();
            for sym in symbols {
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
                            Url::parse("file:///unknown").unwrap()
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

        // Get the symbol name at the cursor position using structured lookup
        let analyzer = self.get_analyzer(&uri).await;
        let symbol_name = analyzer
            .symbol_at(position.line as usize, position.character as usize);

        let Some(symbol_name) = symbol_name else {
            return Ok(None);
        };

        // Search for references across all workspace files
        let analyzers = self.analyzers.lock().await;
        let mut locations = Vec::new();

        for (file_uri, file_analyzer) in analyzers.iter() {
            let refs = file_analyzer.find_references(&symbol_name);
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
}

/// Convert a file:// URI to a PathBuf.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
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
}
