//! Named theme registry with `extends` inheritance (`theme-json` feature).
//!
//! A registry owns a set of JSON theme files and resolves their `extends`
//! relationships into fully-merged `ThemeFile` values. Resolution is validated
//! eagerly so missing bases and extension cycles surface as load errors instead
//! of silent fallbacks.

use std::collections::BTreeMap;
use std::path::Path;
use std::{fs, io};

use super::theme::{Theme, set_theme};
use super::theme_json::{PartialTheme, ThemeFile, ThemeJsonError};

/// A set of named themes with resolved inheritance.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ThemeRegistry {
    /// Raw theme files keyed by registry name.
    entries: BTreeMap<String, ThemeFile>,
    /// Fully resolved theme files keyed by registry name.
    resolved: BTreeMap<String, ThemeFile>,
}

impl ThemeRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load every JSON theme file in `directory`.
    ///
    /// The file stem becomes the registry name. All `extends` references must
    /// resolve within the same directory.
    pub fn from_directory(directory: &Path) -> Result<Self, ThemeRegistryError> {
        let mut registry = Self::new();
        for entry in fs::read_dir(directory).map_err(ThemeRegistryError::Io)? {
            let path = entry.map_err(ThemeRegistryError::Io)?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(ToOwned::to_owned)
            else {
                continue;
            };
            if registry.entries.contains_key(&name) {
                return Err(ThemeRegistryError::Duplicate(name));
            }
            let theme = ThemeFile::load(&path).map_err(ThemeRegistryError::Theme)?;
            registry.entries.insert(name, theme);
        }
        registry.resolve_all()?;
        Ok(registry)
    }

    /// Register a theme by name and re-resolve all inheritance.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        theme: ThemeFile,
    ) -> Result<(), ThemeRegistryError> {
        let name = name.into();
        if self.entries.contains_key(&name) {
            return Err(ThemeRegistryError::Duplicate(name));
        }
        self.entries.insert(name, theme);
        self.resolve_all()
    }

    /// All registered theme names in stable sorted order.
    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// The fully resolved theme for a registry name.
    pub fn resolved(&self, name: &str) -> Option<&ThemeFile> {
        self.resolved.get(name)
    }

    /// The raw registered theme for a registry name.
    pub fn raw(&self, name: &str) -> Option<&ThemeFile> {
        self.entries.get(name)
    }

    /// Resolve and install a named theme's dark or light mode.
    ///
    /// Returns `false` when the name is not registered.
    pub fn install(&self, ctx: &egui::Context, name: &str, dark: bool) -> bool {
        let Some(theme) = self.resolved(name) else {
            return false;
        };
        set_theme(
            ctx,
            if dark {
                theme.dark_theme()
            } else {
                theme.light_theme()
            },
        );
        true
    }

    /// Re-resolve all registered themes after a file change.
    pub fn reload(&mut self) -> Result<(), ThemeRegistryError> {
        self.resolve_all()
    }

    fn resolve_all(&mut self) -> Result<(), ThemeRegistryError> {
        let mut resolved = BTreeMap::new();
        for name in self.entries.keys() {
            resolve_file(name, &self.entries, &mut resolved, &mut Vec::new())?;
        }
        self.resolved = resolved;
        Ok(())
    }
}

fn resolve_file(
    name: &str,
    entries: &BTreeMap<String, ThemeFile>,
    resolved: &mut BTreeMap<String, ThemeFile>,
    stack: &mut Vec<String>,
) -> Result<ThemeFile, ThemeRegistryError> {
    if let Some(file) = resolved.get(name) {
        return Ok(file.clone());
    }
    if stack.iter().any(|entry| entry == name) {
        let mut cycle = stack.clone();
        cycle.push(name.to_string());
        return Err(ThemeRegistryError::ExtensionLoop(cycle));
    }
    let Some(raw) = entries.get(name) else {
        return Err(ThemeRegistryError::MissingBase {
            theme: stack.last().cloned().unwrap_or_default(),
            base: name.to_string(),
        });
    };

    stack.push(name.to_string());

    let mut dark_base = Theme::dark();
    let mut light_base = Theme::light();
    if let Some(base_name) = raw.extends.as_deref() {
        let base = resolve_file(base_name, entries, resolved, stack)?;
        dark_base = base.dark_theme();
        light_base = base.light_theme();
    }

    let dark = raw.dark.clone().map(|partial| partial.apply_to(dark_base)).unwrap_or(dark_base);
    let light = raw
        .light
        .clone()
        .map(|partial| partial.apply_to(light_base))
        .unwrap_or(light_base);

    let merged = ThemeFile {
        name: raw.name.clone(),
        extends: raw.extends.clone(),
        dark: Some(PartialTheme::full_from(dark)),
        light: Some(PartialTheme::full_from(light)),
    };
    resolved.insert(name.to_string(), merged.clone());
    stack.pop();
    Ok(merged)
}

/// Errors produced while loading or resolving a theme registry.
#[derive(Debug, thiserror::Error)]
pub enum ThemeRegistryError {
    #[error("failed to read theme directory: {0}")]
    Io(#[from] io::Error),
    #[error("failed to load theme file: {0}")]
    Theme(#[from] ThemeJsonError),
    #[error("duplicate theme name: {0}")]
    Duplicate(String),
    #[error("theme '{theme}' extends unknown theme '{base}'")]
    MissingBase { theme: String, base: String },
    #[error("theme extension cycle: {0:?}")]
    ExtensionLoop(Vec<String>),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn write_theme(dir: &Path, name: &str, extends: Option<&str>, base: &str) {
        let extends_json = extends.map(|e| format!(",\"extends\":\"{e}\"")).unwrap_or_default();
        let json = format!(
            r##"{{ "name": "{name}"{extends_json}, "dark": {{ "surface": {{ "base": "{base}" }} }} }}"##
        );
        std::fs::write(dir.join(format!("{name}.json")), json).unwrap();
    }

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("eparts-theme-registry-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn registry_resolves_inheritance_in_stable_order() {
        let dir = temp_dir("inherit");
        write_theme(&dir, "base", None, "#101418");
        write_theme(&dir, "child", Some("base"), "#202428");

        let registry = ThemeRegistry::from_directory(&dir).expect("load registry");
        assert_eq!(registry.names(), vec!["base", "child"]);

        let child = registry.resolved("child").expect("child resolved");
        assert_eq!(child.dark_theme().surface.base, egui::Color32::from_rgb(0x20, 0x24, 0x28));
        assert_eq!(child.dark_theme().text.primary, Theme::dark().text.primary);

        let base = registry.resolved("base").expect("base resolved");
        assert_eq!(base.dark_theme().surface.base, egui::Color32::from_rgb(0x10, 0x14, 0x18));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_rejects_missing_base() {
        let dir = temp_dir("missing");
        write_theme(&dir, "child", Some("missing"), "#202428");

        let err = ThemeRegistry::from_directory(&dir).expect_err("should fail");
        assert!(matches!(err, ThemeRegistryError::MissingBase { .. }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_rejects_extension_cycle() {
        let dir = temp_dir("cycle");
        write_theme(&dir, "a", Some("b"), "#101418");
        write_theme(&dir, "b", Some("a"), "#202428");

        let err = ThemeRegistry::from_directory(&dir).expect_err("should fail");
        assert!(matches!(err, ThemeRegistryError::ExtensionLoop(cycle) if cycle.len() >= 2));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_rejects_duplicate_names() {
        let mut registry = ThemeRegistry::new();
        registry.register("dup", ThemeFile::default()).expect("first registration");
        assert!(matches!(
            registry.register("dup", ThemeFile::default()),
            Err(ThemeRegistryError::Duplicate(_))
        ));
    }

    #[test]
    fn registry_keeps_child_override_over_base() {
        let dir = temp_dir("override");
        write_theme(&dir, "base", None, "#101418");
        let child_json = r##"{ "name":"child", "extends":"base",
            "dark": { "surface": { "base": "#ff0000" }, "text": { "primary": "#00ff00" } } }"##;
        std::fs::write(dir.join("child.json"), child_json).unwrap();

        let registry = ThemeRegistry::from_directory(&dir).expect("load registry");
        let theme = registry.resolved("child").unwrap().dark_theme();
        assert_eq!(theme.surface.base, egui::Color32::from_rgb(0xff, 0, 0));
        assert_eq!(theme.text.primary, egui::Color32::from_rgb(0, 0xff, 0));
        assert_eq!(theme.surface.panel, Theme::dark().surface.panel);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
