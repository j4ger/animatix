//! Shared schema for actor properties and primitives: metadata that would
//! otherwise be duplicated in runtime or analyzer-specific tables. No runtime
//! logic, so analyzer/LSP can consume it without Vello/WGPU.

pub use animatix_core::caps::Applicable;
pub use animatix_core::property::{PropertyId, PropertyValueKind};

use crate::typing::Type;

/// One known built-in property with its applicable actor types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertySpec {
    /// Stable id for this property name.
    pub id: PropertyId,
    /// Canonical property name.
    pub name: &'static str,
    /// Actor source type names this property applies to; boxed because the
    /// `OnceLock` cache owns the table.
    pub actor_types: Box<[&'static str]>,
    /// Inferred/declared property type.
    pub ty: Type,
    /// Finite value kind used by dynamic property tracks.
    pub value_kind: PropertyValueKind,
}

/// Neutral property descriptor shared by built-ins, extensions, and tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyDescriptor {
    /// Stable runtime property id, when registered at runtime; `None` for
    /// manifests and analyzer-only descriptors.
    pub id: Option<PropertyId>,
    /// Canonical source-text property name.
    pub name: String,
    /// Actor source types this property applies to.
    pub actor_types: Vec<String>,
    /// Type annotation consumed by analyzer/typechecker-compatible APIs.
    pub ty: Type,
    /// Finite value kind used by dynamic property tracks.
    pub value_kind: PropertyValueKind,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
    /// Human-readable name for GUI labels, when it differs from `name`.
    pub display_name: Option<String>,
    /// Inspector grouping key.
    pub group: Option<String>,
    /// Help text for tooltips and documentation.
    pub help: Option<String>,
}

/// Description of one action parameter or named modifier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionParam {
    /// Parameter name as written in source, e.g. `"to"`.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Expected type information for docs, completion, and validation.
    pub type_info: String,
}

/// Description of a built-in or extension action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionSignature {
    /// Action verb as written in source, e.g. `"fade-in"`.
    pub name: String,
    /// High-level grouping for UI organization, e.g. `"Motion"`.
    pub category: String,
    /// One-line explanation of what the action does.
    pub description: String,
    /// Positional arguments accepted by the action.
    pub params: Vec<ActionParam>,
    /// Named modifiers accepted by the action.
    pub modifiers: Vec<ActionParam>,
}

/// Description of an extension expression function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionDescriptor {
    /// Function name as written in source.
    pub name: String,
    /// Positional parameter descriptions.
    pub params: Vec<ActionParam>,
    /// Optional return type for language intelligence.
    pub return_type: Option<Type>,
    /// Optional help text.
    pub help: Option<String>,
}

/// Description of an opaque extension service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceDescriptor {
    /// Canonical service name.
    pub name: String,
    /// Optional type information for tooling.
    pub type_info: Option<String>,
    /// Optional help text.
    pub help: Option<String>,
}

/// UI/domain category for a primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimitiveCategory {
    /// Geometric shapes.
    Shape,
    /// Text and typography.
    Text,
    /// Images, SVG, and audio.
    Media,
    /// Plots and charts.
    Plot,
    /// Layout containers.
    Container,
    /// Annotations and callouts.
    Annotation,
}

/// Child-rendering strategy selected by a primitive; re-exported for tooling.
pub use animatix_core::caps::ChildProcessingKind;
/// Engine capabilities that determine which subsystems consume a primitive.
pub use animatix_core::caps::PrimitiveCapabilities;

/// Metadata for a primitive in the shared schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveSpec {
    /// Source text type name, e.g. `Rect`.
    pub type_name: String,
    /// Display name for GUI palettes.
    pub display_name: String,
    /// Author-facing documentation for completions and hover info.
    pub doc: String,
    /// Default syntax stub when inserting or creating this primitive.
    pub default_stub: Option<String>,
    /// UI category.
    pub category: PrimitiveCategory,
    /// Opaque icon id.
    pub icon_id: String,
    /// Whether this primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// Engine capability flags.
    pub capabilities: PrimitiveCapabilities,
    /// Child-rendering strategy used by the scene subtree renderer.
    pub child_processing: ChildProcessingKind,
}

/// Neutral primitive descriptor shared by built-ins, extensions, and tooling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    /// Source text type name, e.g. `Gauge`.
    pub type_name: String,
    /// Display name for GUI palettes.
    pub display_name: String,
    /// UI category.
    pub category: PrimitiveCategory,
    /// Opaque icon id.
    pub icon_id: String,
    /// Whether this primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// Engine capability flags.
    pub capabilities: PrimitiveCapabilities,
    /// Child-rendering strategy used by the scene subtree renderer.
    pub child_processing: ChildProcessingKind,
    /// Names of properties declared by this primitive.
    pub properties: Vec<String>,
}

/// One author-visible effect parameter: its name and value kind.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectParamSpecDef {
    /// Parameter name as authored.
    pub name: Box<str>,
    /// Declared value kind.
    pub kind: PropertyValueKind,
}

/// One author-visible effect kind inside a `Filter` scope. Effects are not
/// primitives; this table lets the analyzer validate/complete declarations.
#[derive(Clone, Debug)]
pub struct EffectSpecDef {
    /// Authored type name (`Blur`).
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Declared marshalable parameters; `enabled: Bool` is implicit and not in
    /// the shader uniform layout.
    pub params: Vec<EffectParamSpecDef>,
}

mod catalog;
mod impls;
mod property_types;
#[cfg(test)]
mod tests;

pub use catalog::{
    builtin_primitive_specs, common_property_names, effect_spec, effect_specs, property_specs,
};
pub(crate) use property_types::raw_property_types;
