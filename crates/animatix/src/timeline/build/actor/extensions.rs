//! Extension and plot actor build helpers.

use super::super::plot::ProcessedPlotActor;
use super::helpers::{ActorKeyframeTiming, ActorKeyframeValues};
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
        ) && let Some(id) = self.world.get_id(label)
        {
            let t_start_ms = time_ms as u64;
            let geometry = self.world.geometry_mut(id);
            preserve_discrete_position_state_before(geometry, t_start_ms);
            set_track_position_binding(geometry, t_start_ms, binding);
            if let Some(bound_position) = bound_position {
                geometry.position.ensure([0.0, 0.0]).add_keyframe(
                    t_start_ms,
                    bound_position,
                    Easing::Linear,
                );
            }
            mark_track_manual_position(geometry, t_start_ms);
        }

        // Declared opacity/size write through the property plan slots.
        let _ = (ty, modifiers);
        for prop in props {
            match prop.name.as_str() {
                "opacity" => {
                    if let Ok(crate::timeline::Value::Num(v)) =
                        super::evaluate_expr(&prop.value, &eval_env)
                        && let Some(id) = self.world.get_id(label)
                    {
                        self.world.style_mut(id).opacity.ensure(1.0).add_keyframe(
                            time_ms as u64,
                            v.clamp(0.0, 1.0) as f32,
                            Easing::Linear,
                        );
                    }
                },
                "size" => {
                    if let Ok(crate::timeline::Value::Vec2(v)) =
                        super::evaluate_expr(&prop.value, &eval_env)
                        && let Some(id) = self.world.get_id(label)
                    {
                        let geometry = self.world.geometry_mut(id);
                        geometry
                            .size
                            .ensure(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
                            .add_keyframe(
                                time_ms as u64,
                                [v[0] as f32, v[1] as f32],
                                Easing::Linear,
                            );
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
        let actor_caps = self.world.caps(track_id);

        for prop in props {
            let subject = format!("{}.{}", label, prop.name);
            if let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name) {
                if primitive.is_some_and(|primitive| primitive.declares_property(&prop.name))
                    && let Some(pv) = crate::timeline::property_engine::parse_property_value(
                        schema.value_type,
                        &prop.value,
                        &eval_env,
                        diagnostics,
                        &subject,
                    )
                {
                    self.world.write_property_field(
                        track_id,
                        schema.field_for_actor(&actor_caps, ty),
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        diagnostics,
                    );
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
                    self.world.property_plan.get_mut(track_id),
                    &ctx,
                    ty,
                    &prop.name,
                    pv,
                    t_start_ms,
                    t_end_ms,
                    easing,
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
        style: &mut crate::timeline::StyleTracks,
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
            let default_caps = animatix_std::caps_for_type("PlotCurve").unwrap_or_default();
            let pv_default = (schema.default_value)(&default_caps);
            let has_duration = t_end_ms > t_start_ms;
            let has_delay = t_start_ms > 0 && !has_duration;
            if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                schema.value_type,
                &prop.value,
                eval_env,
                diagnostics,
                &subject,
            ) {
                use crate::timeline::property_engine::{
                    write_f32, write_f32_list, write_gradient, write_string,
                };
                match schema.field {
                    crate::timeline::ActorField::DashPattern => write_f32_list(
                        &mut style.dash_pattern,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default {
                            PropertyValue::F32List(d) => d,
                            _ => Vec::new(),
                        },
                        has_duration,
                        has_delay,
                    ),
                    crate::timeline::ActorField::DashOffset => write_f32(
                        &mut style.dash_offset,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default {
                            PropertyValue::F32(d) => d,
                            _ => 0.0,
                        },
                        has_duration,
                        has_delay,
                    ),
                    crate::timeline::ActorField::FillGradient => write_gradient(
                        &mut style.fill_gradient,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default {
                            PropertyValue::Gradient(d) => d,
                            _ => Default::default(),
                        },
                        has_duration,
                        has_delay,
                    ),
                    crate::timeline::ActorField::StrokeGradient => write_gradient(
                        &mut style.stroke_gradient,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default {
                            PropertyValue::Gradient(d) => d,
                            _ => Default::default(),
                        },
                        has_duration,
                        has_delay,
                    ),
                    crate::timeline::ActorField::GradientExtend => write_string(
                        &mut style.gradient_extend,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default.clone() {
                            PropertyValue::String(d) => d,
                            _ => String::new(),
                        },
                        has_duration,
                        has_delay,
                    ),
                    crate::timeline::ActorField::GradientSpace => write_string(
                        &mut style.gradient_space,
                        pv,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        match pv_default.clone() {
                            PropertyValue::String(d) => d,
                            _ => String::new(),
                        },
                        has_duration,
                        has_delay,
                    ),
                    _ => {},
                }
            }
        }
    }

    // === Plot Dispatch Methods ===

    /// Dispatch method for plot actor kinds (called from Primitive::build impl)
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

        // The plot host's track must exist before `process_plot_actor` runs: it
        // processes children, and `add_node` requires the parent track to exist
        // rather than defaulting one in. The shared prologue also captures
        // first-declaration status and seeds the hidden-by-default opacity
        // before any start snapshot is taken.
        let frame = self.begin_actor_decl(label, ty, props, None);
        let existing_track_id = frame.track_id;

        if let Some(processed) = self.process_plot_actor(
            label,
            ty,
            props,
            time_ms,
            parent_label,
            children,
            diagnostics,
            existing_track_id,
        ) {
            self.finish_plot_actor_dispatch(
                label,
                props,
                modifiers,
                time_ms,
                parent_label,
                diagnostics,
                frame,
                existing_track_id,
                processed,
            );
        }
    }

    /// Keyframe a resolved plot declaration onto the track the prologue spawned.
    fn finish_plot_actor_dispatch(
        &mut self,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        frame: super::helpers::ActorDeclFrame,
        existing_track_id: crate::timeline::EntityId,
        processed: ProcessedPlotActor,
    ) {
        let ProcessedPlotActor {
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
        } = processed;

        // Use returned values for keyframe insertion
        let mut position = self.world.geometry(existing_track_id).position.last([0.0, 0.0]);
        let eval_env = self.build_eval_env(time_ms as u64);
        for prop in props {
            if (prop.name == "at" || prop.name == "position")
                && let Ok(super::Value::Vec2(pos)) = super::evaluate_expr(&prop.value, &eval_env)
            {
                position = [pos[0] as f32, pos[1] as f32];
            }
        }

        // Resolve `anchor`/`offset` (and scene-relative `at`) exactly like
        // the generic actor path below. The plot builder only consumed
        // absolute `at`/`position`, which silently dropped scene anchoring
        // on plot actors even though the registry marks anchor/offset
        // applicable to them.
        let position_binding = plot_dispatch_position_binding(props, &eval_env, diagnostics, label);
        let size = initial_size;
        // Unified declaration rule (shared with the generic actor path):
        // an explicit `opacity` wins; otherwise a first declaration starts
        // at `default_opacity` and a re-declaration keeps its last authored
        // value. Falling back to 1.0 on re-declaration would silently reset
        // an authored `opacity: 0.05` on the next beat — the render layer
        // already multiplies fill/stroke alpha by node opacity, so the
        // retained value is what should apply.
        let existing_opacity = self.world.style(frame.track_id).opacity.last(1.0);
        let mut opacity = self.declaration_opacity(
            frame.is_first_decl,
            frame.has_explicit_opacity,
            existing_opacity,
        );
        if let Some(prop) = props.iter().find(|p| p.name == "opacity") {
            let subject = format!("{}.{}", label, prop.name);
            opacity =
                evaluate_expr_with_lookup_diagnostic(&prop.value, &eval_env, diagnostics, &subject)
                    .unwrap_or(Value::Num(1.0))
                    .as_num() as f32;
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

        // The prologue above already spawned the track and rebuilt its
        // plan; keep the same entity so a re-declaration writes onto it.
        let track_id = existing_track_id;
        // Plot-family build path only (this dispatch serves Graph,
        // PlotCurve, VectorField, Heatmap, ContourSet, NumberPlane and
        // BarChart), so it owns the plot payload outright: assigning all
        // three fields mirrors the old flat-field overwrite, including the
        // clear on a re-declaration that resolves no plot.
        let payload = self.world.ensure_plot_payload(track_id);
        payload.procedural_plot = procedural_plot;
        payload.bar_layout = bar_layout;
        payload.bar_data = bar_data;

        // Seed plot_param_tracks with initial declaration values. Only create
        // tracks that don't already exist so re-declarations preserve existing
        // keyframes.
        self.seed_plot_param_tracks(track_id);

        // Shared tail: first-seen metadata, position binding, start/end
        // keyframes and shape bounds. Plot hosts are never layout
        // containers, so the container-default binding does not apply, and
        // they are never `Rect`, so corner/dash carry the track values.
        let corner_radius = self.world.shape_or_default(track_id).corner_radius.last(0.0);
        let dash_pattern = self.world.style(track_id).dash_pattern.last(Vec::new());
        let dash_offset = self.world.style(track_id).dash_offset.last(0.0);
        let timing =
            ActorKeyframeTiming::plain(t_start_ms, t_end_ms, easing, duration_ms, delay_ms);
        let values = ActorKeyframeValues {
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
        };
        self.finish_actor_decl(
            &frame,
            false, // plot hosts are never layout containers
            position_binding,
            parent_label,
            &timing,
            values,
        );

        Self::insert_plot_decoration_keyframes(
            self.world.style.get_mut(track_id),
            props,
            &eval_env,
            label,
            t_start_ms,
            t_end_ms,
            easing,
            diagnostics,
        );

        // === Tick Labels ===
        // Axis x, axis y and bar labels differ only in suffix, position and
        // text; build the three families once and lower them through one
        // property construction.
        if let Some(ref tick_data) = tick_label_data {
            self.spawn_plot_tick_labels(label, tick_data, time_ms, diagnostics);
        }
    }

    /// Seed `plot_param_tracks` from a procedural plot's initial parameters.
    fn seed_plot_param_tracks(&mut self, track_id: crate::timeline::EntityId) {
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
    }

    /// Lower a plot declaration's axis and bar labels into `Text` children.
    fn spawn_plot_tick_labels(
        &mut self,
        label: &str,
        tick_data: &super::super::plot::TickLabelData,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut tick_actors: Vec<(String, f64, f64, String)> = Vec::new();
        for (i, &(sx, sy, val)) in tick_data.x_labels.iter().enumerate() {
            tick_actors.push((
                format!("{}_tick_x_{}", label, i),
                sx,
                sy,
                crate::timeline::build::plot::format_axis_tick_label(val),
            ));
        }
        for (i, &(sx, sy, val)) in tick_data.y_labels.iter().enumerate() {
            tick_actors.push((
                format!("{}_tick_y_{}", label, i),
                sx,
                sy,
                crate::timeline::build::plot::format_axis_tick_label(val),
            ));
        }
        for (i, &(sx, sy, ref text)) in tick_data.bar_labels.iter().enumerate() {
            tick_actors.push((format!("{}_bar_label_{}", label, i), sx, sy, text.clone()));
        }

        for (child_label, sx, sy, text) in tick_actors {
            let tick_props = vec![
                Property::new("text", Expr::Str(text)),
                Property::new("at", Expr::Tuple(vec![Expr::Num(sx), Expr::Num(sy)])),
                Property::new("font_size", Expr::Num(10.0)),
                Property::new("color", Expr::Path(vec!["text".to_string(), "muted".to_string()])),
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

/// Collect a plot declaration's `at`/`position`, `anchor` and `offset`
/// expressions and resolve the shared position binding.
fn plot_dispatch_position_binding(
    props: &[Property],
    eval_env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    label: &str,
) -> Option<(crate::timeline::PositionBinding, Option<[f32; 2]>)> {
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
    resolve_position_binding_with_lookup_diagnostic(
        at_expr,
        anchor_expr,
        offset_expr,
        eval_env,
        diagnostics,
        label,
    )
}
