//! Go-to-definition provider.

use std::path::Path;

use animatix_syntax::token::{Token, TokenKind, line_col_to_byte, token_at_byte};

use crate::Workspace;
use crate::symbol_table::SymbolTable;
use crate::types::Location;

/// Find the definition location of a symbol at a cursor position.
pub fn definition_at(
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    workspace: Option<&Workspace>,
    path: Option<&Path>,
    line: usize,
    col: usize,
) -> Option<Location> {
    let byte = line_col_to_byte(source, line, col);
    let token = token_at_byte(tokens, byte)?;
    let text = match &token.kind {
        TokenKind::Ident(name) => name,
        _ => return None,
    };

    // Check if it's a label defined in this file
    if let Some(info) = symbols.labels.get(text) {
        return Some(Location {
            file: None, // Same file
            line: info.line,
            col: info.col,
        });
    }

    // Check if it's a component defined in this file
    if let Some(info) = symbols.components.get(text) {
        return Some(Location {
            file: None,
            line: info.line,
            col: info.col,
        });
    }

    // Check if it's a scene defined in this file
    if let Some(info) = symbols.scenes.get(text) {
        return Some(Location {
            file: None,
            line: info.line,
            col: info.col,
        });
    }

    // Check imported files for cross-file definitions
    if let Some(workspace) = workspace {
        if let Some(path) = path {
            for import in &symbols.imports {
                let import_path = Workspace::resolve_import_path(path, &import.path);
                if let Some(symbols) = workspace.file_symbols(&import_path) {
                    // Check labels in imported file
                    if let Some(info) = symbols.labels.get(text) {
                        return Some(Location {
                            file: Some(import_path.display().to_string()),
                            line: info.line,
                            col: info.col,
                        });
                    }
                    // Check components in imported file
                    if let Some(info) = symbols.components.get(text) {
                        return Some(Location {
                            file: Some(import_path.display().to_string()),
                            line: info.line,
                            col: info.col,
                        });
                    }
                    // Check scenes in imported file
                    if let Some(info) = symbols.scenes.get(text) {
                        return Some(Location {
                            file: Some(import_path.display().to_string()),
                            line: info.line,
                            col: info.col,
                        });
                    }
                }
            }
        }
    }

    None
}
