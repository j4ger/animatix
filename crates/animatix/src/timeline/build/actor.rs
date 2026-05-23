//! Actor building logic: processes actor declarations, generates VelloPaths,
//! inserts keyframes, and dispatches to ActorKind implementations.

use super::*;
use crate::ast::{Expr, InlineItem, Property};
use crate::timeline::actor_kind::find_actor_kind;
use crate::timeline::vello_path::VelloPath;
use crate::timeline::plot::PlotCurveKind;

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    fn generate_actor_paths(
        &self,
        ty: &str,
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
        color: [f32; 4],
        stroke_width: f32,
        stroke_color: [f32; 4],
        fill_opacity: f32,
        extracted: &ExtractedActorProperties,
        eval_env: &Environment,
        vector_shape_state: &VectorShapeState,
        parent_label: Option<&str>,
    ) -> Vec<VelloPath> {
        let primitive = PrimitiveDescriptor::for_actor_type(ty);
        if primitive.is_graph_host() {
            return build_graph_axis_paths(size, extracted.x_domain, extracted.y_domain, stroke_color, false, false, false);
        }

        // VectorField, Heatmap, ContourSet, NumberPlane are build-time only; no runtime re-evaluation.
        if ty == "VectorField" || ty == "Heatmap" || ty == "ContourSet" || ty == "NumberPlane" {
            return vec![];
        }

        if primitive.is_plot_curve() {
            let p_label = parent_label.unwrap_or("").to_string();
            let mut p_x_domain = [-10.0, 10.0];
            let mut p_y_domain = [-10.0, 10.0];
            let mut p_size = [500.0, 500.0];

            if let Some(Value::Vec2(xd)) = self.env.get(&format!("{}_x_domain", p_label)) {
                p_x_domain = xd;
            }
            if let Some(Value::Vec2(yd)) = self.env.get(&format!("{}_y_domain", p_label)) {
                p_y_domain = yd;
            }
            if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                p_size = sz;
            }

            let kind = extracted.kind.unwrap_or(PlotCurveKind::Cartesian);
            let curve_params = PlotCurveParams {
                kind,
                func: &extracted.func,
                p_x_domain,
                p_y_domain,
                p_size,
                t_domain: extracted.t_domain,
                tolerance: extracted.tolerance,
                max_depth: extracted.max_depth,
                resolution: extracted.resolution,
                stroke_width,
                stroke_color,
                eval_env,
            };
            return build_plot_curve_paths(&curve_params);
        }

        if primitive.is_plot() {
            return vec![];
        }

        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let vello_path = build_vector_shape_vello_path(
            shape_type,
            vector_shape_state,
            VectorShapeStyle {
                color,
                stroke_width,
                stroke_color,
                fill_opacity,
            },
        )
        .unwrap_or_else(|| {
            build_shape_vello_path(
                shape_type,
                size,
                line_from,
                line_to,
                arc_angles,
                color,
                stroke_width,
                stroke_color,
                fill_opacity,
            )
        });
        vec![vello_path]
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_actor_keyframes(
        track: &mut AnimationTrack,
        t_start_ms: u64,
        t_end_ms: u64,
        position: [f32; 2],
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
        color: [f32; 4],
        shape_type: ShapeType,
        opacity: f32,
        stroke_width: f32,
        stroke_color: [f32; 4],
        stroke_progress: f32,
        fill_opacity: f32,
        vello_paths: Vec<VelloPath>,
        easing: Easing,
        duration_ms: f64,
        delay_ms: f64,
        supports_morph_options: bool,
        morph_options: MorphOptions,
    ) {
        if duration_ms > 0.0 {
            insert_start_keyframes(track, t_start_ms);
        } else if delay_ms > 0.0 {
            preserve_delayed_values(track, t_start_ms);
        }
        if supports_morph_options {
            track
                .morph_options
                .ensure(MorphOptions::default())
                .add_keyframe(t_end_ms, morph_options, Easing::Linear);
        }

        insert_end_keyframes(
            track,
            t_end_ms,
            position,
            size,
            line_from,
            line_to,
            arc_angles,
            color,
            shape_type,
            opacity,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            vello_paths,
            easing,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn process_actor_decl(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.add_node(label.to_string(), parent_label);

        if let Some(kind) = find_actor_kind(ty) {
            kind.build(self, label, ty, props, modifiers, children, time_ms, parent_label, diagnostics);
            return;
        }

        let Some(kind_id) = super::ActorKindId::from_type_name(ty) else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!("Unknown actor type '{}'", ty),
                )
                .with_subject(label),
            );
            return;
        };

        let primitive = PrimitiveDescriptor::for_actor_type(ty);
        let existing_track = self
            .tracks
            .get(label)
            .cloned()
            .unwrap_or_else(|| AnimationTrack::new(label.to_string()));

        // Math coordinate auto-mapping: if parent is a Graph, map child positions
        // from math coordinates to screen pixels.
        let mapped_props = if let Some(p_label) = parent_label {
            if self.env.get(&format!("{}_x_domain", p_label)).is_some() {
                self.map_props_to_graph_parent(p_label, props, time_ms, diagnostics)
            } else {
                props.to_vec()
            }
        } else {
            props.to_vec()
        };
        let props_ref: &[Property] = &mapped_props;

        let extracted = self.extract_actor_properties(label, ty, props_ref, time_ms, &existing_track, diagnostics);

        if primitive.is_graph_host() {
            self.env.set(&format!("{}_x_domain", label), Value::Vec2(extracted.x_domain));
            self.env.set(&format!("{}_y_domain", label), Value::Vec2(extracted.y_domain));
            self.env.set(
                &format!("{}_size", label),
                Value::Vec2([
                    extracted.initial_size[0] as f64 * 2.0,
                    extracted.initial_size[1] as f64 * 2.0,
                ]),
            );
        }

        self.process_inline_items(time_ms, children, label, diagnostics);
        let eval_env = self.build_eval_env(time_ms as u64);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, 0.0];
        let mut position = existing_track.position.last([0.0, 0.0]);
        let mut size = existing_track.size.last(default_size);
        let mut line_from = existing_track.line_from.last([-50.0, 0.0]);
        let mut line_to = existing_track.line_to.last([50.0, 0.0]);
        let mut arc_angles = existing_track.arc_angles.last(default_arc);
        let mut color = existing_track.color.last(DEFAULT_WHITE);
        let opacity = existing_track.opacity.last(1.0);
        let mut stroke_width = existing_track.stroke_width.last(2.0);
        let mut stroke_color = existing_track.stroke_color.last(DEFAULT_WHITE);
        let mut stroke_progress = existing_track.stroke_progress.last(1.0);
        let mut fill_opacity = existing_track.fill_opacity.last(1.0);
        let mut gap = extracted.gap;
        let mut padding = extracted.padding;
        let mut align = extracted.align.clone();
        let mut cols = extracted.cols;
        let vector_shape = crate::primitives::find_primitive(ty).filter(|p| p.is_shape());
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let mut vector_shape_state = self.build_vector_shape_state(
            ty, props, time_ms, size, line_from, line_to, arc_angles, diagnostics,
        );
        (size, line_from, line_to, arc_angles) = extract_shape_state_values(&vector_shape_state);

        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            morph_options,
        } = parse_timing_modifiers(modifiers, ModifierHost::ActorDeclaration, Some(label), diagnostics);
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let supports_morph_options = existing_track
            .vector_paths
            .as_ref()
            .map(|t| !t.keyframes.is_empty())
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

        let has_explicit_color = props.iter().any(|p| p.name == "color");
        let has_explicit_stroke = props.iter().any(|p| p.name == "stroke" || p.name == "stroke_color");
        let scheme_primitive = crate::primitives::find_primitive(ty);
        if !has_explicit_color {
            if let Some(primitive) = scheme_primitive {
                if let Some(scheme_color) = self.get_default_color(primitive, "color") {
                    color = scheme_color;
                    if primitive.category() == ActorCategory::Plot {
                        stroke_color = scheme_color;
                    }
                }
            }
        }
        if !has_explicit_stroke {
            if let Some(primitive) = scheme_primitive {
                if let Some(scheme_stroke) = self.get_default_color(primitive, "stroke") {
                    stroke_color = scheme_stroke;
                }
            }
        }

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "at" | "anchor" | "offset" => {}
                "color" => {
                    if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                        if let Some(actor_color) = self.auto_color_for_label(label) {
                            color = actor_color;
                            if primitive.is_plot_curve() {
                                stroke_color = actor_color;
                            }
                        } else {
                            diagnostics.push(Diagnostic::warning(
                                DiagnosticCode::UnknownColorReference,
                                DiagnosticPhase::Build,
                                format!(
                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; using the default color instead.",
                                    label
                                ),
                            )
                            .with_subject(&prop_subject));
                        }
                    } else if let Some(resolved_color) = parse_color_in_env_with_lookup_diagnostic(
                        label,
                        "color",
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        color = resolved_color;
                        if primitive.is_plot_curve() {
                            stroke_color = resolved_color;
                        }
                    }
                }
                "gap" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    gap = v.as_num() as f32;
                }
                "padding" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    padding = v.as_num() as f32;
                }
                "align" => {
                    if let Expr::Str(s) = &prop.value {
                        align = Some(s.clone());
                    } else if let Expr::Ident(s) = &prop.value {
                        align = Some(s.clone());
                    }
                }
                "cols" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0));
                    cols = Some(v.as_num().max(1.0) as usize);
                }
                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_width = v.as_num() as f32;
                }
                "stroke_color" | "stroke" => {
                    if let Some(resolved_color) = parse_color_in_env_with_lookup_diagnostic(
                        label,
                        "stroke_color",
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        stroke_color = resolved_color;
                    }
                }
                "stroke_progress" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_progress = v.as_num() as f32;
                }
                "fill_opacity" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    fill_opacity = v.as_num() as f32;
                }
                _ if vector_shape.is_some() => {
                    if apply_vector_shape_property(
                        ty,
                        &prop.name,
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                        &mut vector_shape_state,
                    ) {
                        (size, line_from, line_to, arc_angles) =
                            extract_shape_state_values(&vector_shape_state);
                    }
                }
                _ => {}
            }
        }

        if vector_shape.is_some() {
            finalize_vector_shape_state(ty, &mut vector_shape_state);
            (size, line_from, line_to, arc_angles) = extract_shape_state_values(&vector_shape_state);
        }

        if primitive.is_graph_host() || primitive.is_layout_container() {
            fill_opacity = 0.0;
            stroke_width = 0.0;
        }

        let vello_paths = self.generate_actor_paths(
            ty,
            size,
            line_from,
            line_to,
            arc_angles,
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
            &extracted,
            &eval_env,
            &vector_shape_state,
            parent_label,
        );

        let position_binding = resolve_position_binding_with_lookup_diagnostic(
            extracted.at_expr.as_ref(),
            extracted.anchor_expr.as_ref(),
            extracted.offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            label,
        );

        let track = self
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = kind_id;
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = t_start_ms;
        }

        if let Some((binding, bound_position)) = position_binding {
            preserve_discrete_position_state_before(track, t_start_ms);
            set_track_position_binding(track, t_start_ms, binding);
            if let Some(bound_position) = bound_position {
                position = bound_position;
            }
            mark_track_manual_position(track, t_start_ms);
        } else if primitive.is_layout_container() && parent_label.is_none() {
            preserve_discrete_position_state_before(track, t_start_ms);
            set_track_position_binding(track, t_start_ms, PositionBinding::ContainerDefault {
                anchor: SceneAnchor::Center,
            });
        }

        Self::insert_actor_keyframes(
            track,
            t_start_ms,
            t_end_ms,
            [position[0], position[1]],
            size,
            line_from,
            line_to,
            arc_angles,
            color,
            shape_type,
            opacity,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            vello_paths,
            easing,
            duration_ms,
            delay_ms,
            supports_morph_options,
            morph_options,
        );

        if primitive.is_layout_container() {
            self.register_container_metadata_and_apply_layout(
                label,
                ty,
                time_ms as u64,
                gap,
                padding,
                align.as_deref(),
                cols,
                diagnostics,
            );
        }
    }

    // === ActorKind Dispatch Methods ===

    /// Dispatch method for plot actor kinds (called from ActorKind trait impl)
    pub fn process_plot_actor_dispatch(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(kind_id) = super::ActorKindId::from_type_name(ty) else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!("Unknown actor type '{}'", ty),
                )
                .with_subject(label),
            );
            return;
        };

        let existing_track = self
            .tracks
            .get(label)
            .cloned()
            .unwrap_or_else(|| AnimationTrack::new(label.to_string()));

        if let Some((
            initial_size,
            line_from,
            line_to,
            arc_angles,
            color,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            shape_type,
            vello_paths,
            procedural_plot,
            tick_label_data,
        )) = self.process_plot_actor(
            label,
            ty,
            props,
            time_ms,
            parent_label,
            children,
            diagnostics,
            &existing_track,
        ) {
            // Use returned values for keyframe insertion
            let position = existing_track.position.last([0.0, 0.0]);
            let size = initial_size;
            let opacity = 1.0;

            let ParsedTimingModifiers {
                duration_ms,
                delay_ms,
                easing,
                morph_options: _,
            } = parse_timing_modifiers(
                modifiers,
                ModifierHost::ActorDeclaration,
                Some(label),
                diagnostics,
            );
            let t_start_ms = (time_ms + delay_ms) as u64;
            let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

            let track = self
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| AnimationTrack::new(label.to_string()));
            track.kind = kind_id;
            track.procedural_plot = procedural_plot;

            if track.first_seen_ms == u64::MAX {
                track.first_seen_ms = t_start_ms;
            }

            // === Keyframe Insertion ===
            if duration_ms > 0.0 {
                insert_start_keyframes(track, t_start_ms);
            } else if delay_ms > 0.0 {
                preserve_delayed_values(track, t_start_ms);
            }

            insert_end_keyframes(
                track,
                t_end_ms,
                position,
                size,
                line_from,
                line_to,
                arc_angles,
                color,
                shape_type,
                opacity,
                stroke_width,
                stroke_color,
                stroke_progress,
                fill_opacity,
                vello_paths,
                easing,
            );

            // === Tick Labels ===
            if let Some(ref tick_data) = tick_label_data {
                // X-axis tick labels (positioned below the axis line)
                for (i, &(sx, sy, val)) in tick_data.x_labels.iter().enumerate() {
                    let child_label = format!("{}_tick_x_{}", label, i);
                    let tick_props = vec![
                        Property::new("text", Expr::Str(format!("{:.1}", val))),
                        Property::new("at", Expr::Tuple(vec![Expr::Num(sx), Expr::Num(sy)])),
                        Property::new("font_size", Expr::Num(10.0)),
                        Property::new("color", Expr::Str("#888888".to_string())),
                    ];
                    let _ = self.process_text_actor_decl(
                        "Text",
                        &child_label,
                        &tick_props,
                        &[], // no modifiers
                        time_ms,
                        Some(label),
                        diagnostics,
                    );
                }

                // Y-axis tick labels (positioned to the left of the axis line)
                for (i, &(sx, sy, val)) in tick_data.y_labels.iter().enumerate() {
                    let child_label = format!("{}_tick_y_{}", label, i);
                    let tick_props = vec![
                        Property::new("text", Expr::Str(format!("{:.1}", val))),
                        Property::new("at", Expr::Tuple(vec![Expr::Num(sx), Expr::Num(sy)])),
                        Property::new("font_size", Expr::Num(10.0)),
                        Property::new("color", Expr::Str("#888888".to_string())),
                    ];
                    let _ = self.process_text_actor_decl(
                        "Text",
                        &child_label,
                        &tick_props,
                        &[], // no modifiers
                        time_ms,
                        Some(label),
                        diagnostics,
                    );
                }
            }

            // === Container Layout ===
            let primitive = PrimitiveDescriptor::for_actor_type(ty);
            if primitive.is_layout_container() {
                self.register_container_metadata_and_apply_layout(
                    label,
                    ty,
                    t_start_ms,
                    0.0,
                    0.0,
                    Some("center"),
                    None,
                    diagnostics,
                );
            }
        }
    }
}