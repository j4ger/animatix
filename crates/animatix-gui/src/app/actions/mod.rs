mod property_edit;
use property_edit::{apply_property_edit_to_entity, apply_shape_angle_edit};

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
        // During drag: mutate timeline only, defer source write.
        if is_drag {
            self.apply_child_order_component_edit(&edit);
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
            if let Some(id) = timeline.get_entity_id(&edit.actor) {
                let time_ms =
                    (edit.time_s.unwrap_or(self.preview_store.preview.playback.current_time_s())
                        * 1000.0) as u64;
                let mut legacy_tagged = Vec::new();
                let mut field_writes = Vec::new();
                // Shape angles live in the shape tier, a separate World
                // component: apply them before borrowing the track and its
                // sibling components.
                let angle_edited = apply_shape_angle_edit(
                    timeline.world_mut().shape_mut(id),
                    &edit.property,
                    &edit.value,
                    time_ms,
                );
                if !angle_edited {
                    apply_property_edit_to_entity(
                        timeline.world_mut(),
                        id,
                        &edit.property,
                        &edit.value,
                        time_ms,
                        &mut legacy_tagged,
                        &mut field_writes,
                    );
                }
                for (field, value) in field_writes {
                    timeline.world_mut().write_property_field(
                        id,
                        field,
                        value,
                        time_ms,
                        time_ms,
                        animatix_syntax::easing::Easing::Linear,
                        &mut Vec::new(),
                    );
                }
                for (key, value) in legacy_tagged {
                    let tagged = timeline.world_mut().tagged_tracks_mut(id);
                    let track = tagged.entry(key).or_default().get_or_insert_with(|| {
                        animatix::timeline::PropertyTrack::new(value.clone())
                    });
                    track.set_default_value(value.clone());
                    track.add_keyframe(time_ms, value, animatix_syntax::easing::Easing::Linear);
                }
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
                    // `scope.stage.param` addresses an effect stage parameter.
                    if let Some((stage, param)) = property.split_once('.') {
                        let set_edit = crate::source_edit::SourceEdit::SetEffectParam {
                            scope: actor.clone(),
                            stage: stage.to_string(),
                            param: param.to_string(),
                            value: expr.clone(),
                        };
                        if crate::source_edit::apply_edit(trial, set_edit).is_ok() {
                            return Ok(());
                        }
                        let insert_edit = crate::source_edit::SourceEdit::InsertEffectParam {
                            scope: actor.clone(),
                            stage: stage.to_string(),
                            param: param.to_string(),
                            value: expr.clone(),
                        };
                        return crate::source_edit::apply_edit(trial, insert_edit);
                    }
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

        self.apply_child_order_component_edit(edit);
        self.commit_source(new_source, source_index, flashes);
        Ok(())
    }

    /// Write a `child_order` edit to the component as entity ids.
    ///
    /// The edit carries labels; the component stores `EntityId`s.
    fn apply_child_order_component_edit(&mut self, edit: &panels::PropertyEdit) {
        use crate::app::panels::PropertyValue as PV;
        let (Some(at), PV::StringList(order)) =
            (self.document_store.source.document.active_timeline_mut(), &edit.value)
        else {
            return;
        };
        let timeline = &mut *at.timeline;
        let ids: Vec<_> = order.iter().filter_map(|label| timeline.get_entity_id(label)).collect();
        // `container_layout_mut` invalidates the frame cache itself.
        if let Some(metadata) = timeline.container_layout_mut(&edit.actor) {
            metadata.child_order = ids;
        }
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

// Track capability helpers (registry-driven)
// ─────────────────────────────────────────────────────────────

/// Whether the GUI may offer to nest a new actor inside `track`.
///
/// Resolved from the live registry via the entity's required `actor_type`, so
/// extension containers with generic child processing participate.
pub(crate) fn track_is_nestable_container(
    timeline: &animatix::timeline::Timeline,
    id: animatix::timeline::EntityId,
) -> bool {
    timeline
        .primitive_registry_snapshot()
        .info_of(timeline.world().actor_type(id))
        .map(animatix::timeline::caps_from_info)
        .is_some_and(|caps| caps.is_nestable_container())
}

/// Whether `track` is a plain structural group (the ungroup action target).
pub(crate) fn track_is_group_like(
    timeline: &animatix::timeline::Timeline,
    id: animatix::timeline::EntityId,
) -> bool {
    timeline.world().caps(id).is_group_like()
}

#[cfg(test)]
mod tests {
    use animatix::timeline::{ActorField, PropertyValue};

    use crate::app::panels::PropertyValue as PV;

    use super::apply_property_edit_to_entity;

    /// A GUI edit of a registry-backed tagged property must land in the store
    /// the engine reads (`property_plan`), not only in `tagged_tracks`.
    #[test]
    fn gui_tagged_edit_is_visible_through_property_plan() {
        let mut timeline = animatix::timeline::Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        timeline.world_mut().rebuild_property_plan(id);
        let mut legacy_tagged = Vec::new();
        let mut field_writes = Vec::new();
        apply_property_edit_to_entity(
            timeline.world_mut(),
            id,
            "solo",
            &PV::Bool(true),
            1000,
            &mut legacy_tagged,
            &mut field_writes,
        );
        for (field, value) in field_writes {
            timeline.world_mut().write_property_field(
                id,
                field,
                value,
                1000,
                1000,
                animatix_syntax::easing::Easing::Linear,
                &mut Vec::new(),
            );
        }

        assert_eq!(
            timeline.world().read_property_value(id, ActorField::Tagged("solo"), 1000),
            Some(PropertyValue::Bool(true)),
            "solo edit should be readable at the edited time"
        );
        // An instant write is fenced, so it holds for every later time too.
        assert_eq!(
            timeline.world().read_property_value(id, ActorField::Tagged("solo"), 5000),
            Some(PropertyValue::Bool(true)),
            "solo edit should still be visible after the edited time"
        );
    }

    /// A GUI edit of a shape-tier property must reach the shape component
    /// store: the track itself no longer carries a `shape` field.
    #[test]
    fn gui_shape_edit_reaches_the_shape_tier() {
        let mut timeline = animatix::timeline::Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let mut legacy_tagged = Vec::new();
        let mut field_writes = Vec::new();
        apply_property_edit_to_entity(
            timeline.world_mut(),
            id,
            "corner_radius",
            &PV::F32(12.0),
            500,
            &mut legacy_tagged,
            &mut field_writes,
        );
        assert!(
            field_writes.iter().any(|(field, _)| *field == ActorField::CornerRadius),
            "shape-owned property edits must route through the world field bundle"
        );
        for (field, value) in field_writes {
            timeline.world_mut().write_property_field(
                id,
                field,
                value,
                500,
                500,
                animatix_syntax::easing::Easing::Linear,
                &mut Vec::new(),
            );
        }

        assert_eq!(
            timeline.world().read_property_value(id, ActorField::CornerRadius, 500),
            Some(PropertyValue::F32(12.0)),
            "corner_radius edit should be readable from the shape tier"
        );
    }
}
