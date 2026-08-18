//! Extension manifests for analyzer-only language intelligence.
//!
//! These manifests describe external primitives and properties without loading
//! native runtime plugins. Parsed manifests use the shared schema descriptor
//! shapes, but their property `id` is intentionally `None`: runtime ids are
//! allocated by `ExtensionRegistry`, never guessed by a manifest.

use animatix_syntax::schema::{
    ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveDescriptor,
    PropertyDescriptor, PropertyValueKind,
};
use animatix_syntax::typing::Type;
use serde::{Deserialize, Deserializer};

use crate::symbol_table::SymbolTable;

/// Manifest primitive metadata exposed as the shared schema descriptor.
pub type ManifestPrimitive = PrimitiveDescriptor;
/// Manifest property metadata exposed as the shared schema descriptor.
pub type ManifestProperty = PropertyDescriptor;

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
}

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    library: Option<String>,
    #[serde(default)]
    primitives: Vec<RawPrimitive>,
    #[serde(default)]
    properties: Vec<RawProperty>,
}

#[derive(Deserialize)]
struct RawPrimitive {
    type_name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    icon_id: Option<String>,
    #[serde(default)]
    advanced: bool,
    #[serde(default)]
    child_processing: Option<String>,
}

#[derive(Deserialize)]
struct RawProperty {
    actor_type: String,
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    injectable: bool,
}

impl<'de> Deserialize<'de> for ExtensionManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from_raw(RawManifest::deserialize(deserializer)?))
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
    }

    fn from_raw(raw: RawManifest) -> Self {
        let properties = raw
            .properties
            .into_iter()
            .map(|property| PropertyDescriptor {
                id: None,
                name: property.name,
                actor_types: vec![property.actor_type],
                ty: parse_manifest_type(&property.ty),
                value_kind: manifest_value_kind(&property.ty),
                injectable: property.injectable,
            })
            .collect::<Vec<_>>();

        let primitives = raw
            .primitives
            .into_iter()
            .map(|primitive| {
                let type_name = primitive.type_name;
                let display_name = primitive.display_name.unwrap_or_else(|| type_name.clone());
                let category = primitive
                    .category
                    .as_deref()
                    .and_then(parse_primitive_category)
                    .unwrap_or(PrimitiveCategory::Shape);
                PrimitiveDescriptor {
                    type_name,
                    display_name,
                    category,
                    icon_id: primitive.icon_id.unwrap_or_default(),
                    advanced: primitive.advanced,
                    capabilities: PrimitiveCapabilities::default(),
                    child_processing: primitive
                        .child_processing
                        .as_deref()
                        .and_then(parse_child_processing)
                        .unwrap_or_default(),
                    properties: Vec::new(),
                }
            })
            .collect();

        Self {
            library: raw.library,
            primitives,
            properties,
        }
    }
}

fn parse_primitive_category(value: &str) -> Option<PrimitiveCategory> {
    match value.trim() {
        "Shape" | "Shapes" => Some(PrimitiveCategory::Shape),
        "Text" => Some(PrimitiveCategory::Text),
        "Media" => Some(PrimitiveCategory::Media),
        "Plot" | "Plots" => Some(PrimitiveCategory::Plot),
        "Container" | "Containers" => Some(PrimitiveCategory::Container),
        "Annotation" | "Annotations" => Some(PrimitiveCategory::Annotation),
        _ => None,
    }
}

fn parse_child_processing(value: &str) -> Option<ChildProcessingKind> {
    match value.trim() {
        "Generic" => Some(ChildProcessingKind::Generic),
        "Filter" => Some(ChildProcessingKind::Filter),
        "Mask" => Some(ChildProcessingKind::Mask),
        "Equation" => Some(ChildProcessingKind::Equation),
        _ => None,
    }
}

fn manifest_value_kind(ty: &str) -> PropertyValueKind {
    match ty.trim() {
        "Num" => PropertyValueKind::F32,
        "Str" | "String" => PropertyValueKind::String,
        "Vec2" => PropertyValueKind::Vec2,
        "Vec4" | "Color" => PropertyValueKind::Vec4,
        "List<Vec2>" => PropertyValueKind::PointList,
        _ => PropertyValueKind::Generic,
    }
}

fn parse_manifest_type(ty: &str) -> Type {
    match ty.trim() {
        "Num" => Type::Num,
        "Str" | "String" => Type::Str,
        "Bool" => Type::Bool,
        "Vec2" => Type::Vec2,
        "Vec3" => Type::Vec3,
        "Vec4" => Type::Vec4,
        "Color" => Type::Color,
        "Any" => Type::Any,
        "List<Vec2>" => Type::List(Box::new(Type::Vec2)),
        _ => Type::Any,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_applies_to_symbols() {
        let manifest = ExtensionManifest::from_toml(
            r#"
[[primitives]]
type_name = "Gauge"

[[properties]]
actor_type = "Gauge"
name = "level"
type = "Num"
"#,
        )
        .expect("parse manifest");

        let mut table = SymbolTable::default();
        manifest.apply_to(&mut table);
        assert!(table.types.contains("Gauge"));
        assert_eq!(table.properties.get("Gauge"), Some(&vec!["level".to_string()]));
        assert_eq!(
            table.property_types.get(&("Gauge".to_string(), "level".to_string())),
            Some(&Type::Num)
        );
    }

    #[test]
    fn manifest_uses_shared_primitive_and_property_descriptors() {
        let manifest = ExtensionManifest::from_toml(
            r#"
[[primitives]]
type_name = "Gauge"
display_name = "Gauge Dial"
category = "Plot"
icon_id = "gauge"
advanced = true
child_processing = "Filter"

[[properties]]
actor_type = "Gauge"
name = "level"
type = "Num"
injectable = true
"#,
        )
        .expect("parse manifest");

        let primitive = &manifest.primitives[0];
        assert_eq!(primitive.type_name, "Gauge");
        assert_eq!(primitive.display_name, "Gauge Dial");
        assert_eq!(primitive.category, PrimitiveCategory::Plot);
        assert_eq!(primitive.icon_id, "gauge");
        assert!(primitive.advanced);
        assert_eq!(primitive.child_processing, ChildProcessingKind::Filter);
        assert!(primitive.properties.is_empty(), "manifests must not guess runtime ids");

        let property = &manifest.properties[0];
        assert_eq!(property.id, None);
        assert_eq!(property.actor_types, vec!["Gauge".to_string()]);
        assert_eq!(property.ty, Type::Num);
        assert_eq!(property.value_kind, PropertyValueKind::F32);
        assert!(property.injectable);
    }

    #[test]
    fn merge_preserves_order_and_duplicates() {
        let a = ExtensionManifest::from_toml("[[primitives]]\ntype_name = \"Gauge\"\n").unwrap();
        let b = ExtensionManifest::from_toml("[[primitives]]\ntype_name = \"Dial\"\n").unwrap();
        let merged = ExtensionManifest::merge(&[a, b]);
        assert_eq!(merged.primitives.len(), 2);
        assert_eq!(merged.primitives[0].type_name, "Gauge");
        assert_eq!(merged.primitives[1].type_name, "Dial");
    }
}
