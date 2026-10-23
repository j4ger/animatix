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
use crate::typing::{self, Type as TypedType, TypeEnv as TypedEnv};

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
            let mut signature = typing::ComponentSignature::default();
            for param in &entry.definition.params {
                if let Some(annotation) = &param.param_type {
                    signature
                        .params
                        .insert(param.name.clone(), TypedType::from_annotation(annotation));
                } else if let Some(default) = &param.default {
                    signature
                        .params
                        .insert(param.name.clone(), typing::infer_expr_type(default, &typed));
                }
            }
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
    pub fn check_statements(&mut self, stmts: &[Stmt]) -> Vec<Diagnostic> {
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name, annotation, ..
            } = stmt
            {
                self.typed.register_alias(name, annotation);
            }
        }
        let mut diagnostics = Vec::new();
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name,
                annotation,
                span,
                is_pub: _,
            } = stmt
            {
                if let Some(alias) = self.typed.unresolved_alias(annotation) {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownTypeAlias,
                            DiagnosticPhase::Build,
                            format!("Type alias '{name}' references unknown type alias '{alias}'"),
                        )
                        .with_ast_span(*span)
                        .with_subject(alias),
                    );
                }
            }
        }
        for stmt in stmts {
            self.check_stmt(stmt, &mut diagnostics);
        }
        diagnostics
    }
}
