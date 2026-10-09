use super::property_engine::{parse_property_value, write_property_field};
use super::property_registry::lookup_property;
use super::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, DiagnosticCode,
    DiagnosticPhase, Easing, Expr, Modifier, ModifierHost, MorphOptions, ParsedTimingModifiers,
    Timeline, TrackAccessor, apply_explicit_position_binding, evaluate_expr_with_lookup_diagnostic,
    has_non_default_morph_options, parse_color_in_env_with_lookup_diagnostic,
    parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_modifier_diagnostic,
    resolve_position_binding_with_lookup_diagnostic,
};
use crate::ast::Property;
use crate::renderer::error::RenderError;

#[derive(Clone, Copy)]
enum TextDeclarationKind {
    Text,
    Code,
    Typst,
    Math,
}

impl TextDeclarationKind {
    fn unnamed_label(self) -> &'static str {
        match self {
            Self::Text => "unnamed_text",
            Self::Code => "unnamed_code",
            Self::Typst => "unnamed_typst",
            Self::Math => "unnamed_math",
        }
    }

    /// Registry type name for this text-like actor.
    fn type_name(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Code => "Code",
            Self::Typst => "Typst",
            Self::Math => "Math",
        }
    }

    fn modifier_host(self) -> ModifierHost {
        match self {
            Self::Text => ModifierHost::Text,
            Self::Code => ModifierHost::Code,
            Self::Typst | Self::Math => ModifierHost::Typst,
        }
    }

    fn default_font_size(self) -> f32 {
        match self {
            Self::Code => 24.0,
            Self::Text | Self::Typst | Self::Math => 48.0,
        }
    }

    fn content_matches(self, property_name: &str) -> bool {
        // Uniform content property across text-like kinds: `text`, which the
        // schema now lists for every text-like actor. `Code` also accepts its
        // own spelling. The `content`/`latex`/`math` aliases were removed from
        // the property table on 2026-09-14 — each was a second name for the
        // same storage field, so the analyzer warned about the uniform spelling
        // while the runtime honoured it.
        match self {
            Self::Text | Self::Typst | Self::Math => property_name == "text",
            Self::Code => matches!(property_name, "text" | "code"),
        }
    }

    fn morph_warning(self) -> &'static str {
        match self {
            Self::Text => {
                "Morph-specific modifiers on text declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Code => {
                "Morph-specific modifiers on code declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Typst => {
                "Morph-specific modifiers on typst declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
            Self::Math => {
                "Morph-specific modifiers on math declaration require a re-declaration with non-zero duration; ignoring them for now."
            },
        }
    }
}

impl Timeline {
    fn process_text_declaration(
        &mut self,
        kind: TextDeclarationKind,
        label: Option<&str>,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), RenderError> {
        let label_str =
            label.map(str::to_string).unwrap_or_else(|| kind.unnamed_label().to_string());
        let eval_env = self.build_eval_env(time_ms as u64);
        self.add_node(label_str.clone(), parent_label);
        self.warn_unknown_declaration_properties(&label_str, kind.type_name(), props, diagnostics);
        let had_text_paths = self
            .tracks
            .get(&label_str)
            .map(|track| {
                track.text.text_paths.as_ref().map(|t| !t.keyframes.is_empty()).unwrap_or(false)
            })
            .unwrap_or(false);
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            morph_options,
            ..
        } = parse_timing_modifiers(modifiers, kind.modifier_host(), Some(&label_str), diagnostics);
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
        let mut font_family = String::new();
        let mut font_size = kind.default_font_size();
        let mut font_weight = 400.0f32;
        let mut font_style = "normal".to_string();
        let mut line_height = 1.2f32;
        let mut letter_spacing = 0.0f32;
        let mut word_spacing = 0.0f32;
        let mut max_width = 0.0f32;
        let mut text_align = "left".to_string();
        let mut overflow = "visible".to_string();
        let mut color_rgba: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
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
                },
                "font_family" => {
                    font_family = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default();
                },
                "font_size" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(0.0));
                    font_size = value.as_num() as f32;
                },
                "font_weight" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    );
                    match value {
                        Some(super::Value::Str(s)) => {
                            font_weight = crate::renderer::text::parse_font_weight(&s);
                        },
                        Some(super::Value::Num(n)) => {
                            font_weight = n as f32;
                        },
                        _ => {},
                    }
                },
                "font_style" => {
                    font_style = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "normal".to_string());
                },
                "line_height" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(1.2));
                    line_height = value.as_num() as f32;
                },
                "letter_spacing" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(0.0));
                    letter_spacing = value.as_num() as f32;
                },
                "word_spacing" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(0.0));
                    word_spacing = value.as_num() as f32;
                },
                // Canonical name per spec; `max_width` kept as a legacy alias.
                "text_max_width" | "max_width" => {
                    let value = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(super::Value::Num(0.0));
                    max_width = value.as_num() as f32;
                },
                "text_align" => {
                    text_align = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "left".to_string());
                },
                "overflow" => {
                    overflow = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "visible".to_string());
                },
                "color" => {
                    let resolved_color = if matches!(&prop.value, Expr::Ident(name) if name == "auto")
                    {
                        self.auto_color_for_label(&label_str).or_else(|| {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::UnknownColorReference,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; using the default color instead.",
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
                        color_rgba = resolved_color;
                    }
                },
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                _ => {},
            }
        }

        // Apply scheme-appropriate default color when no explicit color property is provided
        if initial_track_color.is_none() {
            let primitive_type = match kind {
                TextDeclarationKind::Text => "Text",
                TextDeclarationKind::Code => "Code",
                TextDeclarationKind::Typst => "Typst",
                TextDeclarationKind::Math => "Math",
            };
            if let Some(primitive) = self.primitive_registry.find(primitive_type) {
                let caps = animatix_std::caps_for_type(primitive_type).unwrap_or_default();
                if let Some(scheme_color) = self.get_default_color(primitive, &caps, "color") {
                    initial_track_color = Some(scheme_color);
                    color_rgba = scheme_color;
                }
            }
        }

        // Pre-seed opacity for pre-keyframe first declarations without explicit opacity.
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.tracks.contains_key(&label_str);

        let track = self
            .tracks
            .entry(label_str.clone())
            .or_insert_with(|| AnimationTrack::new(label_str.clone(), kind.type_name()));

        // Ensure the track kind matches the declaration type so downstream
        // code (inspector, drag handles) can dispatch correctly.
        track.set_identity(kind.type_name());

        // Record first declaration time so scene evaluation can hide
        // actors before they are declared
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = t_start_ms;
        }

        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            track.hidden_by_default = true;
            track
                .style
                .opacity
                .ensure(1.0)
                .add_keyframe(0, self.default_opacity, Easing::Linear);
        }

        if let Some(track_color) = initial_track_color {
            if delay_ms > 0.0 && duration_ms == 0.0 {
                preserve_instant_delayed_value(&mut track.style.color, t_start_ms);
            }
            track.style.color.ensure(DEFAULT_WHITE).add_keyframe(
                t_start_ms,
                track_color,
                Easing::Linear,
            );
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
                preserve_instant_delayed_value(&mut track.geometry.position, t_start_ms);
            }
            apply_explicit_position_binding(track, t_start_ms, binding, position);
        }

        // Dispatch remaining (unhandled) properties through the generic engine so
        // universal properties like scale, rotation, and opacity actually work on
        // text / math / code actors.
        for prop in props {
            let already_handled = match prop.name.as_str() {
                name if kind.content_matches(name) => true,
                "font_family" | "font_size" | "font_weight" | "font_style" | "line_height"
                | "letter_spacing" | "word_spacing" | "max_width" | "text_align" | "overflow"
                | "color" | "at" | "anchor" | "offset" => true,
                _ => false,
            };
            if already_handled {
                continue;
            }
            let prop_subject = format!("{}.{}", label_str, prop.name);
            if let Some(schema) = lookup_property(&prop.name) {
                if let Some(pv) = parse_property_value(
                    schema.value_type,
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &prop_subject,
                ) {
                    write_property_field(
                        track,
                        schema.field,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        diagnostics,
                    );
                }
            }
        }

        // Store text_content, font_family and font_size on the track so Phase-2 runtime
        // recompilation knows what text and font to use when color or other properties change.
        track.text.text_content.ensure(String::new()).add_keyframe(
            t_end_ms,
            text_content.clone(),
            easing,
        );
        let font_family = if font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            font_family
        };
        track
            .text
            .font_family
            .ensure(crate::renderer::text::DEFAULT_FONT_FAMILY.to_string())
            .add_keyframe(t_end_ms, font_family.clone(), easing);
        track
            .text
            .font_size
            .ensure(kind.default_font_size())
            .add_keyframe(t_end_ms, font_size, easing);
        track.text.font_weight.ensure(400.0).add_keyframe(t_end_ms, font_weight, easing);
        track.text.font_style.ensure("normal".to_string()).add_keyframe(
            t_end_ms,
            font_style.clone(),
            easing,
        );
        track.text.line_height.ensure(1.2).add_keyframe(t_end_ms, line_height, easing);
        track
            .text
            .letter_spacing
            .ensure(0.0)
            .add_keyframe(t_end_ms, letter_spacing, easing);
        track.text.word_spacing.ensure(0.0).add_keyframe(t_end_ms, word_spacing, easing);
        track.text.text_max_width.ensure(0.0).add_keyframe(t_end_ms, max_width, easing);
        track.text.text_align.ensure("left".to_string()).add_keyframe(
            t_end_ms,
            text_align.clone(),
            easing,
        );
        track.text.overflow.ensure("visible".to_string()).add_keyframe(
            t_end_ms,
            overflow.clone(),
            easing,
        );

        // Compile through the process-wide memoized path so that unchanged
        // declarations skip the Typst engine entirely on rebuild (the GUI
        // rebuilds the whole Timeline per keystroke). The declaration build
        // path has always used the Typst engine directly, so the plain-text
        // fast path stays disabled here to preserve output-identical behavior.
        let text_kind = match kind {
            TextDeclarationKind::Text => crate::renderer::text::TextKind::Text,
            TextDeclarationKind::Code => crate::renderer::text::TextKind::Code,
            TextDeclarationKind::Typst => crate::renderer::text::TextKind::Typst,
            TextDeclarationKind::Math => crate::renderer::text::TextKind::Math,
        };
        let compiled = crate::renderer::text::compile_text_cached(
            text_kind,
            &text_content,
            // The generic property loop above already wrote `language` into
            // the track lane; read it back so the build-time precompile shares
            // the render path's cache key (and highlights identically).
            &track.text.language.get(t_end_ms, String::new()),
            // Same palette the render path uses, so the precompile shares its
            // cache entry and the build-time glyphs highlight identically.
            self.text_compiler.borrow().highlight_palette.as_ref(),
            &font_family,
            font_size,
            font_weight,
            &font_style,
            line_height,
            letter_spacing,
            word_spacing,
            color_rgba,
            0.0,
            "left",
            "visible",
            false,
            self.font_context.as_ref(),
        )?;

        // Glyph-coverage probe: the render path, not this precompile, decides
        // whether the fast path shapes the text (hence the compiler's
        // `text_fast_path` flag here, not the declaration's `false` above).
        // Warn when its face set cannot cover the content so tofu is
        // diagnosed at build time instead of drawn in silence.
        let missing_glyphs = crate::renderer::text::missing_text_glyphs(
            text_kind,
            &text_content,
            &font_family,
            font_weight,
            &font_style,
            self.text_compiler.borrow().text_fast_path,
            self.font_context.as_ref(),
        );
        if !missing_glyphs.is_empty() {
            let listed = missing_glyphs
                .iter()
                .map(|c| format!("{c} (U+{:04X})", *c as u32))
                .collect::<Vec<_>>()
                .join(", ");
            let subject = label.unwrap_or(kind.unnamed_label());
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::MissingGlyph,
                    DiagnosticPhase::Build,
                    format!(
                        "Text '{subject}' has no glyph for {listed} in the fast-path face set; \
                         they will be skipped or drawn as boxes. Use characters the font \
                         covers, or register a font that has them."
                    ),
                )
                .with_subject(subject),
            );
        }

        let new_paths = compiled.paths.to_vec();
        let new_half_size = compiled.half_size;

        // Store font metrics on the track for baseline alignment
        // Metrics are set at t_end_ms (when new text appears).
        // For animated transitions, the metrics don't interpolate (text changes discretely).
        track.set_metrics(t_end_ms, compiled.ascent, compiled.descent, compiled.baseline_offset);

        if duration_ms > 0.0 {
            let start_val = track.evaluate_text_paths(t_start_ms);
            track.text.text_paths.ensure(Vec::new()).add_keyframe(
                t_start_ms,
                start_val,
                Easing::Linear,
            );
            let start_size = track.geometry.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
            let start_layout_size =
                track.layout_size_get(t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
            track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                t_start_ms,
                start_size,
                Easing::Linear,
            );
            track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                t_start_ms,
                start_layout_size,
                Easing::Linear,
            );
        } else if delay_ms > 0.0 {
            preserve_instant_delayed_value(&mut track.text.text_paths, t_start_ms);
            preserve_instant_delayed_value(&mut track.geometry.size, t_start_ms);
            preserve_instant_delayed_value(&mut track.geometry.layout_size, t_start_ms);
        }
        if supports_morph_options {
            track.style.morph_options.ensure(MorphOptions::default()).add_keyframe(
                t_end_ms,
                morph_options,
                Easing::Linear,
            );
        }
        track
            .text
            .text_paths
            .ensure(Vec::new())
            .add_keyframe(t_end_ms, new_paths, easing);
        track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            new_half_size,
            easing,
        );
        track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            new_half_size,
            easing,
        );
        Ok(())
    }

    /// Process a text actor declaration (Text, Math, Code, Typst) and add it to the timeline.
    pub fn process_text_actor_decl(
        &mut self,
        actor_type: &str,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), RenderError> {
        let kind = match animatix_std::caps_for_type(actor_type).and_then(|c| c.text) {
            Some(animatix_core::caps::TextKind::Text) => TextDeclarationKind::Text,
            Some(animatix_core::caps::TextKind::Math) => TextDeclarationKind::Math,
            Some(animatix_core::caps::TextKind::Code) => TextDeclarationKind::Code,
            Some(animatix_core::caps::TextKind::Typst) => TextDeclarationKind::Typst,
            None => return Ok(()),
        };
        self.process_text_declaration(
            kind,
            Some(label),
            props,
            modifiers,
            time_ms,
            parent_label,
            diagnostics,
        )
    }
}
