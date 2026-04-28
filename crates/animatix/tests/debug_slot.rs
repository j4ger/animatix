use animatix::module::ModuleGraph;
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
fn debug_slot() {
    let dir = temp_project_dir("slots_debug");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
pub component SlideLayout {
    header: Col {
        @slot
    }
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./slides.amx"

slide: SlideLayout {
    @header {
        title: Text, text: "Hello"
    }
}
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    // Verify slot expansion works
    assert!(!expanded_debug.contains("SlideLayout"));
    assert!(expanded_debug.contains("slide"));
    assert!(expanded_debug.contains("Hello"));
}
