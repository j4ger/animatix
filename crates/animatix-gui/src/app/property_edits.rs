use super::*;
use super::workspace;

impl GuiShell {
    pub(crate) fn handle_keyframe_edit(&mut self, edit: workspace::PropertyEdit) {
        let is_drag = !matches!(self.drag_state, DragState::None);
        if !is_drag || !self.drag_snapshot_taken {
            self.snapshot();
            if is_drag {
                self.drag_snapshot_taken = true;
            }
        }

        // Update in-memory timeline for live preview
        if let Some(ref mut timeline) = self.document.timeline {
            if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                match &edit.value {
                    workspace::PropertyValue::Vec2(v) => {
                        if edit.property == "position" {
                            let pt = track.position.get_or_insert_with(|| {
                                animatix::timeline::PropertyTrack::new(*v)
                            });
                            pt.add_keyframe(time_ms, *v, animatix::easing::Easing::Linear);
                        }
                    }
                    workspace::PropertyValue::Float(v) => {
                        if edit.property == "rotation" {
                            let pt = track.rotation.get_or_insert_with(|| {
                                animatix::timeline::PropertyTrack::new(*v)
                            });
                            pt.add_keyframe(time_ms, *v, animatix::easing::Easing::Linear);
                        }
                    }
                    _ => {}
                }
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
            let source_edit = crate::source_edit_v2::SourceEdit::InsertKeyframe {
                actor: edit.actor.clone(),
                property: edit.property.clone(),
                value: expr,
                time_s: self.preview.current_time_s,
                prev_time_s,
            };
            if crate::source_edit_v2::apply_edit(stmts, source_edit) {
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
    /// via AST mutation + full re-serialization (see [`crate::source_edit_v2`]).
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
                match &edit.value {
                    PropertyValue::Vec2(v) => {
                        match edit.property.as_str() {
                            "position" => {
                                let pt = track.position.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                                // Add a keyframe for live preview — the renderer reads
                                // keyframes via evaluate(), not default_value.
                                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                                pt.add_keyframe(time_ms, *v, animatix::easing::Easing::Linear);
                            }
                            "size" => {
                                // The size track stores half‑extents (w/2, h/2).
                                // Drag sends full size; source writer writes full size;
                                // parser halves on load.  Store half‑extents here too.
                                let half = [v[0] / 2.0, v[1] / 2.0];
                                let pt = track.size.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(half)
                                });
                                pt.default_value = half;
                                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                                pt.add_keyframe(time_ms, half, animatix::easing::Easing::Linear);
                            }
                            "line_from" => {
                                let pt = track.line_from.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "line_to" => {
                                let pt = track.line_to.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "arc_angles" => {
                                let pt = track.arc_angles.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "motion_offset" => {
                                let pt = track.motion_offset.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "offset" => {
                                // "offset" is embedded inside PositionBinding::SceneAnchor.
                                // Update the binding's offset field so the renderer
                                // picks up the new position immediately.
                                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                                if let Some(ref mut pb_track) = track.position_binding {
                                    let current = pb_track.evaluate(time_ms);
                                    if let animatix::timeline::PositionBinding::SceneAnchor {
                                        anchor, ..
                                    } = current
                                    {
                                        let new_binding =
                                            animatix::timeline::PositionBinding::SceneAnchor {
                                                anchor,
                                                offset: *v,
                                            };
                                        pb_track.default_value = new_binding;
                                        pb_track.add_keyframe(
                                            time_ms,
                                            new_binding,
                                            animatix::easing::Easing::Linear,
                                        );
                                    }
                                }
                            }
                            "at" => {
                                // "at" with Vec2 can mean either absolute position
                                // or percent-based positioning.  Check the binding.
                                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                                let binding = track
                                    .position_binding
                                    .as_ref()
                                    .map(|pb| pb.evaluate(time_ms));

                                match binding {
                                    Some(animatix::timeline::PositionBinding::ScenePercent {
                                        ..
                                    }) => {
                                        // Percent-based: v is already [x_frac, y_frac]
                                        if let Some(ref mut pb_track) = track.position_binding {
                                            let new_binding =
                                                animatix::timeline::PositionBinding::ScenePercent {
                                                    x: v[0],
                                                    y: v[1],
                                                    offset: [0.0, 0.0],
                                                };
                                            pb_track.default_value = new_binding;
                                            pb_track.add_keyframe(
                                                time_ms,
                                                new_binding,
                                                animatix::easing::Easing::Linear,
                                            );
                                        }
                                    }
                                    _ => {
                                        // Absolute: treat as regular position
                                        let pt = track.position.get_or_insert_with(|| {
                                            animatix::timeline::PropertyTrack::new(*v)
                                        });
                                        pt.default_value = *v;
                                        pt.add_keyframe(
                                            time_ms,
                                            *v,
                                            animatix::easing::Easing::Linear,
                                        );
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    PropertyValue::Float(v) => {
                        match edit.property.as_str() {
                            "rotation" => {
                                let pt = track.rotation.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                                pt.add_keyframe(time_ms, *v, animatix::easing::Easing::Linear);
                            }
                            "scale" => {
                                let pt = track.scale.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "opacity" => {
                                let pt = track.opacity.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "stroke_width" => {
                                let pt = track.stroke_width.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "stroke_progress" => {
                                let pt = track.stroke_progress.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "fill_opacity" => {
                                let pt = track.fill_opacity.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            _ => {}
                        }
                    }
                    PropertyValue::Color(v) => {
                        match edit.property.as_str() {
                            "color" => {
                                let pt = track.color.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            "stroke_color" => {
                                let pt = track.stroke_color.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(*v)
                                });
                                pt.default_value = *v;
                            }
                            _ => {}
                        }
                    }
                    PropertyValue::Text(v) => {
                        match edit.property.as_str() {
                            "text_content" => {
                                let pt = track.text_content.get_or_insert_with(|| {
                                    animatix::timeline::PropertyTrack::new(v.clone())
                                });
                                pt.default_value = v.clone();
                            }
                            "shape_type" => {
                                // Parse shape type from text
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
                                    let pt = track.shape_type.get_or_insert_with(|| {
                                        animatix::timeline::PropertyTrack::new(shape)
                                    });
                                    pt.default_value = shape;
                                }
                            }
                            _ => {}
                        }
                    }
                }
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
            let set_edit = crate::source_edit_v2::SourceEdit::SetProperty {
                actor: edit.actor.clone(),
                property: edit.property.clone(),
                value: expr.clone(),
            };

            let applied = if crate::source_edit_v2::apply_edit(stmts, set_edit) {
                true
            } else {
                // Property doesn't exist yet — insert it.
                let insert_edit = crate::source_edit_v2::SourceEdit::InsertProperty {
                    actor: edit.actor.clone(),
                    property: edit.property.clone(),
                    value: expr,
                };
                crate::source_edit_v2::apply_edit(stmts, insert_edit)
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
