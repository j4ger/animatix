//! Raw manifest schema, canonical TOML wire shapes, and model conversion.

use animatix_core::callable::{
    CallableDescriptor, CallableParam, CallablePhase, CallableType, DefaultValue,
};
use animatix_syntax::schema::{
    ChildProcessingKind, PrimitiveCapabilities, PrimitiveCategory, PrimitiveDescriptor,
    PropertyDescriptor, PropertyValueKind, ServiceDescriptor,
};
use animatix_syntax::typing::Type;
use serde::{Deserialize, Deserializer, Serialize};

use super::manifest::{ExtensionManifest, ManifestEffect, ManifestEffectParam};

/// Manifest primitive metadata exposed as the shared schema descriptor.
pub type ManifestPrimitive = PrimitiveDescriptor;
/// Manifest property metadata exposed as the shared schema descriptor.
pub type ManifestProperty = PropertyDescriptor;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    #[serde(default)]
    library: Option<String>,
    #[serde(default)]
    primitives: Vec<RawPrimitive>,
    #[serde(default)]
    properties: Vec<RawProperty>,
    #[serde(default)]
    callables: Vec<RawCallable>,
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
#[serde(deny_unknown_fields)]
struct RawCallable {
    namespace: String,
    name: String,
    #[serde(default)]
    params: Vec<RawCallableParam>,
    return_type: String,
    phase: String,
    #[serde(default)]
    help: String,
    #[serde(default)]
    category: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCallableParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(default)]
    default: Option<toml::Value>,
    #[serde(default)]
    optional: bool,
    #[serde(default)]
    variadic: bool,
    #[serde(default)]
    help: String,
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
    callables: Vec<OutputCallable<'a>>,
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
struct OutputCallable<'a> {
    namespace: &'a str,
    name: &'a str,
    params: Vec<OutputCallableParam<'a>>,
    return_type: String,
    phase: &'static str,
    help: &'a str,
    category: &'a str,
}

#[derive(Serialize)]
struct OutputCallableParam<'a> {
    name: &'a str,
    #[serde(rename = "type")]
    ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<toml::Value>,
    #[serde(skip_serializing_if = "is_false")]
    optional: bool,
    #[serde(skip_serializing_if = "is_false")]
    variadic: bool,
    #[serde(skip_serializing_if = "str_is_empty")]
    help: &'a str,
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

fn str_is_empty(value: &str) -> bool {
    value.is_empty()
}

impl<'de> Deserialize<'de> for ExtensionManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::from_raw(RawManifest::deserialize(deserializer)?).map_err(serde::de::Error::custom)
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
            callables: self
                .callables
                .iter()
                .map(|callable| OutputCallable {
                    namespace: &callable.namespace,
                    name: &callable.name,
                    params: callable.params.iter().map(output_callable_param).collect(),
                    return_type: callable_type_name(&callable.return_type),
                    phase: callable_phase_name(callable.phase),
                    help: &callable.help,
                    category: &callable.category,
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

    fn from_raw(raw: RawManifest) -> Result<Self, String> {
        let callables =
            raw.callables.into_iter().map(raw_callable).collect::<Result<Vec<_>, _>>()?;
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
                        // The manifest schema has no transcript flag yet;
                        // parsed transcripts are a built-in (Audio/Captions)
                        // surface until a manifest field is designed.
                        transcript: false,
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

        Ok(Self {
            library: raw.library,
            primitives,
            properties,
            callables,
            services,
            effects,
        })
    }
}

fn raw_callable(raw: RawCallable) -> Result<CallableDescriptor, String> {
    if raw.namespace.split("::").next() == Some("std") {
        return Err("extension callables cannot claim the reserved `std` namespace tree".into());
    }
    let mut params = Vec::with_capacity(raw.params.len());
    for param in raw.params {
        let ty = parse_callable_type(&param.ty)?;
        let has_default = param.default.is_some();
        let default = param.default.map(|value| parse_callable_default(value, &ty)).transpose()?;
        params.push(CallableParam {
            name: param.name,
            ty,
            default,
            optional: param.optional || has_default,
            variadic: param.variadic,
            help: param.help,
        });
    }
    let descriptor = CallableDescriptor {
        namespace: raw.namespace,
        name: raw.name,
        params,
        return_type: parse_callable_type(&raw.return_type)?,
        phase: parse_callable_phase(&raw.phase)?,
        help: raw.help,
        category: raw.category,
    };
    descriptor.validate()?;
    Ok(descriptor)
}

fn parse_callable_default(value: toml::Value, ty: &CallableType) -> Result<DefaultValue, String> {
    match value {
        toml::Value::Integer(value) => Ok(if matches!(ty, CallableType::Duration) {
            DefaultValue::Duration(value as f64)
        } else {
            DefaultValue::Num(value as f64)
        }),
        toml::Value::Float(value) => Ok(if matches!(ty, CallableType::Duration) {
            DefaultValue::Duration(value)
        } else {
            DefaultValue::Num(value)
        }),
        toml::Value::Boolean(value) => Ok(DefaultValue::Bool(value)),
        toml::Value::String(value) => match ty {
            CallableType::Easing | CallableType::Enum(_) => Ok(DefaultValue::Ident(value)),
            CallableType::Duration => parse_duration_seconds(&value)
                .map(DefaultValue::Duration)
                .ok_or_else(|| format!("invalid duration default `{value}`")),
            _ => Ok(DefaultValue::Str(value)),
        },
        other => Err(format!("unsupported callable default value `{other}`")),
    }
}

fn parse_duration_seconds(value: &str) -> Option<f64> {
    let value = value.trim();
    let (number, scale) = if let Some(number) = value.strip_suffix("ms") {
        (number, 0.001)
    } else {
        (value.strip_suffix('s')?, 1.0)
    };
    number.trim().parse::<f64>().ok().map(|value| value * scale)
}

fn parse_callable_phase(value: &str) -> Result<CallablePhase, String> {
    match value {
        "Build" | "build" => Ok(CallablePhase::Build),
        "BuildAndFrame" | "build_and_frame" => Ok(CallablePhase::BuildAndFrame),
        _ => Err(format!("unknown callable phase `{value}`")),
    }
}

fn parse_callable_type(value: &str) -> Result<CallableType, String> {
    let value = value.trim();
    let basic = match value {
        "Any" => Some(CallableType::Any),
        "Num" => Some(CallableType::Num),
        "Bool" => Some(CallableType::Bool),
        "Str" => Some(CallableType::Str),
        "Vec2" => Some(CallableType::Vec2),
        "Vec3" => Some(CallableType::Vec3),
        "Vec4" => Some(CallableType::Vec4),
        "Color" => Some(CallableType::Color),
        "Duration" => Some(CallableType::Duration),
        "Timeline" => Some(CallableType::Timeline),
        "Unit" => Some(CallableType::Unit),
        "Actor" => Some(CallableType::Actor),
        "Scene" => Some(CallableType::Scene),
        "Easing" => Some(CallableType::Easing),
        _ => None,
    };
    if let Some(basic) = basic {
        return Ok(basic);
    }
    if let Some(inner) = type_inner(value, "List") {
        return Ok(CallableType::List(Box::new(parse_callable_type(inner)?)));
    }
    if let Some(inner) = type_inner(value, "Enum") {
        return Ok(CallableType::Enum(
            split_type_parts(inner).into_iter().map(str::to_owned).collect(),
        ));
    }
    if let Some(inner) = type_inner(value, "Tuple") {
        return Ok(CallableType::Tuple(
            split_type_parts(inner)
                .into_iter()
                .map(parse_callable_type)
                .collect::<Result<Vec<_>, _>>()?,
        ));
    }
    if let Some(signature) = value.strip_prefix("Fn(")
        && let Some((params, result)) = signature.split_once(") => ")
    {
        let params = split_type_parts(params)
            .into_iter()
            .enumerate()
            .map(|(index, ty)| Ok((format!("arg{index}"), parse_callable_type(ty)?)))
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(CallableType::Function {
            params,
            ret: Box::new(parse_callable_type(result)?),
        });
    }
    let union = split_type_parts_with(value, '|');
    if union.len() > 1 {
        return Ok(CallableType::Union(
            union.into_iter().map(parse_callable_type).collect::<Result<Vec<_>, _>>()?,
        ));
    }
    Err(format!("unknown callable type `{value}`"))
}

fn type_inner<'a>(value: &'a str, constructor: &str) -> Option<&'a str> {
    let rest = value.strip_prefix(constructor)?.trim();
    let inner = rest
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .or_else(|| rest.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')))
        .or_else(|| rest.strip_prefix('(').and_then(|rest| rest.strip_suffix(')')))?;
    Some(inner.trim())
}

fn split_type_parts(value: &str) -> Vec<&str> {
    split_type_parts_with(value, ',')
}

fn split_type_parts_with(value: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    for (index, ch) in value.char_indices() {
        if matches!(ch, '<' | '[' | '(') {
            depth += 1;
        } else if matches!(ch, '>' | ']' | ')') {
            depth -= 1;
        } else if ch == separator && depth == 0 {
            let part = value[start..index].trim();
            if !part.is_empty() {
                parts.push(part);
            }
            start = index + ch.len_utf8();
        }
    }
    let last = value[start..].trim();
    if !last.is_empty() {
        parts.push(last);
    }
    parts
}

fn callable_type_name(ty: &CallableType) -> String {
    match ty {
        CallableType::Any => "Any".into(),
        CallableType::Num => "Num".into(),
        CallableType::Bool => "Bool".into(),
        CallableType::Str => "Str".into(),
        CallableType::Vec2 => "Vec2".into(),
        CallableType::Vec3 => "Vec3".into(),
        CallableType::Vec4 => "Vec4".into(),
        CallableType::Color => "Color".into(),
        CallableType::Duration => "Duration".into(),
        CallableType::Timeline => "Timeline".into(),
        CallableType::Unit => "Unit".into(),
        CallableType::Actor => "Actor".into(),
        CallableType::Scene => "Scene".into(),
        CallableType::Easing => "Easing".into(),
        CallableType::List(inner) => format!("List[{}]", callable_type_name(inner)),
        CallableType::Tuple(items) => format!(
            "Tuple({})",
            items.iter().map(callable_type_name).collect::<Vec<_>>().join(", ")
        ),
        CallableType::Union(items) => {
            items.iter().map(callable_type_name).collect::<Vec<_>>().join(" | ")
        },
        CallableType::Enum(items) => format!("Enum({})", items.join(", ")),
        CallableType::Function { params, ret } => format!(
            "Fn({}) => {}",
            params
                .iter()
                .map(|(_, ty)| callable_type_name(ty))
                .collect::<Vec<_>>()
                .join(", "),
            callable_type_name(ret)
        ),
    }
}

fn callable_phase_name(phase: CallablePhase) -> &'static str {
    match phase {
        CallablePhase::Build => "Build",
        CallablePhase::BuildAndFrame => "BuildAndFrame",
    }
}

fn output_callable_param(param: &CallableParam) -> OutputCallableParam<'_> {
    OutputCallableParam {
        name: &param.name,
        ty: callable_type_name(&param.ty),
        default: param.default.as_ref().map(output_callable_default),
        optional: param.optional,
        variadic: param.variadic,
        help: &param.help,
    }
}

fn output_callable_default(default: &DefaultValue) -> toml::Value {
    match default {
        DefaultValue::Num(value) | DefaultValue::Duration(value) => toml::Value::Float(*value),
        DefaultValue::Bool(value) => toml::Value::Boolean(*value),
        DefaultValue::Str(value) | DefaultValue::Ident(value) => toml::Value::String(value.clone()),
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
