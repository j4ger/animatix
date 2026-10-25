//! Actor declaration pipeline.

use super::prepare::{ActorPrep, apply_position_binding, apply_size_spec};
use super::*;
use crate::ast::{InlineItem, Property};

impl Timeline {
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

        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, 0.0];
        let existing_shape = self
            .world
            .get_id(label)
            .map(|id| self.world.shape(id))
            .cloned()
            .unwrap_or_default();
        let existing_geometry = self
            .world
            .get_id(label)
            .map(|id| self.world.geometry(id))
            .cloned()
            .unwrap_or_default();
        let existing_style = self
            .world
            .get_id(label)
            .map(|id| self.world.style(id))
            .cloned()
            .unwrap_or_default();
        let mut position = existing_geometry.position.last([0.0, 0.0]);
        let mut size = existing_geometry.size.last(default_size);
        let mut line_from = existing_shape.line_from.last([-50.0, 0.0]);
        let mut line_to = existing_shape.line_to.last([50.0, 0.0]);
        let mut arc_angles = existing_shape.arc_angles.last(default_arc);
        let mut corner_radius = existing_shape.corner_radius.last(0.0);
        let mut color = existing_style.color.last(DEFAULT_WHITE);
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.world.tracks.contains_key(label);
        let mut opacity = if is_first_decl && !has_explicit_opacity {
            self.default_opacity
        } else {
            existing_style.opacity.last(1.0)
        };
        let mut stroke_width = existing_style.stroke_width.last(caps.default_stroke_width());
        let mut stroke_color = existing_style.stroke_color.last(DEFAULT_WHITE);
        let legend_color = self
            .world
            .get_id(label)
            .and_then(|id| self.world.legend(id))
            .and_then(|legend| legend.color);
        let mut stroke_progress = existing_style.stroke_progress.last(1.0);
        let mut fill_opacity = existing_style.fill_opacity.last(1.0);
        let mut dash_pattern = existing_style.dash_pattern.last(Vec::new());
        let mut dash_offset = existing_style.dash_offset.last(0.0);

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
        let supports_morph_options = existing_shape
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
        if matches!(caps.shape, Some(super::ShapeKind::Line | super::ShapeKind::Arrow))
            && let Some(id) = self.world.get_id(label)
        {
            let shape = self.world.shape_mut(id);
            for prop in props {
                if prop.name == "from" || prop.name == "to" {
                    if let Expr::Path(segments) = &prop.value {
                        if segments.len() == 2 {
                            if let Some(anchor) = SceneAnchor::from_str(&segments[1]) {
                                if prop.name == "from" {
                                    shape.from_anchor = Some((segments[0].clone(), anchor));
                                } else {
                                    shape.to_anchor = Some((segments[0].clone(), anchor));
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
            if let Some(parent_id) = self.world.get_id(parent) {
                if self.world.caps(parent_id).layout_container {
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

        let track_id = self.world.spawn(label, ty);
        // The World form clears any capability override so caps re-derive from
        // the declaration type on a re-declaration.
        self.world.set_identity(track_id, ty);
        let track_caps = self.world.caps(track_id);
        self.world.property_plan.get_mut(track_id).rebuild_for(&track_caps, ty);
        self.world.meta.record_first_seen(track_id, t_start_ms);
        if let Some(span) = decl_span
            && self.world.decl_span.get(track_id).is_none()
        {
            self.world.decl_span.get_or_insert_with(track_id, || span);
        }
        // Legend participation is a component, not a track field. Only actors
        // with an explicit color/stroke become automatic legend candidates, so
        // actors with nothing to contribute carry no component at all.
        if let Some(color) = legend_color {
            self.world.ensure_legend(track_id).color = Some(color);
        }
        // Seed Callout defaults before user properties so fields used by targeted
        // geometry and text evaluation are always present, even without explicit props.
        if caps.callout {
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

        // Connector declaration properties are written into the connector
        // geometry lanes here, in the generic body, so the actor keeps the whole
        // generic declaration tail (style, transforms, opacity, gradients).
        // Catalog data decides this (via the descriptor already built above),
        // not a `ty == "Connector"` compare.
        if primitive.connector {
            let geometry = self.world.geometry.get_mut(track_id);
            let shape = self.world.shape.get_mut(track_id);
            geometry.connector_arrow.ensure(true).add_keyframe(t_start_ms, true, easing);
            geometry
                .connector_routing
                .ensure(crate::timeline::ConnectorRouting::Elbow)
                .add_keyframe(t_start_ms, crate::timeline::ConnectorRouting::Elbow, easing);
            shape.corner_radius.ensure(8.0).add_keyframe(t_start_ms, 8.0, easing);
            shape.head_size.ensure(10.0).add_keyframe(t_start_ms, 10.0, easing);

            for prop in props {
                match prop.name.as_str() {
                    "from" => {
                        if let Some(endpoint) =
                            crate::primitives::connector::parse_connector_endpoint(
                                &prop.value,
                                &eval_env,
                            )
                        {
                            geometry
                                .connector_from
                                .ensure(endpoint.clone())
                                .add_keyframe(t_start_ms, endpoint, easing);
                        }
                    },
                    "to" => {
                        if let Some(endpoint) =
                            crate::primitives::connector::parse_connector_endpoint(
                                &prop.value,
                                &eval_env,
                            )
                        {
                            geometry
                                .connector_to
                                .ensure(endpoint.clone())
                                .add_keyframe(t_start_ms, endpoint, easing);
                        }
                    },
                    "routing" => {
                        let routing_str = match &prop.value {
                            Expr::Ident(s) | Expr::Str(s) => s.as_str(),
                            _ => "elbow",
                        };
                        let routing = routing_str
                            .parse::<crate::timeline::ConnectorRouting>()
                            .unwrap_or(crate::timeline::ConnectorRouting::Elbow);
                        geometry
                            .connector_routing
                            .ensure(routing)
                            .add_keyframe(t_start_ms, routing, easing);
                    },
                    "arrow" => {
                        let arrow_val = match &prop.value {
                            Expr::Bool(b) => *b,
                            Expr::Ident(s) => s == "true",
                            _ => true,
                        };
                        geometry
                            .connector_arrow
                            .ensure(arrow_val)
                            .add_keyframe(t_start_ms, arrow_val, easing);
                    },
                    "corner_radius" => {
                        if let Ok(Value::Num(n)) =
                            crate::timeline::evaluate_expr(&prop.value, &eval_env)
                        {
                            shape
                                .corner_radius
                                .ensure(n as f32)
                                .add_keyframe(t_start_ms, n as f32, easing);
                        }
                    },
                    "head_size" => {
                        if let Ok(Value::Num(n)) =
                            crate::timeline::evaluate_expr(&prop.value, &eval_env)
                        {
                            shape
                                .head_size
                                .ensure(n as f32)
                                .add_keyframe(t_start_ms, n as f32, easing);
                        }
                    },
                    _ => {},
                }
            }
        }

        // `perspective` is declared for every actor (default 800) as the focal
        // length of the planned 2.5D perspective tilt, but only `rotate_x` /
        // `rotate_y` are implemented and they apply an *orthographic* tilt — no
        // focal length, no `1/w` foreshortening. So an authored `perspective`
        // is stored and never read. Say so instead of accepting a value that
        // does nothing; the property lands with `PerspectiveBlitPipeline`.
        if props.iter().any(|p| p.name == "perspective") {
            diagnostics.push(Diagnostic::warning(
                crate::diagnostics::DiagnosticCode::UnimplementedProperty,
                crate::diagnostics::DiagnosticPhase::Build,
                format!(
                    "`{label}.perspective` is not implemented yet: `rotate_x`/`rotate_y` apply an \
                     orthographic tilt, so the focal length has no effect. Tracked in \
                     docs/roadmap.md (2.5D perspective tilt)."
                ),
            ));
        }

        // Registry-backed properties that the legacy per-primitive build loop
        // does not reach are written through the generic engine. `Scale` belongs
        // here: an authored `scale:` was dropped on the floor for every actor
        // kind, so `scale: 4.0` on a declaration rendered at 1.0 while
        // `x.scale = 4.0` in a keyframe worked.
        for prop in props {
            if let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name)
                && matches!(
                    schema.field,
                    crate::timeline::ActorField::Tagged(_)
                        | crate::timeline::ActorField::Blend
                        | crate::timeline::ActorField::FillGradient
                        | crate::timeline::ActorField::StrokeGradient
                        | crate::timeline::ActorField::GradientExtend
                        | crate::timeline::ActorField::GradientSpace
                        | crate::timeline::ActorField::Scale
                        | crate::timeline::ActorField::MinWidth
                        | crate::timeline::ActorField::MinHeight
                        | crate::timeline::ActorField::MaxHeight
                        | crate::timeline::ActorField::AudioVolume
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
                    self.world.write_property_field(
                        track_id,
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
        if caps.callout {
            for prop in props {
                let prop_subject = format!("{label}.{}", prop.name);
                if prop.name == "target" {
                    if let Some(target) =
                        crate::timeline::lookup::parse_actor_ref_literal(&prop.value)
                    {
                        self.world.write_property_field(
                            track_id,
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
                if matches!(caps.shape, Some(super::ShapeKind::Line | super::ShapeKind::Arrow))
                    && (prop.name == "from" || prop.name == "to")
                {
                    if let Expr::Path(segments) = &prop.value {
                        if segments.len() == 2 && SceneAnchor::from_str(&segments[1]).is_some() {
                            continue;
                        }
                    }
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
        }

        let style = self.world.style.get_mut(track_id);
        let shape = self.world.shape.get_mut(track_id);
        let geometry = self.world.geometry.get_mut(track_id);

        apply_size_spec(geometry, &primitive, props, label, parent_label, diagnostics);

        // Pre-seed opacity for pre-keyframe first declarations so that
        // insert_start_keyframes captures the correct invisible start value.
        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            self.world
                .meta
                .get_or_insert_with(track_id, crate::timeline::world::EntityMeta::default)
                .hidden_by_default = true;
            style.opacity.ensure(1.0).add_keyframe(0, self.default_opacity, Easing::Linear);
        }

        if let Some(bound) = apply_position_binding(
            geometry,
            position_binding,
            primitive.is_layout_container(),
            parent_label,
            t_start_ms,
        ) {
            position = bound;
        }

        Self::insert_actor_keyframes(
            style,
            geometry,
            shape,
            caps,
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

        self.finalize_actor_container(label, ty, props, time_ms, parent_label, diagnostics);
    }
}
