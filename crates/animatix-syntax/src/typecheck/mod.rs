//! Gradual type checker for Animatix.
//!
//! Validates component properties and unified callable argument contracts.
//!
//! Infers expression and function result types while retaining gradual `Any`
//! behavior for unannotated parameters (`param_type: None`).
//!
//! The implementation is split by domain: component/parameter checks live in
//! `property`, callable and statement checks in `stmt`, standard signatures in
//! `standard_callables`, and expression summaries in `expr`. This module holds the
//! environment type and the public entry point.

mod env;
mod expr;
mod property;
mod standard_callables;
mod stmt;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::typing::TypeEnv as TypedEnv;
pub(super) use standard_callables::{CallableParamType, standard_callable_signature};

/// Type-checking environment.
pub struct TypeEnv<'a> {
    /// Component name → component definition.
    components: &'a HashMap<String, crate::module::ComponentEntry>,
    /// Module-scoped timeline functions: fn_name → template.
    module_fns: &'a HashMap<String, crate::module::FnTemplate>,
    /// Public functions exposed through namespace-qualified imports.
    namespace_fns: HashMap<String, crate::module::FnTemplate>,
    /// Extension callable contracts, keyed by qualified and visible names.
    callables: HashMap<String, animatix_core::callable::CallableDescriptor>,
    /// User-function aliases retain the original named/default parameter contract.
    function_aliases: HashMap<String, String>,
    /// Actor label → component type name (accumulated during AST walk).
    labels: HashMap<String, String>,
    /// Symbol-aware type environment used for expression inference.
    typed: TypedEnv,
    /// When true, unannotated parameters produce warnings.
    strict_types: bool,
    /// Nesting depth while walking ordinary function bodies.
    function_depth: usize,
}
