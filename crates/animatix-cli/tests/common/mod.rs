use std::path::PathBuf;

pub fn temp_project(name: &str, source: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix(&format!("animatix-{name}-"))
        .tempdir()
        .expect("create temp project");
    let input = dir.path().join("scene.amx");
    std::fs::write(&input, source).expect("write scene");
    (dir, input)
}
