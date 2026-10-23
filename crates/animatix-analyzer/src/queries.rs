//! Query surface: completions, diagnostics, hover, navigation, and highlighting.

use animatix_syntax::ast::Span;
use animatix_syntax::token::TokenKind;

use crate::completer;
use crate::definition;
use crate::diagnostics;
use crate::document_symbol;
use crate::duplicates;
use crate::hover;
use crate::references;
use crate::rename;
use crate::unresolved;
use crate::{Analyzer, CompletionItem, Diagnostic, DocumentSymbol, HoverInfo, Location, Workspace};

impl Analyzer {
    /// Completions at cursor position.
    pub fn completions_at(&self, line: usize, col: usize) -> Vec<CompletionItem> {
        completer::completions_at(
            &self.symbols,
            &self.tokens,
            &self.source,
            line,
            col,
            &self.extension_manifest,
        )
    }

    /// All diagnostics (parse errors + semantic checks).
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics_with_config(&self.lint_config)
    }

    /// All diagnostics with explicit lint configuration.
    ///
    /// Memoized: repeated calls with the same configuration reuse the first
    /// result, and any `update()` invalidates it.
    pub fn diagnostics_with_config(&self, config: &diagnostics::LintConfig) -> Vec<Diagnostic> {
        let key = config.fingerprint();
        // A poisoning panic can only come from a panic inside this method,
        // which would already have aborted the caller's work; recovering the
        // guard keeps later queries working instead of cascading.
        {
            let cache = self.diagnostics_cache.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((cached_key, cached)) = cache.as_ref() {
                if *cached_key == key {
                    return cached.as_ref().clone();
                }
            }
        }
        let computed = std::sync::Arc::new(self.compute_diagnostics(config));
        let mut cache = self.diagnostics_cache.lock().unwrap_or_else(|e| e.into_inner());
        *cache = Some((key, computed.clone()));
        computed.as_ref().clone()
    }

    /// Run every diagnostic pass against the current source.
    fn compute_diagnostics(&self, config: &diagnostics::LintConfig) -> Vec<Diagnostic> {
        let mut diagnostics = diagnostics::collect_diagnostics_with_config(
            &self.source,
            &self.parse_errors,
            &self.symbols,
            self.ast.as_deref(),
            &self.tokens,
            config,
        );
        // Scope-aware duplicate-label warnings, unresolved-name warnings, and
        // parse-time warnings (e.g. braced-property silent drops) honor the
        // same lint config as the semantic warnings above.
        let mut warnings = Vec::new();
        if let Some(stmts) = &self.ast {
            warnings.extend(duplicates::collect_duplicate_labels(
                stmts,
                &self.occurrences,
                &self.source,
            ));
            warnings.extend(unresolved::collect_unresolved_variables(
                stmts,
                &self.occurrences,
                &self.symbols,
                &self.tokens,
                &self.source,
            ));
        }
        // Imports that resolve to no file are errors (the CLI build rejects
        // them), surfaced at the import string span.
        warnings
            .extend(unresolved::collect_unresolved_imports(&self.symbols, self.path.as_deref()));
        // Lexical issues: an unterminated string literal explains the parse
        // errors that follow it, so report the real cause too.
        for issue in &self.token_issues {
            let span = Span::from_byte_span(&self.source, issue.span);
            warnings.push(diagnostics::Diagnostic {
                severity: diagnostics::DiagnosticSeverity::Error,
                line: span.start_line.saturating_sub(1),
                col: span.start_col.saturating_sub(1),
                end_line: span.end_line.saturating_sub(1),
                end_col: span.end_col.saturating_sub(1),
                message: issue.kind.to_string(),
                code: Some("unterminated-string".to_string()),
            });
        }
        for warning in &self.parse_warnings {
            warnings.push(Self::convert_parse_warning(&self.source, warning.clone()));
        }
        for warning in warnings {
            let suppressed = warning.severity == diagnostics::DiagnosticSeverity::Warning
                && (config.disable_all_warnings
                    || warning.code.as_deref().is_some_and(|code| config.is_disabled(code)));
            if !suppressed {
                diagnostics.push(warning);
            }
        }
        diagnostics.extend_from_slice(&self.type_diagnostics);
        diagnostics
    }

    /// Hover information at cursor position.
    pub fn hover_at(&self, line: usize, col: usize) -> Option<HoverInfo> {
        hover::hover_at(
            &self.symbols,
            &self.tokens,
            &self.source,
            line,
            col,
            &self.extension_manifest,
        )
    }

    /// Go-to-definition at cursor position.
    ///
    /// Pass `workspace` for cross-file definition lookup.
    pub fn definition_at(
        &self,
        workspace: Option<&Workspace>,
        line: usize,
        col: usize,
    ) -> Option<Location> {
        definition::definition_at(
            &self.symbols,
            &self.tokens,
            &self.source,
            workspace,
            self.path.as_deref(),
            line,
            col,
        )
    }

    /// Find all references to the first declaration of `symbol_name` in this
    /// file. Returns a list of (start_line, start_col, end_line, end_col) ranges.
    pub fn find_references(&self, symbol_name: &str) -> Vec<(usize, usize, usize, usize)> {
        references::find_references(&self.occurrences, &self.source, symbol_name)
    }

    /// Find all references to the binding at a cursor position.
    ///
    /// Lexical scopes are resolved from parser-recorded occurrence scopes, so
    /// shadowed declarations do not leak references from unrelated scopes.
    pub fn find_references_at(&self, line: usize, col: usize) -> Vec<(usize, usize, usize, usize)> {
        references::find_references_at(&self.occurrences, &self.source, line, col)
    }

    /// Resolve a rename at a cursor position.
    ///
    /// Returns the ranges to rewrite (scope-aware, so same-named bindings in
    /// other scopes are untouched) or a reason the symbol cannot be renamed.
    pub fn rename_at(
        &self,
        line: usize,
        col: usize,
    ) -> Result<rename::RenameTarget, rename::RenameRejection> {
        rename::rename_at(&self.occurrences, &self.symbols, &self.source, line, col)
    }

    /// Document symbols (outline view).
    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        document_symbol::document_symbols(&self.symbols)
    }

    /// Get the symbol name at a cursor position.
    /// Returns the identifier or type name at the given line/column,
    /// or None if the position is not on a symbol.
    pub fn symbol_at(&self, line: usize, col: usize) -> Option<String> {
        let byte = animatix_syntax::token::line_col_to_byte(&self.source, line, col);
        let token = animatix_syntax::token::token_at_byte(&self.tokens, byte)?;
        match &token.kind {
            TokenKind::Ident(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Return `(start_byte, end_byte, role)` for every token in the source.
    ///
    /// Identifier roles come from parser-recorded occurrences; lexical tokens
    /// use the tokenizer's shared role vocabulary.
    pub fn token_roles(&self) -> Vec<(usize, usize, &'static str)> {
        let occurrences_by_start: std::collections::HashMap<
            usize,
            &animatix_syntax::occurrence::Occurrence,
        > = self.occurrences.iter().map(|o| (o.span.start, o)).collect();
        self.tokens
            .iter()
            .map(|token| {
                let role = match &token.kind {
                    TokenKind::Ident(_) => occurrences_by_start
                        .get(&token.span.start)
                        .map(|o| o.kind.token_role())
                        .unwrap_or("variable"),
                    _ => animatix_syntax::highlight::lexical_role(token),
                };
                (token.span.start, token.span.end, role)
            })
            .collect()
    }
}
