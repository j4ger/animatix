//! Raw manifest schema, canonical TOML wire shapes, and model conversion.

use animatix_syntax::schema::{
    ActionParam, ActionSignature, ChildProcessingKind, FunctionDescriptor, PrimitiveCapabilities,
    PrimitiveCategory, PrimitiveDescriptor, PropertyDescriptor, PropertyValueKind,
    ServiceDescriptor,
};
use animatix_syntax::typing::Type;
use serde::{Deserialize, Deserializer, Serialize};

use super::manifest::{ExtensionManifest, ManifestEffect, ManifestEffectParam};

/// Manifest primitive metadata exposed as the shared schema descriptor.
pub type ManifestPrimitive = PrimitiveDescriptor;
/// Manifest property metadata exposed as the shared schema descriptor.
pub type ManifestProperty = PropertyDescriptor;

#[derive(Deserialize)]
struct RawManifest {
    #[serde(default)]
    library: Option<String>,
    #[serde(default)]
    primitives: Vec<RawPrimitive>,
    #[serde(default)]
    properties: Vec<RawProperty>,
    #[serde(default)]
    actions: Vec<RawAction>,
    #[serde(default)]
    functions: Vec<RawFunction>,
    #[serde(default)]
    services: Vec<RawService>,
    #[serde(default)]
    effects: Vec<RawEffect>,
}

#[derive(Deserialize)]
struct RawEffect {
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    params: Vec<RawEffectParam>,
}

#[derive(Deserialize)]
struct RawEffectParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
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
    plot_geometry: bool,
    #[serde(default)]
    plot_host: bool,
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

#[derive(Deserialize)]
struct RawAction {
    name: String,
    category: String,
    description: String,
    #[serde(default)]
    params: Vec<RawActionParam>,
    #[serde(default)]
    modifiers: Vec<RawActionParam>,
}

#[derive(Deserialize)]
struct RawActionParam {
    name: String,
    description: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Deserialize)]
struct RawFunction {
    name: String,
    #[serde(default)]
    params: Vec<RawActionParam>,
    #[serde(default)]
    return_type: Option<String>,
    #[serde(default)]
    help: Option<String>,
}

#[derive(Deserialize)]
struct RawService {
    name: String,
    #[serde(rename = "type")]
    ty: Option<String>,
    #[serde(default)]
    help: Option<String>,
}

#[derive(Serialize)]
struct OutputManifest<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    library: Option<&'a str>,
    primitives: Vec<OutputPrimitive<'a>>,
    properties: Vec<OutputProperty<'a>>,
    actions: Vec<OutputAction<'a>>,
    functions: Vec<OutputFunction<'a>>,
    services: Vec<OutputService<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    effects: Vec<OutputEffect<'a>>,
}

#[derive(Serialize)]
struct OutputEffect<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    params: Vec<OutputEffectParam<'a>>,
}

#[derive(Serialize)]
struct OutputEffectParam<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    ty: &'a str,
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
    plot_geometry: bool,
    #[serde(skip_serializing_if = "is_false")]
    plot_host: bool,
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
    ty: String,
    #[serde(skip_serializing_if = "is_false")]
    injectable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<&'a str>,
}

#[derive(Serialize)]
struct OutputAction<'a> {
    name: &'a str,
    category: &'a str,
    description: &'a str,
    params: Vec<OutputActionParam<'a>>,
    modifiers: Vec<OutputActionParam<'a>>,
}

#[derive(Serialize)]
struct OutputActionParam<'a> {
    name: &'a str,
    description: &'a str,
    #[serde(rename = "type")]
    ty: &'a str,
}

#[derive(Serialize)]
struct OutputFunction<'a> {
    name: &'a str,
    params: Vec<OutputActionParam<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    help: Option<&'a str>,
}

#[derive(Serialize)]
struct OutputService<'a> {
    name: &'a str,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    ty: Option<&'a str>,
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
                    plot_geometry: primitive.capabilities.plot_geometry,
                    plot_host: primitive.capabilities.plot_host,
                    is_container: primitive.capabilities.is_container,
                    is_shape: primitive.capabilities.is_shape,
                    child_processing: output_child_processing(primitive.child_processing),
                    properties: primitive.properties.iter().map(String::as_str).collect(),
                })
                .collect(),
            properties: self
                .properties
                .iter()
                .flat_map(|property| {
                    property.actor_types.iter().map(move |actor_type| OutputProperty {
                        actor_type,
                        name: &property.name,
                        ty: match &property.ty {
                            animatix_syntax::typing::Type::Enum(_) => property.ty.to_string(),
                            _ => output_property_type(property.value_kind).to_string(),
                        },
                        injectable: property.injectable,
                        display_name: property.display_name.as_deref(),
                        group: property.group.as_deref(),
                        help: property.help.as_deref(),
                    })
                })
                .collect(),
            actions: self
                .actions
                .iter()
                .map(|action| OutputAction {
                    name: &action.name,
                    category: &action.category,
                    description: &action.description,
                    params: action.params.iter().map(output_action_param).collect(),
                    modifiers: action.modifiers.iter().map(output_action_param).collect(),
                })
                .collect(),
            functions: self
                .functions
                .iter()
                .map(|function| OutputFunction {
                    name: &function.name,
                    params: function.params.iter().map(output_action_param).collect(),
                    return_type: function.return_type.as_ref().map(ToString::to_string),
                    help: function.help.as_deref(),
                })
                .collect(),
            services: self
                .services
                .iter()
                .map(|service| OutputService {
                    name: &service.name,
                    ty: service.type_info.as_deref(),
                    help: service.help.as_deref(),
                })
                .collect(),
            effects: self
                .effects
                .iter()
                .map(|effect| OutputEffect {
                    name: &effect.name,
                    display_name: effect.display_name.as_deref(),
                    params: effect
                        .params
                        .iter()
                        .map(|param| OutputEffectParam {
                            name: &param.name,
                            ty: &param.ty,
                        })
                        .collect(),
                })
                .collect(),
        };
        toml::to_string(&output).map_err(|err| err.to_string())
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
                        plot_geometry: primitive.plot_geometry,
                        plot_host: primitive.plot_host,
                        is_container: primitive.is_container,
                        is_shape: primitive.is_shape,
                        has_visual_content: !primitive.is_container
                            && (primitive.is_shape
                                || primitive.text_paths
                                || primitive.image_payload
                                || primitive.vector_paths
                                || primitive.plot_geometry
                                || primitive.plot_host
                                || matches!(
                                    category,
                                    PrimitiveCategory::Shape
                                        | PrimitiveCategory::Text
                                        | PrimitiveCategory::Plot
                                        | PrimitiveCategory::Annotation
                                )),
                        stroke_primary: false,
                        callout: false,
                        legend_host: false,
                        time_dependent: false,
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

        let actions = raw
            .actions
            .into_iter()
            .map(|action| ActionSignature {
                name: action.name,
                category: action.category,
                description: action.description,
                params: action.params.into_iter().map(raw_action_param).collect(),
                modifiers: action.modifiers.into_iter().map(raw_action_param).collect(),
            })
            .collect::<Vec<_>>();

        let functions = raw
            .functions
            .into_iter()
            .map(|function| FunctionDescriptor {
                name: function.name,
                params: function.params.into_iter().map(raw_action_param).collect(),
                return_type: function.return_type.as_deref().map(parse_manifest_type),
                help: function.help,
            })
            .collect::<Vec<_>>();

        let services = raw
            .services
            .into_iter()
            .map(|service| ServiceDescriptor {
                name: service.name,
                type_info: service.ty,
                help: service.help,
            })
            .collect::<Vec<_>>();

        let effects = raw
            .effects
            .into_iter()
            .map(|effect| ManifestEffect {
                name: effect.name,
                display_name: effect.display_name,
                params: effect
                    .params
                    .into_iter()
                    .map(|param| ManifestEffectParam {
                        name: param.name,
                        ty: param.ty,
                    })
                    .collect(),
            })
            .collect();

        Self {
            library: raw.library,
            primitives,
            properties,
            actions,
            functions,
            services,
            effects,
        }
    }
}

fn raw_action_param(param: RawActionParam) -> ActionParam {
    ActionParam {
        name: param.name,
        description: param.description,
        type_info: param.ty,
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
        "Glass" => Some(ChildProcessingKind::Glass),
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

/// Parse a manifest type annotation, falling back to [`Type::Any`] for unknown
/// strings (manifests are lenient). The grammar itself lives in the shared
/// [`Type::parse`]; this only provides the lenient fallback.
pub(super) fn parse_manifest_type(ty: &str) -> Type {
    Type::parse(ty).unwrap_or(Type::Any)
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

fn output_action_param(param: &ActionParam) -> OutputActionParam<'_> {
    OutputActionParam {
        name: &param.name,
        description: &param.description,
        ty: &param.type_info,
    }
}

fn output_child_processing(kind: ChildProcessingKind) -> Option<&'static str> {
    match kind {
        ChildProcessingKind::Generic => None,
        ChildProcessingKind::Filter => Some("Filter"),
        ChildProcessingKind::Glass => Some("Glass"),
        ChildProcessingKind::Mask => Some("Mask"),
        ChildProcessingKind::Equation => Some("Equation"),
        ChildProcessingKind::Captions => Some("Captions"),
    }
}
