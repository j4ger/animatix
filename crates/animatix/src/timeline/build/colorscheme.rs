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
        // `Code` syntax highlighting recolours tokens through this palette so
        // it follows the active colorscheme. The palette is a timeline-wide
        // constant (every Code actor shares the current scheme's accents), so it
        // lives on the timeline-scoped TextCompiler — mirroring how every other
        // primitive's colour is resolved from the scheme at build time, only at
        // timeline scope instead of per-actor.
        self.text_compiler.borrow_mut().highlight_palette =
            Some(self.colorscheme.highlight_palette());
    }

    // === Colorscheme Declaration Parsing ===

    pub(super) fn load_colorscheme_declarations(
        &mut self,
        ast: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.external_colorschemes =
            super::color_inputs::load_colorscheme_declarations(ast, diagnostics);
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

            if setting.name == "bpm" {
                match &setting.value {
                    Expr::Num(bpm) if *bpm > 0.0 => {
                        self.beat_ms = Some(60_000.0 / bpm);
                    },
                    other => diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidConfigValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Config key 'bpm' expects a positive tempo in beats per minute, got {other:?}; beat stamps fall back to the default 120 bpm."
                            ),
                        )
                        .with_subject("bpm"),
                    ),
                }
                continue;
            }

            if setting.name == "seamless_loop" {
                self.seamless_loop = match &setting.value {
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

            if setting.name == "resolution" {
                self.resolution = match &setting.value {
                    Expr::Tuple(values) if values.len() == 2 => match (&values[0], &values[1]) {
                        (Expr::Num(w), Expr::Num(h)) if *w > 0.0 && *h > 0.0 => {
                            Some((*w as u32, *h as u32))
                        },
                        _ => {
                            diagnostics.push(
                                    Diagnostic::warning(
                                        DiagnosticCode::InvalidConfigValue,
                                        DiagnosticPhase::Build,
                                        "Config key 'resolution' expects a positive (width, height) tuple."
                                            .to_string(),
                                    )
                                    .with_subject("resolution"),
                                );
                            None
                        },
                    },
                    _ => {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidConfigValue,
                                DiagnosticPhase::Build,
                                "Config key 'resolution' expects a (width, height) tuple."
                                    .to_string(),
                            )
                            .with_subject("resolution"),
                        );
                        None
                    },
                };
                continue;
            }

            if setting.name == "duration" {
                self.declared_duration_s = match &setting.value {
                    Expr::Num(seconds) if *seconds > 0.0 => Some(*seconds),
                    _ => {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidConfigValue,
                                DiagnosticPhase::Build,
                                "Config key 'duration' expects a positive number of seconds."
                                    .to_string(),
                            )
                            .with_subject("duration"),
                        );
                        None
                    },
                };
                continue;
            }

            if setting.name != "colorscheme" {
                // Every key this builder handles was `continue`d above;
                // `strict_types` belongs to the typechecker pass. Anything else
                // reaching here is a key no pass reads, so report it rather than
                // dropping it silently — silence is indistinguishable from a
                // working key.
                let known_keys = animatix_syntax::config_keys::config_key_names();
                if !known_keys.contains(&setting.name.as_str()) {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownConfigKey,
                            DiagnosticPhase::Build,
                            format!(
                                "Unknown config key '{}': it is ignored. Valid keys are {}.",
                                setting.name,
                                known_keys.join(", ")
                            ),
                        )
                        .with_subject(&setting.name),
                    );
                }
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

            if let Some(colorscheme) =
                super::color_inputs::select_colorscheme(&raw_name, &self.external_colorschemes)
            {
                self.apply_colorscheme(colorscheme);
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
        let cycle_len = self.colorscheme.auto_cycle.len();
        if cycle_len == 0 {
            return None;
        }

        let slot = match self.auto_color_slot(label) {
            Some(slot) => slot,
            None => {
                let slot = self.next_auto_color_index;
                // At build time the declaration's entity may not exist yet;
                // `set_auto_color_slot` buffers the slot until it is spawned.
                self.set_auto_color_slot(label, slot);
                self.next_auto_color_index += 1;
                slot
            },
        };

        Some(self.colorscheme.auto_cycle[slot % cycle_len])
    }
}
