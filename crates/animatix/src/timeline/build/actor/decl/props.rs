//! Declaration-time property passes for the generic actor build.
//!
//! Each pass owns one property family and is called from
//! [`Timeline::process_actor_decl`](super::Timeline::process_actor_decl) at the
//! point in the declaration lifecycle where its writes belong: the explicit
//! style/geometry pass runs before the track is spawned, while the connector and
//! registry-complement passes need the spawned track.

use super::super::helpers::ActorKeyframeValues;
use super::super::prepare::apply_size_spec;
use super::*;
use crate::ast::{Expr, Property};
use crate::timeline::vello_path::VelloPath;

/// Style and geometry values the explicit declaration pass accumulates before
/// the track exists.
///
/// Keeping them together lets the property loop stay a single ordinal pass over
/// `props` (an authored `size` and `color` must still apply in declaration
/// order) without a dozen mutable borrows crossing the helper boundary.
pub(super) struct DeclarationPropertyState {
    pub color: [f32; 4],
    pub stroke_color: [f32; 4],
    pub stroke_width: f32,
    pub stroke_progress: f32,
    pub opacity: f32,
    pub fill_opacity: f32,
    pub dash_pattern: Vec<f32>,
    pub dash_offset: f32,
    pub stroke_color_explicitly_set: bool,
    pub size: [f32; 2],
    pub line_from: [f32; 2],
    pub line_to: [f32; 2],
    pub arc_angles: [f32; 2],
    pub corner_radius: f32,
    pub vector_shape_state: VectorShapeState,
}

/// Values the declaration tail needs in addition to
/// [`DeclarationPropertyState`], all captured before the track is spawned.
pub(super) struct DeclarationSeed {
    pub state: DeclarationPropertyState,
    pub position: [f32; 2],
    pub shape_type: ShapeType,
    pub vector_shape: bool,
    pub legend_color: Option<[f32; 4]>,
    pub has_explicit_color: bool,
    pub has_explicit_stroke: bool,
}

impl Timeline {
    /// Capture the existing track's style/geometry (if any) and build the
    /// vector-shape state from the declaration.
    ///
    /// This is the last read of the pre-declaration components: every later
    /// pass works from this seeded state and the authored properties, so a
    /// first declaration and a re-declaration share one starting point.
    pub(super) fn seed_declaration_property_state(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        caps: &ActorCaps,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> DeclarationSeed {
        let default_size = DEFAULT_LAYOUT_HALF_SIZE;
        let default_arc = [0.0, 0.0];
        let existing_shape = self
            .world
            .get_id(label)
            .map(|id| self.world.shape_or_default(id))
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
        let position = existing_geometry.position.last([0.0, 0.0]);
        let mut size = existing_geometry.size.last(default_size);
        let mut line_from = existing_shape.line_from.last([-50.0, 0.0]);
        let mut line_to = existing_shape.line_to.last([50.0, 0.0]);
        let mut arc_angles = existing_shape.arc_angles.last(default_arc);
        let mut corner_radius = existing_shape.corner_radius.last(0.0);
        let color = existing_style.color.last(DEFAULT_WHITE);
        let has_explicit_opacity = props.iter().any(|p| p.name == "opacity");
        let is_first_decl = !self.world.tracks.contains_key(label);
        let opacity = if is_first_decl && !has_explicit_opacity {
            self.default_opacity
        } else {
            existing_style.opacity.last(1.0)
        };
        let stroke_width = existing_style.stroke_width.last(caps.default_stroke_width());
        let stroke_color = existing_style.stroke_color.last(DEFAULT_WHITE);
        let legend_color = self
            .world
            .get_id(label)
            .and_then(|id| self.world.legend(id))
            .and_then(|legend| legend.color);
        let stroke_progress = existing_style.stroke_progress.last(1.0);
        let fill_opacity = existing_style.fill_opacity.last(1.0);
        let dash_pattern = existing_style.dash_pattern.last(Vec::new());
        let dash_offset = existing_style.dash_offset.last(0.0);

        let vector_shape = self
            .primitive_registry
            .info_of(ty)
            .is_some_and(|info| info.capabilities.is_shape);
        let shape_type = shape_type_for_actor(ty).unwrap_or(ShapeType::Rect);
        let vector_shape_state = self.build_vector_shape_state(
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

        let has_explicit_color = props.iter().any(|p| p.name == "color");
        let has_explicit_stroke =
            props.iter().any(|p| p.name == "stroke" || p.name == "stroke_color");

        DeclarationSeed {
            state: DeclarationPropertyState {
                color,
                stroke_color,
                stroke_width,
                stroke_progress,
                opacity,
                fill_opacity,
                dash_pattern,
                dash_offset,
                stroke_color_explicitly_set: has_explicit_stroke,
                size,
                line_from,
                line_to,
                arc_angles,
                corner_radius,
                vector_shape_state,
            },
            position,
            shape_type,
            vector_shape,
            legend_color,
            has_explicit_color,
            has_explicit_stroke,
        }
    }

    /// Seed the style from the active colorscheme when the declaration does not
    /// name a color/stroke.
    pub(super) fn apply_scheme_default_colors(
        &self,
        ty: &str,
        caps: &ActorCaps,
        has_explicit_color: bool,
        has_explicit_stroke: bool,
        state: &mut DeclarationPropertyState,
    ) {
        if !has_explicit_color
            && let Some(primitive) = self.primitive_registry.find(ty)
            && let Some(scheme_color) = self.get_default_color(primitive, caps, "color")
        {
            state.color = scheme_color;
            if caps.category == ActorCategory::Plot {
                state.stroke_color = scheme_color;
            }
        }
        if !has_explicit_stroke
            && let Some(primitive) = self.primitive_registry.find(ty)
            && let Some(scheme_stroke) = self.get_default_color(primitive, caps, "stroke")
        {
            state.stroke_color = scheme_stroke;
        }
    }

    /// Apply the declaration's explicit style/geometry properties in authored
    /// order.
    ///
    /// This is the pre-spawn pass: it only computes the values the declaration
    /// tail writes into the fresh keyframes, so it must not touch the world.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_explicit_declaration_props(
        &mut self,
        state: &mut DeclarationPropertyState,
        label: &str,
        ty: &str,
        props: &[Property],
        eval_env: &Environment,
        primitive: &PrimitiveFamilyDescriptor,
        has_vector_shape: bool,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for prop in props {
            let prop_subject = format!("{}.{}", label, prop.name);
            match prop.name.as_str() {
                "at" | "anchor" | "offset" => {},
                "color" => {
                    if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                        if let Some(actor_color) = self.auto_color_for_label(label) {
                            state.color = actor_color;
                            if primitive.is_plot_curve() {
                                state.stroke_color = actor_color;
                            }
                        } else {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::UnknownColorReference,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; using the default color instead.",
                                        label
                                    ),
                                )
                                .with_subject(&prop_subject),
                            );
                        }
                    } else if let Some(resolved_color) = parse_color_in_env_with_lookup_diagnostic(
                        label,
                        "color",
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        state.color = resolved_color;
                        if primitive.is_plot_curve() {
                            state.stroke_color = resolved_color;
                        }
                    }
                },

                "stroke_width" | "width" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    state.stroke_width = v.as_num() as f32;
                },
                "stroke_color" | "stroke" => {
                    state.stroke_color_explicitly_set = true;
                    if let Some(resolved_color) = parse_color_in_env_with_lookup_diagnostic(
                        label,
                        "stroke_color",
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    ) {
                        state.stroke_color = resolved_color;
                    }
                },
                "stroke_progress" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    state.stroke_progress = v.as_num() as f32;
                },
                "opacity" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(1.0));
                    state.opacity = v.as_num() as f32;
                },
                "fill_opacity" => {
                    let v = evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    state.fill_opacity = v.as_num() as f32;
                },
                "dash_pattern" => {
                    match evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        eval_env,
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
                                state.dash_pattern = values;
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
                        eval_env,
                        diagnostics,
                        &prop_subject,
                    )
                    .unwrap_or(Value::Num(0.0));
                    state.dash_offset = v.as_num() as f32;
                },
                _ if has_vector_shape
                    && apply_vector_shape_property(
                        ty,
                        &prop.name,
                        &prop.value,
                        eval_env,
                        diagnostics,
                        &prop_subject,
                        &mut state.vector_shape_state,
                    ) =>
                {
                    (
                        state.size,
                        state.line_from,
                        state.line_to,
                        state.arc_angles,
                        state.corner_radius,
                    ) = extract_shape_state_values(&state.vector_shape_state);
                },
                _ => {},
            }
        }
    }

    /// Apply the declaration-tail rules that depend on the resolved style and
    /// shape state (stroke-only shapes, vector-shape finalization, container
    /// fill suppression).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn finalize_declaration_state(
        state: &mut DeclarationPropertyState,
        ty: &str,
        caps: &ActorCaps,
        primitive: &PrimitiveFamilyDescriptor,
        props: &[Property],
        has_explicit_color: bool,
        has_explicit_stroke: bool,
        vector_shape: bool,
    ) {
        if !state.stroke_color_explicitly_set && caps.shape.is_some_and(|s| s.is_stroke_only()) {
            state.stroke_color = state.color;
        }

        if vector_shape {
            finalize_vector_shape_state(ty, &mut state.vector_shape_state);
            (
                state.size,
                state.line_from,
                state.line_to,
                state.arc_angles,
                state.corner_radius,
            ) = extract_shape_state_values(&state.vector_shape_state);
        }

        if primitive.is_graph_host() || primitive.is_layout_container() {
            state.fill_opacity = 0.0;
            state.stroke_width = 0.0;
        }

        // A stroked path without an authored fill renders as a line; closed
        // shapes (Rect/Ellipse/Polygon), paths with an authored color, and
        // paths with an authored `fill_opacity` keep their fill.
        if has_explicit_stroke
            && !has_explicit_color
            && !props.iter().any(|p| p.name == "fill_opacity")
            && caps.shape == Some(ShapeKind::Path)
        {
            state.fill_opacity = 0.0;
        }
    }

    /// Seed Callout defaults before user properties so fields used by targeted
    /// geometry and text evaluation are always present, even without explicit
    /// props.
    pub(super) fn seed_callout_defaults(
        &mut self,
        track_id: EntityId,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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

    /// Write connector geometry lanes for a connector declaration.
    ///
    /// Connectors keep the generic declaration tail (style, transforms,
    /// opacity, gradients) and only add their endpoint/routing lanes here.
    pub(super) fn apply_connector_declaration_props(
        &mut self,
        track_id: EntityId,
        props: &[Property],
        eval_env: &Environment,
        t_start_ms: u64,
        easing: Easing,
    ) {
        self.world.ensure_shape(track_id);
        let geometry = self.world.geometry.get_mut(track_id);
        let shape = self.world.shape.get_mut(track_id).expect("shape component");
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
                    if let Some(endpoint) = crate::primitives::connector::parse_connector_endpoint(
                        &prop.value,
                        eval_env,
                    ) {
                        geometry
                            .connector_from
                            .ensure(endpoint.clone())
                            .add_keyframe(t_start_ms, endpoint, easing);
                    }
                },
                "to" => {
                    if let Some(endpoint) = crate::primitives::connector::parse_connector_endpoint(
                        &prop.value,
                        eval_env,
                    ) {
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
                    if let Ok(Value::Num(n)) = crate::timeline::evaluate_expr(&prop.value, eval_env)
                    {
                        shape
                            .corner_radius
                            .ensure(n as f32)
                            .add_keyframe(t_start_ms, n as f32, easing);
                    }
                },
                "head_size" => {
                    if let Ok(Value::Num(n)) = crate::timeline::evaluate_expr(&prop.value, eval_env)
                    {
                        shape.head_size.ensure(n as f32).add_keyframe(t_start_ms, n as f32, easing);
                    }
                },
                _ => {},
            }
        }
    }

    /// `perspective` is declared for every actor (default 800) as the focal
    /// length of the planned 2.5D perspective tilt, but only `rotate_x` /
    /// `rotate_y` are implemented and they apply an *orthographic* tilt — no
    /// focal length, no `1/w` foreshortening. So an authored `perspective` is
    /// stored and never read; say so instead of accepting a value that does
    /// nothing.
    pub(super) fn warn_unimplemented_perspective(
        label: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
    }

    /// Warn when a child of a layout container uses `at` or `position`.
    /// Layout-managed children should use `transform` for visual offsets.
    pub(super) fn warn_layout_managed_position(
        &self,
        label: &str,
        parent_label: Option<&str>,
        extracted: &ExtractedActorProperties,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(parent) = parent_label
            && let Some(parent_id) = self.world.get_id(parent)
            && self.world.caps(parent_id).layout_container
        {
            let has_at = extracted.at_expr.is_some();
            let has_position = props.iter().any(|p| p.name == "position");
            let has_anchor = extracted.anchor_expr.is_some();
            if has_at || has_position {
                diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::AbsolutePositionOnLayoutManagedChild,
                                DiagnosticPhase::Build,
                                format!(
                                    "Actor '{label}' in container '{parent}' has 'at'/'position', which is applied as a manual local offset on top of its layout slot: the actor keeps its slot but can overlap its siblings. Use 'transform' for visual offsets that leave layout untouched.",
                                ),
                            )
                            .with_subject(label),
                        );
            }
            // `offset` only modifies an `at`/`anchor` binding. With
            // neither, the child stays layout-managed and the offset is
            // dropped silently, so name the child and the property.
            if extracted.offset_expr.is_some() && !has_at && !has_position && !has_anchor {
                diagnostics.push(
                            Diagnostic::warning(
                                crate::diagnostics::DiagnosticCode::IgnoredOffset,
                                crate::diagnostics::DiagnosticPhase::Build,
                                format!(
                                    "Actor '{label}' in container '{parent}' has 'offset' which is ignored: a layout-managed child's position comes from the container, and 'offset' only applies with 'at' or 'anchor'. Use 'transform' for a visual offset.",
                                ),
                            )
                            .with_subject(label),
                        );
            }
        }
    }

    /// Write registry-backed declaration properties the per-primitive build
    /// path does not reach.
    ///
    /// Without this pass those properties would be dropped for every actor
    /// kind. `line_from`/`line_to` are mirrored back into `state` so the
    /// generated path uses the same values the track just received.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_registry_complement_props(
        &mut self,
        track_id: EntityId,
        primitive: &PrimitiveFamilyDescriptor,
        caps: &ActorCaps,
        props: &[Property],
        eval_env: &Environment,
        label: &str,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
        state: &mut DeclarationPropertyState,
    ) {
        let actor_type = self.world.actor_type(track_id).to_string();
        for prop in props {
            if let Some(schema) = crate::timeline::property_registry::lookup_property(&prop.name) {
                let field = schema.field_for_actor(caps, &actor_type);
                if !matches!(
                    field,
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
                        | crate::timeline::ActorField::MaxItemWidth
                        | crate::timeline::ActorField::AudioVolume
                        | crate::timeline::ActorField::HeadSize
                        | crate::timeline::ActorField::LabelAt
                        | crate::timeline::ActorField::CalloutTarget
                        | crate::timeline::ActorField::CalloutStandoff
                        | crate::timeline::ActorField::CalloutToOffset
                        | crate::timeline::ActorField::LineFrom
                        | crate::timeline::ActorField::LineTo
                        | crate::timeline::ActorField::TextContent
                        | crate::timeline::ActorField::TextMaxWidth
                ) {
                    continue;
                }
                if field == crate::timeline::ActorField::CalloutTarget {
                    let target_str = crate::timeline::lookup::parse_actor_ref_literal(&prop.value)
                        .or_else(|| match crate::timeline::evaluate_expr(&prop.value, eval_env) {
                            Ok(Value::Str(s)) => Some(s),
                            _ => None,
                        });
                    if let Some(target) = target_str {
                        self.world.write_property_field(
                            track_id,
                            field,
                            crate::timeline::property_engine::PropertyValue::String(target),
                            t_start_ms,
                            t_end_ms,
                            easing,
                            diagnostics,
                        );
                    }
                    continue;
                }
                if primitive.connector
                    && (field == crate::timeline::ActorField::LineFrom
                        || field == crate::timeline::ActorField::LineTo)
                {
                    continue;
                }
                if matches!(caps.shape, Some(ShapeKind::Line | ShapeKind::Arrow))
                    && (prop.name == "from" || prop.name == "to")
                    && let Expr::Path(segments) = &prop.value
                    && segments.len() == 2
                    && SceneAnchor::from_str(&segments[1]).is_some()
                {
                    continue;
                }
                let prop_subject = format!("{label}.{}", prop.name);
                if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                    schema.value_type,
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                ) {
                    if let crate::timeline::property_engine::PropertyValue::Vec2(v) = &pv {
                        if field == crate::timeline::ActorField::LineFrom {
                            state.line_from = *v;
                        } else if field == crate::timeline::ActorField::LineTo {
                            state.line_to = *v;
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
    }
}

impl DeclarationPropertyState {
    /// Convert the accumulated declaration state into the keyframe payload the
    /// shared declaration tail writes.
    pub(super) fn into_keyframe_values(
        self,
        position: [f32; 2],
        shape_type: ShapeType,
        vello_paths: Vec<VelloPath>,
    ) -> ActorKeyframeValues {
        ActorKeyframeValues {
            position,
            size: self.size,
            line_from: self.line_from,
            line_to: self.line_to,
            arc_angles: self.arc_angles,
            corner_radius: self.corner_radius,
            color: self.color,
            shape_type,
            opacity: self.opacity,
            stroke_width: self.stroke_width,
            stroke_color: self.stroke_color,
            stroke_progress: self.stroke_progress,
            fill_opacity: self.fill_opacity,
            dash_pattern: self.dash_pattern,
            dash_offset: self.dash_offset,
            vello_paths,
        }
    }
}

impl Timeline {
    /// Declaration passes that need the spawned track: legend participation,
    /// Callout seeds, connector lanes, the unimplemented-property warning, the
    /// registry complement, and the prepared layout size spec.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_spawned_declaration_props(
        &mut self,
        track_id: EntityId,
        caps: &ActorCaps,
        primitive: &PrimitiveFamilyDescriptor,
        legend_color: Option<[f32; 4]>,
        extracted: &ExtractedActorProperties,
        props: &[Property],
        eval_env: &Environment,
        label: &str,
        parent_label: Option<&str>,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
        state: &mut DeclarationPropertyState,
    ) {
        // Legend participation is a component, not a track field. Only actors
        // with an explicit color/stroke become automatic legend candidates, so
        // actors with nothing to contribute carry no component at all.
        if let Some(color) = legend_color {
            self.world.ensure_legend(track_id).color = Some(color);
        }
        if caps.callout {
            self.seed_callout_defaults(track_id, t_start_ms, t_end_ms, easing, diagnostics);
        }
        // Connector declaration properties are written into the connector
        // geometry lanes here, in the generic body, so the actor keeps the whole
        // generic declaration tail (style, transforms, opacity, gradients).
        // Catalog data decides this, not a `ty == "Connector"` compare.
        if primitive.connector {
            self.apply_connector_declaration_props(track_id, props, eval_env, t_start_ms, easing);
        }
        Self::warn_unimplemented_perspective(label, props, diagnostics);
        self.write_registry_complement_props(
            track_id,
            primitive,
            caps,
            props,
            eval_env,
            label,
            t_start_ms,
            t_end_ms,
            easing,
            diagnostics,
            state,
        );
        {
            let geometry = self.world.geometry.get_mut(track_id);
            apply_size_spec(
                geometry,
                primitive,
                extracted.size_decl.as_ref(),
                label,
                parent_label,
                diagnostics,
            );
        }
    }
}
