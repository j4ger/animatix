//! Extension context for registering runtime capabilities.
//!
//! This is the cordis-inspired container for a single build or document:
//! primitives, actions, expression functions, and typed services all register
//! through one context. A later plugin loader can construct/dispose contexts
//! around rebuilds and hot reload.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use animatix_syntax::schema::{PropertyId, PropertyValueKind};

use crate::primitives::{Primitive, PrimitiveRegistrationError, PrimitiveRegistry};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
use crate::timeline::{Environment, EvalError, Value};

/// A native expression function registered by an extension.
pub type ExtensionFunction =
    dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync;

/// A schema-driven external property.
///
/// The id is allocated by the context and stored in the actor's
/// [`crate::timeline::PropertyPlan`]; frame-time access never resolves the
/// property name by hashing strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionPropertySpec {
    /// Stable id for this property within the owning context.
    pub id: PropertyId,
    /// Actor type that owns this property, e.g. `Gauge`.
    pub actor_type: String,
    /// Canonical source property name.
    pub name: String,
    /// Finite value kind used by the dynamic track.
    pub kind: PropertyValueKind,
    /// Optional precise type annotation for tooling; falls back to `kind`.
    pub ty: Option<animatix_syntax::typing::Type>,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
    /// Human-readable name for GUI labels, when it differs from `name`.
    pub display_name: Option<String>,
    /// Inspector grouping key.
    pub group: Option<String>,
    /// Help text for tooltips and documentation.
    pub help: Option<String>,
}

/// Error returned when an external property cannot be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyRegistrationError {
    /// The property name is already taken by a built-in or context property.
    Duplicate(String),
}

impl std::fmt::Display for PropertyRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "property '{name}' is already registered"),
        }
    }
}

impl std::error::Error for PropertyRegistrationError {}

/// Error returned when an action, function, or service name is already taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionRegistrationError {
    /// The name is already registered in this context.
    Duplicate(String),
}

impl std::fmt::Display for ExtensionRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "extension '{name}' is already registered"),
        }
    }
}

impl std::error::Error for ExtensionRegistrationError {}

/// Registry of extension-registered properties.
///
/// Built-in properties are not materialized here: they live in the shared
/// descriptor table plus `timeline::property_registry::PROPERTY_REGISTRY`, and
/// copying them into this context is what made a 140-row duplicate possible.
/// Only properties registered by an extension are stored and queried here.
#[derive(Default)]
pub struct PropertyRegistry {
    extensions: Vec<ExtensionPropertySpec>,
    next_property_id: u32,
}

mod property;
mod registry;
#[cfg(test)]
mod tests;

/// The unified extension registry: primitives, properties, actions, functions,
/// and services registered by an extension for one build or document.
#[derive(Default)]
pub struct ExtensionRegistry {
    primitives: Arc<PrimitiveRegistry>,
    properties: PropertyRegistry,
    actions: Vec<Box<dyn BuiltinAction>>,
    functions: Vec<(String, Arc<ExtensionFunction>)>,
    function_descriptors: Vec<animatix_syntax::schema::FunctionDescriptor>,
    services: HashMap<String, Arc<dyn Any + Send + Sync>>,
    service_descriptors: Vec<animatix_syntax::schema::ServiceDescriptor>,
}

/// Compatibility alias for the unified extension registry.
pub type ExtensionContext = ExtensionRegistry;
