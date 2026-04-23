use animatix::module::{ModuleError, ModuleGraph};
use animatix::timeline::Timeline;
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
    label: Text, text: title
}

component InternalCard(title: "Private") {
    private_label: Text, text: title
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
    label: Text, text: title
}
"#,
    );

    write_file(
        &second,
        r#"
pub component MetricCard(title: "Two") {
    label: Text, text: title
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

#[test]
fn load_program_expand_components_produces_build_input() {
    let dir = temp_project_dir("compile_boundary");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Throughput") {
    title_text: Text, text: title
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(!expanded_debug.contains("MetricCard"));
    assert!(expanded_debug.contains("card"));
    assert!(expanded_debug.contains("Latency"));

    let timeline = Timeline::build(&expanded);
    assert!(timeline.tracks.contains_key("card"));
}

#[test]
fn load_program_collects_namespaced_pub_let_exports() {
    let dir = temp_project_dir("namespaced_exports");
    let entry = dir.join("scene.amx");
    let theme = dir.join("theme.amx");

    write_file(
        &theme,
        r#"
pub let accent = (0.38, 0.78, 1.0, 1.0)
pub let background = (0.04, 0.06, 0.09, 1.0)
let private = (1.0, 0.0, 0.0, 1.0)
"#,
    );

    write_file(
        &entry,
        r#"
import "./theme.amx" as theme

panel: Rect, size: (200, 100), color: theme.accent
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    assert!(program.namespaces.contains_key("theme"));
    let theme_ns = program.namespaces.get("theme").unwrap();
    assert!(theme_ns.exports.contains_key("accent"));
    assert!(theme_ns.exports.contains_key("background"));
    assert!(!theme_ns.exports.contains_key("private"));

    // Verify the expanded statements do NOT include theme's statements
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");
    assert!(!expanded_debug.contains("0.38")); // theme's pub let values not flattened
    assert!(!expanded_debug.contains("background")); // theme's other pub let not flattened
    assert!(expanded_debug.contains("panel"));
}

#[test]
fn load_program_aliased_import_does_not_flatten() {
    let dir = temp_project_dir("aliased_no_flatten");
    let entry = dir.join("scene.amx");
    let helper = dir.join("helper.amx");

    write_file(
        &helper,
        r#"
pub let offset = 120
hidden: Circle, radius: 50
"#,
    );

    write_file(
        &entry,
        r#"
import "./helper.amx" as helper

visible: Rect, size: (100, 100)
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();

    // The hidden circle from helper should NOT be in expanded statements
    let expanded_debug = format!("{expanded:#?}");
    assert!(!expanded_debug.contains("hidden"));
    assert!(expanded_debug.contains("visible"));
}
