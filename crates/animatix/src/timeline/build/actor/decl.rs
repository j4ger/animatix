//! Actor declaration pipeline.

use super::helpers::ActorKeyframeTiming;
use super::prepare::ActorPrep;
use super::*;
use crate::ast::{InlineItem, Property};
use props::DeclarationSeed;

mod props;

impl Timeline {
    /// Warn when a later declaration reuses an existing scene-global label
    /// under a different parent. Same-parent re-declaration is legal morphing;
    /// cross-parent reuse silently moves the shared entity and leaves the old
    /// scope without that child.
    fn warn_duplicate_label_parent(
        &self,
        label: &str,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        decl_span: Option<crate::ast::Span>,
    ) {
        let Some(existing_id) = self.world.get_id(label) else {
            return;
        };
        let old_parent = self
            .world
            .hierarchy(existing_id)
            .parent
            .and_then(|parent_id| self.world.label(parent_id));
        match (old_parent, parent_label) {
            (Some(old_parent), Some(new_parent)) if old_parent != new_parent => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::DuplicateLabel,
                        DiagnosticPhase::Build,
                        format!(
                            "Actor label '{label}' is already declared under '{old_parent}'; \
                             reusing it under '{new_parent}' re-parents it and leaves the old \
                             scope without that child"
                        ),
                    )
                    .with_subject(label)
                    .with_ast_span(decl_span),
                );
            },
            (None, Some(new_parent)) => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::DuplicateLabel,
                        DiagnosticPhase::Build,
                        format!(
                            "Actor label '{label}' is already declared at scene root; reusing \
                             it inside '{new_parent}' re-parents the shared actor"
                        ),
                    )
                    .with_subject(label)
                    .with_ast_span(decl_span),
                );
            },
            _ => {}, // Same-parent or un-parented redeclarations do not emit reparenting warnings.
        }
    }

    pub(crate) fn process_actor_decl(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        decl_span: Option<crate::ast::Span>,
    ) {
        self.warn_duplicate_label_parent(label, parent_label, diagnostics, decl_span);
        self.add_node(label.to_string(), parent_label);

        if self.try_dispatch_prior(
            label,
            ty,
            props,
            modifiers,
            children,
            time_ms,
            parent_label,
            diagnostics,
            decl_span,
        ) {
            return;
        }

        let ActorPrep {
            caps,
            primitive,
            extracted,
        } = match self.prepare_actor_build(
            label,
            ty,
            props,
            children,
            time_ms,
            parent_label,
            diagnostics,
        ) {
            Some(prep) => prep,
            None => return,
        };
        let eval_env = self.build_eval_env(time_ms as u64);

        let DeclarationSeed {
            mut state,
            position,
            shape_type,
            vector_shape,
            legend_color: existing_legend_color,
            has_explicit_color,
            has_explicit_stroke,
        } = self.seed_declaration_property_state(label, ty, props, time_ms, &caps, diagnostics);

        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            morph_options,
            ..
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::ActorDeclaration,
            Some(label),
            diagnostics,
        );
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let supports_morph_options =
            self.validate_decl_morph_options(label, duration_ms, morph_options, diagnostics);

        self.apply_scheme_default_colors(
            ty,
            &caps,
            has_explicit_color,
            has_explicit_stroke,
            &mut state,
        );
        self.apply_explicit_declaration_props(
            &mut state,
            label,
            ty,
            props,
            &eval_env,
            &primitive,
            vector_shape,
            diagnostics,
        );

        self.warn_unknown_declaration_properties(label, ty, props, diagnostics);

        let legend_color = if has_explicit_color {
            Some(state.color)
        } else if has_explicit_stroke {
            Some(state.stroke_color)
        } else {
            existing_legend_color
        };

        Self::finalize_declaration_state(
            &mut state,
            ty,
            &caps,
            &primitive,
            props,
            has_explicit_color,
            has_explicit_stroke,
            vector_shape,
        );

        self.apply_pre_spawn_line_anchor_refs(label, &caps, props);

        let vello_paths = self.generate_actor_paths(
            label,
            ty,
            state.size,
            state.line_from,
            state.line_to,
            state.arc_angles,
            state.color,
            state.stroke_width,
            state.stroke_color,
            state.fill_opacity,
            &extracted,
            &eval_env,
            &state.vector_shape_state,
            parent_label,
        );

        self.warn_layout_managed_position(label, parent_label, &extracted, props, diagnostics);

        let position_binding = resolve_position_binding_with_lookup_diagnostic(
            extracted.at_expr.as_ref(),
            extracted.anchor_expr.as_ref(),
            extracted.offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            label,
        );

        let frame = self.begin_actor_decl(label, ty, props, decl_span);
        let track_id = frame.track_id;
        self.apply_spawned_declaration_props(
            track_id,
            &caps,
            &primitive,
            legend_color,
            &extracted,
            props,
            &eval_env,
            label,
            parent_label,
            t_start_ms,
            t_end_ms,
            easing,
            diagnostics,
            &mut state,
        );

        let timing = ActorKeyframeTiming {
            t_start_ms,
            t_end_ms,
            easing,
            duration_ms,
            delay_ms,
            supports_morph_options,
            morph_options,
        };
        let values = state.into_keyframe_values(position, shape_type, vello_paths);
        self.finish_actor_decl(
            &frame,
            primitive.is_layout_container(),
            position_binding,
            parent_label,
            &timing,
            values,
        );

        if caps.transcript {
            super::super::speech::attach_transcript_from_props(
                self,
                label,
                props,
                &eval_env,
                diagnostics,
            );
        }

        self.write_extension_properties_for_decl(label, ty, props, modifiers, time_ms, diagnostics);

        self.finalize_actor_container(label, ty, props, time_ms, parent_label, diagnostics);
    }

    /// Record `from:`/`to:` actor-anchor refs (`pen.center`) in the track's
    /// shape side channel for frame-time resolution.
    pub(super) fn apply_pre_spawn_line_anchor_refs(
        &mut self,
        label: &str,
        caps: &ActorCaps,
        props: &[Property],
    ) {
        if !matches!(caps.shape, Some(ShapeKind::Line | ShapeKind::Arrow)) {
            return;
        }
        let Some(id) = self.world.get_id(label) else {
            return;
        };
        let shape = self.world.ensure_shape(id);
        for prop in props {
            if prop.name != "from" && prop.name != "to" {
                continue;
            }
            let crate::ast::Expr::Path(segments) = &prop.value else {
                continue;
            };
            if segments.len() != 2 {
                continue;
            }
            let Some(anchor) = SceneAnchor::from_str(&segments[1]) else {
                continue;
            };
            if prop.name == "from" {
                shape.from_anchor = Some((segments[0].clone(), anchor));
            } else {
                shape.to_anchor = Some((segments[0].clone(), anchor));
            }
        }
    }

    fn validate_decl_morph_options(
        &self,
        label: &str,
        duration_ms: f64,
        morph_options: crate::timeline::morph::MorphOptions,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        let supports_morph_options = self
            .world
            .get_id(label)
            .map(|id| self.world.shape_or_default(id))
            .and_then(|shape| shape.vector_paths.as_ref())
            .map(|track| !track.keyframes.is_empty())
            .unwrap_or(false)
            && duration_ms > 0.0;
        if has_non_default_morph_options(morph_options) && !supports_morph_options {
            push_modifier_diagnostic(
                diagnostics,
                DiagnosticCode::InvalidModifierValue,
                "Morph-specific modifiers on actor declarations require a path-morphing re-declaration with non-zero duration; ignoring them for now."
                    .to_string(),
                Some(label),
            );
        }
        supports_morph_options
    }
}
