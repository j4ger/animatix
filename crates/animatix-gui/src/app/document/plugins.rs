//! Per-document plugin lifecycle manager for the GUI.
//!
//! The manager owns discovery, native library loading, installation, disposal,
//! and automatic change polling. Rebuilds share the same `ExtensionContext`
//! Arc instead of re-loading libraries on every background rebuild.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use animatix::extension_context::ExtensionContext;
use animatix::extension_plugin::{ExtensionPlugin, PluginDisposer, PluginLoader};
use animatix_analyzer::{
    ExtensionManifest, ManifestSource, discover_manifest_paths, fingerprint_sources,
    load_manifest_source,
};

/// A user-facing plugin load/install failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginIssue {
    /// Optional file path that caused the issue.
    pub path: Option<PathBuf>,
    /// Human-readable message.
    pub message: String,
}

/// Immutable snapshot used by the plugin status panel.
#[derive(Clone, Debug)]
pub(crate) struct PluginSnapshot {
    /// Discovered manifest sources in priority order.
    pub sources: Vec<ManifestSource>,
    /// Load/install issues from the latest reload.
    pub issues: Vec<PluginIssue>,
    /// Loaded plugin names from the active context.
    pub plugin_names: Vec<String>,
}

struct PluginState {
    context: Arc<ExtensionContext>,
    manifest: ExtensionManifest,
    sources: Vec<ManifestSource>,
    issues: Vec<PluginIssue>,
    plugin_names: Vec<String>,
    fingerprint: u64,
    _disposers: Vec<PluginDisposer>,
}

/// Owns the current extension context and last-known-good plugin state.
pub(crate) struct DocumentPluginManager {
    document_path: PathBuf,
    workspace_root: PathBuf,
    explicit_plugin_paths: Vec<PathBuf>,
    in_process_plugins: Vec<Arc<dyn ExtensionPlugin>>,
    state: Mutex<PluginState>,
    last_check: Mutex<Instant>,
}

const POLL_INTERVAL: Duration = Duration::from_millis(500);

impl DocumentPluginManager {
    /// Create a manager and load plugins for `document_path`.
    pub(crate) fn new(document_path: PathBuf, workspace_root: PathBuf) -> Self {
        let mut manager = Self {
            document_path,
            workspace_root,
            explicit_plugin_paths: Vec::new(),
            in_process_plugins: Vec::new(),
            state: Mutex::new(PluginState {
                context: Arc::new(ExtensionContext::new()),
                manifest: ExtensionManifest::default(),
                sources: Vec::new(),
                issues: Vec::new(),
                plugin_names: Vec::new(),
                fingerprint: 0,
                _disposers: Vec::new(),
            }),
            last_check: Mutex::new(Instant::now()),
        };
        manager.reload();
        manager
    }

    /// Create a manager with injected in-process plugins for tests.
    #[cfg(test)]
    pub(crate) fn with_plugins(
        document_path: PathBuf,
        workspace_root: PathBuf,
        plugins: Vec<Arc<dyn ExtensionPlugin>>,
    ) -> Self {
        let mut manager = Self::new(document_path, workspace_root);
        manager.in_process_plugins = plugins;
        manager.reload();
        manager
    }

    /// Retarget the manager to another document/workspace and reload.
    pub(crate) fn set_document(&mut self, document_path: PathBuf, workspace_root: PathBuf) {
        self.document_path = document_path;
        self.workspace_root = workspace_root;
        self.reload();
    }

    /// Set explicit plugin paths in priority order (highest first).
    pub(crate) fn set_explicit_plugin_paths(&mut self, paths: Vec<PathBuf>) {
        self.explicit_plugin_paths = paths;
        self.reload();
    }

    /// Read explicit plugin paths.
    pub(crate) fn explicit_plugin_paths(&self) -> Vec<PathBuf> {
        self.explicit_plugin_paths.clone()
    }

    /// Clone the active extension context for a rebuild.
    pub(crate) fn context(&self) -> Arc<ExtensionContext> {
        self.state.lock().expect("plugin state lock").context.clone()
    }

    /// Clone the active analyzer manifest.
    pub(crate) fn manifest(&self) -> ExtensionManifest {
        self.state.lock().expect("plugin state lock").manifest.clone()
    }

    /// Read the status-panel snapshot.
    pub(crate) fn snapshot(&self) -> PluginSnapshot {
        let state = self.state.lock().expect("plugin state lock");
        PluginSnapshot {
            sources: state.sources.clone(),
            issues: state.issues.clone(),
            plugin_names: state.plugin_names.clone(),
        }
    }

    /// Force a reload and return true when any state changed.
    pub(crate) fn reload(&mut self) -> bool {
        self.reload_inner(true)
    }

    /// Poll manifest/library files for changes and reload when needed.
    pub(crate) fn poll(&mut self) -> bool {
        let now = Instant::now();
        let mut last = self.last_check.lock().expect("plugin poll lock");
        if now.saturating_duration_since(*last) < POLL_INTERVAL {
            return false;
        }
        *last = now;
        drop(last);
        self.reload_inner(false)
    }

    fn reload_inner(&mut self, force: bool) -> bool {
        let (sources, mut issues) = self.discover_sources();
        let fingerprint = fingerprint_sources(&sources);
        {
            let state = self.state.lock().expect("plugin state lock");
            if !force && state.fingerprint == fingerprint {
                return false;
            }
        }

        let (loader, plugin_names) = self.build_loader(&sources, &mut issues);

        // A missing/invalid native library is fatal for this candidate set.
        // Keep the previous context so a bad plugin can never disable working
        // plugins that were already installed.
        if issues.iter().any(|issue| issue.path.is_some()) {
            let mut state = self.state.lock().expect("plugin state lock");
            state.sources = sources.clone();
            state.issues = issues;
            state.fingerprint = fingerprint;
            return true;
        }

        let mut context = ExtensionContext::new();
        let disposers = match loader.install_all(&mut context) {
            Ok(disposers) => {
                for name in &plugin_names {
                    tracing::info!(
                        "Installed plugin '{name}' for {}",
                        self.document_path.display()
                    );
                }
                disposers
            },
            Err(err) => {
                issues.push(PluginIssue {
                    path: None,
                    message: err.to_string(),
                });
                // Keep the previous last-known-good context on install failure.
                let mut state = self.state.lock().expect("plugin state lock");
                state.sources = sources.clone();
                state.issues = issues;
                state.fingerprint = fingerprint;
                return true;
            },
        };

        let manifest = ExtensionManifest::merge(
            &sources.iter().map(|source| source.manifest.clone()).collect::<Vec<_>>(),
        );
        let mut state = self.state.lock().expect("plugin state lock");
        let changed = force || state.fingerprint != fingerprint;
        state.context = Arc::new(context);
        state.manifest = manifest;
        state.sources = sources;
        state.issues = issues;
        state.plugin_names = plugin_names;
        state.fingerprint = fingerprint;
        state._disposers = disposers;
        changed
    }

    fn discover_sources(&self) -> (Vec<ManifestSource>, Vec<PluginIssue>) {
        let mut sources = Vec::new();
        let mut issues = Vec::new();
        for path in discover_manifest_paths(
            self.document_path.parent(),
            Some(&self.workspace_root),
            &self.explicit_plugin_paths,
        ) {
            match load_manifest_source(&path) {
                Ok(source) => sources.push(source),
                Err(issue) => {
                    tracing::warn!(
                        "Failed to parse plugin manifest {}: {}",
                        issue.path.display(),
                        issue.message
                    );
                    issues.push(PluginIssue {
                        path: Some(issue.path),
                        message: issue.message,
                    });
                },
            }
        }
        (sources, issues)
    }

    fn build_loader(
        &self,
        sources: &[ManifestSource],
        issues: &mut Vec<PluginIssue>,
    ) -> (PluginLoader, Vec<String>) {
        let mut loader = PluginLoader::new();
        for plugin in &self.in_process_plugins {
            loader.register_shared(Arc::clone(plugin));
        }

        #[cfg(feature = "plugin-loading")]
        for source in sources {
            let Some(library) = source.manifest.library.as_deref() else {
                continue;
            };
            let library_path = source.path.parent().unwrap_or_else(|| Path::new(".")).join(library);
            match animatix::extension_plugin::NativePlugin::load(&library_path) {
                Ok(plugin) => loader.register(Box::new(plugin)),
                Err(err) => {
                    issues.push(PluginIssue {
                        path: Some(library_path),
                        message: err.to_string(),
                    });
                },
            }
        }

        let plugin_names = loader.list().into_iter().map(|info| info.name).collect();
        (loader, plugin_names)
    }
}

/// Generate and validate a `.amx-plugin.toml` manifest from a native library.
#[cfg(feature = "plugin-loading")]
pub(crate) fn generate_manifest_for_library(
    library: &Path,
    output: &Path,
) -> Result<String, String> {
    let plugin = animatix::extension_plugin::NativePlugin::load(library)
        .map_err(|err| format!("Cannot load {}: {err}", library.display()))?;
    let mut ctx = ExtensionContext::new();
    let disposer = plugin
        .install(&mut ctx)
        .map_err(|err| format!("Cannot install {}: {err}", library.display()))?;

    let registry = ctx.primitive_registry();
    let primitives = registry
        .specs()
        .into_iter()
        .filter(|spec| !registry.is_builtin(&spec.type_name))
        .collect::<Vec<_>>();
    let manifest_library = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .and_then(|parent| relative_path(parent, library))
        .unwrap_or_else(|| library.to_path_buf())
        .to_string_lossy()
        .into_owned();
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
    std::fs::write(output, &toml)
        .map_err(|err| format!("Cannot write {}: {err}", output.display()))?;
    disposer(&mut ctx);
    Ok(output.display().to_string())
}

#[cfg(feature = "plugin-loading")]
fn relative_path(from_dir: &Path, target: &Path) -> Option<PathBuf> {
    let from = std::fs::canonicalize(from_dir).ok()?;
    let target = std::fs::canonicalize(target).ok()?;
    let mut from_components = from.components().peekable();
    let mut target_components = target.components().peekable();
    while from_components.peek() == target_components.peek() {
        from_components.next();
        target_components.next();
    }
    let mut result = PathBuf::new();
    for _ in from_components {
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

    struct DoublePlugin;

    impl ExtensionPlugin for DoublePlugin {
        fn name(&self) -> &str {
            "double"
        }

        fn install(
            &self,
            ctx: &mut ExtensionContext,
        ) -> Result<PluginDisposer, animatix::extension_plugin::PluginError> {
            ctx.register_function("double", |args, _env| {
                let Some(animatix::timeline::Value::Num(n)) = args.first() else {
                    return Err(animatix::timeline::EvalError::TypeMismatch(
                        "double expects one number".to_string(),
                    ));
                };
                Ok(animatix::timeline::Value::Num(*n * 2.0))
            })
            .expect("register function");
            Ok(Box::new(|ctx| {
                ctx.remove_function("double");
            }))
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_plugin_manager_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn injects_in_process_plugins_into_context() {
        let dir = temp_dir("inprocess");
        let entry = dir.join("test.amx");
        std::fs::write(&entry, "#0s\n").expect("write source");
        let manager = DocumentPluginManager::with_plugins(
            entry,
            dir.clone(),
            vec![Arc::new(DoublePlugin) as Arc<dyn ExtensionPlugin>],
        );
        let context = manager.context();
        let mut env = animatix::timeline::Environment::new();
        context.install_functions(&mut env);
        assert!(matches!(env.get("double"), Some(animatix::timeline::Value::NativeFn(_))));
    }

    #[test]
    fn atomic_reload_keeps_last_good_context_on_failure() {
        let dir = temp_dir("atomic");
        let entry = dir.join("test.amx");
        std::fs::write(&entry, "#0s\n").expect("write source");
        let mut manager = DocumentPluginManager::with_plugins(
            entry.clone(),
            dir.clone(),
            vec![Arc::new(DoublePlugin) as Arc<dyn ExtensionPlugin>],
        );
        let before = manager.context();

        let manifest = dir.join("bad.amx-plugin.toml");
        std::fs::write(
            &manifest,
            "library = \"missing.so\"\n[[primitives]]\ntype_name = \"Gauge\"\n",
        )
        .expect("write manifest");
        manager.reload();

        let after = manager.context();
        assert!(
            Arc::ptr_eq(&before, &after),
            "failed reload must keep last-known-good context (before={:p}, after={:p}, issues={:?})",
            Arc::as_ptr(&before),
            Arc::as_ptr(&after),
            manager.snapshot().issues,
        );
        assert!(
            manager.snapshot().issues.iter().any(|issue| issue.path.is_some()),
            "missing library should surface an issue"
        );
    }
}
