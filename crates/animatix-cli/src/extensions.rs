use std::path::{Path, PathBuf};

use animatix::extension_plugin::{NativePlugin, PluginDisposer, PluginLoader};
use animatix_analyzer::ExtensionManifest;

/// Extensions loaded from CLI `--plugin` arguments.
pub struct CliExtensions {
    loader: PluginLoader,
    pub manifests: Vec<ExtensionManifest>,
}

impl CliExtensions {
    pub fn new() -> Self {
        Self {
            loader: PluginLoader::new(),
            manifests: Vec::new(),
        }
    }

    pub fn install_into(
        &self,
        ctx: &mut animatix::extension_context::ExtensionContext,
    ) -> Result<Vec<PluginDisposer>, String> {
        self.loader.install_all(ctx).map_err(|err| err.to_string())
    }

    pub fn merged_manifest(&self) -> ExtensionManifest {
        ExtensionManifest::merge(&self.manifests)
    }
}

/// Load manifests and native plugin libraries named on the command line.
pub fn load_cli_extensions(paths: &[PathBuf]) -> Result<CliExtensions, String> {
    let mut extensions = CliExtensions::new();
    for path in paths {
        if is_manifest_path(path) {
            let source = std::fs::read_to_string(path)
                .map_err(|err| format!("Cannot read {}: {err}", path.display()))?;
            let manifest = ExtensionManifest::from_toml(&source)
                .map_err(|err| format!("Invalid manifest {}: {err}", path.display()))?;
            if let Some(library) = manifest.library.as_deref() {
                let library_path = path.parent().unwrap_or_else(|| Path::new(".")).join(library);
                let plugin = NativePlugin::load(&library_path).map_err(|err| err.to_string())?;
                extensions.loader.register(Box::new(plugin));
            }
            extensions.manifests.push(manifest);
        } else if is_native_library_path(path) {
            let plugin = NativePlugin::load(path).map_err(|err| err.to_string())?;
            extensions.loader.register(Box::new(plugin));
        } else {
            return Err(format!(
                "Unsupported plugin path '{}': expected a .amx-plugin.toml manifest or native library",
                path.display()
            ));
        }
    }
    Ok(extensions)
}

pub fn is_manifest_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".amx-plugin.toml"))
}

pub fn is_native_library_path(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("so" | "dylib" | "dll"))
}
