use crate::ast::{Expr, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::env::{Environment, Value};
use crate::timeline::utils::parse_color;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltInColorscheme {
    DefaultDark,
    DefaultLight,
    EditorialDark,
}

impl BuiltInColorscheme {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "default-dark" => Some(Self::DefaultDark),
            "default-light" => Some(Self::DefaultLight),
            "editorial-dark" => Some(Self::EditorialDark),
            _ => None,
        }
    }

    pub fn resolved(self) -> ResolvedColorscheme {
        let mut colors = BTreeMap::new();
        let auto_cycle = match self {
            Self::DefaultDark => {
                colors.insert("scene.background".to_string(), [0.0, 0.0, 0.0, 1.0]);
                colors.insert("text.primary".to_string(), [1.0, 1.0, 1.0, 1.0]);
                colors.insert("text.secondary".to_string(), [0.75, 0.8, 0.88, 1.0]);
                colors.insert("surface.primary".to_string(), [0.11, 0.16, 0.24, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.18, 0.21, 0.27, 1.0]);
                colors.insert("accent.primary".to_string(), [0.38, 0.78, 1.0, 1.0]);
                colors.insert("accent.success".to_string(), [0.35, 0.86, 0.63, 1.0]);
                colors.insert("accent.warning".to_string(), [0.98, 0.83, 0.44, 1.0]);
                colors.insert("accent.danger".to_string(), [1.0, 0.46, 0.54, 1.0]);
                colors.insert("stroke.default".to_string(), [1.0, 1.0, 1.0, 1.0]);
                vec![
                    [0.38, 0.78, 1.0, 1.0],
                    [0.35, 0.86, 0.63, 1.0],
                    [1.0, 0.46, 0.54, 1.0],
                    [0.98, 0.83, 0.44, 1.0],
                ]
            }
            Self::DefaultLight => {
                colors.insert("scene.background".to_string(), [0.97, 0.98, 1.0, 1.0]);
                colors.insert("text.primary".to_string(), [0.08, 0.1, 0.14, 1.0]);
                colors.insert("text.secondary".to_string(), [0.27, 0.33, 0.4, 1.0]);
                colors.insert("surface.primary".to_string(), [0.9, 0.93, 0.97, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.84, 0.88, 0.93, 1.0]);
                colors.insert("accent.primary".to_string(), [0.16, 0.48, 0.88, 1.0]);
                colors.insert("accent.success".to_string(), [0.18, 0.66, 0.46, 1.0]);
                colors.insert("accent.warning".to_string(), [0.86, 0.62, 0.12, 1.0]);
                colors.insert("accent.danger".to_string(), [0.84, 0.29, 0.35, 1.0]);
                colors.insert("stroke.default".to_string(), [0.08, 0.1, 0.14, 1.0]);
                vec![
                    [0.16, 0.48, 0.88, 1.0],
                    [0.18, 0.66, 0.46, 1.0],
                    [0.84, 0.29, 0.35, 1.0],
                    [0.86, 0.62, 0.12, 1.0],
                ]
            }
            Self::EditorialDark => {
                colors.insert("scene.background".to_string(), [0.04, 0.06, 0.09, 1.0]);
                colors.insert("text.primary".to_string(), [0.97, 0.98, 1.0, 1.0]);
                colors.insert("text.secondary".to_string(), [0.73, 0.8, 0.89, 1.0]);
                colors.insert("surface.primary".to_string(), [0.11, 0.16, 0.24, 1.0]);
                colors.insert("surface.secondary".to_string(), [0.17, 0.22, 0.3, 1.0]);
                colors.insert("accent.primary".to_string(), [0.38, 0.78, 1.0, 1.0]);
                colors.insert("accent.success".to_string(), [0.35, 0.86, 0.63, 1.0]);
                colors.insert("accent.warning".to_string(), [0.98, 0.83, 0.44, 1.0]);
                colors.insert("accent.danger".to_string(), [1.0, 0.46, 0.54, 1.0]);
                colors.insert("stroke.default".to_string(), [0.97, 0.98, 1.0, 1.0]);
                vec![
                    [0.38, 0.78, 1.0, 1.0],
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
pub struct ResolvedColorscheme {
    pub name: String,
    pub colors: BTreeMap<String, [f32; 4]>,
    pub auto_cycle: Vec<[f32; 4]>,
}

impl ResolvedColorscheme {
    pub fn color(&self, key: &str) -> Option<[f32; 4]> {
        self.colors.get(key).copied()
    }

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

    pub fn merge_with_base(&mut self, base: &ResolvedColorscheme) {
        let mut merged = base.colors.clone();
        merged.extend(self.colors.clone());
        self.colors = merged;

        if self.auto_cycle.is_empty() {
            self.auto_cycle = base.auto_cycle.clone();
        }
    }
}
