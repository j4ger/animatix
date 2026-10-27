use super::*;

impl DocumentController<'_> {
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
    pub(crate) fn paste_unique_label(&self, base: &str) -> String {
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
    pub(crate) fn has_actor_label(&self, label: &str) -> bool {
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
    pub(crate) fn unique_label(&self, ty: &str) -> String {
        crate::app::utils::labels::unique_label(
            self.document_store.source.document.active_timeline(),
            ty,
        )
    }

    /// Prune stale keyframe selection entries after a document mutation.
    /// Retains only scene-qualified identities whose property/time still exist.
    pub(crate) fn prune_stale_keyframe_selections(&mut self) {
        if self.ui_store.selection.selected_keyframes.is_empty() {
            return;
        }
        let document = &self.document_store.source.document;
        self.ui_store.selection.selected_keyframes.retain(|keyframe| {
            let Some((_id, geometry, style, shape, text)) =
                resolve_timeline_for_scene(document, keyframe.scene.as_deref()).and_then(
                    |timeline| {
                        let id = timeline.get_entity_id(&keyframe.actor)?;
                        Some((
                            id,
                            timeline.world().geometry(id),
                            timeline.world().style(id),
                            timeline.world().shape_or_default(id),
                            timeline.world().text_or_default(id),
                        ))
                    },
                )
            else {
                return false;
            };
            crate::app::document::timeline_diff::collect_per_property_keyframes(
                geometry, style, shape, text, None,
            )
            .into_iter()
            .any(|(property, times)| {
                property == keyframe.property && times.contains(&keyframe.time_ms)
            })
        });
    }
}
