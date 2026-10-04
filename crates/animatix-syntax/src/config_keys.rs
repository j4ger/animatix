//! The single source for the language's `config { ... }` keys.
//!
//! Mirrors [`crate::schema`]'s role for effects and primitives: everything the
//! author can write inside a `config` block is declared here once, and the
//! consumers derive their knowledge from this table — the timeline build's
//! unknown-key warning, the composition path's scene-scoped validation, and the
//! scope table in `docs/spec.md` (pinned by
//! [`config_scope_table_matches_catalog`]).
//!
//! A key has exactly one [`ConfigScope`], which is where it is *honoured*: a
//! key scoped to a stricter unit warns when it appears somewhere narrower (a
//! scene-level `resolution` cannot change the document's canvas). Adding a key
//! is one row here plus the code that applies it.

/// Where a `config` key is honoured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigScope {
    /// Applies to one scene; scenes may override the prelude.
    Scene,
    /// Applies to the whole document; set in the top-level (prelude) config.
    Composition,
    /// Gates a whole-program phase (e.g. type checking strictness).
    Program,
}

/// The value shape a `config` key accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigValueKind {
    /// A bare number.
    Number,
    /// A quoted string (or a bare identifier, for preset-style keys).
    Text,
    /// `true`/`false`, also accepted as a string for authoring convenience.
    Boolean,
    /// A `(width, height)` tuple of positive numbers.
    Size,
}

/// One `config` key: its name, where it applies, what it accepts.
#[derive(Clone, Copy, Debug)]
pub struct ConfigKeyInfo {
    /// The key as written inside a `config` block.
    pub name: &'static str,
    /// The document unit the key applies to.
    pub scope: ConfigScope,
    /// The value shape the key accepts.
    pub value: ConfigValueKind,
    /// One-line summary for completions and the reference table.
    pub summary: &'static str,
}

/// Every `config` key the language recognises, sorted by name.
pub static CONFIG_KEYS: &[ConfigKeyInfo] = &[
    ConfigKeyInfo {
        name: "bpm",
        scope: ConfigScope::Scene,
        value: ConfigValueKind::Number,
        summary: "Tempo that resolves beat stamps (`#2b`, `[4b]`) into time.",
    },
    ConfigKeyInfo {
        name: "colorscheme",
        scope: ConfigScope::Scene,
        value: ConfigValueKind::Text,
        summary: "Built-in or declared colorscheme name (e.g. \"editorial-dark\").",
    },
    ConfigKeyInfo {
        name: "dynamic_layout",
        scope: ConfigScope::Scene,
        value: ConfigValueKind::Boolean,
        summary: "Recompute container layout per frame (needed by `swap`/`reorder`).",
    },
    ConfigKeyInfo {
        name: "duration",
        scope: ConfigScope::Scene,
        value: ConfigValueKind::Number,
        summary: "Overrides the keyframe-inferred scene length; content past it never plays.",
    },
    ConfigKeyInfo {
        name: "export_preset",
        scope: ConfigScope::Composition,
        value: ConfigValueKind::Text,
        summary: "Export preset name the CLI video/GIF paths pick up (e.g. \"1080p30\").",
    },
    ConfigKeyInfo {
        name: "resolution",
        scope: ConfigScope::Composition,
        value: ConfigValueKind::Size,
        summary: "Canvas size and export dimensions; set once in the prelude.",
    },
    ConfigKeyInfo {
        name: "strict_types",
        scope: ConfigScope::Program,
        value: ConfigValueKind::Boolean,
        summary: "Strict type checking for the whole file.",
    },
    ConfigKeyInfo {
        name: "seamless_loop",
        scope: ConfigScope::Scene,
        value: ConfigValueKind::Boolean,
        summary: "Assert this scene is replayed as a loop; the build then checks the seam.",
    },
    ConfigKeyInfo {
        name: "text_fast_path",
        scope: ConfigScope::Program,
        value: ConfigValueKind::Boolean,
        summary: "Route plain Latin `Text` through the fast shaping path (default on).",
    },
];

/// Look up a key by name.
pub fn config_key(name: &str) -> Option<&'static ConfigKeyInfo> {
    CONFIG_KEYS.iter().find(|key| key.name == name)
}

/// Every recognised key name, sorted — the timeline build's allow-list.
pub fn config_key_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = CONFIG_KEYS.iter().map(|key| key.name).collect();
    names.sort_unstable();
    names
}

/// Keys a scene's own config block may set. The prelude provides everything
/// else; a stricter scope appearing in a scene warns as out of place.
pub fn scene_scoped_config_key_names() -> Vec<&'static str> {
    CONFIG_KEYS
        .iter()
        .filter(|key| key.scope == ConfigScope::Scene)
        .map(|key| key.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `docs/spec.md`'s "Config key scopes" table is generated knowledge: it
    /// must name exactly the keys in this catalog, with matching scopes. The
    /// same pattern the primitives catalog uses for the LLM checklist.
    #[test]
    fn config_scope_table_matches_catalog() {
        let spec = include_str!("../../../docs/spec.md");
        let start = spec
            .find("**Config key scopes:**")
            .expect("spec.md has a config key scopes table");
        let rows: Vec<&str> = spec[start..]
            .lines()
            .skip_while(|line| !line.starts_with("| `"))
            .take_while(|line| line.starts_with("|"))
            .collect();
        assert!(!rows.is_empty(), "no table rows found in spec.md");

        let mut documented: Vec<(&str, ConfigScope)> = Vec::new();
        for row in rows {
            let cells: Vec<&str> = row.split('|').collect();
            let name = cells[1].trim().trim_matches('`');
            let scope = match cells[2].trim() {
                "Scene" => ConfigScope::Scene,
                "Composition" => ConfigScope::Composition,
                "Program" => ConfigScope::Program,
                other => panic!("spec.md row '{name}' has an unknown scope '{other}'"),
            };
            documented.push((name, scope));
        }
        assert!(!documented.is_empty(), "no table rows found in spec.md");

        let mut catalog: Vec<(&str, ConfigScope)> =
            CONFIG_KEYS.iter().map(|key| (key.name, key.scope)).collect();
        documented.sort_unstable();
        catalog.sort_unstable();

        assert_eq!(
            documented, catalog,
            "docs/spec.md config table is out of sync with CONFIG_KEYS"
        );
    }
}
