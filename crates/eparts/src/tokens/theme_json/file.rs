use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tokens::theme::Theme;

use super::partial::PartialTheme;

/// A single dark/light theme file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional name of another theme in the same registry that this theme
    /// extends. Missing fields inherit from the resolved base theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    /// Dark-mode theme. Omitted fields fall back to the resolved base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<PartialTheme>,
    /// Light-mode theme. Omitted fields fall back to the resolved base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<PartialTheme>,
}

impl ThemeFile {
    /// Build a `ThemeFile` from the built-in dark and light themes.
    pub fn builtin() -> Self {
        Self {
            name: Some("Animatix".to_string()),
            extends: None,
            dark: Some(PartialTheme::full_from(Theme::dark())),
            light: Some(PartialTheme::full_from(Theme::light())),
        }
    }

    /// Resolve the dark theme, falling back to built-in dark.
    pub fn dark_theme(&self) -> Theme {
        self.dark.clone().unwrap_or_default().apply_to(Theme::dark())
    }

    /// Resolve the light theme, falling back to built-in light.
    pub fn light_theme(&self) -> Theme {
        self.light.clone().unwrap_or_default().apply_to(Theme::light())
    }

    /// Parse a theme file from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize this theme file as pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Load a theme file from disk.
    pub fn load(path: &Path) -> Result<Self, ThemeJsonError> {
        let source = std::fs::read_to_string(path).map_err(ThemeJsonError::Io)?;
        Self::from_json(&source).map_err(ThemeJsonError::Json)
    }

    /// Save this theme file to disk.
    pub fn save(&self, path: &Path) -> Result<(), ThemeJsonError> {
        let source = self.to_json_pretty().map_err(ThemeJsonError::Json)?;
        std::fs::write(path, source).map_err(ThemeJsonError::Io)
    }
}

/// A small error type for theme file I/O and JSON parsing.
#[derive(Debug, thiserror::Error)]
pub enum ThemeJsonError {
    #[error("failed to read theme file: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid theme JSON: {0}")]
    Json(#[from] serde_json::Error),
}
