//! Coordinate resolution, path generation and keyframe insertion.

use super::prepare::apply_position_binding;
use super::*;
use crate::ast::Property;
use crate::timeline::animation_track::{GeometryTracks, ShapeTracks, StyleTracks};
use crate::timeline::vello_path::VelloPath;

/// The track a declaration writes to, plus the facts the shared prologue
/// captured before it created (or reused) that track.
pub(super) struct ActorDeclFrame {
    pub track_id: EntityId,
    pub caps: ActorCaps,
    /// Whether the label had no track before this declaration.
    pub is_first_decl: bool,
    pub has_explicit_opacity: bool,
}

/// Timing and easing inputs for one declaration's keyframe insertion.
pub(super) struct ActorKeyframeTiming {
    pub t_start_ms: u64,
    pub t_end_ms: u64,
    pub easing: Easing,
    pub duration_ms: f64,
    pub delay_ms: f64,
    pub supports_morph_options: bool,
    pub morph_options: MorphOptions,
}

impl ActorKeyframeTiming {
    /// Plain timing for declaration paths with no path-morph support (the plot
    /// family), where morph modifiers are ignored.
    pub(super) fn plain(
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        duration_ms: f64,
        delay_ms: f64,
    ) -> Self {
        Self {
            t_start_ms,
            t_end_ms,
            easing,
            duration_ms,
            delay_ms,
            supports_morph_options: false,
            morph_options: MorphOptions::default(),
        }
    }
}

/// End-state values one declaration writes into the actor's tracks.
pub(super) struct ActorKeyframeValues {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub line_from: [f32; 2],
    pub line_to: [f32; 2],
    pub arc_angles: [f32; 2],
    pub corner_radius: f32,
    pub color: [f32; 4],
    pub shape_type: ShapeType,
    pub opacity: f32,
    pub stroke_width: f32,
    pub stroke_color: [f32; 4],
    pub stroke_progress: f32,
    pub fill_opacity: f32,
    pub dash_pattern: Vec<f32>,
    pub dash_offset: f32,
    pub vello_paths: Vec<VelloPath>,
}

impl Timeline {
    /// Shared declaration prologue for the generic and plot actor paths.
    ///
    /// Captures first-declaration status before creating the track, re-derives
    /// caps from the declared type (a re-declaration must not keep a stale
    /// override), rebuilds the property plan, records the declaration span and
    /// seeds the hidden-by-default opacity. The seed lands at t=0 so a start
    /// snapshot taken later in [`Self::finish_actor_decl`] reads it.
    pub(super) fn begin_actor_decl(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        decl_span: Option<crate::ast::Span>,
    ) -> ActorDeclFrame {
        let is_first_decl = !self.world.tracks.contains_key(label);
        let track_id = self.world.spawn(label, ty);
        // The World form clears any capability override so caps re-derive from
        // the declaration type on a re-declaration.
        self.world.set_identity(track_id, ty);
        let caps = self.world.caps(track_id);
        self.world.property_plan.get_mut(track_id).rebuild_for(&caps, ty);
        if let Some(span) = decl_span
            && self.world.decl_span.get(track_id).is_none()
        {
            self.world.decl_span.get_or_insert_with(track_id, || span);
        }
        let has_explicit_opacity = props.iter().any(|prop| prop.name == "opacity");
        if is_first_decl && !has_explicit_opacity && self.default_opacity != 1.0 {
            self.world
                .meta
                .get_or_insert_with(track_id, crate::timeline::world::EntityMeta::default)
                .hidden_by_default = true;
            self.world.style_mut(track_id).opacity.ensure(1.0).add_keyframe(
                0,
                self.default_opacity,
                Easing::Linear,
            );
        }
        ActorDeclFrame {
            track_id,
            caps,
            is_first_decl,
            has_explicit_opacity,
        }
    }

    /// Unified declaration-opacity rule for both actor paths.
    ///
    /// A first declaration with no explicit `opacity` starts at
    /// `default_opacity` (0 before the first keyframe, so the actor is hidden);
    /// a later declaration with no explicit `opacity` keeps the last authored
    /// value instead of snapping back to 1.0. An explicit opacity always wins,
    /// applied by the caller's property walk.
    pub(super) fn declaration_opacity(
        &self,
        is_first_decl: bool,
        has_explicit_opacity: bool,
        existing: f32,
    ) -> f32 {
        if is_first_decl && !has_explicit_opacity {
            self.default_opacity
        } else {
            existing
        }
    }

    /// Shared declaration tail: bind position, write start/end keyframes,
    /// record first-seen metadata and bound a single-size shape.
    ///
    /// `camera_follow`/`parallax` stay in `process_body`'s actor arm: that runs
    /// for every actor family, including the ones that never reach this tail,
    /// so reading the declaration here would double-apply it.
    pub(super) fn finish_actor_decl(
        &mut self,
        frame: &ActorDeclFrame,
        is_layout_container: bool,
        position_binding: Option<(PositionBinding, Option<[f32; 2]>)>,
        parent_label: Option<&str>,
        timing: &ActorKeyframeTiming,
        values: ActorKeyframeValues,
    ) {
        self.world.meta.record_first_seen(frame.track_id, timing.t_start_ms);
        self.world.ensure_shape(frame.track_id);
        let size = values.size;
        let has_single_size_keyframe;
        {
            let style = self.world.style.get_mut(frame.track_id);
            let shape = self.world.shape.get_mut(frame.track_id).expect("shape component");
            let geometry = self.world.geometry.get_mut(frame.track_id);
            let mut values = values;
            if let Some(bound) = apply_position_binding(
                geometry,
                position_binding,
                is_layout_container,
                parent_label,
                timing.t_start_ms,
            ) {
                values.position = bound;
            }
            Self::insert_actor_keyframes(style, geometry, shape, frame.caps, timing, values);
            has_single_size_keyframe =
                geometry.size.as_ref().is_none_or(|track| track.keyframes().len() <= 1);
        }
        if frame.caps.is_shape && has_single_size_keyframe {
            let bounds =
                kurbo::Rect::new(-size[0] as f64, -size[1] as f64, size[0] as f64, size[1] as f64);
            self.world.set_local_bounds(frame.track_id, bounds);
        }
    }

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

    /// Write a declaration's start, morph and end keyframes in that order.
    pub(super) fn insert_actor_keyframes(
        style: &mut StyleTracks,
        geometry: &mut GeometryTracks,
        shape: &mut ShapeTracks,
        caps: ActorCaps,
        timing: &ActorKeyframeTiming,
        values: ActorKeyframeValues,
    ) {
        if timing.duration_ms > 0.0 {
            insert_start_keyframes(style, geometry, shape, caps, timing.t_start_ms);
        } else if timing.delay_ms > 0.0 {
            preserve_delayed_values(style, geometry, shape, timing.t_start_ms);
        }
        if timing.supports_morph_options {
            style.morph_options.ensure(MorphOptions::default()).add_keyframe(
                timing.t_end_ms,
                timing.morph_options,
                Easing::Linear,
            );
        }

        Self::insert_end_keyframes(
            style,
            geometry,
            shape,
            caps,
            timing.t_end_ms,
            values,
            timing.easing,
        );
    }

    /// Insert end keyframes at `t_end_ms` with the given values and easing.
    fn insert_end_keyframes(
        style: &mut StyleTracks,
        geometry: &mut GeometryTracks,
        shape: &mut ShapeTracks,
        caps: ActorCaps,
        t_end_ms: u64,
        values: ActorKeyframeValues,
        easing: Easing,
    ) {
        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, std::f32::consts::PI];

        shape
            .vector_paths
            .ensure(Vec::new())
            .add_keyframe(t_end_ms, values.vello_paths, easing);
        geometry
            .position
            .ensure([0.0, 0.0])
            .add_keyframe(t_end_ms, values.position, easing);
        geometry.size.ensure(default_size).add_keyframe(t_end_ms, values.size, easing);
        geometry
            .ensure_layout_size(default_size)
            .add_keyframe(t_end_ms, values.size, easing);
        shape
            .line_from
            .ensure([-50.0, 0.0])
            .add_keyframe(t_end_ms, values.line_from, easing);
        shape.line_to.ensure([50.0, 0.0]).add_keyframe(t_end_ms, values.line_to, easing);
        shape
            .arc_angles
            .ensure(default_arc)
            .add_keyframe(t_end_ms, values.arc_angles, easing);
        shape
            .corner_radius
            .ensure(0.0)
            .add_keyframe(t_end_ms, values.corner_radius, easing);
        style.color.ensure(DEFAULT_WHITE).add_keyframe(t_end_ms, values.color, easing);
        shape
            .shape_type
            .ensure(ShapeType::Rect)
            .add_keyframe(t_end_ms, values.shape_type, easing);
        style.opacity.ensure(1.0).add_keyframe(t_end_ms, values.opacity, easing);
        style
            .dash_pattern
            .ensure(Vec::new())
            .add_keyframe(t_end_ms, values.dash_pattern, easing);
        style.dash_offset.ensure(0.0).add_keyframe(t_end_ms, values.dash_offset, easing);
        style.stroke_width.ensure(caps.default_stroke_width()).add_keyframe(
            t_end_ms,
            values.stroke_width,
            easing,
        );
        style.stroke_color.ensure(DEFAULT_WHITE).add_keyframe(
            t_end_ms,
            values.stroke_color,
            easing,
        );
        style
            .stroke_progress
            .ensure(1.0)
            .add_keyframe(t_end_ms, values.stroke_progress, easing);
        style
            .fill_opacity
            .ensure(1.0)
            .add_keyframe(t_end_ms, values.fill_opacity, easing);
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
    /// viewport.
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
