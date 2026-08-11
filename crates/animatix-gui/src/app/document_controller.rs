//! Document-level AST mutation operations extracted from `GuiShell`.
//!
//! `DocumentController` borrows the three core stores and provides focused
//! methods for manipulating the AST, source text, editor buffer, and UI state.
//! It does **not** manage undo snapshots — the caller (`GuiShell`) is
//! responsible for calling `snapshot()` before delegating to a controller
//! method.

use std::time::Duration;

use crate::app::preview::DragState;
use crate::app::stores::*;
use crate::source_edit;

// ---------------------------------------------------------------------------
// DocumentController
// ---------------------------------------------------------------------------

pub(crate) struct DocumentController<'a> {
    pub document_store: &'a mut DocumentStore,
    pub preview_store: &'a mut PreviewStore,
    pub ui_store: &'a mut UiStore,
}

impl DocumentController<'_> {
    /// Apply the mutated AST back to source_text + editor buffer, and schedule
    /// a pending rebuild.
    ///
    /// This is called from a scope block that already owns the final
    /// `stmts` slice, so we receive the pre-computed source/index to avoid
    /// a second mutable borrow of `document_store`.
    fn apply_source(
        &mut self,
        new_source: String,
        source_index: animatix_syntax::source_index::SourceIndex,
    ) {
        let ui_after = self.ui_store.snapshot_with_preview(self.preview_store);
        self.document_store.commit_source(new_source, source_index, ui_after);
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );
    }

    // ── Actor management ────────────────────────────────────────────────

    /// Create a new actor from a type/label/position.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_create_actor(
        &mut self,
        ty: &str,
        label: &str,
        position: [f32; 2],
        extra_props: Vec<animatix_syntax::ast::Property>,
    ) {
        let mut props = crate::app::actions::default_props_for_actor(
            ty,
            position,
            self.document_store.source.document.scene_dimensions,
        );
        props.extend(extra_props);

        // If a container is selected, offer to insert inside it
        let container =
            self.ui_store.selection.selected_actors.iter().next().cloned().filter(|sel| {
                self.document_store.source.document.timeline.as_ref().is_some_and(|t| {
                    t.get_track(sel).is_some_and(|tr| {
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

        if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
            let edit = crate::source_edit::SourceEdit::InsertActor {
                ty: ty.into(),
                label: label.into(),
                props,
                container: container.clone(),
                time_s: self.preview_store.preview.playback.current_time_s(),
            };

            if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                let new_source = animatix_syntax::to_source::stmts_to_source(stmts);
                let source_index = animatix_syntax::source_index::SourceIndex::build(stmts);
                self.apply_source(new_source, source_index);
                self.preview_store.preview.status = format!(
                    "Created {} ({}) at ({:.0}, {:.0})",
                    label, ty, position[0], position[1]
                );
            } else {
                self.document_store.abort_snapshot();
                self.preview_store.preview.status =
                    format!("Failed to create {} — source edit failed", label);
                return;
            }
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to create actor — no AST available".to_string();
            return;
        }

        // Auto-select the new actor
        self.ui_store.selection.selected_actors.clear();
        self.ui_store.selection.selected_actors.insert(label.into());
        self.preview_store.preview_dirty = true;
    }

    // ── Actor management ────────────────────────────────────────────────

    /// Duplicate an actor, preserving its type and properties.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_duplicate_actor(&mut self, original_label: &str) {
        let new_label = self.unique_label(original_label);

        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to duplicate — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::DuplicateActor {
            original_label: original_label.to_string(),
            new_label: new_label.clone(),
        };
        if source_edit::apply_edit(stmts, edit).is_err() {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to duplicate — actor '{}' not found", original_label);
            return;
        }

        // Commit source — scope block drops stmts borrow before accessing self
        let (new_source, source_index) = {
            (
                animatix_syntax::to_source::stmts_to_source(stmts),
                animatix_syntax::source_index::SourceIndex::build(stmts),
            )
        };
        self.apply_source(new_source, source_index);

        // Select new actor and start move drag
        self.ui_store.selection.selected_actors.clear();
        self.ui_store.selection.selected_actors.insert(new_label.clone());
        self.preview_store.preview_dirty = true;
        self.preview_store.preview.status =
            format!("Duplicated '{}' → '{}'", original_label, new_label);

        // Start move drag for the new actor at the original position
        let time_ms = (self.preview_store.preview.playback.current_time_s() * 1000.0) as u64;
        if let Some(timeline) = self.document_store.source.document.active_timeline() {
            if let Some(track) = timeline.get_track(original_label) {
                let position = track
                    .geometry
                    .position
                    .as_ref()
                    .map(|p| p.evaluate(time_ms))
                    .unwrap_or([0.0, 0.0]);
                self.ui_store.interaction.drag_state = DragState::Move {
                    primary: new_label.clone(),
                    actors: vec![(new_label, position)],
                    start_scene: kurbo::Point::new(position[0] as f64, position[1] as f64),
                };
            }
        }
    }

    /// Delete all selected actors from the source AST.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_delete_selected_actors(&mut self) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "Failed to delete — no AST available".to_string();
            return;
        };

        let to_delete: Vec<String> =
            self.ui_store.selection.selected_actors.iter().cloned().collect();
        if to_delete.is_empty() {
            self.document_store.abort_snapshot();
            return;
        }

        let mut deleted = 0;
        for label in &to_delete {
            let edit = crate::source_edit::SourceEdit::DeleteActor {
                label: label.clone(),
            };
            if crate::source_edit::apply_edit(stmts, edit).is_ok() {
                deleted += 1;
            }
        }

        if deleted == 0 {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "No actors deleted".to_string();
            return;
        }

        let (new_source, source_index) = (
            animatix_syntax::to_source::stmts_to_source(stmts),
            animatix_syntax::source_index::SourceIndex::build(stmts),
        );
        let ui_after = self.ui_store.snapshot_with_preview(self.preview_store);
        self.document_store.commit_source(new_source, source_index, ui_after);
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );

        // Clear selection
        self.ui_store.selection.selected_actors.clear();
        self.preview_store.preview_dirty = true;
        self.preview_store.preview.status = format!("Deleted {} actor(s)", deleted);
    }

    // ── Scene/transition edits ──────────────────────────────────────────

    /// Update the transition on a scene's play statement.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_transition(
        &mut self,
        from_scene: &str,
        transition: animatix_syntax::ast::Transition,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store.preview.status =
                "Failed to set transition — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetTransition {
            from_scene: from_scene.into(),
            transition: Some(transition.clone()),
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
                format!("Set transition on '{}' → {}ms", from_scene, transition.duration_ms);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to set transition on '{}'", from_scene);
        }
    }

    /// Update the play target for a scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_play_target(&mut self, from_scene: &str, target: Option<String>) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to set play target — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetPlayTarget {
            scene: from_scene.into(),
            target: target.clone(),
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            if let Some(ref t) = target {
                self.preview_store.preview.status =
                    format!("Set play target: '{}' → '{}'", from_scene, t);
            } else {
                self.preview_store.preview.status =
                    format!("Removed play target from '{}'", from_scene);
            }
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to set play target on '{}'", from_scene);
        }
    }

    /// Set explicit duration for a scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_scene_duration(&mut self, scene: &str, duration_s: Option<f64>) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to set scene duration — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetSceneDuration {
            scene: scene.into(),
            duration_s,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            match duration_s {
                Some(d) => {
                    self.preview_store.preview.status =
                        format!("Set scene '{}' duration to {:.2}s", scene, d);
                },
                None => {
                    self.preview_store.preview.status =
                        format!("Removed explicit duration from scene '{}'", scene);
                },
            }
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to set duration for scene '{}'", scene);
        }
    }

    // ── Keyframe edits ──────────────────────────────────────────────────

    /// Handle a keyframe easing change request.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_keyframe_easing(
        &mut self,
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
    pub(crate) fn handle_delete_keyframe(&mut self, actor: &str, property: &str, time_s: f64) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store
                .preview
                .set_status_error("Failed to delete keyframe — no AST available");
            self.document_store.abort_snapshot();
            return;
        };

        let edit = source_edit::SourceEdit::DeleteKeyframe {
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

    /// Move a keyframe from one time to another.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_move_keyframe(
        &mut self,
        actor: &str,
        property: &str,
        old_time_s: f64,
        new_time_s: f64,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to move keyframe — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::MoveKeyframeTime {
            actor: actor.into(),
            property: property.into(),
            old_time_s,
            new_time_s,
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let new_source = animatix_syntax::to_source::stmts_to_source(stmts);
            let source_index = animatix_syntax::source_index::SourceIndex::build(stmts);
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status = format!(
                "Moved keyframe '{}.{}' from {:.2}s to {:.2}s",
                actor, property, old_time_s, new_time_s
            );
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = format!(
                "Failed to move keyframe '{}.{}' from {:.2}s — not found",
                actor, property, old_time_s
            );
        }
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

    // ── Actor hierarchy / scene refactoring ─────────────────────────────

    /// Reparent an actor under a new parent (or to top-level).
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_reparent_actor(&mut self, actor: &str, new_parent: Option<String>) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "Failed to reparent — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::Reparent {
            actor: actor.into(),
            new_parent: new_parent.clone(),
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            if let Some(ref parent) = new_parent {
                self.preview_store.preview.status =
                    format!("Reparented '{}' under '{}'", actor, parent);
            } else {
                self.preview_store.preview.status = format!("Reparented '{}' to top level", actor);
            }
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = format!("Failed to reparent '{}'", actor);
        }
    }

    /// Extract selected actors into a new scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_extract_scene(
        &mut self,
        actor_labels: Vec<String>,
        new_scene_name: String,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to extract scene — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::ExtractScene {
            actor_labels: actor_labels.clone(),
            new_scene_name: new_scene_name.clone(),
        };

        if source_edit::apply_edit(stmts, edit).is_ok() {
            let (new_source, source_index) = {
                (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status = format!(
                "Extracted {} actor(s) into scene '{}'",
                actor_labels.len(),
                new_scene_name
            );
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "Failed to extract scene".to_string();
        }
    }

    /// Move selected actors to an existing scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_move_to_scene(&mut self, actor_labels: Vec<String>, target_scene: String) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to move actors — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::MoveToScene {
            actor_labels: actor_labels.clone(),
            target_scene: target_scene.clone(),
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
                format!("Moved {} actor(s) to scene '{}'", actor_labels.len(), target_scene);
        } else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                format!("Failed to move actors to scene '{}'", target_scene);
        }
    }

    // ── Paste ───────────────────────────────────────────────────────────

    /// Paste actors from the clipboard into the current scene.
    ///
    /// For each clipboard actor:
    ///   - Clones the declaration with a unique label (`_copy` suffix + dedup)
    ///   - Clones all keyframe assignment statements referencing the original actor
    ///   - Renames references to the new label
    ///   - Shifts absolute keyframe times by `current_time_s`
    ///   - Inserts everything into the AST at the end
    ///
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn paste_actors(&mut self) {
        let current_time_s = self.preview_store.preview.playback.current_time_s();
        let clipboard = self.ui_store.clipboard.clipboard_actors.clone();

        // Pre-generate all unique labels before mutating the AST.
        let label_map: Vec<(String, String)> = clipboard
            .iter()
            .map(|orig| (orig.clone(), self.paste_unique_label(orig)))
            .collect();

        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status = "Failed to paste — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::PasteActors {
            clipboard: label_map.clone(),
            time_s: current_time_s,
        };
        if source_edit::apply_edit(stmts, edit).is_err() {
            self.document_store.abort_snapshot();
            self.preview_store.preview.status =
                "Failed to paste — actor(s) not found in AST".to_string();
            return;
        }

        let pasted_labels: Vec<String> =
            label_map.iter().map(|(_, new_label)| new_label.clone()).collect();

        // Commit source — scope block drops stmts borrow
        let (new_source, source_index) = {
            (
                animatix_syntax::to_source::stmts_to_source(stmts),
                animatix_syntax::source_index::SourceIndex::build(stmts),
            )
        };
        self.apply_source(new_source, source_index);

        // Select the pasted actors
        self.ui_store.selection.selected_actors.clear();
        for label in &pasted_labels {
            self.ui_store.selection.selected_actors.insert(label.clone());
        }
        self.preview_store.preview_dirty = true;
        self.preview_store.preview.status = format!("Pasted {} actor(s)", pasted_labels.len());
    }

    // ── Label utilities ─────────────────────────────────────────────────

    /// Generate a unique label for pasted actors using `_copy` suffix.
    fn paste_unique_label(&self, base: &str) -> String {
        let candidate = format!("{}_copy", base);
        if !self.has_actor_label(&candidate) {
            return candidate;
        }
        for i in 1..=9999 {
            let candidate = format!("{}_{}", base, i);
            if !self.has_actor_label(&candidate) {
                return candidate;
            }
        }
        // Fallback: append timestamp to guarantee uniqueness
        format!(
            "{}_{}",
            base,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                % 100_000
        )
    }

    /// Check if an actor label already exists in the timeline (or in clipboard).
    fn has_actor_label(&self, label: &str) -> bool {
        if self
            .document_store
            .source
            .document
            .timeline
            .as_ref()
            .is_some_and(|t| t.has_actor(label))
        {
            return true;
        }
        if self.ui_store.clipboard.clipboard_actors.contains(&label.to_string()) {
            return true;
        }
        if let Some(ref stmts) = self.document_store.source.document.raw_statements {
            if source_edit::find_actor_decl(stmts, label).is_some() {
                return true;
            }
        }
        false
    }

    /// Generate a unique label for a new actor of the given type.
    fn unique_label(&self, ty: &str) -> String {
        crate::app::utils::labels::unique_label(
            self.document_store.source.document.active_timeline(),
            ty,
        )
    }

    /// Prune stale keyframe selection entries after a document mutation.
    /// Retains only triples whose (actor, property, time_ms) still exist.
    pub(crate) fn prune_stale_keyframe_selections(&mut self) {
        if self.ui_store.selection.selected_keyframes.is_empty() {
            return;
        }
        self.ui_store
            .selection
            .selected_keyframes
            .retain(|(actor, _property, time_ms)| {
                let Some(track) = self
                    .document_store
                    .source
                    .document
                    .active_timeline()
                    .and_then(|t| t.get_track(actor))
                else {
                    return false;
                };
                let mut found = false;
                macro_rules! check {
                    ($opt:expr) => {
                        if let Some(ref pt) = $opt {
                            if pt.keyframes().contains_key(time_ms) {
                                found = true;
                            }
                        }
                    };
                }
                check!(track.geometry.position);
                check!(track.geometry.motion_offset);
                check!(track.geometry.rotation);
                check!(track.geometry.scale);
                check!(track.geometry.size);
                check!(track.geometry.layout_size);
                check!(track.style.color);
                check!(track.style.opacity);
                check!(track.style.stroke_width);
                check!(track.style.stroke_color);
                check!(track.style.stroke_progress);
                check!(track.style.fill_opacity);
                check!(track.style.line_cap);
                check!(track.style.line_join);
                check!(track.text.text_content);
                check!(track.text.font_family);
                check!(track.text.font_size);
                check!(track.shape.shape_type);
                check!(track.shape.line_from);
                check!(track.shape.line_to);
                check!(track.shape.arc_angles);
                check!(track.shape.points);
                check!(track.shape.commands);
                check!(track.shape.vector_paths);
                check!(track.shape.head_size);
                check!(track.filter.filter_blur);
                check!(track.filter.filter_brightness);
                check!(track.filter.filter_contrast);
                check!(track.filter.filter_saturate);
                found
            });
    }
}
