use animatix::timeline::TrackAccessor;

use super::{panels, *};

impl GuiShell {
    pub(crate) fn handle_keyframe_edit(&mut self, edit: panels::PropertyEdit) {
        let is_drag = self.is_dragging();
        self.maybe_snapshot(&edit, is_drag);

        if edit.property == "child_order" {
            self.apply_child_order_edit(edit, is_drag);
            return;
        }

        // During drag: mutate timeline only, defer source write.
        if is_drag {
            self.apply_timeline_edit(&edit);
            self.defer_drag_edit(edit);
            self.preview_store.preview_dirty = true;
            return;
        }

        // Non-drag: validate source first, then apply atomically.
        if let Err(err) = self.apply_keyframe_source_edit(&edit) {
            tracing::error!("source edit failed for {}.{}: {}", edit.actor, edit.property, err);
            self.preview_store.preview.set_status_error(format!(
                "⚠ Edited {}.{} @ {:.2}s — {}",
                edit.actor,
                edit.property,
                self.preview_store.preview.playback.current_time_s(),
                err
            ));
        } else {
            let prev_time_s = self
                .document_store
                .source
                .document
                .prev_keyframe_time(self.preview_store.preview.playback.current_time_s());
            let delta_s = self.preview_store.preview.playback.current_time_s() - prev_time_s;
            self.preview_store.preview.status = if delta_s < self.ui_store.keyframe_merge_window_s {
                format!("Merged {}.{} @ {:.2}s", edit.actor, edit.property, prev_time_s)
            } else {
                format!(
                    "Keyframe {}.{} @ {:.2}s",
                    edit.actor,
                    edit.property,
                    self.preview_store.preview.playback.current_time_s()
                )
            };
        }
        self.preview_store.preview_dirty = true;
    }

    pub(crate) fn handle_property_edit(&mut self, edit: panels::PropertyEdit) {
        if edit.create_keyframe {
            self.handle_keyframe_edit(edit);
            return;
        }

        let is_drag = self.is_dragging();
        self.maybe_snapshot(&edit, is_drag);

        if edit.property == "child_order" {
            self.apply_child_order_edit(edit, is_drag);
            return;
        }

        // During drag: mutate timeline only, defer source write.
        if is_drag {
            self.apply_timeline_edit(&edit);
            self.defer_drag_edit(edit);
            self.preview_store.preview_dirty = true;
            return;
        }

        // Non-drag: validate source first, then apply atomically.
        if let Err(err) = self.apply_property_source_edit(&edit) {
            tracing::error!("source edit failed for {}.{}: {}", edit.actor, edit.property, err);
            self.preview_store
                .preview
                .set_status_error(format!("⚠ Edited {}.{} — {}", edit.actor, edit.property, err));
        } else {
            self.preview_store.preview.status =
                format!("Edited {}.{} — source updated", edit.actor, edit.property);
        }
        self.preview_store.preview_dirty = true;
    }

    /// Detach a targeted callout: bake `from`, `to`, `label_at` as manual properties
    /// and remove `target` from the actor declaration.
    pub(crate) fn handle_detach_callout(
        &mut self,
        actor: String,
        from: [f32; 2],
        to: [f32; 2],
        label_at: [f32; 2],
    ) {
        use animatix_syntax::ast::Expr;

        use crate::source_edit::SourceEdit;

        let result = if let Some(ref mut stmts) = self.document_store.source.document.raw_statements
        {
            let actor_c = actor.clone();
            try_apply_source_edit(stmts, |trial| {
                // Set from
                let from_expr =
                    Expr::Tuple(vec![Expr::Num(from[0] as f64), Expr::Num(from[1] as f64)]);
                let _ = crate::source_edit::apply_edit(
                    trial,
                    SourceEdit::SetProperty {
                        actor: actor_c.clone(),
                        property: "from".into(),
                        value: from_expr.clone(),
                    },
                )
                .or_else(|err| {
                    tracing::warn!("failed to set property 'from' on actor '{}': {}", actor_c, err);
                    crate::source_edit::apply_edit(
                        trial,
                        SourceEdit::InsertProperty {
                            actor: actor_c.clone(),
                            property: "from".into(),
                            value: from_expr,
                        },
                    )
                });
                // Set to
                let to_expr = Expr::Tuple(vec![Expr::Num(to[0] as f64), Expr::Num(to[1] as f64)]);
                let _ = crate::source_edit::apply_edit(
                    trial,
                    SourceEdit::SetProperty {
                        actor: actor_c.clone(),
                        property: "to".into(),
                        value: to_expr.clone(),
                    },
                )
                .or_else(|err| {
                    tracing::warn!("failed to set property 'to' on actor '{}': {}", actor_c, err);
                    crate::source_edit::apply_edit(
                        trial,
                        SourceEdit::InsertProperty {
                            actor: actor_c.clone(),
                            property: "to".into(),
                            value: to_expr,
                        },
                    )
                });
                // Set label_at
                let label_expr =
                    Expr::Tuple(vec![Expr::Num(label_at[0] as f64), Expr::Num(label_at[1] as f64)]);
                let _ = crate::source_edit::apply_edit(
                    trial,
                    SourceEdit::SetProperty {
                        actor: actor_c.clone(),
                        property: "label_at".into(),
                        value: label_expr.clone(),
                    },
                )
                .or_else(|err| {
                    tracing::warn!(
                        "failed to set property 'label_at' on actor '{}': {}",
                        actor_c,
                        err
                    );
                    crate::source_edit::apply_edit(
                        trial,
                        SourceEdit::InsertProperty {
                            actor: actor_c.clone(),
                            property: "label_at".into(),
                            value: label_expr,
                        },
                    )
                });
                // Remove target
                let result = crate::source_edit::apply_edit(
                    trial,
                    SourceEdit::RemoveProperty {
                        actor: actor_c.clone(),
                        property: "target".into(),
                    },
                );
                if let Err(err) = result {
                    tracing::warn!(
                        "failed to remove property 'target' on actor '{}': {}",
                        actor_c,
                        err
                    );
                }
                Ok(())
            })
        } else {
            Err(crate::source_edit::SourceEditError::Generic("No AST available".to_string()))
        };

        match result {
            Ok((new_source, source_index, flashes)) => {
                self.commit_source(new_source, source_index, flashes);
                self.preview_store.preview.status = format!("Detached callout '{}'", actor);
            },
            Err(err) => {
                tracing::error!("detach callout '{}' failed: {}", actor, err);
            },
        }
        self.preview_store.preview_dirty = true;
    }

    fn apply_child_order_edit(&mut self, edit: panels::PropertyEdit, is_drag: bool) {
        use crate::app::panels::PropertyValue as PV;

        // During drag: mutate timeline only, defer source write.
        if is_drag {
            if let Some(at) = self.document_store.source.document.active_timeline_mut() {
                let timeline = &mut *at.timeline;
                if let Some(metadata) = timeline.container_metadata_mut().get_mut(&edit.actor) {
                    if let PV::StringList(order) = &edit.value {
                        metadata.child_order = order.clone();
                        timeline.invalidate_frame_cache();
                    }
                }
            }
            self.defer_drag_edit(edit);
            self.preview_store.preview_dirty = true;
            return;
        }

        if let Err(err) = self.apply_child_order_source_edit(&edit) {
            self.preview_store
                .preview
                .set_status_error(format!("⚠ Edited {}.child_order — {}", edit.actor, err));
        } else {
            self.preview_store.preview.status =
                format!("Edited {}.child_order — source updated", edit.actor);
        }
        self.preview_store.preview_dirty = true;
    }

    /// Flush all pending drag edits to source.
    pub(crate) fn flush_pending_drag_edits(&mut self) {
        let pending: Vec<panels::PropertyEdit> =
            self.ui_store.interaction.pending_drag_source_edits.drain(..).collect();
        if pending.is_empty() {
            return;
        }

        for edit in pending {
            if edit.property == "child_order" {
                if let Err(err) = self.apply_child_order_source_edit(&edit) {
                    tracing::error!("flush child_order failed: {}", err);
                }
                continue;
            }

            let result = if edit.create_keyframe {
                self.apply_keyframe_source_edit(&edit)
            } else {
                self.apply_property_source_edit(&edit)
            };

            if let Err(err) = result {
                tracing::error!("flush edit failed for {}.{}: {}", edit.actor, edit.property, err);
            }
        }

        self.preview_store.preview_dirty = true;
    }

    // ── Private helpers ────────────────────────────────────────────

    fn is_dragging(&self) -> bool {
        self.ui_store.interaction.is_dragging()
    }

    fn maybe_snapshot(&mut self, edit: &panels::PropertyEdit, is_drag: bool) {
        if !is_drag || !self.ui_store.interaction.drag_snapshot_taken {
            self.snapshot(crate::app::commands::UndoLabel::PropertyEdit(edit.clone()));
            if is_drag {
                self.ui_store.interaction.drag_snapshot_taken = true;
            }
        }
    }

    fn defer_drag_edit(&mut self, edit: panels::PropertyEdit) {
        self.ui_store.interaction.pending_drag_source_edits.push(edit);
    }

    fn apply_timeline_edit(&mut self, edit: &panels::PropertyEdit) {
        if let Some(at) = self.document_store.source.document.active_timeline_mut() {
            let timeline = &mut *at.timeline;
            if let Some(track) = timeline.tracks_mut().get_mut(&edit.actor) {
                let time_ms =
                    (edit.time_s.unwrap_or(self.preview_store.preview.playback.current_time_s())
                        * 1000.0) as u64;
                apply_property_edit_to_track(track, &edit.property, &edit.value, time_ms);
            }
            timeline.invalidate_frame_cache();
        }
    }

    fn apply_keyframe_source_edit(
        &mut self,
        edit: &panels::PropertyEdit,
    ) -> Result<(), crate::source_edit::SourceEditError> {
        let expr = crate::app::commands::property_value_to_expr(edit.value.clone())
            .map_err(crate::source_edit::SourceEditError::Generic)?;
        let edit_time_s =
            edit.time_s.unwrap_or(self.preview_store.preview.playback.current_time_s());
        let prev_time_s = self.document_store.source.document.prev_keyframe_time(edit_time_s);
        let delta_s = edit_time_s - prev_time_s;

        let (new_source, source_index, flashes) =
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                let scene = self.document_store.source.document.active_scene.clone();
                let source_edit = if delta_s < self.ui_store.keyframe_merge_window_s {
                    crate::source_edit::SourceEdit::MergeKeyframe {
                        scene,
                        actor: edit.actor.clone(),
                        property: edit.property.clone(),
                        value: expr.clone(),
                        time_s: prev_time_s,
                    }
                } else {
                    crate::source_edit::SourceEdit::InsertKeyframe {
                        scene,
                        actor: edit.actor.clone(),
                        property: edit.property.clone(),
                        value: expr.clone(),
                        time_s: edit_time_s,
                        prev_time_s,
                    }
                };
                try_apply_source_edit(stmts, |trial| {
                    crate::source_edit::apply_edit(trial, source_edit)
                })?
            } else {
                return Err(crate::source_edit::SourceEditError::Generic(
                    "No AST available".to_string(),
                ));
            };

        self.apply_timeline_edit(edit);
        self.commit_source(new_source, source_index, flashes);
        Ok(())
    }

    fn apply_property_source_edit(
        &mut self,
        edit: &panels::PropertyEdit,
    ) -> Result<(), crate::source_edit::SourceEditError> {
        let expr = crate::app::commands::property_value_to_expr(edit.value.clone())
            .map_err(crate::source_edit::SourceEditError::Generic)?;

        let (new_source, source_index, flashes) =
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                let actor = edit.actor.clone();
                let property = edit.property.clone();
                try_apply_source_edit(stmts, |trial| {
                    let set_edit = crate::source_edit::SourceEdit::SetProperty {
                        actor: actor.clone(),
                        property: property.clone(),
                        value: expr.clone(),
                    };
                    if crate::source_edit::apply_edit(trial, set_edit).is_ok() {
                        Ok(())
                    } else {
                        let insert_edit = crate::source_edit::SourceEdit::InsertProperty {
                            actor: actor.clone(),
                            property: property.clone(),
                            value: expr.clone(),
                        };
                        crate::source_edit::apply_edit(trial, insert_edit)
                    }
                })?
            } else {
                return Err(crate::source_edit::SourceEditError::Generic(
                    "No AST available".to_string(),
                ));
            };

        self.apply_timeline_edit(edit);
        self.commit_source(new_source, source_index, flashes);
        Ok(())
    }

    fn apply_child_order_source_edit(
        &mut self,
        edit: &panels::PropertyEdit,
    ) -> Result<(), crate::source_edit::SourceEditError> {
        use crate::app::panels::PropertyValue as PV;

        let (new_source, source_index, flashes) =
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                if let PV::StringList(order) = edit.value.clone() {
                    let edit_op = crate::source_edit::SourceEdit::ReorderContainerChildren {
                        container: edit.actor.clone(),
                        new_order: order,
                    };
                    try_apply_source_edit(stmts, |trial| {
                        crate::source_edit::apply_edit(trial, edit_op)
                    })?
                } else {
                    return Err(crate::source_edit::SourceEditError::Generic(
                        "Invalid value type for child_order".to_string(),
                    ));
                }
            } else {
                return Err(crate::source_edit::SourceEditError::Generic(
                    "No AST available".to_string(),
                ));
            };

        if let Some(at) = self.document_store.source.document.active_timeline_mut() {
            let timeline = &mut *at.timeline;
            if let Some(metadata) = timeline.container_metadata_mut().get_mut(&edit.actor) {
                if let PV::StringList(order) = &edit.value {
                    metadata.child_order = order.clone();
                    timeline.invalidate_frame_cache();
                }
            }
        }
        self.commit_source(new_source, source_index, flashes);
        Ok(())
    }

    fn commit_source(
        &mut self,
        new_source: String,
        source_index: animatix_syntax::source_index::SourceIndex,
        flashes: Vec<f64>,
    ) {
        let ui_after = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.commit_source(new_source, source_index, ui_after);
        self.document_store.source.document.rescan_keyframe_lines();
        for time in flashes {
            self.preview_store.preview.flashed_keyframe_times.push((time, Instant::now()));
        }
        self.preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
    }
}

// ─────────────────────────────────────────────────────────────
// Atomic source-edit helper
// ─────────────────────────────────────────────────────────────

/// Apply a source edit atomically using a trial clone.
///
/// 1. Clones `stmts`.
/// 2. Calls `apply_fn` with the cloned statements.
/// 3. If `apply_fn` returns `Ok(())`, serializes the trial AST back to source, builds a new source
///    index, and commits the clone back to `stmts`.
/// 4. If `apply_fn` returns `Err`, leaves `stmts` untouched and propagates the error.
///
/// Callers should validate the edit (e.g. `PropertyValue → Expr` round-trip)
/// *before* invoking this helper.
fn try_apply_source_edit<F>(
    stmts: &mut Vec<animatix_syntax::ast::Stmt>,
    apply_fn: F,
) -> Result<
    (String, animatix_syntax::source_index::SourceIndex, Vec<f64>),
    crate::source_edit::SourceEditError,
>
where
    F: FnOnce(
        &mut Vec<animatix_syntax::ast::Stmt>,
    ) -> Result<(), crate::source_edit::SourceEditError>,
{
    crate::source_edit::clear_adjust_flash_queue();
    let mut trial = stmts.clone();
    apply_fn(&mut trial)?;

    let new_source = animatix_syntax::to_source::stmts_to_source(&trial);
    let source_index = animatix_syntax::source_index::SourceIndex::build(&trial);
    let flashes = crate::source_edit::drain_adjust_flash_queue();

    *stmts = trial;
    Ok((new_source, source_index, flashes))
}

// ─────────────────────────────────────────────────────────────
fn parse_gui_color(text: &str) -> Option<[f32; 4]> {
    if text == "auto" {
        return None;
    }
    if let Some(hex) = text.strip_prefix('#') {
        let value = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            6 => Some([
                ((value >> 16) & 0xff) as f32 / 255.0,
                ((value >> 8) & 0xff) as f32 / 255.0,
                (value & 0xff) as f32 / 255.0,
                1.0,
            ]),
            8 => Some([
                ((value >> 24) & 0xff) as f32 / 255.0,
                ((value >> 16) & 0xff) as f32 / 255.0,
                ((value >> 8) & 0xff) as f32 / 255.0,
                (value & 0xff) as f32 / 255.0,
            ]),
            _ => None,
        };
    }
    if let Some(rest) = text.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts = rest.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() == 4 {
            let r = parts[0].parse::<f32>().ok()?;
            let g = parts[1].parse::<f32>().ok()?;
            let b = parts[2].parse::<f32>().ok()?;
            let a = parts[3].parse::<f32>().ok()?;
            return Some([r, g, b, a]);
        }
    }
    if let Some(rest) = text.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts = rest.split(',').map(str::trim).collect::<Vec<_>>();
        if parts.len() == 3 {
            let r = parts[0].parse::<f32>().ok()?;
            let g = parts[1].parse::<f32>().ok()?;
            let b = parts[2].parse::<f32>().ok()?;
            return Some([r, g, b, 1.0]);
        }
    }
    Some(animatix::timeline::parse_color(&animatix_syntax::ast::Expr::Ident(
        text.to_string(),
    )))
}

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
    use animatix::timeline::{PropertyTrack, TrackFieldMut};

    use crate::app::panels::PropertyValue as PV;

    let linear = animatix_syntax::easing::Easing::Linear;

    fn write_tagged_value(
        track: &mut animatix::timeline::AnimationTrack,
        key: &str,
        pv: animatix::timeline::PropertyValue,
        time_ms: u64,
        linear: animatix_syntax::easing::Easing,
    ) {
        let tagged = track
            .tagged_tracks
            .entry(key.to_string())
            .or_default()
            .get_or_insert_with(|| PropertyTrack::new(pv.clone()));
        tagged.set_default_value(pv.clone());
        tagged.add_keyframe(time_ms, pv, linear);
    }

    if property == "place" {
        if let PV::String(choice) = value
            && let Some(place) = animatix::timeline::animation_track::CalloutPlace::from_str(choice)
        {
            track
                .geometry
                .callout_place
                .ensure(animatix::timeline::animation_track::CalloutPlace::Right)
                .add_keyframe(time_ms, place, linear);
            let tagged = track
                .tagged_tracks
                .entry("callout_place".to_string())
                .or_default()
                .get_or_insert_with(|| {
                    PropertyTrack::new(animatix::timeline::PropertyValue::Enum(choice.clone()))
                });
            tagged.set_default_value(animatix::timeline::PropertyValue::Enum(choice.clone()));
            tagged.add_keyframe(
                time_ms,
                animatix::timeline::PropertyValue::Enum(choice.clone()),
                linear,
            );
        }
        return;
    }

    if property == "legend" {
        let pv = match value {
            PV::Bool(true) => animatix::timeline::PropertyValue::Variant {
                name: "auto".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::Bool(true)),
            },
            PV::Bool(false) => animatix::timeline::PropertyValue::Variant {
                name: "hidden".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::Bool(false)),
            },
            PV::String(label) => animatix::timeline::PropertyValue::Variant {
                name: "label".to_string(),
                value: Box::new(animatix::timeline::PropertyValue::String(label.clone())),
            },
            _ => return,
        };
        track.legend.legend_declared = true;
        let tagged = track
            .tagged_tracks
            .entry("legend".to_string())
            .or_default()
            .get_or_insert_with(|| PropertyTrack::new(pv.clone()));
        tagged.set_default_value(pv.clone());
        tagged.add_keyframe(time_ms, pv, linear);
        return;
    }

    if track.kind == animatix::timeline::ActorKindId::Legend {
        match property {
            "title" => {
                if let PV::String(v) = value {
                    track.legend.title = v.clone();
                    write_tagged_value(
                        track,
                        "legend_title",
                        animatix::timeline::PropertyValue::String(v.clone()),
                        time_ms,
                        linear,
                    );
                }
                return;
            },
            "font_size" => {
                if let PV::F32(v) = value {
                    track.legend.font_size = *v;
                    write_tagged_value(
                        track,
                        "legend_font_size",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                    );
                }
                return;
            },
            "swatch_size" => {
                if let PV::F32(v) = value {
                    track.legend.swatch_size = *v;
                    write_tagged_value(
                        track,
                        "legend_swatch_size",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                    );
                }
                return;
            },
            "gap" => {
                if let PV::F32(v) = value {
                    track.legend.gap = *v;
                    write_tagged_value(
                        track,
                        "legend_gap",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                    );
                }
                return;
            },
            "text_max_width" => {
                if let PV::F32(v) = value {
                    track.legend.text_max_width = *v;
                    write_tagged_value(
                        track,
                        "legend_text_max_width",
                        animatix::timeline::PropertyValue::F32(*v),
                        time_ms,
                        linear,
                    );
                }
                return;
            },
            "label_color" => {
                if let PV::String(v) = value {
                    let parsed = parse_gui_color(v);
                    track.legend.label_color = parsed;
                    if let Some(color) = parsed {
                        write_tagged_value(
                            track,
                            "legend_label_color",
                            animatix::timeline::PropertyValue::Color(color),
                            time_ms,
                            linear,
                        );
                    }
                }
                return;
            },
            _ => {},
        }
    }

    // ── Special / compound properties (not covered by generic registry dispatch) ──
    match property {
        "size" => {
            if let PV::Vec2(v) = value {
                let half = [v[0] / 2.0, v[1] / 2.0];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(half));
                pt.set_default_value(half);
                pt.add_keyframe(time_ms, half, linear);
            }
            return;
        },
        "offset" => {
            if let PV::Vec2(v) = value {
                if let Some(ref mut pb_track) = track.geometry.position_binding {
                    let current = pb_track.evaluate(time_ms);
                    if let animatix::timeline::PositionBinding::SceneAnchor { anchor, .. } = current
                    {
                        let new_binding =
                            animatix::timeline::PositionBinding::SceneAnchor { anchor, offset: *v };
                        pb_track.set_default_value(new_binding);
                        pb_track.add_keyframe(time_ms, new_binding, linear);
                    }
                }
            }
            return;
        },
        "at" => {
            if let PV::Vec2(v) = value {
                let binding =
                    track.geometry.position_binding.as_ref().map(|pb| pb.evaluate(time_ms));
                match binding {
                    Some(animatix::timeline::PositionBinding::ScenePercent { .. }) => {
                        if let Some(ref mut pb_track) = track.geometry.position_binding {
                            let new_binding = animatix::timeline::PositionBinding::ScenePercent {
                                x: v[0],
                                y: v[1],
                                offset: [0.0, 0.0],
                            };
                            pb_track.set_default_value(new_binding);
                            pb_track.add_keyframe(time_ms, new_binding, linear);
                        }
                    },
                    _ => {
                        let pt =
                            track.geometry.position.get_or_insert_with(|| PropertyTrack::new(*v));
                        pt.set_default_value(*v);
                        pt.add_keyframe(time_ms, *v, linear);
                    },
                }
            }
            return;
        },
        "radius" => {
            if let PV::F32(v) = value {
                let size = [*v, *v];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            return;
        },
        "radius_x" => {
            if let PV::F32(v) = value {
                let current =
                    track.geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            return;
        },
        "radius_y" => {
            if let PV::F32(v) = value {
                let current =
                    track.geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            return;
        },
        "start_angle" => {
            if let PV::F32(v) = value {
                let current = track.shape.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
                let angles = [*v, current[1]];
                let pt = track.shape.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
                pt.set_default_value(angles);
                pt.add_keyframe(time_ms, angles, linear);
            }
            return;
        },
        "sweep_angle" => {
            if let PV::F32(v) = value {
                let current = track.shape.arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
                let angles = [current[0], *v];
                let pt = track.shape.arc_angles.get_or_insert_with(|| PropertyTrack::new(angles));
                pt.set_default_value(angles);
                pt.add_keyframe(time_ms, angles, linear);
            }
            return;
        },
        "tip_length" => {
            if let PV::F32(v) = value {
                let current =
                    track.geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [*v, current[1]];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            return;
        },
        "tip_width" => {
            if let PV::F32(v) = value {
                let current =
                    track.geometry.size.get(time_ms, animatix::timeline::DEFAULT_LAYOUT_HALF_SIZE);
                let size = [current[0], *v];
                let pt = track.geometry.size.get_or_insert_with(|| PropertyTrack::new(size));
                pt.set_default_value(size);
                pt.add_keyframe(time_ms, size, linear);
            }
            return;
        },
        _ => {},
    }

    // ── Registry-driven dispatch for standard properties ──
    if let Some(schema) = animatix::timeline::property_registry::lookup_property(property) {
        if let Some(field_mut) = track.field_mut(schema.field) {
            match (field_mut, value) {
                (TrackFieldMut::Vec2(f), PV::Vec2(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::F32(f), PV::F32(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::Vec4(f), PV::Color(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(*v));
                    pt.set_default_value(*v);
                    pt.add_keyframe(time_ms, *v, linear);
                },
                (TrackFieldMut::String(f), PV::String(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                    pt.set_default_value(v.clone());
                    pt.add_keyframe(time_ms, v.clone(), linear);
                },
                (TrackFieldMut::PointList(f), PV::PointList(v)) => {
                    let pt = f.get_or_insert_with(|| PropertyTrack::new(v.clone()));
                    pt.set_default_value(v.clone());
                    pt.add_keyframe(time_ms, v.clone(), linear);
                },
                (TrackFieldMut::ShapeType(f), PV::String(v)) => {
                    if let Ok(shape) = v.parse::<animatix::timeline::ShapeType>() {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(shape));
                        pt.set_default_value(shape);
                        pt.add_keyframe(time_ms, shape, linear);
                    }
                },
                (TrackFieldMut::PlacementMode(f), PV::String(v)) => {
                    let mode = match v.as_str() {
                        "manual" => Some(animatix::timeline::PlacementMode::Manual),
                        "layout" => Some(animatix::timeline::PlacementMode::LayoutManaged),
                        _ => None,
                    };
                    if let Some(mode) = mode {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(mode));
                        pt.set_default_value(mode);
                        pt.add_keyframe(time_ms, mode, linear);
                    }
                },
                (TrackFieldMut::CalloutPlace(f), PV::String(v)) => {
                    if let Some(place) =
                        animatix::timeline::animation_track::CalloutPlace::from_str(v.as_str())
                    {
                        let pt = f.get_or_insert_with(|| PropertyTrack::new(place));
                        pt.set_default_value(place);
                        pt.add_keyframe(time_ms, place, linear);
                    }
                },
                _ => {},
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Actor default properties (used by DocumentController in command_handlers.rs)
// ─────────────────────────────────────────────────────────────

/// Generate default properties for a new actor.
pub fn default_props_for_actor(
    ty: &str,
    _position: [f32; 2],
    _scene_dimensions: animatix::timeline::SceneDimensions,
) -> Vec<animatix_syntax::ast::Property> {
    use animatix_syntax::ast::{Expr, Property};
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
