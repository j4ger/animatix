//! Actor building logic: processes actor declarations, generates VelloPaths,
//! inserts keyframes, and dispatches to ActorKind implementations.

use super::plot::ProcessedPlotActor;
use super::*;
use crate::ast::{Expr, InlineItem, Property};
use crate::timeline::actor_caps::find_actor_kind;
use crate::timeline::plot::PlotCurveKind;
use crate::timeline::vello_path::VelloPath;

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    fn generate_actor_paths(
        &self,
        label: &str,
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
        let primitive = self
            .primitive_registry
            .info_of(ty)
            .map(PrimitiveFamilyDescriptor::from_info)
            .unwrap_or_default();
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

        // VectorField, Heatmap, ContourSet, and NumberPlane are handled by
        // process_plot_actor (without build-time path generation — vector/
        // heatmap/contour plots are re-sampled at frame time when dynamic or
        // transitioning).
        if matches!(ty, "VectorField" | "Heatmap" | "ContourSet" | "NumberPlane") {
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
            // This generic fallback path has no diagnostics channel; eval
            // failures here degrade to NaN as before (the plot-specific
            // declaration path reports them).
            let mut ignored_diagnostics = Vec::new();
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
                label,
                // This fallback drops diagnostics, so the frame-injected name
                // set is irrelevant here; the plot-specific declaration path
                // reports probe failures.
                param_names: &[],
            };
            return build_plot_curve_paths(&curve_params, &mut ignored_diagnostics);
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
        corner_radius: f32,
        color: [f32; 4],
        shape_type: ShapeType,
        opacity: f32,
        stroke_width: f32,
        stroke_color: [f32; 4],
        stroke_progress: f32,
        fill_opacity: f32,
        dash_pattern: Vec<f32>,
        dash_offset: f32,
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
            corner_radius,
            color,
            shape_type,
            opacity,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            dash_pattern,
            dash_offset,
            vello_paths,
            easing,
        );
    }

    /// Warn about declaration properties no build path consumes.
    ///
    /// A name is known when the property registry binds it, the actor type's
    /// primitive declares it, or an extension registry declares it for the
    /// type; everything else is dropped silently by every consumer, which is
    /// how a typo'd property (`colour:`) ships as a scene that "works" without
    /// the value. The known sets are the single-source tables, so this warning
    /// cannot drift from what the build actually reads.
    /// A property name the registry knows, but which this actor's primitive
    /// never reads.
    ///
    /// `unknown-property` above catches a typo; this catches the worse case,
    /// because a misspelling is at least visibly wrong while this one is
    /// indistinguishable from correct source — `stroke_width: 6` on a `Text`
    /// names a real property that `Text` ignores, and the value disappears with
    /// no diagnostic anywhere. It is the same class as the plot-family `opacity`
    /// and `Arrow` `color` drops, found by eye this round.
    ///
    /// The predicate is `Applicable::includes`, the table the inspector already
    /// filters its property list with, so the editor and the build cannot
    /// disagree about what a type takes. Two honest limits:
    ///
    /// - Rows declared `Applicable::Everything` (`color`, `opacity`, `at`, …)
    ///   pass unconditionally, so a primitive that ignores one of those is
    ///   invisible here. That is a defect in the table, not in this check, and
    ///   the fix is to narrow the row.
    /// - Coverage matches `warn_unknown_declaration_properties`' exactly, which
    ///   means the plot-family dispatch is not covered at all (it never calls
    ///   either function). Its runtime parameters are the reason: `freq: 2` on
    ///   `func: (x) => sin(freq * x)` is read by the author's closure, not by a
    ///   primitive, and the exemption would have to be `plot_runtime_params`.
    fn warn_inapplicable_declaration_properties(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(caps) = animatix_std::caps_for_type(ty) else {
            // Extension primitives have no catalog caps; their property surface
            // is the extension registry, which `warn_unknown_declaration_
            // properties` already consults.
            return;
        };
        for prop in props {
            let Some(descriptor) = animatix_core::property::descriptor(&prop.name) else {
                continue; // an unknown name is the other warning's subject
            };
            if descriptor.applicable.includes(&caps, ty) {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InapplicableProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor '{label}' ({ty}) declares '{}' but {ty} never reads it, so the \
                         value is dropped. Check the primitive's property list, or move the \
                         value to a property {ty} consumes.",
                        prop.name
                    ),
                )
                .with_subject(format!("{label}.{}", prop.name))
                .with_byte_span(prop.value_span),
            );
        }
    }

    pub(crate) fn warn_unknown_declaration_properties(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let declared = self
            .primitive_registry
            .find(ty)
            .map(|primitive| primitive.declared_property_names());
        for prop in props {
            if crate::timeline::property_registry::lookup_property(&prop.name).is_some()
                || declared
                    .as_ref()
                    .is_some_and(|names| names.iter().any(|name| *name == prop.name))
                || self
                    .extensions
                    .as_ref()
                    .is_some_and(|registry| registry.property_spec(ty, &prop.name).is_some())
            {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor '{label}' ({ty}) declares '{}' which no build path consumes; \
                         the property is dropped. Check the spelling against the property \
                         registry or the primitive's declared properties.",
                        prop.name
                    ),
                )
                .with_subject(format!("{label}.{}", prop.name)),
            );
        }
        // Same call sites, same coverage: a name that exists but is ignored by
        // this primitive is the sibling of a name that does not exist at all.
        self.warn_inapplicable_declaration_properties(label, ty, props, diagnostics);
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

        // Extension primitives are dispatched through the active runtime
        // registry. Built-ins remain on the static actor-kind path below.
        let registry = self.primitive_registry.clone();
        if let Some(primitive) = registry.find(ty) {
            if !registry.is_builtin(ty) {
                let mut ctx = crate::primitives::BuildCtx {
                    timeline: self,
                    time_ms,
                    parent_label,
                    diagnostics,
                };
                if let Err(mut diags) = primitive.build(&mut ctx, label, props, modifiers, children)
                {
                    diagnostics.append(&mut diags);
                }
                if let Some(track) = self.tracks.get_mut(label) {
                    // Capabilities come from the registration info, not the
                    // built-in catalog: an in-process extension can be a
                    // text-like or shape-like primitive and must get the full
                    // built-in treatment.
                    track.actor_type = ty.to_string();
                    if let Some(info) = registry.info_of(ty) {
                        track.set_caps(animatix_std::caps_from_info(info));
                    }
                    // Extension builds create the track; mirror the built-in
                    // path so the actor is visible from its declaration time
                    // instead of being skipped forever (first_seen_ms = MAX).
                    if track.first_seen_ms == u64::MAX {
                        track.first_seen_ms = time_ms as u64;
                    }
                }
                self.process_inline_items(time_ms, children, label, diagnostics);
                // Extension builds only create the track; apply the common
                // actor properties (at/anchor/offset, opacity, size) so
                // extension actors behave like built-ins.
                self.apply_extension_common_properties(
                    label,
                    ty,
                    props,
                    modifiers,
                    time_ms,
                    diagnostics,
                );
                self.write_extension_properties_for_decl(
                    label,
                    ty,
                    props,
                    modifiers,
                    time_ms,
                    diagnostics,
                );
                self.warn_unknown_declaration_properties(label, ty, props, diagnostics);
                let mut ctx = crate::primitives::BuildCtx {
                    timeline: self,
                    time_ms,
                    parent_label,
                    diagnostics,
                };
                if let Err(mut diags) = primitive.finalize_container_build(&mut ctx, label, props) {
                    diagnostics.append(&mut diags);
                }
                return;
            }
        }

        let Some(caps) = animatix_std::caps_for_type(ty) else {
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

        let primitive = self
            .primitive_registry
            .info_of(ty)
            .map(PrimitiveFamilyDescriptor::from_info)
            .unwrap_or_default();
        let existing_track = self
            .tracks
            .get(label)
            .cloned()
            .unwrap_or_else(|| AnimationTrack::new(label.to_string(), ty));

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
                .or_insert_with(|| AnimationTrack::new(label.to_string(), ty));
            early_track.set_identity(ty);
            early_track.rebuild_property_plan();
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
        let mut corner_radius = existing_track.shape.corner_radius.last(0.0);
        let mut color = existing_track.style.color.last(DEFAULT_WHITE);
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        // Deliberately computed after the early track creation above: recursive
        // containers (Row/Col/Grid/Stack/Group/Mask/Filter) with children stay
        // visible-by-default because `expand_group_targets` targets their leaves
        // and skips the container, so a container-level seed could never be
        // lifted. Self-drawing containers that are *not* expanded (Graph and the
        // other plot hosts) are seeded in `process_plot_actor_dispatch` instead.
        let is_first_decl = !self.tracks.contains_key(label);
        let mut opacity = if is_first_decl && !has_explicit_opacity {
            self.default_opacity
        } else {
            existing_track.style.opacity.last(1.0)
        };
        let mut stroke_width = existing_track.style.stroke_width.last(default_stroke_width(ty));
        let mut stroke_color = existing_track.style.stroke_color.last(DEFAULT_WHITE);
        let legend_color = existing_track.legend.color;
        let mut stroke_progress = existing_track.style.stroke_progress.last(1.0);
        let mut fill_opacity = existing_track.style.fill_opacity.last(1.0);
        let mut dash_pattern = existing_track.style.dash_pattern.last(Vec::new());
        let mut dash_offset = existing_track.style.dash_offset.last(0.0);

        let primitive_registry = std::sync::Arc::clone(&self.primitive_registry);
        let vector_shape = primitive_registry.info_of(ty).filter(|info| info.capabilities.is_shape);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let mut vector_shape_state = self.build_vector_shape_state(
            ty,
            props,
            time_ms,
            size,
            line_from,
            line_to,
            arc_angles,
            corner_radius,
            diagnostics,
        );
        (size, line_from, line_to, arc_angles, corner_radius) =
            extract_shape_state_values(&vector_shape_state);

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
        if !has_explicit_color {
            if let Some(primitive) = self.primitive_registry.find(ty) {
                if let Some(scheme_color) = self.get_default_color(primitive, &caps, "color") {
                    color = scheme_color;
                    if caps.category == ActorCategory::Plot {
                        stroke_color = scheme_color;
                    }
                }
            }
        }
        if !has_explicit_stroke {
            if let Some(primitive) = self.primitive_registry.find(ty) {
                if let Some(scheme_stroke) = self.get_default_color(primitive, &caps, "stroke") {
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
                "opacity" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0));
                    opacity = v.as_num() as f32;
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
                "dash_pattern" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        Some(Value::List(items)) => {
                            let mut values = Vec::with_capacity(items.len());
                            for item in items.iter() {
                                match item {
                                    Value::Num(n) => values.push(*n as f32),
                                    other => diagnostics.push(
                                        Diagnostic::warning(
                                            DiagnosticCode::InvalidPropertyValue,
                                            DiagnosticPhase::Build,
                                            format!("dash_pattern expects numbers, got {other:?}"),
                                        )
                                        .with_subject(&prop_subject),
                                    ),
                                }
                            }
                            if !values.is_empty() {
                                dash_pattern = values;
                            }
                        },
                        Some(other) => diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!("dash_pattern expects a list of numbers, got {other:?}"),
                            )
                            .with_subject(&prop_subject),
                        ),
                        None => {}, // eval error already reported as a diagnostic
                    }
                },
                "dash_offset" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    dash_offset = v.as_num() as f32;
                },
                _ if vector_shape.is_some()
                    && apply_vector_shape_property(
                        ty,
                        &prop.name,
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &prop_subject,
                        &mut vector_shape_state,
                    ) =>
                {
                    (size, line_from, line_to, arc_angles, corner_radius) =
                        extract_shape_state_values(&vector_shape_state);
                },
                _ => {},
            }
        }

        self.warn_unknown_declaration_properties(label, ty, props, diagnostics);

        let legend_color = if has_explicit_color {
            Some(color)
        } else if has_explicit_stroke {
            Some(stroke_color)
        } else {
            legend_color
        };

        // Stroke-only shapes have no independent fill, so an authored `color:`
        // is the colour the author means the drawing to take.
        if !stroke_color_explicitly_set && caps.shape.is_some_and(|s| s.is_stroke_only()) {
            stroke_color = color;
        }

        if vector_shape.is_some() {
            finalize_vector_shape_state(ty, &mut vector_shape_state);
            (size, line_from, line_to, arc_angles, corner_radius) =
                extract_shape_state_values(&vector_shape_state);
        }

        if primitive.is_graph_host() || primitive.is_layout_container() {
            fill_opacity = 0.0;
            stroke_width = 0.0;
        }

        // A stroke-only Path (explicit `stroke:`, no explicit `color:`) is
        // line-like: suppress the default scheme fill. Vello implicitly closes
        // open paths when filling, so the fill otherwise renders a hand-drawn
        // line as a dark filled dome. Closed shapes (Rect/Ellipse/Polygon),
        // paths with an authored color, and paths with an authored
        // `fill_opacity` keep their fill.
        if has_explicit_stroke
            && !has_explicit_color
            && !props.iter().any(|p| p.name == "fill_opacity")
            && caps.shape == Some(super::ShapeKind::Path)
        {
            fill_opacity = 0.0;
        }

        // G6: Detect actor-anchor refs in `from`/`to` property declarations.
        // Store them in the track's side-channel so the primitive's frame-time
        // `evaluate` method can resolve them each frame.
        // G6: Detect actor-anchor refs in `from`/`to` property declarations.
        // Store them in the track's side-channel so the primitive's frame-time
        // `evaluate` method can resolve them each frame.
        if matches!(caps.shape, Some(super::ShapeKind::Line | super::ShapeKind::Arrow)) {
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
            label,
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
                if parent_track.caps.layout_container {
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
            .or_insert_with(|| AnimationTrack::new(label.to_string(), ty));
        track.set_identity(ty);
        track.rebuild_property_plan();
        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = t_start_ms;
        }
        if let Some(pl) = parent_label {
            track.parent = Some(pl.to_string());
        }
        track.legend.color = legend_color;

        // Seed Callout defaults before user properties so fields used by targeted
        // geometry and text evaluation are always present, even without explicit props.
        if ty == "Callout" {
            let defaults = [
                (
                    crate::timeline::property_registry::ActorField::TextContent,
                    crate::timeline::property_engine::PropertyValue::String(String::new()),
                ),
                (
                    crate::timeline::property_registry::ActorField::LabelAt,
                    crate::timeline::property_engine::PropertyValue::Vec2([0.0, 50.0]),
                ),
                (
                    crate::timeline::property_registry::ActorField::CalloutTarget,
                    crate::timeline::property_engine::PropertyValue::String(String::new()),
                ),
                (
                    crate::timeline::property_registry::ActorField::Tagged("callout_place"),
                    crate::timeline::property_engine::PropertyValue::Enum("right".to_string()),
                ),
                (
                    crate::timeline::property_registry::ActorField::CalloutStandoff,
                    crate::timeline::property_engine::PropertyValue::F32(40.0),
                ),
                (
                    crate::timeline::property_registry::ActorField::CalloutToOffset,
                    crate::timeline::property_engine::PropertyValue::Vec2([0.0, 0.0]),
                ),
                (
                    crate::timeline::property_registry::ActorField::HeadSize,
                    crate::timeline::property_engine::PropertyValue::F32(10.0),
                ),
            ];
            for (field, pv) in defaults {
                crate::timeline::property_engine::write_property_field(
                    track,
                    field,
                    pv,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    diagnostics,
                );
            }
        }

        // Registry-backed tagged union properties are not part of the legacy
        // per-primitive build loop, so write them through the generic engine.
        for prop in props {
            if let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name)
                && matches!(
                    schema.field,
                    crate::timeline::ActorField::Tagged(_) | crate::timeline::ActorField::Blend
                )
            {
                let prop_subject = format!("{label}.{}", prop.name);
                if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                    schema.value_type,
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &prop_subject,
                ) {
                    crate::timeline::property_engine::write_property_field(
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

        // Callout annotation properties that live outside the tagged storage map.
        // These are handled by the generic build path now; the primitive no longer
        // re-implements keyframe writing for them.
        if ty == "Callout" {
            for prop in props {
                let prop_subject = format!("{label}.{}", prop.name);
                if prop.name == "target" {
                    if let Some(target) =
                        crate::timeline::lookup::parse_actor_ref_literal(&prop.value)
                    {
                        crate::timeline::property_engine::write_property_field(
                            track,
                            crate::timeline::property_registry::ActorField::CalloutTarget,
                            crate::timeline::property_engine::PropertyValue::String(target),
                            t_start_ms,
                            t_end_ms,
                            easing,
                            diagnostics,
                        );
                    }
                    continue;
                }
                let (value_type, field) = match prop.name.as_str() {
                    "from" => (
                        crate::timeline::property_registry::ValueType::Vec2,
                        crate::timeline::property_registry::ActorField::LineFrom,
                    ),
                    "to" => (
                        crate::timeline::property_registry::ValueType::Vec2,
                        crate::timeline::property_registry::ActorField::LineTo,
                    ),
                    "head_size" => (
                        crate::timeline::property_registry::ValueType::F32,
                        crate::timeline::property_registry::ActorField::HeadSize,
                    ),
                    "label" => (
                        crate::timeline::property_registry::ValueType::String,
                        crate::timeline::property_registry::ActorField::TextContent,
                    ),
                    "label_at" => (
                        crate::timeline::property_registry::ValueType::Vec2,
                        crate::timeline::property_registry::ActorField::LabelAt,
                    ),
                    _ => continue,
                };
                if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                    value_type,
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &prop_subject,
                ) {
                    if let crate::timeline::property_engine::PropertyValue::Vec2(v) = &pv {
                        if field == crate::timeline::property_registry::ActorField::LineFrom {
                            line_from = *v;
                        } else if field == crate::timeline::property_registry::ActorField::LineTo {
                            line_to = *v;
                        }
                    }
                    crate::timeline::property_engine::write_property_field(
                        track,
                        field,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        diagnostics,
                    );
                }
            }
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
            track.hidden_by_default = true;
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
            corner_radius,
            color,
            shape_type,
            opacity,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            dash_pattern,
            dash_offset,
            vello_paths,
            easing,
            duration_ms,
            delay_ms,
            supports_morph_options,
            morph_options,
        );

        self.write_extension_properties_for_decl(label, ty, props, modifiers, time_ms, diagnostics);

        let primitive_registry = std::sync::Arc::clone(&self.primitive_registry);
        if let Some(p) = primitive_registry.find(ty) {
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

    /// Write registered external properties on an actor declaration.
    ///
    /// This runs after the primitive has created/updated its track so both
    /// built-in and custom primitives can expose schema-driven properties
    /// without hand-writing storage or keyframe code.
    /// Apply common actor properties to an extension-typed actor.
    ///
    /// Extension primitives build their own track and write only their own
    /// declared properties; the shared properties (`at`/`anchor`/`offset`,
    /// `opacity`, `size`, ...) fall through to here so they behave like
    /// built-in actors.
    fn apply_extension_common_properties(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Position / anchor / offset resolution.
        let eval_env = self.build_eval_env(time_ms as u64);
        let mut at_expr: Option<crate::ast::Expr> = None;
        let mut anchor_expr: Option<crate::ast::Expr> = None;
        let mut offset_expr: Option<crate::ast::Expr> = None;
        for prop in props {
            match prop.name.as_str() {
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                _ => {},
            }
        }
        if let Some((binding, bound_position)) = resolve_position_binding_with_lookup_diagnostic(
            at_expr.as_ref(),
            anchor_expr.as_ref(),
            offset_expr.as_ref(),
            &eval_env,
            diagnostics,
            label,
        ) {
            if let Some(track) = self.tracks.get_mut(label) {
                let t_start_ms = time_ms as u64;
                preserve_discrete_position_state_before(track, t_start_ms);
                set_track_position_binding(track, t_start_ms, binding);
                if let Some(bound_position) = bound_position {
                    track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
                        t_start_ms,
                        bound_position,
                        Easing::Linear,
                    );
                }
                mark_track_manual_position(track, t_start_ms);
            }
        }

        // Declared opacity/size write through the property plan slots.
        let _ = (ty, modifiers);
        for prop in props {
            match prop.name.as_str() {
                "opacity" => {
                    if let Ok(crate::timeline::Value::Num(v)) =
                        super::evaluate_expr(&prop.value, &eval_env)
                    {
                        if let Some(track) = self.tracks.get_mut(label) {
                            track.style.opacity.ensure(1.0).add_keyframe(
                                time_ms as u64,
                                v.clamp(0.0, 1.0) as f32,
                                Easing::Linear,
                            );
                        }
                    }
                },
                "size" => {
                    if let Ok(crate::timeline::Value::Vec2(v)) =
                        super::evaluate_expr(&prop.value, &eval_env)
                    {
                        if let Some(track) = self.tracks.get_mut(label) {
                            track
                                .geometry
                                .size
                                .ensure(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
                                .add_keyframe(
                                    time_ms as u64,
                                    [v[0] as f32, v[1] as f32],
                                    Easing::Linear,
                                );
                        }
                    }
                },
                _ => {},
            }
        }
    }

    fn write_extension_properties_for_decl(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(ctx) = self.extensions.clone() else {
            return;
        };
        let primitive = self.primitive_registry.find(ty);
        if !props.iter().any(|prop| {
            ctx.property_spec(ty, &prop.name).is_some()
                || primitive.is_some_and(|primitive| primitive.declares_property(&prop.name))
        }) {
            return;
        }

        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            ..
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::ActorDeclaration,
            Some(label),
            diagnostics,
        );
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let eval_env = self.build_eval_env(time_ms as u64);
        let Some(track) = self.tracks.get_mut(label) else {
            return;
        };

        for prop in props {
            let subject = format!("{}.{}", label, prop.name);
            if let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name) {
                if primitive.is_some_and(|primitive| primitive.declares_property(&prop.name)) {
                    if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                        schema.value_type,
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &subject,
                    ) {
                        crate::timeline::property_engine::write_property_field(
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
                continue;
            }
            let Some(spec) = ctx.property_spec(ty, &prop.name) else {
                continue;
            };
            if let Some(pv) = crate::timeline::property_engine::parse_extension_property_value(
                spec.kind,
                spec.ty.as_ref(),
                &prop.value,
                &eval_env,
                diagnostics,
                &subject,
            ) {
                crate::timeline::property_engine::write_extension_property_slot(
                    track, &ctx, ty, &prop.name, pv, t_start_ms, t_end_ms, easing,
                );
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
        if animatix_std::caps_for_type(ty).is_none() {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!("Unknown actor type '{}'", ty),
                )
                .with_subject(label),
            );
            return;
        }

        // First-declaration status must be captured BEFORE the track exists:
        // a first declaration with children is seeded hidden-by-default.
        let is_first_decl = !self.tracks.contains_key(label);

        // Create the plot host's track before `process_plot_actor` runs: it
        // processes children, and `add_node` requires the parent track to exist
        // rather than defaulting one in.
        {
            let entry = self
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| AnimationTrack::new(label.to_string(), ty));
            entry.set_identity(ty);
            entry.rebuild_property_plan();
        }
        let existing_track = self
            .tracks
            .get(label)
            .cloned()
            .expect("plot host track created immediately above");

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

            // Resolve `anchor`/`offset` (and scene-relative `at`) exactly like
            // the generic actor path below. The plot builder only consumed
            // absolute `at`/`position`, which silently dropped scene anchoring
            // on plot actors even though the registry marks anchor/offset
            // applicable to them.
            let mut at_expr: Option<&Expr> = None;
            let mut anchor_expr: Option<&Expr> = None;
            let mut offset_expr: Option<&Expr> = None;
            for prop in props {
                match prop.name.as_str() {
                    "at" | "position" => at_expr = Some(&prop.value),
                    "anchor" => anchor_expr = Some(&prop.value),
                    "offset" => offset_expr = Some(&prop.value),
                    _ => {}, // Not a position property.
                }
            }
            let position_binding = resolve_position_binding_with_lookup_diagnostic(
                at_expr,
                anchor_expr,
                offset_expr,
                &eval_env,
                diagnostics,
                label,
            );
            let size = initial_size;
            let authored_opacity = props.iter().find(|p| p.name == "opacity");
            let has_explicit_opacity = authored_opacity.is_some();
            // An authored `opacity:` used to be detected and then thrown away:
            // `has_explicit_opacity` only ever pushed the value to the `1.0`
            // branch, so `opacity: 0.05` on any plot actor rendered at full
            // strength and warned about nothing. The render layer already
            // multiplies fill/stroke alpha by node opacity — the parse was the
            // only missing piece, and it is shared by Graph, BarChart,
            // ContourSet and VectorField.
            let opacity = match authored_opacity {
                Some(prop) => {
                    let subject = format!("{}.{}", label, prop.name);
                    evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &subject,
                    )
                    .unwrap_or(Value::Num(1.0))
                    .as_num() as f32
                },
                None if is_first_decl => self.default_opacity,
                None => 1.0,
            };

            let ParsedTimingModifiers {
                duration_ms,
                delay_ms,
                easing,
                morph_options: _,
                ..
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
                .or_insert_with(|| AnimationTrack::new(label.to_string(), ty));
            track.set_identity(ty);
            track.rebuild_property_plan();
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
                track.hidden_by_default = true;
                track.style.opacity.ensure(1.0).add_keyframe(
                    0,
                    self.default_opacity,
                    Easing::Linear,
                );
            }

            if let Some((binding, bound_position)) = position_binding {
                preserve_discrete_position_state_before(track, t_start_ms);
                set_track_position_binding(track, t_start_ms, binding);
                if let Some(bound_position) = bound_position {
                    position = bound_position;
                }
                mark_track_manual_position(track, t_start_ms);
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
                // Plot hosts are never `Rect`; carry the track value through.
                track.shape.corner_radius.last(0.0),
                color,
                shape_type,
                opacity,
                stroke_width,
                stroke_color,
                stroke_progress,
                fill_opacity,
                track.style.dash_pattern.last(Vec::new()),
                track.style.dash_offset.last(0.0),
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
                        Property::new(
                            "color",
                            Expr::Path(vec!["text".to_string(), "muted".to_string()]),
                        ),
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
                        Property::new(
                            "color",
                            Expr::Path(vec!["text".to_string(), "muted".to_string()]),
                        ),
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

                // Bar labels (positioned below each bar)
                for (i, &(sx, sy, ref text)) in tick_data.bar_labels.iter().enumerate() {
                    let child_label = format!("{}_bar_label_{}", label, i);
                    let tick_props = vec![
                        Property::new("text", Expr::Str(text.clone())),
                        Property::new("at", Expr::Tuple(vec![Expr::Num(sx), Expr::Num(sy)])),
                        Property::new("font_size", Expr::Num(10.0)),
                        Property::new(
                            "color",
                            Expr::Path(vec!["text".to_string(), "muted".to_string()]),
                        ),
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
