use std::path::Path;
use tracing::info;

pub fn run_describe(library: &Path, output: Option<&Path>) -> Result<(), String> {
    let toml = animatix_plugin_tooling::generate_manifest_toml(library, output)?;
    match output {
        Some(path) => {
            std::fs::write(path, &toml)
                .map_err(|err| format!("Cannot write {}: {err}", path.display()))?;
            info!("Wrote plugin manifest: {}", path.display());
            Ok(())
        },
        None => {
            print!("{toml}");
            Ok(())
        },
    }
}
