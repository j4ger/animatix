//! Identifier occurrences derived from the AST and lossless token stream.
//!
//! This module turns the shared role classifier into a concrete list of
//! `(byte_span, name, kind)` records consumed by the analyzer, GUI, and LSP.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::ast::{ByteSpan, Stmt};
use crate::highlight;
use crate::symbol_table::SymbolTable;
use crate::token::TokenKind;

/// The syntactic role of an identifier occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccurrenceKind {
    /// Actor or variable label.
    Label,
    /// Type name.
    Type,
    /// Property name.
    Property,
    /// Component or action parameter name.
    Parameter,
    /// Function or method name.
    Function,
    /// Action verb.
    Action,
    /// Scene name.
    Scene,
    /// Component definition name.
    Component,
    /// Type alias name.
    TypeAlias,
    /// Import alias.
    ImportAlias,
    /// Match wildcard `_`.
    Wildcard,
    /// Any other identifier reference.
    Variable,
}

/// A single identifier occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Byte range of the identifier.
    pub span: ByteSpan,
    /// Source text of the identifier.
    pub name: String,
    /// Syntactic role.
    pub kind: OccurrenceKind,
    /// Lexical scope identifier, populated during scope resolution.
    pub scope_id: Option<u32>,
}

/// Collect occurrences for every identifier token in `source`.
pub fn collect(stmts: &[Stmt], source: &str) -> Vec<Occurrence> {
    let tokens = crate::token::tokenize(source);
    let symbols = SymbolTable::build_from_ast(stmts);
    let label_names = highlight::collect_label_names(stmts);
    let property_names: HashSet<String> =
        symbols.properties.values().flat_map(|props| props.iter().cloned()).collect();
    let param_names: HashSet<String> = symbols
        .components
        .values()
        .flat_map(|c| c.params.iter().map(|p| p.name.clone()))
        .collect();

    tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| {
            let name = match &token.kind {
                TokenKind::Ident(name) => name.clone(),
                _ => return None,
            };
            let role = highlight::classify_token(
                idx,
                token,
                &tokens,
                &symbols,
                &label_names,
                &property_names,
                &param_names,
            );
            Some(Occurrence {
                span: token.span,
                name,
                kind: role_to_kind(role),
                scope_id: None,
            })
        })
        .collect()
}

fn role_to_kind(role: &str) -> OccurrenceKind {
    match role {
        "label" => OccurrenceKind::Label,
        "type" => OccurrenceKind::Type,
        "property" => OccurrenceKind::Property,
        "parameter" => OccurrenceKind::Parameter,
        "function" => OccurrenceKind::Function,
        "action" => OccurrenceKind::Action,
        "scene" => OccurrenceKind::Scene,
        "component" => OccurrenceKind::Component,
        "typealias" => OccurrenceKind::TypeAlias,
        "importalias" => OccurrenceKind::ImportAlias,
        "wildcard" => OccurrenceKind::Wildcard,
        _ => OccurrenceKind::Variable,
    }
}

thread_local! {
    static PARSER_OCCURRENCES: RefCell<Vec<Vec<Occurrence>>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` while recording parser-side occurrences.
pub(crate) fn with_occurrences<T>(f: impl FnOnce() -> T) -> (T, Vec<Occurrence>) {
    PARSER_OCCURRENCES.with(|stack| stack.borrow_mut().push(Vec::new()));
    let result = f();
    let occurrences = PARSER_OCCURRENCES.with(|stack| stack.borrow_mut().pop().unwrap_or_default());
    (result, occurrences)
}

/// Record a parser-side identifier occurrence.
pub(crate) fn record(kind: OccurrenceKind, name: String, span: ByteSpan) {
    PARSER_OCCURRENCES.with(|stack| {
        if let Some(current) = stack.borrow_mut().last_mut() {
            current.push(Occurrence {
                span,
                name,
                kind,
                scope_id: None,
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    fn occ(source: &str) -> Vec<(String, OccurrenceKind)> {
        let (ast, _) = parse_source(source);
        let stmts = ast.unwrap_or_default();
        collect(&stmts, source).into_iter().map(|o| (o.name, o.kind)).collect()
    }

    #[test]
    fn classifies_actor_label_type_and_property() {
        let roles = occ("title: Text, size: (100, 100)\n");
        assert!(roles.iter().any(|(n, k)| n == "title" && *k == OccurrenceKind::Label));
        assert!(roles.iter().any(|(n, k)| n == "Text" && *k == OccurrenceKind::Type));
        assert!(roles.iter().any(|(n, k)| n == "size" && *k == OccurrenceKind::Property));
    }

    #[test]
    fn classifies_action_target_and_call() {
        let roles = occ("fade-in card[0] [1s]\nlet x = cos(0)\n");
        assert!(roles.iter().any(|(n, k)| n == "fade-in" && *k == OccurrenceKind::Action));
        assert!(roles.iter().any(|(n, k)| n == "card" && *k == OccurrenceKind::Label));
        assert!(roles.iter().any(|(n, k)| n == "cos" && *k == OccurrenceKind::Function));
    }
}
