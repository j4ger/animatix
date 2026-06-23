//! Colorscheme resolution and the precedence model for color assignment.
//!
//! Precedence order (lowest to highest):
//! 1. Runtime hardcoded default (white `[1,1,1,1]`)
//! 2. Colorscheme primitive-type defaults (when property omitted)
//! 3. Alias-based declaration defaults (e.g., `color: text.primary`)
//! 4. `color: auto` from scheme auto pool
//! 5. Explicit declaration values
//! 6. Later timed assignments
//! 7. Frame-local reactive overrides (`always`)
//!
//! Colorscheme resolution is build-time, not frame-time, for deterministic preview/export.

use crate::ast::{Expr, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::env::{Environment, Value};
use crate::timeline::utils::parse_color;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Built-in colorscheme presets.
pub enum BuiltInColorscheme {
    /// Dark theme with blue accents.
    DefaultDark,
    /// Light theme with blue accents.
    DefaultLight,
    /// Editorial dark theme with refined contrast.
    EditorialDark,
}

impl BuiltInColorscheme {
    /// Parse a colorscheme name into a built-in variant.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "default-dark" => Some(Self::DefaultDark),
            "default-light" => Some(Self::DefaultLight),
            "editorial-dark" => Some(Self::EditorialDark),
            _ => None,
        }
    }

    /// Resolve this built-in colorscheme into a full color map.
    pub fn resolved(self) -> ResolvedColorscheme {
        let mut colors = BTreeMap::new();
        let auto_cycle = match self {
            Self::DefaultDark => {
                colors.insert("scene.background".to_string(), [0.0, 0.0, 0.0, 1.0]);
                colors.insert("text.primary".to_string(), [1.0, 1.0, 1.0, 1.0]);
                colors.insert("text.secondary".to_string(), [0.75, 0.8, 0.88, 1.0]);
                colors.insert("text.muted".to_string(), [0.5, 0.55, 0.6, 1.0]);
                colors.insert("surface.primary".to_string(), [0.11, 0.16, 0.24, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.18, 0.21, 0.27, 1.0]);
                colors.insert("accent.primary".to_string(), [0.38, 0.78, 1.0, 1.0]);
                colors.insert("accent.secondary".to_string(), [0.85, 0.55, 0.95, 1.0]);
                colors.insert("accent.info".to_string(), [0.50, 0.80, 0.95, 1.0]);
                colors.insert("accent.success".to_string(), [0.35, 0.86, 0.63, 1.0]);
                colors.insert("accent.warning".to_string(), [0.98, 0.83, 0.44, 1.0]);
                colors.insert("accent.danger".to_string(), [1.0, 0.46, 0.54, 1.0]);
                colors.insert("stroke.default".to_string(), [1.0, 1.0, 1.0, 1.0]);
                vec![
                    [0.38, 0.78, 1.0, 1.0],
                    [0.85, 0.55, 0.95, 1.0],
                    [0.50, 0.80, 0.95, 1.0],
                    [0.35, 0.86, 0.63, 1.0],
                    [1.0, 0.46, 0.54, 1.0],
                    [0.98, 0.83, 0.44, 1.0],
                ]
            }
            Self::DefaultLight => {
                colors.insert("scene.background".to_string(), [0.97, 0.98, 1.0, 1.0]);
                colors.insert("text.primary".to_string(), [0.08, 0.1, 0.14, 1.0]);
                colors.insert("text.secondary".to_string(), [0.27, 0.33, 0.4, 1.0]);
                colors.insert("text.muted".to_string(), [0.58, 0.62, 0.68, 1.0]);
                colors.insert("surface.primary".to_string(), [0.9, 0.93, 0.97, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.84, 0.88, 0.93, 1.0]);
                colors.insert("accent.primary".to_string(), [0.16, 0.48, 0.88, 1.0]);
                colors.insert("accent.secondary".to_string(), [0.65, 0.35, 0.85, 1.0]);
                colors.insert("accent.info".to_string(), [0.25, 0.65, 0.90, 1.0]);
                colors.insert("accent.success".to_string(), [0.18, 0.66, 0.46, 1.0]);
                colors.insert("accent.warning".to_string(), [0.86, 0.62, 0.12, 1.0]);
                colors.insert("accent.danger".to_string(), [0.84, 0.29, 0.35, 1.0]);
                colors.insert("stroke.default".to_string(), [0.08, 0.1, 0.14, 1.0]);
                vec![
                    [0.16, 0.48, 0.88, 1.0],
                    [0.65, 0.35, 0.85, 1.0],
                    [0.25, 0.65, 0.90, 1.0],
                    [0.18, 0.66, 0.46, 1.0],
                    [0.84, 0.29, 0.35, 1.0],
                    [0.86, 0.62, 0.12, 1.0],
                ]
            }
            Self::EditorialDark => {
                colors.insert("scene.background".to_string(), [0.04, 0.06, 0.09, 1.0]);
                colors.insert("text.primary".to_string(), [0.97, 0.98, 1.0, 1.0]);
                colors.insert("text.secondary".to_string(), [0.73, 0.8, 0.89, 1.0]);
                colors.insert("text.muted".to_string(), [0.5, 0.56, 0.65, 1.0]);
                colors.insert("surface.primary".to_string(), [0.11, 0.16, 0.24, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.17, 0.22, 0.3, 1.0]);
                colors.insert("accent.primary".to_string(), [0.38, 0.78, 1.0, 1.0]);
                colors.insert("accent.secondary".to_string(), [0.82, 0.58, 0.92, 1.0]);
                colors.insert("accent.info".to_string(), [0.52, 0.78, 0.93, 1.0]);
                colors.insert("accent.success".to_string(), [0.35, 0.86, 0.63, 1.0]);
                colors.insert("accent.warning".to_string(), [0.98, 0.83, 0.44, 1.0]);
                colors.insert("accent.danger".to_string(), [1.0, 0.46, 0.54, 1.0]);
                colors.insert("stroke.default".to_string(), [0.97, 0.98, 1.0, 1.0]);
                vec![
                    [0.38, 0.78, 1.0, 1.0],
                    [0.82, 0.58, 0.92, 1.0],
                    [0.52, 0.78, 0.93, 1.0],
                    [0.35, 0.86, 0.63, 1.0],
                    [1.0, 0.46, 0.54, 1.0],
                    [0.98, 0.83, 0.44, 1.0],
                ]
            }
        };

        let name = match self {
            Self::DefaultDark => "default-dark",
            Self::DefaultLight => "default-light",
            Self::EditorialDark => "editorial-dark",
        };

        ResolvedColorscheme {
            name: name.to_string(),
            colors,
            auto_cycle,
        }
    }
}

#[derive(Clone, Debug)]
/// A fully-resolved colorscheme with named colors and an auto-assignment cycle.
pub struct ResolvedColorscheme {
    /// Name of the colorscheme.
    pub name: String,
    /// Map of color keys to RGBA values.
    pub colors: BTreeMap<String, [f32; 4]>,
    /// Pool of colors for automatic assignment.
    pub auto_cycle: Vec<[f32; 4]>,
}

impl ResolvedColorscheme {
    /// Look up a color by its key.
    pub fn color(&self, key: &str) -> Option<[f32; 4]> {
        self.colors.get(key).copied()
    }

    /// Inject all defined colors into the runtime environment.
    pub fn seed_environment(&self, env: &mut Environment) {
        for (key, color) in &self.colors {
            env.set(
                key,
                Value::Color([
                    color[0] as f64,
                    color[1] as f64,
                    color[2] as f64,
                    color[3] as f64,
                ]),
            );
        }
    }

    /// Build a ResolvedColorscheme from a list of AST properties.
    pub fn from_properties(
        name: String,
        properties: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Self> {
        let mut colors = BTreeMap::new();
        let mut auto_cycle = Vec::new();

        for prop in properties {
            if prop.name == "auto" {
                if let Expr::Tuple(items) = &prop.value {
                    for item in items {
                        let color = parse_color(item);
                        if color != [0.0, 0.0, 0.0, 1.0] || matches!(item, Expr::Tuple(_)) {
                            auto_cycle.push(color);
                        }
                    }
                }
                continue;
            }

            let color = parse_color(&prop.value);
            if color != [0.0, 0.0, 0.0, 1.0] || matches!(&prop.value, Expr::Tuple(_)) {
                colors.insert(prop.name.clone(), color);
            } else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidColorschemeData,
                        DiagnosticPhase::Build,
                        format!(
                            "Colorscheme '{}' property '{}' has invalid color value; skipping.",
                            name, prop.name
                        ),
                    )
                    .with_subject(&prop.name),
                );
            }
        }

        Some(ResolvedColorscheme {
            name,
            colors,
            auto_cycle,
        })
    }

    /// Merge this colorscheme on top of a base, inheriting missing colors.
    pub fn merge_with_base(&mut self, base: &ResolvedColorscheme) {
        let mut merged = base.colors.clone();
        merged.extend(self.colors.clone());
        self.colors = merged;

        if self.auto_cycle.is_empty() {
            self.auto_cycle = base.auto_cycle.clone();
        }
    }

    /// Returns the appropriate default color for a primitive and property.
    pub fn default_color_for_primitive(
        &self,
        primitive: &dyn crate::primitives::Primitive,
        property: &str,
    ) -> Option<[f32; 4]> {
        let key = primitive.default_color_key(property)?;
        self.color(key)
    }
}
