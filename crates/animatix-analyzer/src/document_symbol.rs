//! Document symbols (outline view) provider.

use std::collections::HashSet;

use crate::symbol_table::{LabelKind, SymbolTable};
use crate::types::{DocumentSymbol, SymbolKind};

/// Get all document symbols for outline view.
pub fn document_symbols(symbols: &SymbolTable) -> Vec<DocumentSymbol> {
    let mut result = Vec::new();

    for (name, info) in &symbols.labels {
        let kind = match info.kind {
            LabelKind::Actor => SymbolKind::Actor,
            LabelKind::Let => SymbolKind::Variable,
            LabelKind::For => SymbolKind::Variable,
            LabelKind::Always => SymbolKind::Block,
            LabelKind::Component => SymbolKind::Component,
        };
        result.push(DocumentSymbol {
            name: name.clone(),
            kind,
            line: info.line,
            col: info.col,
            detail: info.ty.clone(),
        });
    }

    let mut seen: HashSet<String> = result.iter().map(|s| s.name.clone()).collect();
    for (name, info) in &symbols.components {
        if !seen.insert(name.clone()) {
            continue;
        }
        result.push(DocumentSymbol {
            name: name.clone(),
            kind: SymbolKind::Component,
            line: info.line,
            col: info.col,
            detail: Some(format!("({} params)", info.params.len())),
        });
    }

    result.sort_by_key(|symbol| symbol.line);
    result
}
