use super::*;
use super::panels;
use animatix::timeline::TrackAccessor;
use crate::validation::validate_roundtrip;

impl GuiShell {
    pub(crate) fn handle_keyframe_edit(&mut self, edit: panels::PropertyEdit) {
        let is_drag = !matches!(self.drag_state, DragState::None) || self.inspector_input_drag_active;
        if !is_drag || !self.drag_snapshot_taken {
            self.snapshot();
            if is_drag { self.drag_snapshot_taken = true; }
        }
        if edit.property == "child_order" { self.apply_child_order_edit(edit); return; }

        if let Some(ref mut timeline) = self.document.timeline {
            if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                apply_property_edit_to_track(track, &edit.property, &edit.value, time_ms);
                timeline.invalidate_frame_cache();
            }
        }

        let prev_time_s = self.document.prev_keyframe_time(self.preview.current_time_s);
        let delta_s = self.preview.current_time_s - prev_time_s;
        let source_result = if let Some(ref mut stmts) = self.document.raw_statements {
            let expr = animatix::ast::Expr::from(edit.value.clone());
            let validation_expr = expr.clone();
            let source_edit = if delta_s < self.keyframe_merge_window_s {
                crate::source_edit::SourceEdit::MergeKeyframe { actor: edit.actor.clone(), property: edit.property.clone(), value: expr, time_s: prev_time_s }
            } else {
                crate::source_edit::SourceEdit::InsertKeyframe { actor: edit.actor.clone(), property: edit.property.clone(), value: expr, time_s: self.preview.current_time_s, prev_time_s }
            };

            if crate::source_edit::apply_edit(stmts, source_edit) {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                if let Err(err) = validate_roundtrip(&validation_expr, &edit.value) {
                    tracing::error!("round-trip validation failed for {}.{}: {}", edit.actor, edit.property, err);
                    self.preview.status = format!("⚠ Edited {}.{} @ {:.2}s — round-trip validation failed: {}", edit.actor, edit.property, self.preview.current_time_s, err);
                }
                Some((new_source, animatix::source_index::SourceIndex::build(stmts)))
            } else { None }
        } else { None };

        let source_written = if let Some((new_source, source_index)) = source_result {
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(source_index);
            self.document.rescan_keyframe_lines();
            true
        } else { false };

        self.preview_dirty = true;
        if source_written {
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = if delta_s < self.keyframe_merge_window_s {
                format!("Merged {}.{} @ {:.2}s", edit.actor, edit.property, prev_time_s)
            } else {
                format!("Keyframe {}.{} @ {:.2}s", edit.actor, edit.property, self.preview.current_time_s)
            };
        } else {
            self.preview.status = format!("Keyframe {}.{} @ {:.2}s — visual only", edit.actor, edit.property, self.preview.current_time_s);
        }
    }

    pub(crate) fn handle_property_edit(&mut self, edit: panels::PropertyEdit) {
        if edit.create_keyframe { self.handle_keyframe_edit(edit); return; }
        let is_drag = !matches!(self.drag_state, DragState::None) || self.inspector_input_drag_active;
        if !is_drag || !self.drag_snapshot_taken { self.snapshot(); if is_drag { self.drag_snapshot_taken = true; } }
        if edit.property == "child_order" { self.apply_child_order_edit(edit); return; }

        if let Some(ref mut timeline) = self.document.timeline {
            if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                apply_property_edit_to_track(track, &edit.property, &edit.value, time_ms);
            }
            timeline.invalidate_frame_cache();
        }

        let source_result = if let Some(ref mut stmts) = self.document.raw_statements {
            let expr = animatix::ast::Expr::from(edit.value.clone());
            let validation_expr = expr.clone();
            let set_edit = crate::source_edit::SourceEdit::SetProperty { actor: edit.actor.clone(), property: edit.property.clone(), value: expr.clone() };
            let applied = if crate::source_edit::apply_edit(stmts, set_edit) { true } else {
                let insert_edit = crate::source_edit::SourceEdit::InsertProperty { actor: edit.actor.clone(), property: edit.property.clone(), value: expr };
                crate::source_edit::apply_edit(stmts, insert_edit)
            };

            if applied {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                if let Err(err) = validate_roundtrip(&validation_expr, &edit.value) {
                    tracing::error!("round-trip validation failed for {}.{}: {}", edit.actor, edit.property, err);
                    self.preview.status = format!("⚠ Edited {}.{} — round-trip validation failed: {}", edit.actor, edit.property, err);
                }
                Some((new_source, animatix::source_index::SourceIndex::build(stmts)))
            } else { None }
        } else { None };

        let source_written = if let Some((new_source, source_index)) = source_result {
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(source_index);
            true
        } else { false };

        self.preview_dirty = true;
        if source_written {
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = format!("Edited {}.{} — source updated", edit.actor, edit.property);
        } else {
            self.preview.status = format!("Edited {}.{} — visual only (no source span)", edit.actor, edit.property);
        }
    }

    fn apply_child_order_edit(&mut self, edit: panels::PropertyEdit) {
        use crate::app::panels::PropertyValue as PV;
        let source_result = if let (Some(ref mut timeline), PV::StringList(order)) = (self.document.timeline.as_mut(), edit.value.clone()) {
            if let Some(metadata) = timeline.container_metadata.get_mut(&edit.actor) {
                metadata.child_order = order.clone();
                // layout_children is computed on demand via Timeline::layout_children_for
                timeline.invalidate_frame_cache();
            }
            if let Some(ref mut stmts) = self.document.raw_statements {
                let applied = crate::source_edit::apply_edit(stmts, crate::source_edit::SourceEdit::ReorderContainerChildren { container: edit.actor.clone(), new_order: order });
                if applied { Some((animatix::to_source::stmts_to_source(stmts), animatix::source_index::SourceIndex::build(stmts))) } else { None }
            } else { None }
        } else { None };

        let source_written = if let Some((new_source, source_index)) = source_result {
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(source_index);
            true
        } else { false };

        self.preview_dirty = true;
        if source_written {
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = format!("Edited {}.child_order — source updated", edit.actor);
        } else {
            self.preview.status = format!("Edited {}.child_order — visual only (no source span)", edit.actor);
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Unified timeline update helper
// ─────────────────────────────────────────────────────────────

/// Apply a property edit to an in-memory `AnimationTrack`.
///
/// This helper centralises the per-property dispatch so both keyframe-mode
/// and overwrite-mode edits stay consistent.  It always adds a keyframe at
/// `time_ms` (in addition to updating `default_value`) so that the renderer's
/// `evaluate()` immediately sees the new value.
fn apply_property_edit_to_track(
    track: &mut animatix::timeline::AnimationTrack,
    property: &str,
    value: &panels::PropertyValue,
    time_ms: u64,
) {
    use animatix::timeline::PropertyTrack;
    use crate::app::panels::PropertyValue as PV;

    let linear = animatix::easing::Easing::Linear;

    match property {
        // ── Vec2 properties ──
        "position" => {
            if let PV::Vec2(v) = value {
                let pt = track.position.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "size" => {
            if let PV::Vec2(v) = value {
                // The size track stores half‑extents (w/2, h/2).
                let half = [v[0] / 2.0, v[1] / 2.0];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(half));
                pt.default_value = half;
                pt.add_keyframe(time_ms, half, linear);
            }
        }
        "line_from" => {
            if let PV::Vec2(v) = value {
                let pt = track.line_from.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "line_to" => {
            if let PV::Vec2(v) = value {
                let pt = track.line_to.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "arc_angles" => {
            if let PV::Vec2(v) = value {
                let pt = track.arc_angles.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "motion_offset" => {
            if let PV::Vec2(v) = value {
                let pt = track.motion_offset.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "offset" => {
            if let PV::Vec2(v) = value {
                if let Some(ref mut pb_track) = track.position_binding {
                    let current = pb_track.evaluate(time_ms);
                    if let animatix::timeline::PositionBinding::SceneAnchor { anchor, .. } = current {
                        let new_binding = animatix::timeline::PositionBinding::SceneAnchor {
                            anchor,
                            offset: *v,
                        };
                        pb_track.default_value = new_binding;
                        pb_track.add_keyframe(time_ms, new_binding, linear);
                    }
                }
            }
        }
        "at" => {
            if let PV::Vec2(v) = value {
                let binding = track
                    .position_binding
                    .as_ref()
                    .map(|pb| pb.evaluate(time_ms));

                match binding {
                    Some(animatix::timeline::PositionBinding::ScenePercent { .. }) => {
                        if let Some(ref mut pb_track) = track.position_binding {
                            let new_binding =
                                animatix::timeline::PositionBinding::ScenePercent {
                                    x: v[0],
                                    y: v[1],
                                    offset: [0.0, 0.0],
                                };
                            pb_track.default_value = new_binding;
                            pb_track.add_keyframe(time_ms, new_binding, linear);
                        }
                    }
                    _ => {
                        let pt = track.position.get_or_insert_with(|| PropertyTrack::new(*v));
                        pt.default_value = *v;
                        pt.add_keyframe(time_ms, *v, linear);
                    }
                }
            }
        }

        // ── Float properties ──
        "rotation" => {
            if let PV::Float(v) = value {
                let pt = track.rotation.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "scale" => {
            if let PV::Float(v) = value {
                let pt = track.scale.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "opacity" => {
            if let PV::Float(v) = value {
                let pt = track.opacity.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "stroke_width" | "width" => {
            if let PV::Float(v) = value {
                let pt = track.stroke_width.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "radius" => {
            if let PV::Float(v) = value {
                let size = [*v, *v];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "radius_x" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "radius_y" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "start_angle" => {
            if let PV::Float(v) = value {
                let current = track.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
                let angles = [*v, current[1]];
                let pt = track.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
                pt.default_value = angles;
                pt.add_keyframe(time_ms, angles, linear);
            }
        }
        "sweep_angle" => {
            if let PV::Float(v) = value {
                let current = track.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
                let angles = [current[0], *v];
                let pt = track.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
                pt.default_value = angles;
                pt.add_keyframe(time_ms, angles, linear);
            }
        }
        "tip_length" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "tip_width" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "stroke_progress" => {
            if let PV::Float(v) = value {
                let pt = track.stroke_progress.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "fill_opacity" => {
            if let PV::Float(v) = value {
                let pt = track.fill_opacity.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }

        // ── Color properties ──
        "color" => {
            if let PV::Color(v) = value {
                let pt = track.color.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "stroke_color" | "stroke" => {
            if let PV::Color(v) = value {
                let pt = track.stroke_color.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }

        // ── Text / enum properties ──
        "text_content" | "text" | "latex" | "math" | "code" => {
            if let PV::Text(v) = value {
                let pt = track.text_content.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                pt.default_value = v.clone();
                pt.add_keyframe(time_ms, v.clone(), linear);
            }
        }
        "font_family" => {
            if let PV::Text(v) = value {
                let pt = track.font_family.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                pt.default_value = v.clone();
                pt.add_keyframe(time_ms, v.clone(), linear);
            }
        }
        "font_size" => {
            if let PV::Float(v) = value {
                let pt = track.font_size.get_or_insert_with(|| PropertyTrack::new(*v));
                pt.default_value = *v;
                pt.add_keyframe(time_ms, *v, linear);
            }
        }
        "shape_type" => {
            if let PV::Text(v) = value {
                use animatix::timeline::ShapeType;
                let shape = v.parse::<ShapeType>().ok();
                if let Some(shape) = shape {
                    let pt = track.shape_type.get_or_insert_with(|| PropertyTrack::new(shape));
                    pt.default_value = shape;
                    pt.add_keyframe(time_ms, shape, linear);
                }
            }
        }
        "points" => {
            if let PV::PointList(v) = value {
                let pt = track.points.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                pt.default_value = v.clone();
                pt.add_keyframe(time_ms, v.clone(), linear);
            }
        }
        "placement_mode" => {
            if let PV::Text(v) = value {
                use animatix::timeline::PlacementMode;
                let mode = match v.as_str() {
                    "manual" => Some(PlacementMode::Manual),
                    "layout" => Some(PlacementMode::LayoutManaged),
                    _ => None,
                };
                if let Some(mode) = mode {
                    let pt = track.placement_mode.get_or_insert_with(|| PropertyTrack::new(mode));
                    pt.default_value = mode;
                    pt.add_keyframe(time_ms, mode, linear);
                }
            }
        }

        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────
// Actor Creation
// ─────────────────────────────────────────────────────────────

impl GuiShell {
    /// Create a new actor from the GUI.
    ///
    /// Generates default properties, inserts into source, auto-selects,
    /// and schedules a rebuild.
    pub(crate) fn handle_create_actor(&mut self, ty: &str, label: &str, position: [f32; 2]) {
        self.snapshot();

        let props = default_props_for_actor(ty, position, self.document.scene_dimensions);

        // If a container is selected, offer to insert inside it
        let container = self.selected_actors.iter().next().cloned().filter(|sel| {
            self.document
                .timeline
                .as_ref()
                .is_some_and(|t| {
                    t.get_track(sel)
                        .is_some_and(|tr| {
                            matches!(
                                tr.kind,
                                animatix::timeline::ActorKindId::Row
                                    | animatix::timeline::ActorKindId::Col
                                    | animatix::timeline::ActorKindId::Grid
                                    | animatix::timeline::ActorKindId::Stack
                                    | animatix::timeline::ActorKindId::Group
                            )
                        })
                })
        });

        if let Some(ref mut stmts) = self.document.raw_statements {
            let edit = crate::source_edit::SourceEdit::InsertActor {
                ty: ty.into(),
                label: label.into(),
                props,
                container: container.clone(),
                time_s: self.preview.current_time_s,
            };

            if crate::source_edit::apply_edit(stmts, edit) {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                self.document.source_text = new_source.clone();
                self.editor.replace_text(new_source);
                self.document.is_dirty = true;
                self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                self.pending_rebuild_at = Some(std::time::Instant::now() + REBUILD_DEBOUNCE);
                self.preview.status = format!("Created {} ({}) at ({:.0}, {:.0})", label, ty, position[0], position[1]);
            } else {
                self.preview.status = format!("Failed to create {} — source edit failed", label);
                return;
            }
        } else {
            self.preview.status = "Failed to create actor — no AST available".to_string();
            return;
        }

        // Auto-select the new actor
        self.selected_actors.clear();
        self.selected_actors.insert(label.into());
        self.preview_dirty = true;
    }

    /// Rename an actor and all references to it.
    pub(crate) fn handle_rename_actor(&mut self, old_label: &str, new_label: &str) {
        if old_label == new_label {
            return;
        }
        if new_label.is_empty() {
            self.preview.status = "Rename failed — label cannot be empty".to_string();
            return;
        }
        // Check uniqueness
        if let Some(ref timeline) = self.document.timeline {
            if timeline.has_actor(new_label) {
                self.preview.status = format!("Rename failed — '{}' already exists", new_label);
                return;
            }
        }

        self.snapshot();

        if let Some(ref mut stmts) = self.document.raw_statements {
            let edit = crate::source_edit::SourceEdit::RenameActor {
                old_label: old_label.into(),
                new_label: new_label.into(),
            };
            crate::source_edit::apply_edit(stmts, edit);
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = format!("Renamed {} → {}", old_label, new_label);
        } else {
            self.preview.status = "Rename failed — no AST available".to_string();
            return;
        }

        // Update selection to the new name
        if self.selected_actors.contains(old_label) {
            self.selected_actors.remove(old_label);
            self.selected_actors.insert(new_label.into());
        }
        self.preview_dirty = true;
    }
}

/// Generate default properties for a new actor.
fn default_props_for_actor(
    ty: &str,
    _position: [f32; 2],
    _scene_dimensions: animatix::timeline::SceneDimensions,
) -> Vec<animatix::ast::Property> {
    use animatix::ast::{Expr, Property};
    let scene = animatix::timeline::SceneDimensions {
        width: _scene_dimensions.width,
        height: _scene_dimensions.height,
    };

    if let Some(primitive) = animatix::primitives::find_primitive(ty) {
        primitive.default_props(&scene)
    } else {
        vec![Property {
            name: "at".into(),
            value: Expr::Tuple(vec![
                Expr::Num(scene.width as f64 / 2.0),
                Expr::Num(scene.height as f64 / 2.0),
            ]),
            value_span: None,
            trailing_comment: None,
        }]
    }
}
