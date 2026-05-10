use super::*;
use super::workspace;
use animatix::timeline::TrackAccessor;

impl GuiShell {
    pub(crate) fn handle_keyframe_edit(&mut self, edit: workspace::PropertyEdit) {
        let is_drag = !matches!(self.drag_state, DragState::None);
        if !is_drag || !self.drag_snapshot_taken {
            self.snapshot();
            if is_drag {
                self.drag_snapshot_taken = true;
            }
        }

        // Update in-memory timeline for live preview (all properties, not just position/rotation)
        if let Some(ref mut timeline) = self.document.timeline {
            if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                apply_property_edit_to_track(track, &edit.property, &edit.value, time_ms);
                timeline.invalidate_frame_cache();
            }
        }

        // Insert keyframe block into source via AST mutation.
        const MERGE_WINDOW_S: f64 = 0.05;
        let prev_time_s = self.document.prev_keyframe_time(self.preview.current_time_s);
        let delta_s = self.preview.current_time_s - prev_time_s;
        let source_written = if delta_s < MERGE_WINDOW_S {
            false
        } else if let Some(ref mut stmts) = self.document.raw_statements {
            let expr = animatix::ast::Expr::from(edit.value.clone());
            let source_edit = crate::source_edit::SourceEdit::InsertKeyframe {
                actor: edit.actor.clone(),
                property: edit.property.clone(),
                value: expr,
                time_s: self.preview.current_time_s,
                prev_time_s,
            };
            if crate::source_edit::apply_edit(stmts, source_edit) {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                self.document.source_text = new_source.clone();
                self.editor.replace_text(new_source);
                self.document.is_dirty = true;
                self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                self.document.rescan_keyframe_lines();
                true
            } else {
                false
            }
        } else {
            false
        };

        self.preview_dirty = true;
        if source_written {
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = format!(
                "Keyframe {}.{} @ {:.2}s",
                edit.actor, edit.property, self.preview.current_time_s
            );
        } else {
            self.preview.status = format!(
                "Keyframe {}.{} @ {:.2}s — visual only",
                edit.actor, edit.property, self.preview.current_time_s
            );
        }
    }

    /// Handle a property edit from the inspector panel.
    ///
    /// Updates the in-memory timeline and persists the change back to the .amx source file
    /// via AST mutation + full re-serialization (see [`crate::source_edit`]).
    pub(crate) fn handle_property_edit(&mut self, edit: workspace::PropertyEdit) {
        use workspace::PropertyValue;

        // ── Keyframe mode: insert a new timestamp instead of overwriting ──────
        if edit.create_keyframe {
            self.handle_keyframe_edit(edit);
            return;
        }

        // Take a snapshot for undo before making changes.
        // During a drag, only snapshot once (on the first edit) so that one
        // drag-start → drag-end counts as a single undo entry.
        let is_drag = !matches!(self.drag_state, DragState::None);
        if !is_drag || !self.drag_snapshot_taken {
            self.snapshot();
            if is_drag {
                self.drag_snapshot_taken = true;
            }
        }

        // Apply the edit to the in-memory timeline if it exists
        if let Some(ref mut timeline) = self.document.timeline {
            if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                apply_property_edit_to_track(track, &edit.property, &edit.value, time_ms);
            }

            // Invalidate the frame cache so the next evaluate() produces a fresh
            // scene reflecting the track mutations above.
            timeline.invalidate_frame_cache();
        }

        // Persist the change back to the .amx source file via AST mutation +
        // full re-serialization. This replaces the old byte-span surgery model.
        let source_written = if let Some(ref mut stmts) = self.document.raw_statements {
            let expr = animatix::ast::Expr::from(edit.value.clone());

            // Try SetProperty first (update existing property).
            let set_edit = crate::source_edit::SourceEdit::SetProperty {
                actor: edit.actor.clone(),
                property: edit.property.clone(),
                value: expr.clone(),
            };

            let applied = if crate::source_edit::apply_edit(stmts, set_edit) {
                true
            } else {
                // Property doesn't exist yet — insert it.
                let insert_edit = crate::source_edit::SourceEdit::InsertProperty {
                    actor: edit.actor.clone(),
                    property: edit.property.clone(),
                    value: expr,
                };
                crate::source_edit::apply_edit(stmts, insert_edit)
            };

            if applied {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                self.document.source_text = new_source.clone();
                self.editor.replace_text(new_source);
                self.document.is_dirty = true;
                self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                true
            } else {
                false
            }
        } else {
            false
        };

        // Mark preview as dirty to trigger a re-render
        self.preview_dirty = true;

        if source_written {
            // Schedule a debounced rebuild to re-parse the modified source
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = format!(
                "Edited {}.{} — source updated",
                edit.actor, edit.property
            );
        } else {
            self.preview.status = format!(
                "Edited {}.{} — visual only (no source span)",
                edit.actor, edit.property
            );
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
    value: &workspace::PropertyValue,
    time_ms: u64,
) {
    use animatix::timeline::PropertyTrack;
    use workspace::PropertyValue as PV;

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
                let current = track.size.get(time_ms, [50.0, 50.0]);
                let size = [*v, current[1]];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "radius_y" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, [50.0, 50.0]);
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
                let current = track.size.get(time_ms, [50.0, 50.0]);
                let size = [*v, current[1]];
                let pt = track.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.default_value = size;
                pt.add_keyframe(time_ms, size, linear);
            }
        }
        "tip_width" => {
            if let PV::Float(v) = value {
                let current = track.size.get(time_ms, [50.0, 50.0]);
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
                let shape = match v.as_str() {
                    "Rect" => Some(ShapeType::Rect),
                    "Circle" => Some(ShapeType::Circle),
                    "Line" => Some(ShapeType::Line),
                    "Ellipse" => Some(ShapeType::Ellipse),
                    "Arc" => Some(ShapeType::Arc),
                    "Polygon" => Some(ShapeType::Polygon),
                    "Path" => Some(ShapeType::Path),
                    "Arrow" => Some(ShapeType::Arrow),
                    "Graph" => Some(ShapeType::Graph),
                    "Plot" => Some(ShapeType::Plot),
                    _ => None,
                };
                if let Some(shape) = shape {
                    let pt = track.shape_type.get_or_insert_with(|| PropertyTrack::new(shape));
                    pt.default_value = shape;
                    pt.add_keyframe(time_ms, shape, linear);
                }
            }
        }

        _ => {}
    }
}
