//! Extension manifests for analyzer-only language intelligence.
//!
//! These manifests describe external primitives and properties without loading
//! native runtime plugins. The LSP and GUI can consume them without depending
//! on the runtime crate.

use animatix_syntax::typing::Type;
use serde::Deserialize;

use crate::symbol_table::SymbolTable;

/// A primitive declared by an extension manifest.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct ManifestPrimitive {
    /// Source text type name, e.g. `Gauge`.
    pub type_name: String,
    /// Optional display name for hover/palette metadata.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Optional UI category label.
    #[serde(default)]
    pub category: Option<String>,
    /// Optional opaque icon id.
    #[serde(default)]
    pub icon_id: Option<String>,
    /// Whether the primitive should be shown in an advanced menu.
    #[serde(default)]
    pub advanced: bool,
}

/// A property declared by an extension manifest.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ManifestProperty {
    /// Actor type that owns this property.
    pub actor_type: String,
    /// Canonical source property name.
    pub name: String,
    /// Type annotation used by the type checker, e.g. `Num`, `Str`, `Color`.
    #[serde(rename = "type")]
    pub ty: String,
}

/// Collection of analyzer-only extension metadata.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct ExtensionManifest {
    /// Optional native library path, resolved relative to the manifest file by
    /// CLI consumers. Analyzer and LSP ignore this field.
    #[serde(default)]
    pub library: Option<String>,
    /// Primitive declarations.
    #[serde(default)]
    pub primitives: Vec<ManifestPrimitive>,
    /// Property declarations.
    #[serde(default)]
    pub properties: Vec<ManifestProperty>,
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
            let property_type = parse_manifest_type(&property.ty);
            let properties = table.properties.entry(property.actor_type.clone()).or_default();
            if !properties.iter().any(|existing| existing == &property.name) {
                properties.push(property.name.clone());
            }
            table
                .property_types
                .insert((property.actor_type.clone(), property.name.clone()), property_type);
        }
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
    fn merge_preserves_order_and_duplicates() {
        let a = ExtensionManifest::from_toml("[[primitives]]\ntype_name = \"Gauge\"\n").unwrap();
        let b = ExtensionManifest::from_toml("[[primitives]]\ntype_name = \"Dial\"\n").unwrap();
        let merged = ExtensionManifest::merge(&[a, b]);
        assert_eq!(merged.primitives.len(), 2);
        assert_eq!(merged.primitives[0].type_name, "Gauge");
        assert_eq!(merged.primitives[1].type_name, "Dial");
    }
}
