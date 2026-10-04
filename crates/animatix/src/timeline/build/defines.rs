//! `--set NAME=VALUE` build-time overrides.
//!
//! The point is template × data batching: one `.amx` file, N renders with
//! different text, colors or data. The value side is parsed with the
//! language's own grammar, so an override can carry any build-time expression
//! the file itself could carry.

use crate::ast::{Expr, Stmt};
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

/// One parsed override: the environment name and the expression that seeds it.
pub(crate) struct BuildDefine {
    /// Environment name, as written on the command line.
    pub(crate) name: String,
    /// Parsed right-hand side, evaluated against the build environment.
    pub(crate) value: Expr,
}

/// True for a bare identifier (`_`/letter first, then letters, digits, `_`).
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {},
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Parse `NAME=VALUE` pairs into defines.
///
/// Each value is parsed as the right-hand side of a top-level `let`, so
/// `--set accent=(1.0,0.2,0.3)` or `--set rows={1, 2, 3}` mean what they would
/// mean in the file. There is deliberately no second parser: the single
/// tokenizer produces the same AST source text does.
pub(crate) fn parse_defines(
    raw: &[(String, String)],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<BuildDefine> {
    let mut defines = Vec::with_capacity(raw.len());
    for (name, value) in raw {
        if !is_identifier(name) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("`--set {name}={value}`: '{name}' is not a valid identifier."),
                )
                .with_subject(name),
            );
            continue;
        }
        let source = format!("let {name} = {value}\n");
        let (stmts, errors) = animatix_syntax::parser::parse_source(&source);
        let Some(stmts) = stmts.filter(|_| errors.is_empty()) else {
            let detail = errors
                .first()
                .map(|e| e.message.clone())
                .unwrap_or_else(|| "no expression found".to_string());
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "`--set {name}={value}`: the value is not a valid expression ({detail})."
                    ),
                )
                .with_subject(name),
            );
            continue;
        };
        let found = stmts.iter().find_map(|stmt| match stmt {
            Stmt::LetDecl { value, .. } => Some(value.clone()),
            _ => None,
        });
        match found {
            Some(value) => defines.push(BuildDefine {
                name: name.clone(),
                value,
            }),
            None => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("`--set {name}={value}`: the value did not parse to an expression."),
                )
                .with_subject(name),
            ),
        }
    }
    defines
}
