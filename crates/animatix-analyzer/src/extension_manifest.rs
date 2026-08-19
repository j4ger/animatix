//! Extension manifests for analyzer-only language intelligence.
//!
//! These manifests describe external primitives and properties without loading
//! native runtime plugins. Parsed manifests use the shared schema descriptor
//! shapes, but their property `id` is intentionally `None`: runtime ids are
//! allocated by `ExtensionRegistry`, never guessed by a manifest.

use animatix_syntax::schema::{
    ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveDescriptor,
    PrimitiveSpec, PropertyDescriptor, PropertyValueKind,
};
use animatix_syntax::typing::Type;
use serde::{Deserialize, Deserializer, Serialize};

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
    #[serde(default)]
    properties: Vec<String>,
    #[serde(default)]
    text_paths: bool,
    #[serde(default)]
    vector_paths: bool,
    #[serde(default)]
    image_payload: bool,
    #[serde(default)]
    layout_container: bool,
    #[serde(default)]
    morphable_paths: bool,
    #[serde(default)]
    vector_reveal_target: bool,
    #[serde(default)]
    plot_geometry: bool,
    #[serde(default)]
    is_container: bool,
    #[serde(default)]
    is_shape: bool,
}

#[derive(Deserialize)]
struct RawProperty {
    actor_type: String,
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    injectable: bool,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    help: Option<String>,
}

#[derive(Serialize)]
struct OutputManifest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    library: Option<&'a str>,
    primitives: Vec<OutputPrimitive<'a>>,
    properties: Vec<OutputProperty<'a>>,
}

#[derive(Serialize)]
struct OutputPrimitive<'a> {
    type_name: &'a str,
    display_name: &'a str,
    category: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_id: Option<&'a str>,
    #[serde(skip_serializing_if = "is_false")]
    advanced: bool,
    #[serde(skip_serializing_if = "is_false")]
    text_paths: bool,
    #[serde(skip_serializing_if = "is_false")]
    vector_paths: bool,
    #[serde(skip_serializing_if = "is_false")]
    image_payload: bool,
    #[serde(skip_serializing_if = "is_false")]
    layout_container: bool,
    #[serde(skip_serializing_if = "is_false")]
    morphable_paths: bool,
    #[serde(skip_serializing_if = "is_false")]
    vector_reveal_target: bool,
    #[serde(skip_serializing_if = "is_false")]
    plot_geometry: bool,
    #[serde(skip_serializing_if = "is_false")]
    is_container: bool,
    #[serde(skip_serializing_if = "is_false")]
    is_shape: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    child_processing: Option<&'static str>,
    properties: Vec<&'a str>,
}

#[derive(Serialize)]
struct OutputProperty<'a> {
    actor_type: &'a str,
    name: &'a str,
    #[serde(rename = "type")]
    ty: &'static str,
    #[serde(skip_serializing_if = "is_false")]
    injectable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<&'a str>,
}

fn is_false(value: &bool) -> bool {
    !*value
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

    /// Build a manifest from runtime descriptors, stripping runtime property ids.
    ///
    /// This is the single-source path for generating analyzer metadata from a
    /// loaded native plugin or in-process extension context.
    pub fn from_runtime(
        library: Option<String>,
        primitives: &[PrimitiveSpec],
        properties: &[PropertyDescriptor],
    ) -> Self {
        Self {
            library,
            primitives: primitives
                .iter()
                .map(|spec| {
                    let declared = properties
                        .iter()
                        .filter(|property| property.actor_types.iter().any(|ty| ty == &spec.type_name))
                        .map(|property| property.name.clone())
                        .collect();
                    PrimitiveDescriptor {
                        type_name: spec.type_name.clone(),
                        display_name: spec.display_name.clone(),
                        category: spec.category,
                        icon_id: spec.icon_id.clone(),
                        advanced: spec.advanced,
                        capabilities: spec.capabilities,
                        child_processing: spec.child_processing,
                        properties: declared,
                    }
                })
                .collect(),
            properties: properties
                .iter()
                .map(|property| PropertyDescriptor {
                    id: None,
                    name: property.name.clone(),
                    actor_types: property.actor_types.clone(),
                    ty: property.ty.clone(),
                    value_kind: property.value_kind,
                    injectable: property.injectable,
                    display_name: property.display_name.clone(),
                    group: property.group.clone(),
                    help: property.help.clone(),
                })
                .collect(),
        }
    }

    /// Serialize this manifest as canonical TOML.
    pub fn to_toml(&self) -> Result<String, String> {
        let output = OutputManifest {
            library: self.library.as_deref(),
            primitives: self
                .primitives
                .iter()
                .map(|primitive| OutputPrimitive {
                    type_name: &primitive.type_name,
                    display_name: &primitive.display_name,
                    category: primitive.category.label(),
                    icon_id: (!primitive.icon_id.is_empty()).then_some(primitive.icon_id.as_str()),
                    advanced: primitive.advanced,
                    text_paths: primitive.capabilities.text_paths,
                    vector_paths: primitive.capabilities.vector_paths,
                    image_payload: primitive.capabilities.image_payload,
                    layout_container: primitive.capabilities.layout_container,
                    morphable_paths: primitive.capabilities.morphable_paths,
                    vector_reveal_target: primitive.capabilities.vector_reveal_target,
                    plot_geometry: primitive.capabilities.plot_geometry,
                    is_container: primitive.capabilities.is_container,
                    is_shape: primitive.capabilities.is_shape,
                    child_processing: output_child_processing(primitive.child_processing),
                    properties: primitive
                        .properties
                        .iter()
                        .map(String::as_str)
                        .collect(),
                })
                .collect(),
            properties: self
                .properties
                .iter()
                .flat_map(|property| {
                    property.actor_types.iter().map(move |actor_type| OutputProperty {
                        actor_type,
                        name: &property.name,
                        ty: output_property_type(property.value_kind),
                        injectable: property.injectable,
                        display_name: property.display_name.as_deref(),
                        group: property.group.as_deref(),
                        help: property.help.as_deref(),
                    })
                })
                .collect(),
        };
        toml::to_string(&output).map_err(|err| err.to_string())
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
                display_name: property.display_name,
                group: property.group,
                help: property.help,
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
                    capabilities: PrimitiveCapabilities {
                        text_paths: primitive.text_paths,
                        vector_paths: primitive.vector_paths,
                        image_payload: primitive.image_payload,
                        layout_container: primitive.layout_container,
                        morphable_paths: primitive.morphable_paths,
                        vector_reveal_target: primitive.vector_reveal_target,
                        plot_geometry: primitive.plot_geometry,
                        is_container: primitive.is_container,
                        is_shape: primitive.is_shape,
                    },
                    child_processing: primitive
                        .child_processing
                        .as_deref()
                        .and_then(parse_child_processing)
                        .unwrap_or_default(),
                    properties: primitive.properties,
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
        "U32" => PropertyValueKind::U32,
        "Bool" => PropertyValueKind::Bool,
        "Str" | "String" => PropertyValueKind::String,
        "Vec2" => PropertyValueKind::Vec2,
        "Vec4" | "Color" => PropertyValueKind::Vec4,
        "List<Vec2>" => PropertyValueKind::PointList,
        _ => PropertyValueKind::Generic,
    }
}

fn parse_manifest_type(ty: &str) -> Type {
    match ty.trim() {
        "Num" | "U32" => Type::Num,
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

fn output_property_type(kind: PropertyValueKind) -> &'static str {
    match kind {
        PropertyValueKind::F32 => "Num",
        PropertyValueKind::U32 => "U32",
        PropertyValueKind::Bool => "Bool",
        PropertyValueKind::Vec2 => "Vec2",
        PropertyValueKind::Vec4 => "Vec4",
        PropertyValueKind::String => "Str",
        PropertyValueKind::PointList => "List<Vec2>",
        PropertyValueKind::Generic => "Any",
    }
}

fn output_child_processing(kind: ChildProcessingKind) -> Option<&'static str> {
    match kind {
        ChildProcessingKind::Generic => None,
        ChildProcessingKind::Filter => Some("Filter"),
        ChildProcessingKind::Mask => Some("Mask"),
        ChildProcessingKind::Equation => Some("Equation"),
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

    #[test]
    fn runtime_manifest_roundtrips_through_toml() {
        let primitive = PrimitiveSpec {
            type_name: "Pulse".to_string(),
            display_name: "Pulse".to_string(),
            category: PrimitiveCategory::Shape,
            icon_id: "extension:pulse".to_string(),
            advanced: false,
            capabilities: PrimitiveCapabilities {
                vector_paths: true,
                is_shape: true,
                ..PrimitiveCapabilities::default()
            },
            child_processing: ChildProcessingKind::Filter,
        };
        let property = PropertyDescriptor {
            id: Some(animatix_syntax::schema::PropertyId(1_000_000)),
            name: "glow".to_string(),
            actor_types: vec!["Pulse".to_string()],
            ty: Type::Num,
            value_kind: PropertyValueKind::U32,
            injectable: true,
            display_name: Some("Glow".to_string()),
            group: Some("Pulse".to_string()),
            help: Some("Pulse radius glow amount".to_string()),
        };

        let manifest = ExtensionManifest::from_runtime(
            Some("libdemo.so".to_string()),
            &[primitive],
            &[property],
        );
        let toml = manifest.to_toml().expect("serialize manifest");
        assert!(toml.contains("icon_id = \"extension:pulse\""));
        assert!(toml.contains("type = \"U32\""));
        assert!(toml.contains("child_processing = \"Filter\""));

        let parsed = ExtensionManifest::from_toml(&toml).expect("parse manifest");
        assert_eq!(parsed, manifest);
        assert_eq!(parsed.properties[0].id, None);
    }
}
