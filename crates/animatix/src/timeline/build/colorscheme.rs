//! Colorscheme seeding, declaration parsing, inheritance resolution,
//! config processing, and auto-color assignment.

use super::*;

impl Timeline {
    // === Colorscheme Seeding ===

    pub(super) fn apply_colorscheme(&mut self, colorscheme: ResolvedColorscheme) {
        colorscheme.seed_environment(&mut self.env);
        let background = colorscheme.color("scene.background").unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let mut bg_track = PropertyTrack::new(background);
        bg_track.add_keyframe(0, background, Easing::Linear);
        self.background_color = bg_track;
        self.colorscheme = colorscheme;
    }

    // === Colorscheme Declaration Parsing ===

    pub(super) fn load_colorscheme_declarations(
        &mut self,
        ast: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut schemes: std::collections::HashMap<String, ResolvedColorscheme> =
            std::collections::HashMap::new();
        let mut inheritance_edges: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for stmt in ast {
            if let Stmt::LetDecl {
                name,
                value: Expr::Construct(type_name, properties),
                ..
            } = stmt
            {
                if type_name == "Colorscheme" {
                    // Extract extends from properties
                    let mut extends = None;
                    let mut scheme_props = Vec::new();
                    for prop in properties {
                        if prop.name == "extends" {
                            if let Expr::Str(base) = &prop.value {
                                extends = Some(base.clone());
                            }
                        } else {
                            scheme_props.push(prop.clone());
                        }
                    }
                    if let Some(scheme) = ResolvedColorscheme::from_properties(
                        name.clone(),
                        &scheme_props,
                        diagnostics,
                    ) {
                        if let Some(base_name) = extends {
                            inheritance_edges.insert(name.clone(), base_name);
                        }
                        schemes.insert(name.clone(), scheme);
                    }
                }
            }
        }

        let mut resolved: std::collections::HashMap<String, ResolvedColorscheme> =
            std::collections::HashMap::new();

        for name in schemes.keys() {
            if let Some(scheme) = self.resolve_colorscheme_with_inheritance(
                name,
                &schemes,
                &inheritance_edges,
                &mut resolved,
                &mut std::collections::HashSet::new(),
                diagnostics,
            ) {
                resolved.insert(name.clone(), scheme);
            }
        }

        self.external_colorschemes = resolved;
    }

    // === Colorscheme Inheritance Resolution ===

    pub(super) fn resolve_colorscheme_with_inheritance(
        &self,
        name: &str,
        schemes: &std::collections::HashMap<String, ResolvedColorscheme>,
        edges: &std::collections::HashMap<String, String>,
        resolved: &mut std::collections::HashMap<String, ResolvedColorscheme>,
        visiting: &mut std::collections::HashSet<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ResolvedColorscheme> {
        if let Some(scheme) = resolved.get(name) {
            return Some(scheme.clone());
        }

        if !visiting.insert(name.to_string()) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::ColorschemeInheritanceCycle,
                    DiagnosticPhase::Build,
                    format!("Colorscheme inheritance cycle detected involving '{}'.", name),
                )
                .with_subject(name),
            );
            return None;
        }

        let mut scheme = schemes.get(name)?.clone();

        if let Some(base_name) = edges.get(name) {
            if let Some(base_builtin) = BuiltInColorscheme::from_name(base_name) {
                scheme.merge_with_base(&base_builtin.resolved());
            } else if let Some(base_resolved) = self.resolve_colorscheme_with_inheritance(
                base_name,
                schemes,
                edges,
                resolved,
                visiting,
                diagnostics,
            ) {
                scheme.merge_with_base(&base_resolved);
            } else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownColorscheme,
                        DiagnosticPhase::Build,
                        format!(
                            "Colorscheme '{}' extends unknown base '{}'; using as-is.",
                            name, base_name
                        ),
                    )
                    .with_subject(name),
                );
            }
        }

        visiting.remove(name);
        Some(scheme)
    }

    // === Config Processing ===

    pub(super) fn apply_config_settings(
        &mut self,
        settings: &[crate::ast::Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for setting in settings {
            if setting.name == "dynamic_layout" {
                self.dynamic_layout = match &setting.value {
                    Expr::Bool(b) => *b,
                    Expr::Str(s) => s.parse().unwrap_or(false),
                    _ => false,
                };
                continue;
            }

            if setting.name == "text_fast_path" {
                let enabled = match &setting.value {
                    Expr::Bool(b) => *b,
                    Expr::Str(s) => s.parse().unwrap_or(true),
                    _ => true,
                };
                if let Ok(mut compiler) = self.text_compiler.try_borrow_mut() {
                    compiler.text_fast_path = enabled;
                }
                continue;
            }

            if setting.name == "export_preset" {
                self.export_preset =
                    config_string_value(&setting.value).or_else(|| match &setting.value {
                        Expr::Ident(name) => Some(name.clone()),
                        _ => None,
                    });
                continue;
            }

            if setting.name != "colorscheme" {
                continue;
            }

            let Some(raw_name) = config_string_value(&setting.value) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        "Config key 'colorscheme' expects a built-in scheme name string such as \"editorial-dark\"."
                            .to_string(),
                    )
                    .with_subject("colorscheme"),
                );
                continue;
            };

            if let Some(built_in) = BuiltInColorscheme::from_name(&raw_name) {
                self.apply_colorscheme(built_in.resolved());
                continue;
            }

            if let Some(external) = self.external_colorschemes.get(&raw_name).cloned() {
                self.apply_colorscheme(external);
                continue;
            }

            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownColorscheme,
                    DiagnosticPhase::Build,
                    format!(
                        "Unknown colorscheme '{raw_name}'; using the default-dark built-in scheme instead."
                    ),
                )
                .with_subject("colorscheme"),
            );
        }
    }

    pub(crate) fn auto_color_for_label(&mut self, label: &str) -> Option<[f32; 4]> {
        if self.colorscheme.auto_cycle.is_empty() {
            return None;
        }

        let slot = if let Some(slot) = self.auto_color_assignments.get(label) {
            *slot
        } else {
            let slot = self.next_auto_color_index;
            self.auto_color_assignments.insert(label.to_string(), slot);
            self.next_auto_color_index += 1;
            slot
        };

        Some(self.colorscheme.auto_cycle[slot % self.colorscheme.auto_cycle.len()])
    }
}
