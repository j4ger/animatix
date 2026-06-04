//! Edits related to config and import statements.

use animatix_syntax::ast::{Expr, Property, Stmt};

use super::SourceEditError;

/// Set or update a config property.
/// If no config block exists, one is created at the top of the statement list.
pub(super) fn set_config_property(stmts: &mut Vec<Stmt>, key: &str, value: Expr) -> Result<(), SourceEditError> {
    // Find existing config block
    let config_idx = stmts.iter().position(|s| matches!(s, Stmt::Config { .. }));

    if let Some(idx) = config_idx {
        if let Stmt::Config { settings, .. } = &mut stmts[idx] {
            if let Some(prop) = settings.iter_mut().find(|p| p.name == key) {
                prop.value = value;
                return Ok(());
            }
            settings.push(Property {
                name: key.into(),
                value,
                value_span: None,
                trailing_comment: None,
            });
            return Ok(());
        }
    }

    // No config block exists — create one at the top
    stmts.insert(0, Stmt::Config {
        settings: vec![Property {
            name: key.into(),
            value,
            value_span: None,
            trailing_comment: None,
        }],
        span: None,
    });
    Ok(())
}

/// Insert an import statement at the top of the file, after any existing imports.
pub(super) fn insert_import(stmts: &mut Vec<Stmt>, path: &str) -> Result<(), SourceEditError> {
    // Check for duplicate
    if stmts.iter().any(|s| matches!(s, Stmt::Import { path: p, .. } if p == path)) {
        return Err(SourceEditError::Generic(format!("Import '{}' already exists", path)));
    }

    let import_stmt = Stmt::Import {
        path: path.into(),
        alias: None,
        span: None,
    };

    // Find insertion point: after the last import, or at the very top
    let last_import_idx = stmts.iter().rposition(|s| matches!(s, Stmt::Import { .. }));
    if let Some(idx) = last_import_idx {
        stmts.insert(idx + 1, import_stmt);
    } else {
        stmts.insert(0, import_stmt);
    }
    Ok(())
}
