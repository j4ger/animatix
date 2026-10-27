//! Early dispatch, prepare state, size spec and position binding helpers.

use super::*;
use crate::ast::{InlineItem, Property};
use crate::timeline::animation_track::GeometryTracks;

pub(super) fn apply_size_spec(
    geometry: &mut GeometryTracks,
    primitive: &PrimitiveFamilyDescriptor,
    props: &[Property],
    label: &str,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let is_container = primitive.is_layout_container();
    for prop in props {
        if prop.name == "size" {
            let spec = crate::timeline::taffy_layout::parse_size_spec(&prop.value);
            geometry.size_spec = Some(spec);

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

impl Timeline {
    pub(super) fn try_dispatch_prior(
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
    ) -> bool {
        if let Some(def) = self.behavior_registry.find(ty).cloned() {
            // Shapes, layout containers, and connector/callout annotations are handled
            // inline by the generic `process_actor_decl` pipeline.
            let is_inline = matches!(
                def.family,
                crate::timeline::behavior::BehaviorFamily::Shape
                    | crate::timeline::behavior::BehaviorFamily::Container
                    | crate::timeline::behavior::BehaviorFamily::Group
                    | crate::timeline::behavior::BehaviorFamily::Effect
                    | crate::timeline::behavior::BehaviorFamily::Equation
                    | crate::timeline::behavior::BehaviorFamily::Captions
            ) || def.caps.category == animatix_core::caps::ActorCategory::Container
                || def.caps.callout
                || def.caps.connector;

            if !is_inline {
                let coordinate_context = self.resolve_coordinate_context(parent_label);
                let mut ctx = crate::primitives::BuildCtx {
                    timeline: self,
                    label,
                    time_ms,
                    parent_label,
                    diagnostics,
                    coordinate_context,
                };
                if let Err(mut diags) =
                    def.primitive().build(&mut ctx, label, props, modifiers, children)
                {
                    diagnostics.append(&mut diags);
                }
                if let Some(id) = self.world.get_id(label) {
                    if self.world.decl_span.get(id).is_none()
                        && let Some(span) = decl_span
                    {
                        self.world.decl_span.get_or_insert_with(id, || span);
                    }
                }
                return true;
            }
        }

        // Extension primitives are dispatched through the active runtime
        // registry. Built-ins remain on the static actor-kind path below.
        let registry = self.primitive_registry.clone();
        if let Some(primitive) = registry.find(ty) {
            if !registry.is_builtin(ty) {
                let coordinate_context = self.resolve_coordinate_context(parent_label);
                let mut ctx = crate::primitives::BuildCtx {
                    timeline: self,
                    time_ms,
                    parent_label,
                    label,
                    diagnostics,
                    coordinate_context,
                };
                if let Err(mut diags) = primitive.build(&mut ctx, label, props, modifiers, children)
                {
                    diagnostics.append(&mut diags);
                }
                let mut extension_id = None;
                if let Some(id) = self.world.get_id(label) {
                    if self.world.decl_span.get(id).is_none()
                        && let Some(span) = decl_span
                    {
                        self.world.decl_span.get_or_insert_with(id, || span);
                    }
                    extension_id = Some(id);
                }
                if let Some(id) = extension_id {
                    // Identity lives in `World`; set it from the registration
                    // type so caps and plan lookups resolve the extension
                    // primitive (and a re-declaration cannot keep a stale type).
                    self.world.set_identity(id, ty);
                    // Capabilities come from the registration info, not the
                    // built-in catalog: an in-process extension can be a
                    // text-like or shape-like primitive and must get the full
                    // built-in treatment.
                    if let Some(info) = registry.info_of(ty) {
                        self.world.set_caps(id, animatix_std::caps_from_info(info));
                    }
                    // Extension builds create the track; mirror the built-in
                    // path so the actor is visible from its declaration time
                    // instead of being skipped forever (first_seen_ms = MAX).
                    self.world.meta.record_first_seen(id, time_ms as u64);
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
                let coordinate_context = self.resolve_coordinate_context(parent_label);
                let mut ctx = crate::primitives::BuildCtx {
                    timeline: self,
                    time_ms,
                    parent_label,
                    label,
                    diagnostics,
                    coordinate_context,
                };
                if let Err(mut diags) = primitive.finalize_container_build(&mut ctx, label, props) {
                    diagnostics.append(&mut diags);
                }
                return true;
            }
        }
        false
    }
}

pub(super) struct ActorPrep {
    pub(super) caps: animatix_core::caps::ActorCaps,
    pub(super) primitive: PrimitiveFamilyDescriptor,
    pub(super) extracted: ExtractedActorProperties,
}

impl Timeline {
    pub(super) fn prepare_actor_build(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ActorPrep> {
        let Some(caps) = animatix_std::caps_for_type(ty) else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!("Unknown actor type '{}'", ty),
                )
                .with_subject(label),
            );
            return None;
        };

        let primitive = self
            .primitive_registry
            .info_of(ty)
            .map(PrimitiveFamilyDescriptor::from_info)
            .unwrap_or_default();
        // Math coordinate auto-mapping: if the parent carries a coordinate
        // frame component, map child positions from math coordinates to screen
        // pixels.
        let mapped_props = if let Some(p_label) = parent_label {
            if self.coordinate_frame_for(p_label).is_some() {
                self.map_props_to_graph_parent(p_label, props, time_ms, diagnostics)
            } else {
                props.to_vec()
            }
        } else {
            props.to_vec()
        };
        let props_ref: &[Property] = &mapped_props;

        let extracted = self.extract_actor_properties(label, ty, props_ref, time_ms, diagnostics);

        if primitive.is_graph_host() {
            let frame = crate::timeline::CoordinateFrameComponent {
                x_domain: extracted.x_domain,
                y_domain: extracted.y_domain,
                x_scale: extracted.x_scale,
                y_scale: extracted.y_scale,
                padding: extracted.graph_padding,
                grid: false,
                ticks: false,
                tick_labels: "auto".to_string(),
            };
            if let Some(host_id) = self.world.tracks.get_id(label) {
                self.set_coordinate_frame(host_id, frame.clone());
            }
            // Derived env entries for authored expressions, never a source.
            self.env.set(&format!("{}_x_domain", label), Value::Vec2(frame.x_domain));
            self.env.set(&format!("{}_y_domain", label), Value::Vec2(frame.y_domain));
            self.env.set(
                &format!("{}_size", label),
                Value::Vec2([
                    extracted.initial_size[0] as f64 * 2.0,
                    extracted.initial_size[1] as f64 * 2.0,
                ]),
            );
        }

        // Set the parent's identity before processing children so the
        // layout-managed-child check can inspect the parent's kind. Only create
        // the track entry if the actor has children (otherwise skip to avoid
        // breaking is_first_decl).
        if !children.is_empty() {
            let track_id = self.world.spawn(label, ty);
            self.world.set_identity(track_id, ty);
            let track_caps = self.world.caps(track_id);
            self.world.property_plan.get_mut(track_id).rebuild_for(&track_caps, ty);
        }

        self.process_inline_items(time_ms, children, label, diagnostics);
        Some(ActorPrep {
            caps,
            primitive,
            extracted,
        })
    }
}

pub(super) fn apply_position_binding(
    geometry: &mut GeometryTracks,
    position_binding: Option<(PositionBinding, Option<[f32; 2]>)>,
    is_layout_container: bool,
    parent_label: Option<&str>,
    t_start_ms: u64,
) -> Option<[f32; 2]> {
    let mut bound_position = None;
    if let Some((binding, bound)) = position_binding {
        preserve_discrete_position_state_before(geometry, t_start_ms);
        set_track_position_binding(geometry, t_start_ms, binding);
        if let Some(bound) = bound {
            bound_position = Some(bound);
        }
        mark_track_manual_position(geometry, t_start_ms);
    } else if is_layout_container && parent_label.is_none() {
        preserve_discrete_position_state_before(geometry, t_start_ms);
        set_track_position_binding(
            geometry,
            t_start_ms,
            PositionBinding::ContainerDefault {
                anchor: SceneAnchor::Center,
            },
        );
    }
    bound_position
}

impl Timeline {
    pub(super) fn finalize_actor_container(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let primitive_registry = std::sync::Arc::clone(&self.primitive_registry);
        if let Some(p) = primitive_registry.find(ty) {
            let coordinate_context = self.resolve_coordinate_context(parent_label);
            let mut ctx = crate::primitives::BuildCtx {
                timeline: self,
                time_ms,
                parent_label,
                label,
                diagnostics,
                coordinate_context,
            };
            if let Err(mut diags) = p.finalize_container_build(&mut ctx, label, props) {
                diagnostics.append(&mut diags);
            }
        }
    }
}
