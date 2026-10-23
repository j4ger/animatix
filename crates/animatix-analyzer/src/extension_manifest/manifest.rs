//! The public manifest model and its TOML entry points.

use animatix_syntax::schema::{
    ActionSignature, FunctionDescriptor, PrimitiveDescriptor, PropertyDescriptor, ServiceDescriptor,
};
use animatix_syntax::typing::Type;

use super::schema::parse_manifest_type;
use crate::symbol_table::SymbolTable;

/// Collection of analyzer-only extension metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionManifest {
    /// Optional native library path, resolved relative to the manifest file by
    /// CLI consumers. Analyzer and LSP ignore this field.
    pub library: Option<String>,
    /// Primitive declarations.
    pub primitives: Vec<PrimitiveDescriptor>,
    /// Property declarations.
    pub properties: Vec<PropertyDescriptor>,
    /// Action declarations.
    pub actions: Vec<ActionSignature>,
    /// Function declarations.
    pub functions: Vec<FunctionDescriptor>,
    /// Service declarations.
    pub services: Vec<ServiceDescriptor>,
    /// Plugin-authored post-processing effect declarations.
    pub effects: Vec<ManifestEffect>,
}

/// Manifest metadata for one plugin-authored effect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestEffect {
    /// Authored type name used in `.amx` (`pix: Pixelate`).
    pub name: String,
    /// Human-readable name for GUI labels and completion detail.
    pub display_name: Option<String>,
    /// Declared parameters.
    pub params: Vec<ManifestEffectParam>,
}

/// One parameter of a manifest-declared effect.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManifestEffectParam {
    /// Parameter name as authored.
    pub name: String,
    /// Declared value type string (`"Num"`, `"Bool"`, `"Vec2"`, `"Vec4"`).
    pub ty: String,
}

impl ManifestEffect {
    /// The name used for completion detail lines.
    pub fn display_label(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }
}

impl ExtensionManifest {
    /// Parse a TOML manifest.
    pub fn from_toml(source: &str) -> Result<Self, String> {
        toml::from_str(source).map_err(|err| err.to_string())
    }

    /// Merge multiple manifests, preserving declaration order.
    pub fn merge(manifests: &[Self]) -> Self {
        let mut merged = Self::default();
        for manifest in manifests {
            if merged.library.is_none() {
                merged.library.clone_from(&manifest.library);
            }
            merged.primitives.extend(manifest.primitives.iter().cloned());
            merged.properties.extend(manifest.properties.iter().cloned());
            merged.actions.extend(manifest.actions.iter().cloned());
            merged.functions.extend(manifest.functions.iter().cloned());
            merged.services.extend(manifest.services.iter().cloned());
            merged.effects.extend(manifest.effects.iter().cloned());
        }
        merged
    }

    /// Apply this manifest to a symbol table so completions and diagnostics see it.
    pub fn apply_to(&self, table: &mut SymbolTable) {
        for primitive in &self.primitives {
            table.types.insert(primitive.type_name.clone());
        }
        for property in &self.properties {
            for actor_type in &property.actor_types {
                let properties = table.properties.entry(actor_type.clone()).or_default();
                if !properties.iter().any(|existing| existing == &property.name) {
                    properties.push(property.name.clone());
                }
                table
                    .property_types
                    .insert((actor_type.clone(), property.name.clone()), property.ty.clone());
            }
        }
        // Extension types also accept the shared common properties (at,
        // opacity, size, ...), which the schema registers per built-in actor
        // type. Merge those so the analyzer does not flag them as unknown.
        let common_specs = {
            let all = animatix_syntax::schema::property_specs();
            let common = animatix_syntax::schema::common_property_names();
            all.into_iter().filter(|spec| common.contains(&spec.name)).collect::<Vec<_>>()
        };
        for primitive in &self.primitives {
            let actor_type = primitive.type_name.clone();
            for spec in &common_specs {
                let properties = table.properties.entry(actor_type.clone()).or_default();
                if !properties.iter().any(|existing| existing == spec.name) {
                    properties.push(spec.name.to_string());
                }
                table
                    .property_types
                    .entry((actor_type.clone(), spec.name.to_string()))
                    .or_insert_with(|| spec.ty.clone());
            }
        }
        // Plugin-authored effects behave like built-in effects for the
        // analyzer: the type is known, the declared parameters (plus the
        // implicit `enabled`) complete and validate.
        for effect in &self.effects {
            table.types.insert(effect.name.clone());
            table.effect_names.insert(effect.name.clone());
            let properties = table.properties.entry(effect.name.clone()).or_default();
            for param in &effect.params {
                if !properties.iter().any(|existing| existing == &param.name) {
                    properties.push(param.name.clone());
                }
                table.property_types.insert(
                    (effect.name.clone(), param.name.clone()),
                    parse_manifest_type(&param.ty),
                );
            }
            if !properties.iter().any(|existing| existing == "enabled") {
                properties.push("enabled".to_string());
            }
            table
                .property_types
                .insert((effect.name.clone(), "enabled".to_string()), Type::Bool);
        }
        for action in &self.actions {
            table.actions.insert(action.name.clone());
        }
        for function in &self.functions {
            table.functions.insert(function.name.clone());
        }
        for service in &self.services {
            table.services.insert(service.name.clone());
        }
    }
}
