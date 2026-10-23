use animatix_syntax::schema::{
    ActionParam, ActionSignature, ChildProcessingKind, FunctionDescriptor, PrimitiveCapabilities,
    PrimitiveCategory, PrimitiveSpec, PropertyDescriptor, PropertyValueKind, ServiceDescriptor,
};
use animatix_syntax::typing::Type;

use super::*;
use crate::SymbolTable;

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
    assert!(table.properties.get("Gauge").is_some_and(|props| {
        props.contains(&"level".to_string()) && props.contains(&"at".to_string())
    }));
    assert_eq!(
        table.property_types.get(&("Gauge".to_string(), "level".to_string())),
        Some(&Type::Num)
    );
}

#[test]
fn applies_action_function_and_service_names() {
    let manifest = ExtensionManifest::from_toml(
        r#"
[[actions]]
name = "pulse"
category = "Native"
description = "Demo action"

[[functions]]
name = "double"

[[services]]
name = "demo.pulse"
"#,
    )
    .expect("parse manifest");

    let mut table = SymbolTable::default();
    manifest.apply_to(&mut table);
    assert!(table.actions.contains("pulse"));
    assert!(table.functions.contains("double"));
    assert!(table.services.contains("demo.pulse"));
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
fn manifest_parses_enum_property_type_and_roundtrips() {
    let manifest = ExtensionManifest::from_toml(
        r#"
[[properties]]
actor_type = "Gauge"
name = "direction"
type = "Enum(left, right, top)"
"#,
    )
    .expect("parse manifest");
    assert_eq!(
        manifest.properties[0].ty,
        Type::Enum(vec!["left".to_string(), "right".to_string(), "top".to_string()])
    );
    let toml = manifest.to_toml().expect("serialize manifest");
    assert!(toml.contains("Enum(left, right, top)"));
    let parsed = ExtensionManifest::from_toml(&toml).expect("parse roundtrip");
    assert_eq!(parsed, manifest);
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
        doc: "Pulse animation primitive.".to_string(),
        default_stub: None,
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

    let action = ActionSignature {
        name: "pulse".to_string(),
        category: "Native".to_string(),
        description: "Demo pulse action".to_string(),
        params: vec![ActionParam {
            name: "target".to_string(),
            description: "Target actor".to_string(),
            type_info: "Str".to_string(),
        }],
        modifiers: vec![],
    };
    let function = FunctionDescriptor {
        name: "double".to_string(),
        params: vec![ActionParam {
            name: "value".to_string(),
            description: "Input value".to_string(),
            type_info: "Num".to_string(),
        }],
        return_type: Some(Type::Num),
        help: Some("Doubles a value".to_string()),
    };
    let service = ServiceDescriptor {
        name: "demo.pulse".to_string(),
        type_info: Some("usize".to_string()),
        help: None,
    };

    let manifest = ExtensionManifest::from_runtime(
        Some("libdemo.so".to_string()),
        &[primitive],
        &[property],
        &[action],
        &[function],
        &[service],
        &[],
    );
    let toml = manifest.to_toml().expect("serialize manifest");
    assert!(toml.contains("icon_id = \"extension:pulse\""));
    assert!(toml.contains("type = \"U32\""));
    assert!(toml.contains("child_processing = \"Filter\""));
    assert!(toml.contains("[[actions]]"));
    assert!(toml.contains("[[functions]]"));
    assert!(toml.contains("[[services]]"));

    let parsed = ExtensionManifest::from_toml(&toml).expect("parse manifest");
    assert_eq!(parsed, manifest);
    assert_eq!(parsed.properties[0].id, None);
}
