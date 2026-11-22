//! Contract tests, not performance measurements; registered by engine_contracts.

#[path = "../benches/full_pipeline_support.rs"]
mod support;

use support::{
    build_only, file_pipeline, parsed, required_source, source_pipeline, workload_paths,
};

const SOURCE: &str = "#0s\nbox: Rect, size: (100, 100), at: (400, 300)\n";

#[test]
fn build_boundary_accepts_an_existing_ast() {
    let ast = parsed(SOURCE);
    assert!(!ast.is_empty());
    let _ = build_only(&ast);
    let _ = build_only(&ast);
}

#[test]
fn source_override_needs_no_entry_file_and_does_not_write_one() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("virtual.amx");
    let _ = source_pipeline(&entry, SOURCE);
    assert!(!entry.exists());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn source_override_resolves_relative_imports_without_overwriting_disk() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.amx");
    let module = dir.path().join("sizes.amx");
    std::fs::write(&entry, "unsaved editor buffer differs from this file").unwrap();
    std::fs::write(&module, "pub let edge = 100\n").unwrap();
    let source =
        "import \"./sizes.amx\" as sizes\n#0s\nbox: Rect, size: (sizes.edge, sizes.edge)\n";
    let original = std::fs::read(&entry).unwrap();
    let _ = source_pipeline(&entry, source);
    assert_eq!(std::fs::read(&entry).unwrap(), original);
    std::fs::remove_file(module).unwrap();
    assert!(std::panic::catch_unwind(|| source_pipeline(&entry, source)).is_err());
}

#[test]
fn file_pipeline_keeps_real_file_read_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("entry.amx");
    std::fs::write(&entry, SOURCE).unwrap();
    let _ = file_pipeline(&entry);
    std::fs::remove_file(&entry).unwrap();
    assert!(std::panic::catch_unwind(|| file_pipeline(&entry)).is_err());
}

#[test]
fn mandatory_workloads_exist_and_are_nonempty() {
    for path in workload_paths() {
        let _ = required_source(&path);
    }
}

#[test]
fn missing_or_empty_workload_fails_instead_of_omitting_a_row() {
    let dir = tempfile::tempdir().unwrap();
    let entry = dir.path().join("missing.amx");
    assert!(std::panic::catch_unwind(|| required_source(&entry)).is_err());
    std::fs::write(&entry, " \n\t").unwrap();
    assert!(std::panic::catch_unwind(|| required_source(&entry)).is_err());
}
