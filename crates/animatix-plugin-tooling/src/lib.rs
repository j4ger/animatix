//! Shared native plugin manifest tooling for the CLI and GUI.
//!
//! Loading a native library, installing it into a scratch extension context,
//! collecting runtime descriptors, and generating the analyzer manifest is
//! identical in `animatix plugin describe` and the GUI plugin status dialog.
//! Keeping it here prevents those two consumers from drifting.

use std::path::{Path, PathBuf};

use animatix::extension_context::ExtensionContext;
use animatix::extension_plugin::{ExtensionPlugin, NativePlugin};
use animatix_analyzer::ExtensionManifest;

/// Generate and validate a `.amx-plugin.toml` body from a native library.
///
/// When `output` is provided, the recorded `library` field is made relative to
/// that output file's directory; otherwise the library path is used as-is.
pub fn generate_manifest_toml(library: &Path, output: Option<&Path>) -> Result<String, String> {
    let plugin = NativePlugin::load(library)
        .map_err(|err| format!("Cannot load {}: {err}", library.display()))?;
    let mut ctx = ExtensionContext::new();
    let disposer = plugin
        .install(&mut ctx)
        .map_err(|err| format!("Cannot install {}: {err}", library.display()))?;

    let result = (|| {
        let registry = ctx.primitive_registry();
        let primitives = registry
            .specs()
            .into_iter()
            .filter(|spec| !registry.is_builtin(&spec.type_name))
            .collect::<Vec<_>>();
        let manifest_library = output
            .map(|output| {
                let parent = output
                    .parent()
                    .filter(|path| !path.as_os_str().is_empty())
                    .unwrap_or_else(|| Path::new("."));
                relative_path(parent, library)
                    .unwrap_or_else(|| library.to_path_buf())
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| library.to_string_lossy().into_owned());
        let manifest = ExtensionManifest::from_runtime(
            Some(manifest_library),
            &primitives,
            &ctx.extension_property_descriptors(),
            &ctx.action_signatures(),
            &ctx.function_descriptors(),
            &ctx.service_descriptors(),
        );
        let toml = manifest.to_toml()?;
        let parsed = ExtensionManifest::from_toml(&toml).map_err(|err| err.to_string())?;
        if parsed != manifest {
            return Err("Generated manifest failed validation round-trip".to_string());
        }
        Ok(toml)
    })();

    disposer(&mut ctx);
    result
}

/// Compute a `target` path relative to `from_dir`.
pub fn relative_path(from_dir: &Path, target: &Path) -> Option<PathBuf> {
    let from = std::fs::canonicalize(from_dir).ok()?;
    let target = std::fs::canonicalize(target).ok()?;
    let mut from_components = from.components().peekable();
    let mut target_components = target.components().peekable();
    while from_components.peek() == target_components.peek() {
        from_components.next();
        target_components.next();
    }
    let mut result = PathBuf::new();
    for component in from_components {
        if matches!(component, std::path::Component::Prefix(_)) {
            return None;
        }
        result.push("..");
    }
    for component in target_components {
        result.push(component.as_os_str());
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_plugin_tooling_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn relative_path_uses_common_ancestor() {
        let dir = temp_dir("common");
        let from = dir.join("src");
        let target = dir.join("target").join("debug").join("libplugin.so");
        std::fs::create_dir_all(&from).expect("create from dir");
        std::fs::create_dir_all(target.parent().expect("target parent"))
            .expect("create target dir");
        std::fs::write(&target, b"demo").expect("write target");

        let relative = relative_path(&from, &target).expect("relative path");
        assert_eq!(relative, PathBuf::from("../target/debug/libplugin.so"));
    }

    #[test]
    fn relative_path_handles_unrelated_roots() {
        let root = temp_dir("unrelated");
        let from = root.join("a").join("b");
        let target = root.join("c").join("d");
        std::fs::create_dir_all(&from).expect("create from dir");
        std::fs::create_dir_all(&target).expect("create target dir");
        std::fs::write(&target.join("file"), b"demo").expect("write target");

        let relative = relative_path(&from, &target.join("file")).expect("relative path");
        assert_eq!(relative, PathBuf::from("../../c/d/file"));
    }
}
