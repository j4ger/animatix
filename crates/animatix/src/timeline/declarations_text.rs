use super::{
    apply_explicit_position_binding, evaluate_expr_with_lookup_diagnostic,
    has_non_default_morph_options, parse_color_in_env_with_lookup_diagnostic,
    parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_modifier_diagnostic,
    resolve_position_binding_with_lookup_diagnostic, AnimationTrack, Diagnostic, DiagnosticCode,
    DiagnosticPhase, Easing, Expr, Modifier, ModifierHost, ParsedTimingModifiers, Stmt, Timeline,
};
use crate::ast::Property;

#[derive(Clone, Copy)]
enum TextDeclarationKind {
    Text,
    Math,
    Code,
}

impl TextDeclarationKind {
    fn unnamed_label(self) -> &'static str {
        match self {
            Self::Text => "unnamed_text",
            Self::Math => "unnamed_math",
            Self::Code => "unnamed_code",
        }
    }

    fn modifier_host(self) -> ModifierHost {
        match self {
            Self::Text => ModifierHost::Text,
            Self::Math => ModifierHost::Math,
            Self::Code => ModifierHost::Code,
        }
    }

    fn default_font_size(self) -> f32 {
        match self {
            Self::Code => 24.0,
            Self::Text | Self::Math => 48.0,
        }
    }

    fn content_matches(self, property_name: &str) -> bool {
        match self {
            Self::Text => property_name == "text",
            Self::Math => matches!(property_name, "latex" | "math"),
            Self::Code => property_name == "code",
        }
    }

    fn morph_warning(self) -> &'static str {
        match self {
            Self::Text => {
                "Morph-specific modifiers on text declaration require a re-declaration with non-zero duration; ignoring them for now."
            }
            Self::Math => {
                "Morph-specific modifiers on math declaration require a re-declaration with non-zero duration; ignoring them for now."
            }
            Self::Code => {
                "Morph-specific modifiers on code declaration require a re-declaration with non-zero duration; ignoring them for now."
            }
        }
    }
}

impl Timeline {
    pub(super) fn process_text_like_statement(
        &mut self,
        stmt: &Stmt,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match stmt {
            Stmt::Text {
                label,
                props,
                modifiers,
            } => self.process_text_declaration(
                TextDeclarationKind::Text,
                label.as_deref(),
                props,
                modifiers,
                time_ms,
                parent_label,
                diagnostics,
            ),
            Stmt::Math {
                label,
                props,
                modifiers,
            } => self.process_text_declaration(
                TextDeclarationKind::Math,
                label.as_deref(),
                props,
                modifiers,
                time_ms,
                parent_label,
                diagnostics,
            ),
            Stmt::Code {
                label,
                props,
                modifiers,
            } => self.process_text_declaration(
                TextDeclarationKind::Code,
                label.as_deref(),
                props,
                modifiers,
                time_ms,
                parent_label,
                diagnostics,
            ),
            _ => unreachable!("process_text_like_statement only handles text-like statements"),
        }
    }

    fn process_text_declaration(
        &mut self,
        kind: TextDeclarationKind,
        label: Option<&str>,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let label_str = label
            .map(str::to_string)
            .unwrap_or_else(|| kind.unnamed_label().to_string());
        let eval_env = self.build_eval_env(time_ms as u64);
        self.add_node(label_str.clone(), parent_label);
        let had_text_paths = self
            .tracks
            .get(&label_str)
            .map(|track| !track.text_paths.keyframes.is_empty())
            .unwrap_or(false);
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            morph_options,
        } = parse_timing_modifiers(
            modifiers,
            kind.modifier_host(),
            Some(&label_str),
            diagnostics,
        );
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let supports_morph_options = had_text_paths && duration_ms > 0.0;

        if has_non_default_morph_options(morph_options) && !supports_morph_options {
            push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::InvalidModifierValue,
                kind.morph_warning().to_string(),
                Some(&label_str),
            );
        }

        let mut text_content = String::new();
        let mut font_size = kind.default_font_size();
        let mut color = typst::visualize::Color::from_u8(255, 255, 255, 255);
        let mut initial_track_color: Option<[f32; 4]> = None;
        let mut at_expr: Option<Expr> = None;
        let mut anchor_expr: Option<Expr> = None;
        let mut offset_expr: Option<Expr> = None;

        for prop in props {
            let prop_subject = format!("{}.{}", label_str, prop.name);
            match prop.name.as_str() {
                name if kind.content_matches(name) => {
                    text_content = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_default();
                }
                "font_size" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(0.0));
                    font_size = value.as_num() as f32;
                }
                "color" => {
                    let resolved_color = if matches!(&prop.value, Expr::Ident(name) if name == "auto")
                    {
                        self.auto_color_for_label(&label_str).or_else(|| {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::UnknownColorReference,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                        label_str
                                    ),
                                )
                                .with_subject(&prop_subject),
                            );
                            None
                        })
                    } else {
                        parse_color_in_env_with_lookup_diagnostic(
                            &label_str,
                            "color",
                            &prop.value,
                            &eval_env,
                            diagnostics,
                            &prop_subject,
                        )
                    };

                    if let Some(resolved_color) = resolved_color {
                        initial_track_color = Some(resolved_color);
                        color = typst::visualize::Color::from_u8(
                            (resolved_color[0] * 255.0) as u8,
                            (resolved_color[1] * 255.0) as u8,
                            (resolved_color[2] * 255.0) as u8,
                            (resolved_color[3] * 255.0) as u8,
                        );
                    }
                }
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                _ => {}
            }
        }

        // Apply scheme-appropriate default color when no explicit color property is provided
        if initial_track_color.is_none() {
            let primitive_type = match kind {
                TextDeclarationKind::Text => "Text",
                TextDeclarationKind::Math => "Math",
                TextDeclarationKind::Code => "Code",
            };
            if let Some(scheme_color) = self.get_default_color(primitive_type, "color") {
                initial_track_color = Some(scheme_color);
                color = typst::visualize::Color::from_u8(
                    (scheme_color[0] * 255.0) as u8,
                    (scheme_color[1] * 255.0) as u8,
                    (scheme_color[2] * 255.0) as u8,
                    (scheme_color[3] * 255.0) as u8,
                );
            }
        }

        let track = self
            .tracks
            .entry(label_str.clone())
            .or_insert_with(|| AnimationTrack::new(label_str.clone()));

        if let Some(track_color) = initial_track_color {
            if delay_ms > 0.0 && duration_ms == 0.0 {
                preserve_instant_delayed_value(&mut track.color, t_start_ms);
            }
            track
                .color
                .add_keyframe(t_start_ms, track_color, Easing::Linear);
        }

        if let Some((binding, position)) = resolve_position_binding_with_lookup_diagnostic(
            at_expr.as_ref(),
            anchor_expr.as_ref(),
            offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            &label_str,
        ) {
            if delay_ms > 0.0 && duration_ms == 0.0 {
                preserve_discrete_position_state_before(track, t_start_ms);
                preserve_instant_delayed_value(&mut track.position, t_start_ms);
            }
            apply_explicit_position_binding(track, t_start_ms, binding, position);
        }

        let frame = match kind {
            TextDeclarationKind::Text => {
                crate::renderer::text::compile_text(&text_content, font_size, color)
            }
            TextDeclarationKind::Math => {
                crate::renderer::text::compile_math(&text_content, font_size, color)
            }
            TextDeclarationKind::Code => {
                crate::renderer::text::compile_code(&text_content, font_size, color)
            }
        };
        let new_paths = crate::renderer::text::extract_glyphs(&frame);
        let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

        if duration_ms > 0.0 {
            let start_val = track.evaluate_text_paths(t_start_ms);
            track
                .text_paths
                .add_keyframe(t_start_ms, start_val, Easing::Linear);
            let start_size = track.size.evaluate(t_start_ms);
            track
                .size
                .add_keyframe(t_start_ms, start_size, Easing::Linear);
        } else if delay_ms > 0.0 {
            preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
            preserve_instant_delayed_value(&mut track.size, t_start_ms);
        }
        if supports_morph_options {
            track
                .morph_options
                .add_keyframe(t_end_ms, morph_options, Easing::Linear);
        }
        track.text_paths.add_keyframe(t_end_ms, new_paths, easing);
        track.size.add_keyframe(t_end_ms, new_half_size, easing);
    }

    pub(super) fn process_text_actor_decl(
        &mut self,
        actor_type: &str,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let kind = match actor_type {
            "Text" => TextDeclarationKind::Text,
            "Math" => TextDeclarationKind::Math,
            "Code" => TextDeclarationKind::Code,
            _ => return,
        };

        self.process_text_declaration(
            kind,
            Some(label),
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        );
    }
}
