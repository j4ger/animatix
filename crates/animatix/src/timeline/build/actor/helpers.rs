//! Coordinate resolution, path generation, keyframe insertion and warnings.

use super::*;
use crate::ast::Property;
use crate::timeline::animation_track::{GeometryTracks, ShapeTracks};
use crate::timeline::vello_path::VelloPath;

impl Timeline {
    /// Resolve an active CoordinateContext if the given parent label refers to a coordinate host.
    pub(crate) fn resolve_coordinate_context(
        &self,
        parent_label: Option<&str>,
    ) -> Option<crate::timeline::CoordinateContext> {
        let p_label = parent_label?;
        let host_id = self.world.tracks.get_id(p_label)?;
        let frame = self.coordinate_frame(host_id)?;
        let p_size = self
            .env
            .get(crate::timeline::env_keys::side_channel(p_label, "size").as_str())
            .or_else(|| self.env.get(&format!("{}_size", p_label)))
            .and_then(|v| match v {
                Value::Vec2(sz) => Some(sz),
                _ => None,
            })
            .unwrap_or([500.0, 500.0]);
        Some(crate::timeline::CoordinateContext::new(host_id, frame, p_size))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn generate_actor_paths(
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
            let mut p_padding = [0.0; 4];

            // The parent host's frame component is the only coordinate source.
            if let Some(frame) = self.coordinate_frame_for(&p_label) {
                p_x_domain = frame.x_domain;
                p_y_domain = frame.y_domain;
                p_padding = frame.padding;
            }
            if let Some(Value::Vec2(sz)) =
                self.env.get(&format!("{}_size", p_label)).or_else(|| {
                    self.env.get(crate::timeline::env_keys::side_channel(&p_label, "size").as_str())
                })
            {
                p_size = sz;
            }

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
    pub(super) fn insert_actor_keyframes(
        track: &mut AnimationTrack,
        geometry: &mut GeometryTracks,
        shape: &mut ShapeTracks,
        caps: ActorCaps,
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
            insert_start_keyframes(track, geometry, shape, caps, t_start_ms);
        } else if delay_ms > 0.0 {
            preserve_delayed_values(track, geometry, shape, t_start_ms);
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
            geometry,
            shape,
            caps,
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
    /// - Rows declared `Applicable::Everything` (`color`, `opacity`, `at`, …) pass unconditionally,
    ///   so a primitive that ignores one of those is invisible here. That is a defect in the table,
    ///   not in this check, and the fix is to narrow the row.
    /// - Coverage matches `warn_unknown_declaration_properties`' exactly, which means the
    ///   plot-family dispatch is not covered at all (it never calls either function). Its runtime
    ///   parameters are the reason: `freq: 2` on `func: (x) => sin(freq * x)` is read by the
    ///   author's closure, not by a primitive, and the exemption would have to be
    ///   `plot_runtime_params`.
    pub(super) fn warn_inapplicable_declaration_properties(
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
    /// Read the build-time `camera_follow` declaration onto the track.
    ///
    /// Called from `process_body`'s actor arm rather than from
    /// [`Self::process_actor_decl`], because the actor families that build
    /// through their own `process_*_decl` — Text, Media, Audio, plot
    /// primitives, extensions — return before the generic property walk gets
    /// there. A read placed there would work on a `Rect` and be silently ignored
    /// on a `Text`, which is the actor that most wants a HUD pinned to the
    /// viewport. (Measured: the first version of this did exactly that, and the
    /// text overlay in `examples/animation/37_hud_overlay.amx` still moved.)
    pub(crate) fn apply_camera_follow_decl(
        &mut self,
        label: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(prop) = props.iter().find(|p| p.name == "camera_follow") {
            let subject = format!("{label}.camera_follow");
            let parsed = crate::timeline::property_engine::parse_property_value(
                crate::timeline::property_registry::ValueType::Bool,
                &prop.value,
                &self.env,
                diagnostics,
                &subject,
            );
            // A value the bool parser rejects is already reported by it; the flag
            // keeps its default, so the actor stays camera'd rather than guessing.
            if let Some(crate::timeline::property_engine::PropertyValue::Bool(follow)) = parsed {
                if let Some(id) = self.world.tracks.get_id(label) {
                    let meta = self.world.meta_mut(id);
                    meta.camera_follow = follow;
                    if !follow {
                        meta.parallax = 0.0;
                    }
                }
            }
        }

        if let Some(prop) = props.iter().find(|p| p.name == "parallax") {
            let subject = format!("{label}.parallax");
            let parsed = crate::timeline::property_engine::parse_property_value(
                crate::timeline::property_registry::ValueType::F32,
                &prop.value,
                &self.env,
                diagnostics,
                &subject,
            );
            if let Some(crate::timeline::property_engine::PropertyValue::F32(p)) = parsed {
                if let Some(id) = self.world.tracks.get_id(label) {
                    self.world.meta_mut(id).parallax = p;
                }
            }
        }
    }
}
