//! LSP server for the Animatix DSL.
//!
//! This binary provides language intelligence (completions, diagnostics, hover, go-to-definition)
//! to external editors like VS Code and Neovim via the Language Server Protocol.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use animatix_analyzer::{Analyzer, Workspace};
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
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            analyzers: Mutex::new(HashMap::new()),
            cached_workspace: Mutex::new(None),
        }
    }

    /// Update the analyzer for a document. Rebuilds workspace if needed.
    async fn update_analyzer(&self, uri: String, text: String) {
        let path = uri_to_path(&uri);
        let is_new;
        {
            let mut analyzers = self.analyzers.lock().await;
            is_new = !analyzers.contains_key(&uri);
            let analyzer = analyzers
                .entry(uri.clone())
                .or_insert_with(|| Analyzer::new_with_path(&text, path.clone()));
            analyzer.update(&text);
        }

        if is_new {
            self.rebuild_workspace().await;
        } else {
            self.update_workspace_file(&uri, &text).await;
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

        // Search for references across all workspace files
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
        let (line, col) = line_index.byte_to_line_col(start);
        let (_, end_col) = line_index.byte_to_line_col(end);
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

/// Convert a file:// URI to a PathBuf.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let url = url::Url::parse(uri).ok()?;
    url.to_file_path().ok()
}

fn path_to_uri(path: &str) -> Option<Url> {
    Url::parse(&format!("file://{path}")).ok()
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
