//! Gradual type checker for Animatix.
//!
//! Validates component instantiation properties and action invocation arguments
//! against parameter type annotations.
//!
//! This is a lightweight, single-pass checker with no inference and no unification.
//! Unannotated parameters (`param_type: None`) accept any value.
//!
//! The implementation is split by domain: component/parameter checks live in
//! `property`, action invocation checks in `action`, statement walking in
//! `stmt`, and expression summaries in `expr`. This module holds the
//! environment type and the public entry point.

mod action;
mod expr;
mod property;
mod stmt;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::ast::Stmt;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing::{self, TypeEnv as TypedEnv};

/// Type-checking environment.
pub struct TypeEnv<'a> {
    /// Component name → component definition.
    components: &'a HashMap<String, crate::module::ComponentEntry>,
    /// Module-scoped timeline functions: fn_name → template.
    module_fns: &'a HashMap<String, crate::module::FnTemplate>,
    /// Actor label → component type name (accumulated during AST walk).
    labels: HashMap<String, String>,
    /// Symbol-aware type environment used for expression inference.
    typed: TypedEnv,
    /// When true, unannotated parameters produce warnings.
    strict_types: bool,
}

impl<'a> TypeEnv<'a> {
    /// Create a new type environment from a component registry and module actions.
    pub fn new(
        components: &'a HashMap<String, crate::module::ComponentEntry>,
        module_fns: &'a HashMap<String, crate::module::FnTemplate>,
    ) -> Self {
        let mut typed = TypedEnv::with_stdlib();
        for (name, entry) in components {
            let signature = crate::symbol_table::component_signature(
                entry.definition.params.iter().map(|param| {
                    (param.name.as_str(), param.param_type.as_ref(), param.default.as_ref())
                }),
                &mut |default| Some(typing::infer_expr_type(default, &typed)),
            );
            typed.register_component(name, signature);
        }
        Self {
            components,
            module_fns,
            labels: HashMap::new(),
            typed,
            strict_types: false,
        }
    }

    /// Enable or disable strict type mode.
    pub fn with_strict_types(mut self, strict: bool) -> Self {
        self.strict_types = strict;
        self
    }

    /// Register type aliases exported by aliased module imports.
    pub fn register_module_aliases(
        &mut self,
        namespaces: &HashMap<String, crate::module::Namespace>,
    ) {
        fn register_ns(prefix: &str, ns: &crate::module::Namespace, typed: &mut TypedEnv) {
            for (name, annotation) in &ns.type_exports {
                typed.register_alias(&format!("{prefix}::{name}"), annotation);
            }
            for (name, nested) in &ns.namespaces {
                register_ns(&format!("{prefix}.{name}"), nested, typed);
            }
        }
        for (alias, ns) in namespaces {
            register_ns(alias, ns, &mut self.typed);
        }
    }

    /// Check all statements in a program, returning any type errors.
    ///
    /// Two passes only: a top-level alias pre-registration (so forward alias
    /// references resolve) and one recursive check. Unknown-alias warnings are
    /// collected separately and emitted first, preserving the diagnostic order
    /// the previous three-loop implementation produced.
    pub fn check_statements(&mut self, stmts: &[Stmt]) -> Vec<Diagnostic> {
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name, annotation, ..
            } = stmt
            {
                self.typed.register_alias(name, annotation);
            }
        }
        let mut alias_diagnostics = Vec::new();
        let mut diagnostics = Vec::new();
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name,
                annotation,
                span,
                is_pub: _,
            } = stmt
                && let Some(alias) = self.typed.unresolved_alias(annotation)
            {
                alias_diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownTypeAlias,
                        DiagnosticPhase::Build,
                        format!("Type alias '{name}' references unknown type alias '{alias}'"),
                    )
                    .with_ast_span(*span)
                    .with_subject(alias),
                );
            }
            self.check_stmt(stmt, &mut diagnostics);
        }
        alias_diagnostics.extend(diagnostics);
        alias_diagnostics
    }
}
