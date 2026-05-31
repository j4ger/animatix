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
        source_index: animatix::source_index::SourceIndex,
    ) {
        self.document_store.source.document.source_text = new_source.clone();
        self.document_store.source.editor.replace_text(new_source);
        self.document_store.source.document.is_dirty = true;
        self.document_store.source.document.source_index = Some(source_index);
        self.preview_store.pending_rebuild_at =
            Some(std::time::Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
    }

    // ── Actor management ────────────────────────────────────────────────

    /// Create a new actor from a type/label/position.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_create_actor(&mut self, ty: &str, label: &str, position: [f32; 2]) {
        let props = crate::app::actions::default_props_for_actor(
            ty,
            position,
            self.document_store.source.document.scene_dimensions,
        );

        // If a container is selected, offer to insert inside it
        let container =
            self.ui_store
                .selection
                .selected_actors
                .iter()
                .next()
                .cloned()
                .filter(|sel| {
                    self.document_store
                        .source
                        .document
                        .timeline
                        .as_ref()
                        .is_some_and(|t| {
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
                time_s: self.preview_store.preview.playback.current_time_s,
            };

            if crate::source_edit::apply_edit(stmts, edit) {
                let new_source = animatix::to_source::stmts_to_source(stmts);
                let source_index = animatix::source_index::SourceIndex::build(stmts);
                self.apply_source(new_source, source_index);
                self.preview_store.preview.status = format!(
                    "Created {} ({}) at ({:.0}, {:.0})",
                    label, ty, position[0], position[1]
                );
            } else {
                self.preview_store.preview.status =
                    format!("Failed to create {} — source edit failed", label);
                return;
            }
        } else {
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
            self.preview_store.preview.status = "Failed to duplicate — no AST available".to_string();
            return;
        };

        // Find the original actor declaration
        let original_stmt = source_edit::find_actor_decl(stmts, original_label).cloned();

        let Some(mut new_stmt) = original_stmt else {
            self.preview_store.preview.status =
                format!("Failed to duplicate — actor '{}' not found", original_label);
            return;
        };

        // Update label in the new statement
        match &mut new_stmt {
            animatix::ast::Stmt::ActorDecl { label, .. } => *label = new_label.clone(),
            _ => {
                self.preview_store.preview.status =
                    "Failed to duplicate — unsupported actor type".to_string();
                return;
            }
        }

        // Find position to insert (after the original actor)
        if let Some(pos) = stmts.iter().position(|s| {
            matches!(s, animatix::ast::Stmt::ActorDecl { label, .. } if label == original_label)
        }) {
            stmts.insert(pos + 1, new_stmt);
        } else {
            stmts.push(new_stmt);
        }

        // Commit source — scope block drops stmts borrow before accessing self
        let (new_source, source_index) = {
            (
                animatix::to_source::stmts_to_source(stmts),
                animatix::source_index::SourceIndex::build(stmts),
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
        let time_ms = (self.preview_store.preview.playback.current_time_s * 1000.0) as u64;
        if let Some(timeline) = self.document_store.source.document.timeline.as_ref() {
            if let Some(track) = timeline.get_track(original_label) {
                let position = track
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
            self.preview_store.preview.status = "Failed to delete — no AST available".to_string();
            return;
        };

        let to_delete: Vec<String> =
            self.ui_store.selection.selected_actors.iter().cloned().collect();
        if to_delete.is_empty() {
            return;
        }

        let mut deleted = Vec::new();
        for label in &to_delete {
            let pos = stmts.iter().position(|s| {
                matches!(s, animatix::ast::Stmt::ActorDecl { label: l, .. } if l == label)
            });
            if let Some(pos) = pos {
                stmts.remove(pos);
                deleted.push(label.clone());
            }
        }

        if deleted.is_empty() {
            self.preview_store.preview.status = "No actors deleted".to_string();
            return;
        }

        // Commit source — scope block drops stmts borrow
        let (new_source, source_index) = {
            (
                animatix::to_source::stmts_to_source(stmts),
                animatix::source_index::SourceIndex::build(stmts),
            )
        };
        self.apply_source(new_source, source_index);

        // Clear selection
        self.ui_store.selection.selected_actors.clear();
        self.preview_store.preview_dirty = true;
        self.preview_store.preview.status = format!("Deleted {} actor(s)", deleted.len());
    }

    // ── Scene/transition edits ──────────────────────────────────────────

    /// Update the transition on a scene's play statement.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_transition(
        &mut self,
        from_scene: &str,
        transition: animatix::ast::Transition,
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

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status = format!(
                "Set transition on '{}' → {}ms",
                from_scene, transition.duration_ms
            );
        } else {
            self.preview_store.preview.status =
                format!("Failed to set transition on '{}'", from_scene);
        }
    }

    /// Update the play target for a scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_set_play_target(&mut self, from_scene: &str, target: Option<String>) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store.preview.status =
                "Failed to set play target — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::SetPlayTarget {
            scene: from_scene.into(),
            target: target.clone(),
        };

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
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
            self.preview_store.preview.status =
                format!("Failed to set play target on '{}'", from_scene);
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
        easing: animatix::easing::Easing,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
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

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Set easing on '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
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
            self.preview_store.preview.status =
                "Failed to delete keyframe — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::DeleteKeyframe {
            actor: actor.into(),
            property: property.into(),
            time_s,
        };

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Deleted keyframe '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.preview_store.preview.status = format!(
                "Failed to delete keyframe '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            );
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

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status =
                format!("Moved keyframe '{}.{}' from {:.2}s to {:.2}s", actor, property, old_time_s, new_time_s);
        } else {
            self.preview_store.preview.status = format!(
                "Failed to move keyframe '{}.{}' from {:.2}s — not found",
                actor, property, old_time_s
            );
        }
    }

    // ── Actor hierarchy / scene refactoring ─────────────────────────────

    /// Reparent an actor under a new parent (or to top-level).
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_reparent_actor(&mut self, actor: &str, new_parent: Option<String>) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store.preview.status =
                "Failed to reparent — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::Reparent {
            actor: actor.into(),
            new_parent: new_parent.clone(),
        };

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            if let Some(ref parent) = new_parent {
                self.preview_store.preview.status =
                    format!("Reparented '{}' under '{}'", actor, parent);
            } else {
                self.preview_store.preview.status =
                    format!("Reparented '{}' to top level", actor);
            }
        } else {
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
            self.preview_store.preview.status =
                "Failed to extract scene — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::ExtractScene {
            actor_labels: actor_labels.clone(),
            new_scene_name: new_scene_name.clone(),
        };

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status = format!(
                "Extracted {} actor(s) into scene '{}'",
                actor_labels.len(),
                new_scene_name
            );
        } else {
            self.preview_store.preview.status = "Failed to extract scene".to_string();
        }
    }

    /// Move selected actors to an existing scene.
    /// NOTE: The caller should have called `snapshot()` before this.
    pub(crate) fn handle_move_to_scene(
        &mut self,
        actor_labels: Vec<String>,
        target_scene: String,
    ) {
        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store.preview.status =
                "Failed to move actors — no AST available".to_string();
            return;
        };

        let edit = source_edit::SourceEdit::MoveToScene {
            actor_labels: actor_labels.clone(),
            target_scene: target_scene.clone(),
        };

        if source_edit::apply_edit(stmts, edit) {
            let (new_source, source_index) = {
                (
                    animatix::to_source::stmts_to_source(stmts),
                    animatix::source_index::SourceIndex::build(stmts),
                )
            };
            self.apply_source(new_source, source_index);
            self.preview_store.preview.status = format!(
                "Moved {} actor(s) to scene '{}'",
                actor_labels.len(),
                target_scene
            );
        } else {
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
        let current_time_s = self.preview_store.preview.playback.current_time_s;
        let clipboard = self.ui_store.clipboard.clipboard_actors.clone();

        // Pre-generate all unique labels before mutating the AST.
        let label_map: Vec<(String, String)> = clipboard
            .iter()
            .map(|orig| (orig.clone(), self.paste_unique_label(orig)))
            .collect();

        let Some(ref mut stmts) = self.document_store.source.document.raw_statements else {
            self.preview_store.preview.status =
                "Failed to paste — no AST available".to_string();
            return;
        };

        let mut pasted_labels = Vec::new();

        for (original_label, new_label) in &label_map {
            // Find the original actor declaration
            let original_stmt =
                source_edit::find_actor_decl(stmts, original_label).cloned();
            let Some(mut new_stmt) = original_stmt else {
                continue;
            };

            // Update label in the new statement
            match &mut new_stmt {
                animatix::ast::Stmt::ActorDecl { label, .. } => *label = new_label.clone(),
                _ => continue,
            }

            // Insert the declaration at the end (or after the original)
            if let Some(pos) = stmts.iter().position(|s| {
                matches!(s, animatix::ast::Stmt::ActorDecl { label, .. } if label == original_label)
            }) {
                stmts.insert(pos + 1, new_stmt);
            } else {
                stmts.push(new_stmt);
            }

            // Find and clone all keyframe assignments referencing the original actor
            let keyframe_stmts =
                source_edit::find_keyframes_for_actor(stmts, original_label);
            for mut kf in keyframe_stmts {
                // Rename references within the keyframe
                source_edit::rename_all_references(
                    std::slice::from_mut(&mut kf),
                    original_label,
                    new_label,
                );
                // Shift absolute keyframe times by current_time_s
                source_edit::shift_keyframe_times(
                    std::slice::from_mut(&mut kf),
                    current_time_s,
                );
                stmts.push(kf);
            }

            pasted_labels.push(new_label.clone());
        }

        if pasted_labels.is_empty() {
            self.preview_store.preview.status =
                "Failed to paste — actor(s) not found in AST".to_string();
            return;
        }

        // Commit source — scope block drops stmts borrow
        let (new_source, source_index) = {
            (
                animatix::to_source::stmts_to_source(stmts),
                animatix::source_index::SourceIndex::build(stmts),
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
        for i in 1.. {
            let candidate = format!("{}_{}", base, i);
            if !self.has_actor_label(&candidate) {
                return candidate;
            }
        }
        format!("{}_{}", base, 999)
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
        if self
            .ui_store
            .clipboard
            .clipboard_actors
            .contains(&label.to_string())
        {
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
        let base = ty.to_lowercase();
        let existing: std::collections::HashSet<String> = self
            .document_store
            .source
            .document
            .timeline
            .as_ref()
            .map(|t| t.tracks().keys().cloned().collect())
            .unwrap_or_default();
        for i in 1.. {
            let candidate = format!("{}{}", base, i);
            if !existing.contains(&candidate) {
                return candidate;
            }
        }
        format!("{}{}", base, existing.len() + 1)
    }
}