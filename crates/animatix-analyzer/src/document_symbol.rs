//! Document symbols (outline view) provider.

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

    for (name, info) in &symbols.components {
        result.push(DocumentSymbol {
            name: name.clone(),
            kind: SymbolKind::Component,
            line: info.line,
            col: info.col,
            detail: Some(format!("({} params)", info.params.len())),
        });
    }

    result.sort_by(|a, b| a.line.cmp(&b.line));
    result
}
