use kurbo::Shape;

use super::*;
use crate::renderer::text::{TextCompileRequest, TextStyle};
use crate::timeline::TextTracks;
use crate::timeline::entity::EntityId;
use crate::timeline::env::Environment;

struct ParsedTextProperties {
    text_content: String,
    font_family: String,
    font_size: f32,
    font_weight: f32,
    font_style: String,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    text_align: String,
    text_anchor: String,
    overflow: String,
    color_rgba: [f32; 4],
    initial_track_color: Option<[f32; 4]>,
    at_expr: Option<Expr>,
    anchor_expr: Option<Expr>,
    offset_expr: Option<Expr>,
    split_by: Option<String>,
    split_stagger: Option<f32>,
    split_offset_y: Option<f32>,
    split_mask: Option<bool>,
}

impl ParsedTextProperties {
    fn new(kind: TextDeclarationKind) -> Self {
        Self {
            text_content: String::new(),
            font_family: String::new(),
            font_size: kind.default_font_size(),
            font_weight: 400.0,
            font_style: "normal".to_string(),
            line_height: 1.2,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            max_width: 0.0,
            text_align: "left".to_string(),
            text_anchor: "center".to_string(),
            overflow: "visible".to_string(),
            color_rgba: [1.0, 1.0, 1.0, 1.0],
            initial_track_color: None,
            at_expr: None,
            anchor_expr: None,
            offset_expr: None,
            split_by: None,
            split_stagger: None,
            split_offset_y: None,
            split_mask: None,
        }
    }
}

impl Timeline {
    pub(super) fn process_text_declaration(
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
            .world
            .get_id(&label_str)
            .and_then(|id| self.world.text(id)?.text_paths.as_ref())
            .map(|t| !t.keyframes.is_empty())
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

        let parsed = self.parse_text_properties(kind, &label_str, props, &eval_env, diagnostics);

        // Pre-seed opacity for pre-keyframe first declarations without explicit opacity.
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.world.tracks.contains_key(&label_str);

        let track_id = self.world.spawn(&label_str, kind.type_name());
        let style = self.world.style.get_mut(track_id);

        self.world
            .actor_types
            .get_mut(track_id)
            .clone_from(&kind.type_name().to_string());

        let meta = self
            .world
            .meta
            .get_or_insert_with(track_id, crate::timeline::world::EntityMeta::default);
        if meta.first_seen_ms == u64::MAX {
            meta.first_seen_ms = t_start_ms;
        }

        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            self.world
                .meta
                .get_or_insert_with(track_id, crate::timeline::world::EntityMeta::default)
                .hidden_by_default = true;
            style.opacity.ensure(1.0).add_keyframe(0, self.default_opacity, Easing::Linear);
        }

        if let Some(track_color) = parsed.initial_track_color {
            if delay_ms > 0.0 && duration_ms == 0.0 {
                preserve_instant_delayed_value(&mut style.color, t_start_ms);
            }
            style
                .color
                .ensure(DEFAULT_WHITE)
                .add_keyframe(t_start_ms, track_color, Easing::Linear);
        }

        if let Some((binding, position)) = resolve_position_binding_with_lookup_diagnostic(
            parsed.at_expr.as_ref(),
            parsed.anchor_expr.as_ref(),
            parsed.offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            &label_str,
        ) {
            let geometry = self.world.geometry.get_mut(track_id);
            if delay_ms > 0.0 && duration_ms == 0.0 {
                preserve_discrete_position_state_before(geometry, t_start_ms);
                preserve_instant_delayed_value(&mut geometry.position, t_start_ms);
            }
            apply_explicit_position_binding(geometry, t_start_ms, binding, position);
        }

        self.apply_remaining_text_properties(
            track_id,
            kind,
            &label_str,
            props,
            &eval_env,
            t_start_ms,
            t_end_ms,
            easing,
            diagnostics,
        );

        self.world.ensure_text(track_id);
        let text = self.world.text.get_mut(track_id).expect("text component");
        Self::apply_text_track_keyframes(text, &parsed, t_end_ms, easing, kind);

        if kind == TextDeclarationKind::Counter {
            return self.process_counter_declaration(
                &label_str,
                props,
                &eval_env,
                diagnostics,
                &parsed,
                t_end_ms,
                easing,
                track_id,
            );
        }

        self.compile_and_apply_text_paths(
            track_id,
            kind,
            label,
            &label_str,
            &parsed,
            duration_ms,
            delay_ms,
            t_start_ms,
            t_end_ms,
            easing,
            supports_morph_options,
            morph_options,
            diagnostics,
        )
    }

    fn parse_text_split_prop(
        &mut self,
        prop: &Property,
        eval_env: &Environment,
        label_str: &str,
        split_by: &mut Option<String>,
        split_stagger: &mut Option<f32>,
        split_offset_y: &mut Option<f32>,
        split_mask: &mut Option<bool>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        let prop_subject = format!("{}.{}", label_str, prop.name);
        match prop.name.as_str() {
            "split_by" => {
                *split_by = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .map(|v| v.as_str());
                true
            },
            "split_stagger" => {
                *split_stagger = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .map(|v| v.as_num() as f32);
                true
            },
            "split_offset_y" => {
                *split_offset_y = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .map(|v| v.as_num() as f32);
                true
            },
            "split_mask" => {
                *split_mask = match &prop.value {
                    Expr::Bool(b) => Some(*b),
                    Expr::Ident(s) => Some(s == "true" || s == "baseline"),
                    _ => Some(false),
                };
                true
            },
            _ => false,
        }
    }

    fn resolve_text_color(
        &mut self,
        prop: &Property,
        label_str: &str,
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<[f32; 4]> {
        let prop_subject = format!("{}.{}", label_str, prop.name);
        if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
            self.auto_color_for_label(label_str).or_else(|| {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownColorReference,
                        DiagnosticPhase::Build,
                        format!(
                            "Color value 'auto' on '{label_str}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; using the default color instead."
                        ),
                    )
                    .with_subject(&prop_subject),
                );
                None
            })
        } else {
            parse_color_in_env_with_lookup_diagnostic(
                label_str,
                "color",
                &prop.value,
                eval_env,
                diagnostics,
                &prop_subject,
            )
        }
    }

    fn resolve_default_text_color(&self, kind: TextDeclarationKind) -> Option<[f32; 4]> {
        let primitive_type = match kind {
            TextDeclarationKind::Text => "Text",
            TextDeclarationKind::Code => "Code",
            TextDeclarationKind::Typst => "Typst",
            TextDeclarationKind::Math => "Math",
            TextDeclarationKind::Counter => "Counter",
        };
        let primitive = self.primitive_registry.find(primitive_type)?;
        let caps = animatix_std::caps_for_type(primitive_type).unwrap_or_default();
        self.get_default_color(primitive, &caps, "color")
    }

    fn parse_text_properties(
        &mut self,
        kind: TextDeclarationKind,
        label_str: &str,
        props: &[Property],
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> ParsedTextProperties {
        let mut parsed = ParsedTextProperties::new(kind);

        for prop in props {
            if self.parse_text_split_prop(
                prop,
                eval_env,
                label_str,
                &mut parsed.split_by,
                &mut parsed.split_stagger,
                &mut parsed.split_offset_y,
                &mut parsed.split_mask,
                diagnostics,
            ) {
                continue;
            }

            let prop_subject = format!("{}.{}", label_str, prop.name);
            match prop.name.as_str() {
                name if kind.content_matches(name) => {
                    parsed.text_content = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_default();
                },
                "font_family" => {
                    parsed.font_family = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default();
                },
                "font_size" => {
                    parsed.font_size = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map_or(0.0, |v| v.as_num() as f32);
                },
                "font_weight" => {
                    if let Some(val) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        match val {
                            super::Value::Str(s) => {
                                parsed.font_weight = crate::renderer::text::parse_font_weight(&s);
                            },
                            super::Value::Num(n) => {
                                parsed.font_weight = n as f32;
                            },
                            _ => {}, // Unrecognized font weight value kinds fall back to default.
                        }
                    }
                },
                "font_style" => {
                    parsed.font_style = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "normal".to_string());
                },
                "line_height" => {
                    parsed.line_height = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map_or(1.2, |v| v.as_num() as f32);
                },
                "letter_spacing" => {
                    parsed.letter_spacing = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map_or(0.0, |v| v.as_num() as f32);
                },
                "word_spacing" => {
                    parsed.word_spacing = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map_or(0.0, |v| v.as_num() as f32);
                },
                "text_max_width" | "max_width" => {
                    parsed.max_width = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map_or(0.0, |v| v.as_num() as f32);
                },
                "text_align" => {
                    parsed.text_align = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "left".to_string());
                },
                "text_anchor" => {
                    parsed.text_anchor = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "center".to_string());
                },
                "overflow" => {
                    parsed.overflow = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .map(|v| v.as_str())
                    .unwrap_or_else(|| "visible".to_string());
                },
                "color" => {
                    if let Some(resolved_color) =
                        self.resolve_text_color(prop, label_str, eval_env, diagnostics)
                    {
                        parsed.initial_track_color = Some(resolved_color);
                        parsed.color_rgba = resolved_color;
                    }
                },
                "at" => parsed.at_expr = Some(prop.value.clone()),
                "anchor" => parsed.anchor_expr = Some(prop.value.clone()),
                "offset" => parsed.offset_expr = Some(prop.value.clone()),
                _ => {}, // Non-position properties are parsed by apply_remaining_text_properties.
            }
        }

        if parsed.initial_track_color.is_none()
            && let Some(scheme_color) = self.resolve_default_text_color(kind)
        {
            parsed.initial_track_color = Some(scheme_color);
            parsed.color_rgba = scheme_color;
        }

        parsed
    }

    fn apply_remaining_text_properties(
        &mut self,
        track_id: EntityId,
        kind: TextDeclarationKind,
        label_str: &str,
        props: &[Property],
        eval_env: &Environment,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut pending_writes: Vec<(
            crate::timeline::property_registry::ActorField,
            crate::timeline::property_engine::PropertyValue,
        )> = Vec::new();
        let actor_caps = self.world.caps(track_id);
        let actor_type = kind.type_name();
        for prop in props {
            let already_handled = match prop.name.as_str() {
                name if kind.content_matches(name) => true,
                "font_family" | "font_size" | "font_weight" | "font_style" | "line_height"
                | "letter_spacing" | "word_spacing" | "max_width" | "text_max_width"
                | "text_align" | "text_anchor" | "overflow" | "color" | "at" | "anchor"
                | "offset" => true,
                _ => false,
            };
            if already_handled {
                continue;
            }
            let prop_subject = format!("{}.{}", label_str, prop.name);
            if let Some(schema) = lookup_property(&prop.name)
                && let Some(pv) = parse_property_value(
                    schema.value_type,
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
            {
                pending_writes.push((schema.field_for_actor(&actor_caps, actor_type), pv));
            }
        }

        for (field, pv) in pending_writes {
            self.world.write_property_field(
                track_id,
                field,
                pv,
                t_start_ms,
                t_end_ms,
                easing,
                diagnostics,
            );
        }
    }

    fn apply_text_track_keyframes(
        text: &mut TextTracks,
        parsed: &ParsedTextProperties,
        t_end_ms: u64,
        easing: Easing,
        kind: TextDeclarationKind,
    ) {
        text.text_content.ensure(String::new()).add_keyframe(
            t_end_ms,
            parsed.text_content.clone(),
            easing,
        );
        let font_family = if parsed.font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            parsed.font_family.clone()
        };
        text.font_family
            .ensure(crate::renderer::text::DEFAULT_FONT_FAMILY.to_string())
            .add_keyframe(t_end_ms, font_family, easing);
        text.font_size.ensure(kind.default_font_size()).add_keyframe(
            t_end_ms,
            parsed.font_size,
            easing,
        );
        text.font_weight
            .ensure(400.0)
            .add_keyframe(t_end_ms, parsed.font_weight, easing);
        text.font_style.ensure("normal".to_string()).add_keyframe(
            t_end_ms,
            parsed.font_style.clone(),
            easing,
        );
        text.line_height.ensure(1.2).add_keyframe(t_end_ms, parsed.line_height, easing);
        text.letter_spacing
            .ensure(0.0)
            .add_keyframe(t_end_ms, parsed.letter_spacing, easing);
        text.word_spacing
            .ensure(0.0)
            .add_keyframe(t_end_ms, parsed.word_spacing, easing);
        text.text_max_width.ensure(0.0).add_keyframe(t_end_ms, parsed.max_width, easing);
        text.text_align.ensure("left".to_string()).add_keyframe(
            t_end_ms,
            parsed.text_align.clone(),
            easing,
        );
        text.text_anchor.ensure("center".to_string()).add_keyframe(
            t_end_ms,
            parsed.text_anchor.clone(),
            easing,
        );
        text.overflow.ensure("visible".to_string()).add_keyframe(
            t_end_ms,
            parsed.overflow.clone(),
            easing,
        );
        if let Some(ref by) = parsed.split_by {
            text.split_by
                .ensure("char".to_string())
                .add_keyframe(t_end_ms, by.clone(), easing);
        }
        if let Some(stagger) = parsed.split_stagger {
            text.split_stagger.ensure(0.0).add_keyframe(t_end_ms, stagger, easing);
        }
        if let Some(offset_y) = parsed.split_offset_y {
            text.split_offset_y.ensure(0.0).add_keyframe(t_end_ms, offset_y, easing);
        }
        if let Some(mask) = parsed.split_mask {
            text.split_mask.ensure(false).add_keyframe(t_end_ms, mask, easing);
        }
    }

    fn process_counter_declaration(
        &mut self,
        label_str: &str,
        props: &[Property],
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        parsed: &ParsedTextProperties,
        t_end_ms: u64,
        easing: Easing,
        track_id: EntityId,
    ) -> Result<(), RenderError> {
        let mut initial_val = 0.0f32;
        let mut prefix_str = String::new();
        let mut suffix_str = String::new();
        let mut decimals_val = 0u32;
        let mut use_comma_val = false;

        for prop in props {
            match prop.name.as_str() {
                "value" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &format!("{}.value", label_str),
                    ) {
                        initial_val = v.as_num() as f32;
                    }
                },
                "prefix" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &format!("{}.prefix", label_str),
                    ) {
                        prefix_str = v.as_str();
                    }
                },
                "suffix" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &format!("{}.suffix", label_str),
                    ) {
                        suffix_str = v.as_str();
                    }
                },
                "decimals" => {
                    if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &format!("{}.decimals", label_str),
                    ) {
                        decimals_val = v.as_num() as u32;
                    }
                },
                "comma" => {
                    if let Expr::Bool(b) = &prop.value {
                        use_comma_val = *b;
                    }
                },
                _ => {}, // Remaining Counter properties are handled by general text/style pipeline.
            }
        }

        let font_family = if parsed.font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            parsed.font_family.clone()
        };

        let prebaked = crate::primitives::counter::prebake_counter(
            &font_family,
            parsed.font_size,
            parsed.font_weight,
            &parsed.font_style,
            parsed.line_height,
            parsed.color_rgba,
            initial_val,
            decimals_val,
            use_comma_val,
            &prefix_str,
            &suffix_str,
            self.font_context.as_ref(),
        )?;

        let text = self.world.text.get_mut(track_id).expect("text component");
        let geometry = self.world.geometry.get_mut(track_id);

        text.set_metrics(t_end_ms, prebaked.data.ascent, prebaked.data.descent, 0.0);
        text.counter_data = Some(prebaked.data);
        geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            prebaked.initial_half_size,
            easing,
        );
        geometry.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            prebaked.initial_half_size,
            easing,
        );
        Ok(())
    }

    fn compile_and_apply_text_paths(
        &mut self,
        track_id: EntityId,
        kind: TextDeclarationKind,
        label: Option<&str>,
        _label_str: &str,
        parsed: &ParsedTextProperties,
        duration_ms: f64,
        delay_ms: f64,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        supports_morph_options: bool,
        morph_options: MorphOptions,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), RenderError> {
        let text = self.world.text.get_mut(track_id).expect("text component");
        let text_kind = match kind {
            TextDeclarationKind::Text | TextDeclarationKind::Counter => {
                crate::renderer::text::TextKind::Text
            },
            TextDeclarationKind::Code => crate::renderer::text::TextKind::Code,
            TextDeclarationKind::Typst => crate::renderer::text::TextKind::Typst,
            TextDeclarationKind::Math => crate::renderer::text::TextKind::Math,
        };

        let font_family = if parsed.font_family.is_empty() {
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        } else {
            parsed.font_family.clone()
        };

        let language = text.language.get(t_end_ms, String::new());
        let compiled = {
            let text_compiler = self.text_compiler.borrow();
            let request = TextCompileRequest {
                kind: text_kind,
                content: &parsed.text_content,
                language: &language,
                palette: text_compiler.highlight_palette.as_ref(),
                style: TextStyle {
                    font_family: &font_family,
                    font_size: parsed.font_size,
                    font_weight: parsed.font_weight,
                    font_style: &parsed.font_style,
                    line_height: parsed.line_height,
                    letter_spacing: parsed.letter_spacing,
                    word_spacing: parsed.word_spacing,
                    color: parsed.color_rgba,
                },
                max_width: parsed.max_width,
                max_lines: None,
                text_align: &parsed.text_align,
                overflow: &parsed.overflow,
                allow_fast_path: text_compiler.text_fast_path,
            };
            crate::renderer::text::compile_text_cached(&request, self.font_context.as_ref())?
        };

        let missing_glyphs = crate::renderer::text::missing_text_glyphs(
            text_kind,
            &parsed.text_content,
            &font_family,
            parsed.font_weight,
            &parsed.font_style,
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
                    format!("Text '{subject}' has no glyph for {listed} in the fast-path face set; they will be skipped or drawn as boxes. Use characters the font covers, or register a font that has them."),
                )
                .with_subject(subject),
            );
        }

        let mut new_paths = compiled.paths.to_vec();
        let new_half_size = compiled.half_size;
        let anchor_offset_x = match parsed.text_anchor.as_str() {
            "left" => new_half_size[0],
            "right" => -new_half_size[0],
            _ => 0.0,
        };
        if anchor_offset_x != 0.0 {
            let affine = kurbo::Affine::translate((anchor_offset_x as f64, 0.0));
            for p in &mut new_paths {
                p.path.apply_affine(affine);
                p.glyph_center[0] += anchor_offset_x;
            }
        }

        let text = self.world.text.get_mut(track_id).expect("text component");
        let style = self.world.style.get_mut(track_id);
        let geometry = self.world.geometry.get_mut(track_id);

        text.set_metrics(t_end_ms, compiled.ascent, compiled.descent, compiled.baseline_offset);

        if duration_ms > 0.0 {
            let start_val = text.evaluate_paths(style, t_start_ms);
            text.text_paths
                .ensure(Vec::new())
                .add_keyframe(t_start_ms, start_val, Easing::Linear);
            let start_size = geometry.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
            let start_layout_size =
                geometry.layout_size_get(t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
            geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                t_start_ms,
                start_size,
                Easing::Linear,
            );
            geometry.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                t_start_ms,
                start_layout_size,
                Easing::Linear,
            );
        } else if delay_ms > 0.0 {
            preserve_instant_delayed_value(&mut text.text_paths, t_start_ms);
            preserve_instant_delayed_value(&mut geometry.size, t_start_ms);
            preserve_instant_delayed_value(&mut geometry.layout_size, t_start_ms);
        }
        if supports_morph_options {
            style.morph_options.ensure(MorphOptions::default()).add_keyframe(
                t_end_ms,
                morph_options,
                Easing::Linear,
            );
        }
        text.static_split_clip = crate::renderer::text::compute_split_mask_clip(&new_paths);
        text.static_paths = if duration_ms == 0.0 {
            Some(std::sync::Arc::from(new_paths.clone()))
        } else {
            None
        };
        text.hardware_layout = if duration_ms == 0.0 && anchor_offset_x == 0.0 {
            compiled.hardware_layout.clone()
        } else {
            None
        };
        let text_bounds = (duration_ms == 0.0)
            .then(|| {
                let mut text_bounds: Option<kurbo::Rect> = None;
                for p in &new_paths {
                    let b = p.path.bounding_box();
                    text_bounds = Some(match text_bounds {
                        Some(existing) => existing.union(b),
                        None => b,
                    });
                }
                text_bounds
            })
            .flatten();
        text.text_paths.ensure(Vec::new()).add_keyframe(t_end_ms, new_paths, easing);
        geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            new_half_size,
            easing,
        );
        geometry.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_end_ms,
            new_half_size,
            easing,
        );
        if let Some(b) = text_bounds {
            self.world.set_local_bounds(track_id, b);
        }
        Ok(())
    }
}
