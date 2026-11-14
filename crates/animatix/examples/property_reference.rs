//! Generate or check the property tables in `docs/properties.md`.
//!
//! Run with `cargo run -p animatix --example property_reference -- --write`
//! after changing a property contract, or `--check` in the docs gate.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io;
use std::path::PathBuf;

use animatix::timeline::property_registry::{PROPERTY_REGISTRY, PropertyFlags, ValueType};
use animatix_syntax::schema::{PropertySpec, property_specs};
use serde_json::Value;

struct Group {
    id: String,
    properties: Vec<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mode = match std::env::args().nth(1).as_deref() {
        Some("--write") => Mode::Write,
        Some("--check") => Mode::Check,
        _ => {
            return Err(invalid(
                "usage: cargo run -p animatix --example property_reference -- --write|--check",
            ));
        },
    };
    if std::env::args().len() != 2 {
        return Err(invalid("expected exactly one mode: --write or --check"));
    }

    let docs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs");
    let doc_path = docs_dir.join("properties.md");
    let groups_path = docs_dir.join("property-reference-groups.json");
    let groups = read_groups(&groups_path)?;
    let specs = property_specs();
    let specs_by_name = index_specs(&specs)?;
    let runtime_by_name = index_runtime_schema()?;
    validate_group_coverage(&groups, &specs_by_name, &runtime_by_name)?;

    let doc = fs::read_to_string(&doc_path)?;
    let rendered = render_tables(&doc, &groups, &specs_by_name, &runtime_by_name)?;
    match mode {
        Mode::Write => fs::write(doc_path, rendered)?,
        Mode::Check if rendered != doc => {
            return Err(invalid(
                "property reference tables are stale; run `cargo run -p animatix --example property_reference -- --write`",
            ));
        },
        Mode::Check => {},
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Mode {
    Write,
    Check,
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn read_groups(path: &PathBuf) -> Result<Vec<Group>, Box<dyn Error>> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)?;
    let entries = value
        .as_array()
        .ok_or_else(|| invalid("property-reference-groups.json must be a JSON array"))?;
    let mut groups = Vec::with_capacity(entries.len());
    let mut ids = HashSet::new();
    for entry in entries {
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("each property group needs a string `id`"))?;
        if id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'-') {
            return Err(invalid(format!("invalid property group id `{id}`")));
        }
        if !ids.insert(id.to_owned()) {
            return Err(invalid(format!("duplicate property group id `{id}`")));
        }
        let properties = entry
            .get("properties")
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(format!("group `{id}` needs a `properties` array")))?
            .iter()
            .map(|name| {
                name.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("group `{id}` has a non-string property name")))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if properties.is_empty() {
            return Err(invalid(format!("group `{id}` has no properties")));
        }
        groups.push(Group {
            id: id.to_owned(),
            properties,
        });
    }
    Ok(groups)
}

fn index_specs(
    specs: &[PropertySpec],
) -> Result<HashMap<&'static str, &PropertySpec>, Box<dyn Error>> {
    let mut by_name = HashMap::with_capacity(specs.len());
    for spec in specs {
        if by_name.insert(spec.name, spec).is_some() {
            return Err(invalid(format!("duplicate property spec `{}`", spec.name)));
        }
    }
    Ok(by_name)
}

fn index_runtime_schema()
-> Result<HashMap<&'static str, &'static animatix::timeline::PropertySchema>, Box<dyn Error>> {
    let mut by_name = HashMap::with_capacity(PROPERTY_REGISTRY.len());
    for schema in PROPERTY_REGISTRY.iter() {
        if by_name.insert(schema.name, schema).is_some() {
            return Err(invalid(format!("duplicate runtime property `{}`", schema.name)));
        }
    }
    Ok(by_name)
}

fn validate_group_coverage(
    groups: &[Group],
    specs: &HashMap<&'static str, &PropertySpec>,
    runtime: &HashMap<&'static str, &'static animatix::timeline::PropertySchema>,
) -> Result<(), Box<dyn Error>> {
    let mut seen = HashSet::new();
    for group in groups {
        for name in &group.properties {
            if !seen.insert(name.as_str()) {
                return Err(invalid(format!("property `{name}` appears in more than one group")));
            }
            if !specs.contains_key(name.as_str()) {
                return Err(invalid(format!("unknown property `{name}` in group `{}`", group.id)));
            }
            if !runtime.contains_key(name.as_str()) {
                return Err(invalid(format!("property `{name}` has no runtime binding")));
            }
        }
    }
    if seen.len() != specs.len() {
        let missing: Vec<_> = specs.keys().filter(|name| !seen.contains(**name)).copied().collect();
        return Err(invalid(format!("property groups omit descriptors: {missing:?}")));
    }
    if runtime.len() != specs.len() {
        return Err(invalid(format!(
            "property specs and runtime bindings differ in size ({} vs {})",
            specs.len(),
            runtime.len()
        )));
    }
    Ok(())
}

fn render_tables(
    doc: &str,
    groups: &[Group],
    specs: &HashMap<&'static str, &PropertySpec>,
    runtime: &HashMap<&'static str, &'static animatix::timeline::PropertySchema>,
) -> Result<String, Box<dyn Error>> {
    let mut rendered = doc.to_owned();
    for group in groups {
        let start_marker = format!("<!-- generated-property-table:{}:start -->", group.id);
        let end_marker = format!("<!-- generated-property-table:{}:end -->", group.id);
        if rendered.matches(&start_marker).count() != 1
            || rendered.matches(&end_marker).count() != 1
        {
            return Err(invalid(format!("expected one marker pair for group `{}`", group.id)));
        }
        let start = rendered.find(&start_marker).expect("count checked") + start_marker.len();
        let end = rendered.find(&end_marker).filter(|end| *end > start).ok_or_else(|| {
            invalid(format!("end marker precedes start for group `{}`", group.id))
        })?;
        let table = render_group_table(group, specs, runtime)?;
        rendered.replace_range(start..end, &format!("\n{table}\n"));
    }
    Ok(rendered)
}

fn render_group_table(
    group: &Group,
    specs: &HashMap<&'static str, &PropertySpec>,
    runtime: &HashMap<&'static str, &'static animatix::timeline::PropertySchema>,
) -> Result<String, Box<dyn Error>> {
    let mut rows = vec![
        "| Property | Type | Runtime kind | Animated | Assignable | Applies to |".to_owned(),
        "|----------|------|--------------|----------|------------|------------|".to_owned(),
    ];
    for name in &group.properties {
        let spec = specs
            .get(name.as_str())
            .ok_or_else(|| invalid(format!("missing property spec `{name}`")))?;
        let schema = runtime
            .get(name.as_str())
            .ok_or_else(|| invalid(format!("missing runtime schema `{name}`")))?;
        let authored_type = markdown_code(&spec.ty.to_string());
        let runtime_kind = markdown_code(&format_runtime_kind(schema.value_type));
        let animated = flag_cell(schema.flags.contains(PropertyFlags::ANIMATED));
        let assignable = flag_cell(schema.flags.contains(PropertyFlags::ASSIGNABLE));
        let applies_to = if spec.actor_types.is_empty() {
            "—".to_owned()
        } else {
            spec.actor_types
                .iter()
                .map(|actor| markdown_code(actor))
                .collect::<Vec<_>>()
                .join(", ")
        };
        rows.push(format!(
            "| `{}` | {authored_type} | {runtime_kind} | {animated} | {assignable} | {applies_to} |",
            escape_cell(name)
        ));
    }
    Ok(rows.join("\n"))
}

fn flag_cell(enabled: bool) -> &'static str {
    if enabled { "✓" } else { "—" }
}

fn markdown_code(value: &str) -> String {
    format!("`{}`", escape_cell(value))
}

fn escape_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('`', "\\`")
        .replace(['\n', '\r'], " ")
}

fn format_runtime_kind(value_type: ValueType) -> String {
    match value_type {
        ValueType::F32 => "F32".to_owned(),
        ValueType::U32 => "U32".to_owned(),
        ValueType::Vec2 => "Vec2".to_owned(),
        ValueType::Vec4 => "Vec4".to_owned(),
        ValueType::Color => "Color".to_owned(),
        ValueType::String => "String".to_owned(),
        ValueType::Bool => "Bool".to_owned(),
        ValueType::ShapeType => "ShapeType".to_owned(),
        ValueType::PlacementMode => "PlacementMode".to_owned(),
        ValueType::SceneAnchor => "SceneAnchor".to_owned(),
        ValueType::PositionBinding => "PositionBinding".to_owned(),
        ValueType::MorphOptions => "MorphOptions".to_owned(),
        ValueType::CalloutPlace => "CalloutPlace".to_owned(),
        ValueType::PointList => "PointList".to_owned(),
        ValueType::F32List => "F32List".to_owned(),
        ValueType::Gradient => "Gradient".to_owned(),
        ValueType::CommandList => "CommandList".to_owned(),
        ValueType::Transform => "Transform".to_owned(),
        ValueType::BuildTimeOnly => "BuildTimeOnly".to_owned(),
        ValueType::Union(types) => format!(
            "Union<{}>",
            types
                .iter()
                .map(|kind| format_runtime_kind(*kind))
                .collect::<Vec<_>>()
                .join(" | ")
        ),
        ValueType::Sum(variants) => format!(
            "Sum<{}>",
            variants
                .iter()
                .map(|variant| format!(
                    "{}: {}",
                    variant.name,
                    format_runtime_kind(variant.value_type)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValueType::Enum(choices) => format!("Enum<{}>", choices.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs() -> (
        Vec<PropertySpec>,
        HashMap<&'static str, &'static animatix::timeline::PropertySchema>,
    ) {
        let specs = property_specs();
        let runtime_by_name = index_runtime_schema().expect("unique runtime property names");
        (specs, runtime_by_name)
    }

    fn message(result: Result<(), Box<dyn Error>>) -> String {
        result.expect_err("expected validation error").to_string()
    }

    #[test]
    fn rejects_a_missing_table_marker() {
        let (spec_rows, runtime) = inputs();
        let specs = index_specs(&spec_rows).expect("unique property names");
        let group = Group {
            id: "legend".to_owned(),
            properties: vec!["legend".to_owned()],
        };
        let error = render_tables("prose only", &[group], &specs, &runtime)
            .expect_err("missing marker pair must fail");
        assert!(error.to_string().contains("marker pair"));
    }

    #[test]
    fn rejects_duplicate_and_unknown_group_properties() {
        let (spec_rows, runtime) = inputs();
        let specs = index_specs(&spec_rows).expect("unique property names");
        let duplicate = vec![
            Group {
                id: "first".to_owned(),
                properties: vec!["legend".to_owned()],
            },
            Group {
                id: "second".to_owned(),
                properties: vec!["legend".to_owned()],
            },
        ];
        assert!(
            message(validate_group_coverage(&duplicate, &specs, &runtime))
                .contains("more than one group")
        );

        let unknown = vec![Group {
            id: "unknown".to_owned(),
            properties: vec!["not_a_property".to_owned()],
        }];
        assert!(
            message(validate_group_coverage(&unknown, &specs, &runtime))
                .contains("unknown property")
        );
    }

    #[test]
    fn escapes_type_pipes_and_preserves_prose_around_tables() {
        let (spec_rows, runtime) = inputs();
        let specs = index_specs(&spec_rows).expect("unique property names");
        let group = Group {
            id: "legend".to_owned(),
            properties: vec!["legend".to_owned()],
        };
        let doc = concat!(
            "Intro prose.\n",
            "<!-- generated-property-table:legend:start -->\n",
            "old table\n",
            "<!-- generated-property-table:legend:end -->\n",
            "Outro prose.\n"
        );
        let rendered = render_tables(doc, &[group], &specs, &runtime).expect("render table");
        assert!(rendered.starts_with("Intro prose.\n"));
        assert!(rendered.ends_with("Outro prose.\n"));
        assert!(rendered.contains("Bool \\| Str"), "type pipes must be escaped in Markdown");
    }
}
