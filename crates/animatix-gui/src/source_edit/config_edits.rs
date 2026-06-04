//! Edits related to config and import statements.

use animatix_syntax::ast::{Expr, Property, Stmt};

use super::SourceEditError;

/// Set or update a config property.
/// If no config block exists, one is created at the top of the statement list.
///
/// Preserves the quoting style of the existing value: if the current value is
/// an unquoted identifier (`Expr::Ident`), the new value is stored as an
/// identifier when possible (no spaces, no special chars).
pub(super) fn set_config_property(stmts: &mut Vec<Stmt>, key: &str, value: Expr) -> Result<(), SourceEditError> {
    // Find existing config block
    let config_idx = stmts.iter().position(|s| matches!(s, Stmt::Config { .. }));

    if let Some(idx) = config_idx {
        if let Stmt::Config { settings, .. } = &mut stmts[idx] {
            if let Some(prop) = settings.iter_mut().find(|p| p.name == key) {
                prop.value = preserve_quoting_style(&prop.value, value);
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

/// When the old value was an unquoted identifier, keep the new value as an
/// identifier if it is a valid bare identifier (no spaces, no quotes, etc.).
fn preserve_quoting_style(old: &Expr, new: Expr) -> Expr {
    let is_valid_ident = |s: &str| {
        !s.is_empty()
            && !s.contains(' ')
            && !s.contains('"')
            && !s.contains('\'')
            && !s.starts_with(|c: char| c.is_ascii_digit())
    };

    match (old, &new) {
        (Expr::Ident(_), Expr::Str(s)) if is_valid_ident(s) => Expr::Ident(s.clone()),
        _ => new,
    }
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
