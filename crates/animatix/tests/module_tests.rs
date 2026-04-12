use animatix::module::{ModuleError, ModuleGraph};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "animatix_{}_{}_{}",
        name,
        std::process::id(),
        unique
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn load_program_collects_public_imported_components_only() {
    let dir = temp_project_dir("public_components");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Throughput") {
    label: Text { text: title }
}

component InternalCard(title: "Private") {
    private_label: Text { text: title }
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

dashboard: MetricCard, title: "Latency"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    assert!(program.components.contains_key("MetricCard"));
    assert!(!program.components.contains_key("InternalCard"));

    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");
    assert!(expanded_debug.contains("dashboard"));
    assert!(expanded_debug.contains("Latency"));
    assert!(!expanded_debug.contains("MetricCard"));
}

#[test]
fn load_program_rejects_duplicate_component_exports() {
    let dir = temp_project_dir("duplicate_components");
    let entry = dir.join("scene.amx");
    let first = dir.join("first.amx");
    let second = dir.join("second.amx");

    write_file(
        &first,
        r#"
pub component MetricCard(title: "One") {
    label: Text { text: title }
}
"#,
    );

    write_file(
        &second,
        r#"
pub component MetricCard(title: "Two") {
    label: Text { text: title }
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./first.amx"
import "./second.amx"

dashboard: MetricCard
"#,
    );

    let error = ModuleGraph::new().load_program(&entry).unwrap_err();
    match error {
        ModuleError::DuplicateComponent { name, .. } => assert_eq!(name, "MetricCard"),
        other => panic!("expected duplicate component error, got {other:?}"),
    }
}
