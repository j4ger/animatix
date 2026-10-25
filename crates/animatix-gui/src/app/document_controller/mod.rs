//! Document-level AST mutation operations extracted from `GuiShell`.
//!
//! `DocumentController` borrows the three core stores and provides focused
//! methods for manipulating the AST, source text, editor buffer, and UI state.
//! It does **not** manage undo snapshots — the caller (`GuiShell`) is
//! responsible for calling `snapshot()` before delegating to a controller
//! method.

use std::time::Duration;

use crate::app::document::timeline_diff::KeyframeId;
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
        let template = crate::app::primitives::ActorTemplate::with_suggested_label(
            ty,
            Some(label.to_string()),
            Some(position),
            self.document_store.source.document.scene_dimensions,
            self.document_store.source.document.active_timeline(),
            extra_props,
        );
        let props = template.props;

        // If a container is selected, offer to insert inside it
        let container =
            self.ui_store.selection.selected_actors.iter().next().cloned().filter(|sel| {
                self.document_store.source.document.timeline.as_ref().is_some_and(|t| {
                    t.get_entity_id(sel)
                        .is_some_and(|id| crate::app::actions::track_is_nestable_container(t, id))
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
            if let Some(id) = timeline.get_entity_id(original_label) {
                let position = timeline
                    .world()
                    .geometry(id)
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
        let ui_after = self.ui_store.snapshot_with_preview(self.preview_store);
        let ui_after_for_commit = ui_after.clone();
        let label = crate::app::commands::UndoLabel::SetTransition {
            from_scene: from_scene.to_string(),
            transition: transition.clone(),
        };
        let status = self.document_store.with_mutation(
            label,
            ui_after.clone(),
            ui_after,
            move |store| -> Result<String, String> {
                let Some(ref mut stmts) = store.source.document.raw_statements else {
                    return Err("Failed to set transition — no AST available".to_string());
                };
                let edit = source_edit::SourceEdit::SetTransition {
                    from_scene: from_scene.into(),
                    transition: Some(transition.clone()),
                };
                if source_edit::apply_edit(stmts, edit).is_err() {
                    return Err(format!("Failed to set transition on '{}'", from_scene));
                }
                let (new_source, source_index) = (
                    animatix_syntax::to_source::stmts_to_source(stmts),
                    animatix_syntax::source_index::SourceIndex::build(stmts),
                );
                store.commit_source(new_source, source_index, ui_after_for_commit);
                Ok(format!("Set transition on '{}' → {}ms", from_scene, transition.duration_ms))
            },
        );

        self.preview_store.preview.status = match status {
            Ok(status) => status,
            Err(err) => err,
        };
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
}

fn resolve_timeline_for_scene<'a>(
    document: &'a crate::document::DocumentSession,
    scene: Option<&str>,
) -> Option<&'a animatix::timeline::Timeline> {
    match scene {
        None => document.timeline.as_ref(),
        Some(name) => document
            .composition
            .as_ref()
            .and_then(|composition| composition.scenes.get(name))
            .map(|scene| &scene.timeline),
    }
}

mod keyframes;
mod scenes;
