//! Action invocation argument checks.

use std::collections::HashMap;

use super::TypeEnv;
use super::expr::expr_summary;
use crate::ast::{Expr, Modifier, ParamDef};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing;

impl<'a> TypeEnv<'a> {
    pub(super) fn check_action_invocation(
        &self,
        fn_name: &str,
        target: &str,
        component_name: &str,
        params: &[ParamDef],
        modifiers: &[Modifier],
        byte_span: Option<crate::ast::ByteSpan>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Build a set of provided param names from named modifiers
        let provided: HashMap<&str, &Expr> = modifiers
            .iter()
            .filter_map(|m| m.name.as_ref().map(|name| (name.as_str(), &m.value)))
            .collect();

        for param in params {
            if let Some(expected) = &param.param_type {
                if let Some(value) = provided.get(param.name.as_str()) {
                    let actual = typing::infer_expr_type(value, &self.typed);
                    let expected = self.typed.resolve_annotation(expected);
                    if !typing::is_subtype(&actual, &expected) {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::TypeMismatch,
                                DiagnosticPhase::Build,
                                format!(
                                    "Type mismatch: parameter '{}' of action '{}.{}' expects {}, got {} (from {})",
                                    param.name,
                                    component_name,
                                    fn_name,
                                    expected,
                                    actual,
                                    expr_summary(value)
                                ),
                            )
                            .with_subject(format!("{}.{}.{}", component_name, fn_name, param.name))
                            .with_byte_span(byte_span),
                        );
                    }
                } else if param.default.is_none() {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::TypeMismatch,
                            DiagnosticPhase::Build,
                            format!(
                                "Missing required parameter '{}' for action '{}.{}' on '{}'",
                                param.name, component_name, fn_name, target
                            ),
                        )
                        .with_subject(format!("{}.{}.{}", component_name, fn_name, param.name))
                        .with_byte_span(byte_span),
                    );
                }
            }
        }
    }
}
