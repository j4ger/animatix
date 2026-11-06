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
mod env;
mod expr;
mod property;
mod stmt;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::typing::TypeEnv as TypedEnv;

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
