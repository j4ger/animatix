use super::*;

impl DocumentController<'_> {
    // ── Keyframe edits ──────────────────────────────────────────────────

    /// Handle a keyframe easing change request.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_keyframe_easing(
        &mut self,
        scene: Option<String>,
        actor: &str,
        property: &str,
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to set keyframe easing — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetKeyframeEasing {
            scene,
            actor: actor.into(),
            property: property.into(),
            time_s,
            easing,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Set easing on '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = format!(
                "Failed to set easing on '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            );
        }
    }

    /// Handle a keyframe deletion request.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_delete_keyframe(
        &mut self,
        scene: Option<String>,
        actor: &str,
        property: &str,
        time_s: f64,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store
                .preview
                .set_status_error("Failed to delete keyframe — no AST available");
            self.document_store.abort_snapshot();
            return;
        };

        let edit = source_edit::SourceEdit::DeleteKeyframe {
            scene,
            actor: actor.into(),
            property: property.into(),
            time_s,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Deleted keyframe '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.set_status_error(format!(
                "Failed to delete keyframe '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            ));
        }
    }

    /// Replace a keyframe's value in place.
    ///
    /// The edit targets the block at exactly `time_s`; it never inserts a new
    /// block and never applies the `keyframe_merge_window_s` heuristic, so a
    /// vertical drag in the Curves panel cannot silently rewrite a neighbour.
    ///
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_keyframe_value(
        &mut self,
        scene: Option<String>,
        actor: &str,
        property: &str,
        time_s: f64,
        value: crate::app::commands::PropertyValue,
    ) {
        let expr = match crate::app::commands::property_value_to_expr(value) {
            Ok(expr) => expr,
            Err(err) => {
                self.document_store.abort_snapshot();
                self.preview_store
                    .preview
                    .set_status_error(format!("Failed to set keyframe value: {err}"));
                return;
            },
        };

        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to set keyframe value — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetKeyframeValue {
            scene,
            actor: actor.into(),
            property: property.into(),
            value: expr,
            time_s,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = (
                animatix_syntax::to_source::stmts_to_source(stmts),
                animatix_syntax::source_index::SourceIndex::build(stmts),
            );
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Set '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.set_status_error(format!(
                "Failed to set '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            ));
        }
    }

    /// Move several keyframes, applying each `SourceEdit::MoveKeyframeTime`
    /// against the same AST so they share one undo snapshot.
    ///
    /// NOTE: The caller should have called `snapshot()` exactly once before this.
    pub(crate) fn handle_move_keyframes(
        &mut self,
        specs: &[crate::app::commands::MoveKeyframeSpec],
    ) {
        if specs.is_empty() {
            self.document_store.abort_snapshot();
            return;
        }
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to move keyframes — no AST available".to_string();
            return;
        };

        // One source keyframe block holds every assignment at a given actor
        // time, and `MoveKeyframeTime` moves the whole block. Dedupe by block
        // identity so coincident keyframes move once instead of logging a
        // not-found failure for each sibling.
        let mut moved_blocks: std::collections::HashSet<(Option<String>, String, u64)> =
            std::collections::HashSet::new();
        let mut moved = 0usize;
        for spec in specs {
            let block = (
                spec.scene.clone(),
                spec.actor.clone(),
                (spec.old_time_s * 1000.0).round() as u64,
            );
            if !moved_blocks.insert(block) {
                tracing::debug!(
                    actor = %spec.actor,
                    property = %spec.property,
                    old_time_s = spec.old_time_s,
                    "skipping coincident keyframe; its block already moved"
                );
                continue;
            }
            let edit = source_edit::SourceEdit::MoveKeyframeTime {
                scene: spec.scene.clone(),
                actor: spec.actor.clone(),
                property: spec.property.clone(),
                old_time_s: spec.old_time_s,
                new_time_s: spec.new_time_s,
            };
            match source_edit::apply_edit(stmts, edit) {
                Ok(()) => moved += 1,
                Err(err) => tracing::warn!(
                    actor = %spec.actor,
                    property = %spec.property,
                    old_time_s = spec.old_time_s,
                    "failed to move keyframe in batch: {err}"
                ),
            }
        }

        if moved == 0 {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "Failed to move keyframes — none found".to_string();
            return;
        }

        let new_source = animatix_syntax::to_source::stmts_to_source(stmts);
        let source_index = animatix_syntax::source_index::SourceIndex::build(stmts);
        self.apply_source(new_source, source_index);
        self.preview_store.preview.status = format!("Moved {moved} keyframe(s)");
    }

    /// Delete several keyframes against the same AST so they share one undo
    /// snapshot.
    ///
    /// NOTE: The caller should have called `snapshot()` exactly once before this.
    pub(crate) fn handle_delete_keyframes(&mut self, ids: &[KeyframeId]) {
        if ids.is_empty() {
            self.document_store.abort_snapshot();
            return;
        }
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store
                .preview
                .set_status_error("Failed to delete keyframes — no AST available");
            return;
        };

        let mut deleted = 0usize;
        for id in ids {
            let edit = source_edit::SourceEdit::DeleteKeyframe {
                scene: id.scene.clone(),
                actor: id.actor.clone(),
                property: id.property.clone(),
                time_s: id.time_ms as f64 / 1000.0,
            };
            match source_edit::apply_edit(stmts, edit) {
                Ok(()) => deleted += 1,
                Err(err) => tracing::warn!(
                    actor = %id.actor,
                    property = %id.property,
                    time_ms = id.time_ms,
                    "failed to delete keyframe in batch: {err}"
                ),
            }
        }

        if deleted == 0 {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to delete keyframes — none found".to_string();
            return;
        }

        let new_source = animatix_syntax::to_source::stmts_to_source(stmts);
        let source_index = animatix_syntax::source_index::SourceIndex::build(stmts);
        self.apply_source(new_source, source_index);
        self.preview_store.preview.status = format!("Deleted {deleted} keyframe(s)");
    }

    /// Resize an action block's duration.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_resize_action(
        &mut self,
        verb: &str,
        targets: &[String],
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to resize action — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::ResizeAction {
            verb: verb.into(),
            targets: targets.to_vec(),
            old_start_s,
            new_start_s,
            new_duration_s,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Resized action '{verb}' to {:.2}s", new_duration_s);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to resize action '{verb}' at {:.2}s", old_start_s);
        }
    }
}
