//! Component property and parameter checks.

use std::collections::HashMap;

use super::TypeEnv;
use super::expr::expr_summary;
use crate::ast::{ComponentDef, ParamDef, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::expand::INSTANCE_FORWARDED_PROPS;
use crate::typing;

impl<'a> TypeEnv<'a> {
    /// Check that all parameters have type annotations when strict_types is enabled.
    pub(super) fn check_param_annotations(
        &self,
        owner_name: &str,
        owner_kind: &str,
        params: &[ParamDef],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for param in params {
            if param.param_type.is_none() {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::TypeMismatch,
                        DiagnosticPhase::Build,
                        format!(
                            "Parameter '{}' of {} '{}' is missing a type annotation (required in strict mode)",
                            param.name, owner_kind, owner_name
                        ),
                    )
                    .with_subject(format!("{}.{}.{}", owner_kind, owner_name, param.name)),
                );
            }
        }
    }

    /// Warn when a parameter annotation references an alias that was never
    /// declared. The gradual checker still treats it as `Any`, so this does not
    /// reject the file, but it prevents the alias from silently weakening types.
    pub(super) fn check_param_type_aliases(
        &self,
        owner_name: &str,
        owner_kind: &str,
        params: &[ParamDef],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for param in params {
            if let Some(annotation) = &param.param_type {
                if let Some(alias) = self.typed.unresolved_alias(annotation) {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownTypeAlias,
                            DiagnosticPhase::Build,
                            format!(
                                "Parameter '{}' of {} '{}' references unknown type alias '{}'",
                                param.name, owner_kind, owner_name, alias
                            ),
                        )
                        .with_subject(format!("{}.{}.{}", owner_kind, owner_name, param.name)),
                    );
                }
            }
        }
    }

    pub(super) fn check_component_props(
        &self,
        component_name: &str,
        def: &ComponentDef,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Build a map of param name → ParamDef for quick lookup
        let param_map: HashMap<&str, &ParamDef> =
            def.params.iter().map(|p| (p.name.as_str(), p)).collect();

        for prop in props {
            if let Some(param) = param_map.get(prop.name.as_str()) {
                if let Some(expected) = &param.param_type {
                    let actual = typing::infer_expr_type(&prop.value, &self.typed);
                    let expected = self.typed.resolve_annotation(expected);
                    if !typing::is_subtype(&actual, &expected) {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::TypeMismatch,
                                DiagnosticPhase::Build,
                                format!(
                                    "Type mismatch: parameter '{}' of component '{}' expects {}, got {} (from {})",
                                    prop.name,
                                    component_name,
                                    expected,
                                    actual,
                                    expr_summary(&prop.value)
                                ),
                            )
                            .with_subject(format!("{}.{}", component_name, prop.name))
                            .with_byte_span(prop.value_span),
                        );
                    }
                }
            }
        }

        // Warn on extra properties that don't match any defined parameter.
        // Universal actor properties (opacity/at/anchor/offset) are exempt:
        // expansion forwards them onto the root actor, so they do have an
        // effect.
        for prop in props {
            if !param_map.contains_key(prop.name.as_str())
                && !INSTANCE_FORWARDED_PROPS.contains(&prop.name.as_str())
            {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownComponentProperty,
                        DiagnosticPhase::Build,
                        format!(
                            "Unknown property '{}' for component '{}'; this has no effect and may be a typo",
                            prop.name, component_name
                        ),
                    )
                    .with_subject(format!("{}.{}", component_name, prop.name)),
                );
            }
        }
    }
}
