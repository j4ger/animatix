//! LSP server for the Animatix DSL.
//!
//! This binary provides language intelligence (completions, diagnostics, hover, go-to-definition)
//! to external editors like VS Code and Neovim via the Language Server Protocol.

use animatix_analyzer::Analyzer;
use std::collections::HashMap;
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
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            analyzers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get or create an analyzer for a document.
    async fn get_analyzer(&self, uri: &str) -> Analyzer {
        let analyzers = self.analyzers.lock().await;
        analyzers.get(uri).cloned().unwrap_or_else(|| Analyzer::new(""))
    }

    /// Update the analyzer for a document.
    async fn update_analyzer(&self, uri: String, text: String) {
        let mut analyzers = self.analyzers.lock().await;
        let analyzer = analyzers.entry(uri).or_insert_with(|| Analyzer::new(&text));
        analyzer.update(&text);
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
                documentation: item.documentation.map(|d| Documentation::String(d)),
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
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend::new(client));
    Server::new(stdin, stdout, socket).serve(service).await;
}
