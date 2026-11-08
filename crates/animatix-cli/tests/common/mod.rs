use std::path::PathBuf;

pub fn temp_project(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "animatix-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp project");
    let input = dir.join("scene.amx");
    std::fs::write(&input, source).expect("write scene");
    input
}
