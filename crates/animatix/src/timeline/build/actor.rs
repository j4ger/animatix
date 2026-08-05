//! Actor building logic: processes actor declarations, generates VelloPaths,
//! inserts keyframes, and dispatches to ActorKind implementations.

use super::plot::ProcessedPlotActor;
use super::*;
use crate::ast::{Expr, InlineItem, Property};
use crate::timeline::actor_kind::find_actor_kind;
use crate::timeline::plot::PlotCurveKind;
use crate::timeline::vello_path::VelloPath;

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
            return build_graph_axis_paths(
                size,
                extracted.x_domain,
                extracted.y_domain,
                stroke_color,
                false,
                false,
                false,
                extracted.graph_padding,
                extracted.x_scale,
                extracted.y_scale,
            );
        }

        // VectorField, Heatmap, ContourSet, NumberPlane are build-time only; no runtime
        // re-evaluation.
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

            let p_padding = self
                .env
                .get(&format!("{}_padding", p_label))
                .and_then(|v| {
                    if let Value::Vec4(p) = v {
                        Some(p)
                    } else {
                        None
                    }
                })
                .unwrap_or([0.0; 4]);

            let kind = extracted.kind.unwrap_or(PlotCurveKind::Cartesian);
            let curve_params = PlotCurveParams {
                kind,
                func: &extracted.func,
                p_x_domain,
                p_y_domain,
                p_size,
                p_padding,
                t_domain: extracted.t_domain,
                tolerance: extracted.tolerance,
                max_depth: extracted.max_depth,
                resolution: extracted.resolution,
                stroke_width,
                stroke_color,
                eval_env,
                build_quality: self.build_quality,
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
                line_cap: 0,
                line_join: 0,
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
            track.style.morph_options.ensure(MorphOptions::default()).add_keyframe(
                t_end_ms,
                morph_options,
                Easing::Linear,
            );
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

        // H2: Math is deprecated, normalize to Typst
        if ty == "Math" {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::DeprecatedPrimitive,
                    DiagnosticPhase::Build,
                    "'Math' is deprecated. Use 'Typst' instead for math expressions.".to_string(),
                )
                .with_subject(label),
            );
        }
        let ty = if ty == "Math" { "Typst" } else { ty };

        if let Some(kind) = find_actor_kind(ty) {
            kind.build(
                self,
                label,
                ty,
                props,
                modifiers,
                children,
                time_ms,
                parent_label,
                diagnostics,
            );
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

        let extracted = self.extract_actor_properties(
            label,
            ty,
            props_ref,
            time_ms,
            &existing_track,
            diagnostics,
        );

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

        // Set track.kind before processing children so the layout-managed-child
        // check can inspect the parent's kind. Only create track entry if the actor
        // has children (otherwise skip to avoid breaking is_first_decl).
        if !children.is_empty() {
            let early_track = self
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| AnimationTrack::new(label.to_string()));
            early_track.kind = kind_id;
        }

        self.process_inline_items(time_ms, children, label, diagnostics);
        let eval_env = self.build_eval_env(time_ms as u64);

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, 0.0];
        let mut position = existing_track.geometry.position.last([0.0, 0.0]);
        let mut size = existing_track.geometry.size.last(default_size);
        let mut line_from = existing_track.shape.line_from.last([-50.0, 0.0]);
        let mut line_to = existing_track.shape.line_to.last([50.0, 0.0]);
        let mut arc_angles = existing_track.shape.arc_angles.last(default_arc);
        let mut color = existing_track.style.color.last(DEFAULT_WHITE);
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.tracks.contains_key(label);
        let opacity = if is_first_decl && !has_explicit_opacity {
            self.default_opacity
        } else {
            existing_track.style.opacity.last(1.0)
        };
        let mut stroke_width = existing_track.style.stroke_width.last(2.0);
        let mut stroke_color = existing_track.style.stroke_color.last(DEFAULT_WHITE);
        let mut stroke_progress = existing_track.style.stroke_progress.last(1.0);
        let mut fill_opacity = existing_track.style.fill_opacity.last(1.0);

        let vector_shape = crate::primitives::find_primitive(ty).filter(|p| p.is_shape());
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let mut vector_shape_state = self.build_vector_shape_state(
            ty,
            props,
            time_ms,
            size,
            line_from,
            line_to,
            arc_angles,
            diagnostics,
        );
        (size, line_from, line_to, arc_angles) = extract_shape_state_values(&vector_shape_state);

        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            morph_options,
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::ActorDeclaration,
            Some(label),
            diagnostics,
        );
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let supports_morph_options = existing_track
            .shape
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
        let has_explicit_stroke =
            props.iter().any(|p| p.name == "stroke" || p.name == "stroke_color");
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

        let mut stroke_color_explicitly_set = has_explicit_stroke;

        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "at" | "anchor" | "offset" => {},
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
                },

                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_width = v.as_num() as f32;
                },
                "stroke_color" | "stroke" => {
                    stroke_color_explicitly_set = true;
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
                },
                "stroke_progress" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    stroke_progress = v.as_num() as f32;
                },
                "fill_opacity" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    fill_opacity = v.as_num() as f32;
                },
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
                },
                _ => {},
            }
        }

        // For Line actors, inherit stroke_color from color since Line is stroke-only
        if !stroke_color_explicitly_set
            && kind_id == super::ActorKindId::Shape(super::ShapeKind::Line)
        {
            stroke_color = color;
        }

        if vector_shape.is_some() {
            finalize_vector_shape_state(ty, &mut vector_shape_state);
            (size, line_from, line_to, arc_angles) =
                extract_shape_state_values(&vector_shape_state);
        }

        if primitive.is_graph_host() || primitive.is_layout_container() {
            fill_opacity = 0.0;
            stroke_width = 0.0;
        }

        // G6: Detect actor-anchor refs in `from`/`to` property declarations.
        // Store them in the track's side-channel so the primitive's frame-time
        // `evaluate` method can resolve them each frame.
        // G6: Detect actor-anchor refs in `from`/`to` property declarations.
        // Store them in the track's side-channel so the primitive's frame-time
        // `evaluate` method can resolve them each frame.
        if matches!(
            kind_id,
            super::ActorKindId::Shape(super::ShapeKind::Line | super::ShapeKind::Arrow)
        ) {
            if let Some(track) = self.tracks.get_mut(label) {
                for prop in props {
                    if prop.name == "from" || prop.name == "to" {
                        if let Expr::Path(segments) = &prop.value {
                            if segments.len() == 2 {
                                if let Some(anchor) = SceneAnchor::from_str(&segments[1]) {
                                    if prop.name == "from" {
                                        track.shape.from_anchor =
                                            Some((segments[0].clone(), anchor));
                                    } else {
                                        track.shape.to_anchor = Some((segments[0].clone(), anchor));
                                    }
                                }
                            }
                        }
                    }
                }
            }
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

        // Warn when a child of a layout container uses `at` or `position`.
        // Layout-managed children should use `transform` for visual offsets.
        if let Some(parent) = parent_label {
            if let Some(parent_track) = self.tracks.get(parent) {
                if matches!(
                    parent_track.kind,
                    ActorKindId::Row | ActorKindId::Col | ActorKindId::Grid | ActorKindId::Stack
                ) {
                    let has_at = extracted.at_expr.is_some();
                    let has_position = props.iter().any(|p| p.name == "position");
                    if has_at || has_position {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::AbsolutePositionOnLayoutManagedChild,
                                DiagnosticPhase::Build,
                                format!(
                                    "Actor '{label}' in container '{parent}' has 'at'/'position' which is ignored in managed layouts. Use 'transform' instead for visual offsets without disrupting layout.",
                                ),
                            )
                            .with_subject(label),
                        );
                    }
                }
            }
        }

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
        if let Some(pl) = parent_label {
            track.parent = Some(pl.to_string());
        }

        // Phase 7: Parse size spec from `size` property for percentage/auto/fill/fit sizing
        {
            let is_container = primitive.is_layout_container();
            for prop in props {
                if prop.name == "size" {
                    let spec = crate::timeline::taffy_layout::parse_size_spec(&prop.value);
                    track.geometry.size_spec = Some(spec);

                    // Warn on auto/fit for non-container primitives
                    if !is_container {
                        match &prop.value {
                            crate::ast::Expr::Ident(s) if s == "auto" || s == "fit" => {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        DiagnosticCode::InvalidModifierValue,
                                        DiagnosticPhase::Build,
                                        format!(
                                            "size: {} on non-container primitive '{}' — auto/fit sizing only applies to layout containers",
                                            s, label
                                        ),
                                    )
                                    .with_subject(label),
                                );
                            },
                            crate::ast::Expr::Str(s) if s == "auto" || s == "fit" => {
                                diagnostics.push(
                                    Diagnostic::warning(
                                        DiagnosticCode::InvalidModifierValue,
                                        DiagnosticPhase::Build,
                                        format!(
                                            "size: {} on non-container primitive '{}' — auto/fit sizing only applies to layout containers",
                                            s, label
                                        ),
                                    )
                                    .with_subject(label),
                                );
                            },
                            _ => {},
                        }
                    }

                    // Warn on fill at top level (no parent container)
                    if parent_label.is_none() {
                        let is_fill = match &prop.value {
                            crate::ast::Expr::Ident(s) if s == "fill" => true,
                            crate::ast::Expr::Str(s) if s == "fill" => true,
                            _ => false,
                        };
                        if is_fill {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidModifierValue,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "size: fill on top-level actor '{}' — fill only makes sense inside a layout container",
                                        label
                                    ),
                                )
                                .with_subject(label),
                            );
                        }
                    }
                    break;
                }
            }
        }

        // Pre-seed opacity for pre-keyframe first declarations so that
        // insert_start_keyframes captures the correct invisible start value.
        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            track
                .style
                .opacity
                .ensure(1.0)
                .add_keyframe(0, self.default_opacity, Easing::Linear);
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
            set_track_position_binding(
                track,
                t_start_ms,
                PositionBinding::ContainerDefault {
                    anchor: SceneAnchor::Center,
                },
            );
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

        if let Some(p) = crate::primitives::find_primitive(ty) {
            let mut ctx = crate::primitives::BuildCtx {
                timeline: self,
                time_ms,
                parent_label,
                diagnostics,
            };
            if let Err(mut diags) = p.finalize_container_build(&mut ctx, label, props) {
                diagnostics.append(&mut diags);
            }
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

        if let Some(ProcessedPlotActor {
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
        }) = self.process_plot_actor(
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
            let mut position = existing_track.geometry.position.last([0.0, 0.0]);
            let eval_env = self.build_eval_env(time_ms as u64);
            for prop in props {
                if prop.name == "at" || prop.name == "position" {
                    if let Ok(super::Value::Vec2(pos)) =
                        super::evaluate_expr(&prop.value, &eval_env)
                    {
                        position = [pos[0] as f32, pos[1] as f32];
                    }
                }
            }
            let size = initial_size;
            let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
            let is_first_decl = !self.tracks.contains_key(label);
            let opacity = if is_first_decl && !has_explicit_opacity {
                self.default_opacity
            } else {
                1.0
            };

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
            if let Some(pl) = parent_label {
                track.parent = Some(pl.to_string());
            }

            // Seed plot_param_tracks with initial declaration values.
            // Only create tracks that don't already exist so re-declarations
            // preserve existing keyframes.
            if let Some(ref plot) = track.procedural_plot {
                for (name, default_val) in &plot.params {
                    track.plot_param_tracks.entry(name.clone()).or_insert_with(|| {
                        let mut t = super::PropertyTrack::new(*default_val);
                        t.add_keyframe(0, *default_val, super::Easing::Linear);
                        t
                    });
                }
            }

            if track.first_seen_ms == u64::MAX {
                track.first_seen_ms = t_start_ms;
            }

            // Pre-seed opacity for pre-keyframe first declarations so that
            // insert_start_keyframes captures the correct invisible start value.
            if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
                track.style.opacity.ensure(1.0).add_keyframe(
                    0,
                    self.default_opacity,
                    Easing::Linear,
                );
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
                    self.process_text_actor_decl(
                        "Text",
                        &child_label,
                        &tick_props,
                        &[], // no modifiers
                        time_ms,
                        Some(label),
                        diagnostics,
                    )
                    .ok();
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
                    self.process_text_actor_decl(
                        "Text",
                        &child_label,
                        &tick_props,
                        &[], // no modifiers
                        time_ms,
                        Some(label),
                        diagnostics,
                    )
                    .ok();
                }
            }
        }
    }
}
