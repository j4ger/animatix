//! Extension and plot actor build helpers.

use super::super::plot::ProcessedPlotActor;
use super::*;
use crate::ast::{InlineItem, Property};

impl Timeline {
    pub(super) fn apply_extension_common_properties(
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
            if let Some(track) = self.world.tracks.get_mut(label) {
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
                        if let Some(track) = self.world.tracks.get_mut(label) {
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
                        if let Some(track) = self.world.tracks.get_mut(label) {
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

    pub(super) fn write_extension_properties_for_decl(
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
        let Some(track_id) = self.world.tracks.get_id(label) else {
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
                let Some(track) = self.world.tracks.get_by_id_mut(track_id) else {
                    continue;
                };
                crate::timeline::property_engine::write_extension_property_slot(
                    track, &ctx, ty, &prop.name, pv, t_start_ms, t_end_ms, easing,
                );
            }
        }
    }

    /// Seed the decoration properties the plot pipeline does not carry: the dash
    /// pair and the four gradient properties.
    ///
    /// `process_plot_actor` reports back only the style values it understands, and
    /// the plot keyframe insertion writes exactly those, so a plot actor's
    /// declaration could never land these on its track — even though both groups
    /// are declared `Applicable::AllStrokePaths`, which includes `PlotCurve`.
    /// Frame time samples them off the track (`stamp_stroke_decoration`), so the
    /// declaration has to be routed through the generic engine here.
    pub(super) fn insert_plot_decoration_keyframes(
        track: &mut AnimationTrack,
        props: &[Property],
        eval_env: &Environment,
        label: &str,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        const PLOT_DECORATIONS: [&str; 6] = [
            "dash_pattern",
            "dash_offset",
            "fill_gradient",
            "stroke_gradient",
            "gradient_extend",
            "gradient_space",
        ];
        for prop in props {
            if !PLOT_DECORATIONS.contains(&prop.name.as_str()) {
                continue;
            }
            let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name)
            else {
                continue;
            };
            let subject = format!("{label}.{}", prop.name);
            if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                schema.value_type,
                &prop.value,
                eval_env,
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
        let is_first_decl = !self.world.tracks.contains_key(label);

        // Create the plot host's track before `process_plot_actor` runs: it
        // processes children, and `add_node` requires the parent track to exist
        // rather than defaulting one in.
        {
            let entry = self.world.ensure_track(label, ty);
            entry.set_identity(ty);
            entry.rebuild_property_plan();
        }
        let existing_track = self
            .world
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
            bar_layout,
            bar_data,
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

            let track_id = self.world.spawn(label, ty);
            {
                let track =
                    self.world.tracks.get_by_id_mut(track_id).expect("spawn creates the track");
                track.set_identity(ty);
                track.rebuild_property_plan();
            }
            // Plot-family build path only (this dispatch serves Graph,
            // PlotCurve, VectorField, Heatmap, ContourSet, NumberPlane and
            // BarChart), so it owns the plot payload outright: assigning all
            // three fields mirrors the old flat-field overwrite, including the
            // clear on a re-declaration that resolves no plot.
            {
                let payload = self.world.ensure_plot_payload(track_id);
                payload.procedural_plot = procedural_plot;
                // The bar chart's side channel: a `data = {…}` assignment needs
                // the layout to rebuild paths per frame, and the dataset it
                // replaces.
                payload.bar_layout = bar_layout;
                payload.bar_data = bar_data;
            }

            // Seed plot_param_tracks with initial declaration values.
            // Only create tracks that don't already exist so re-declarations
            // preserve existing keyframes.
            let param_names: Vec<(String, f64)> = self
                .world
                .procedural_plot(track_id)
                .map(|plot| plot.params.iter().map(|(n, v)| (n.clone(), *v)).collect())
                .unwrap_or_default();
            if !param_names.is_empty() {
                let param_tracks = &mut self.world.ensure_plot_payload(track_id).plot_param_tracks;
                for (name, default_val) in param_names {
                    param_tracks.entry(name).or_insert_with(|| {
                        let mut t = super::PropertyTrack::new(default_val);
                        t.add_keyframe(0, default_val, super::Easing::Linear);
                        t
                    });
                }
            }

            self.world.meta.record_first_seen(track_id, t_start_ms);

            let track = self.world.tracks.get_by_id_mut(track_id).expect("spawn creates the track");

            // Pre-seed opacity for pre-keyframe first declarations so that
            // insert_start_keyframes captures the correct invisible start value.
            if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
                self.world
                    .meta
                    .get_or_insert_with(track.id, crate::timeline::world::EntityMeta::default)
                    .hidden_by_default = true;
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

            Self::insert_plot_decoration_keyframes(
                track,
                props,
                &eval_env,
                label,
                t_start_ms,
                t_end_ms,
                easing,
                diagnostics,
            );

            // === Tick Labels ===
            if let Some(ref tick_data) = tick_label_data {
                // X-axis tick labels (positioned below the axis line)
                for (i, &(sx, sy, val)) in tick_data.x_labels.iter().enumerate() {
                    let child_label = format!("{}_tick_x_{}", label, i);
                    let label_str = crate::timeline::build::plot::format_axis_tick_label(val);
                    let tick_props = vec![
                        Property::new("text", Expr::Str(label_str)),
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
                    let label_str = crate::timeline::build::plot::format_axis_tick_label(val);
                    let tick_props = vec![
                        Property::new("text", Expr::Str(label_str)),
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
