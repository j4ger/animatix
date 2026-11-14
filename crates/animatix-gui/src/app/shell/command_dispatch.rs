use std::path::PathBuf;

use animatix_syntax::ast::{Expr, Property, Transition};
use animatix_syntax::easing::Easing;

use crate::app::commands::{Align, Axis, MoveKeyframeSpec, PropertyEdit, PropertyValue};
use crate::app::document::timeline_diff::KeyframeId;

use super::*;

impl GuiShell {
    pub(crate) fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            // ── Document / File ───────────────────────────────────────
            Command::OpenFile(path) => self.command_open_file(path),
            Command::Save => self.command_save(),
            Command::Reload => self.command_reload(),
            Command::ReloadPlugins => self.command_reload_plugins(),
            Command::Rebuild => self.command_rebuild(),
            Command::ToggleExpandDir(path) => self.command_toggle_expand_dir(path),
            Command::SwitchWorkspace(path) => self.command_switch_workspace(path),

            // ── Playback ──────────────────────────────────────────────
            Command::TogglePlayback => self.command_toggle_playback(),
            Command::ScrubTo(next_time) => self.command_scrub_to(next_time),
            Command::PrevKeyframe => self.command_prev_keyframe(),
            Command::NextKeyframe => self.command_next_keyframe(),
            Command::FrameStepForward => self.command_frame_step_forward(),
            Command::FrameStepBackward => self.command_frame_step_backward(),
            Command::StepNext => self.command_step_next(),
            Command::StepPrevious => self.command_step_previous(),
            Command::TogglePresentationMode => self.command_toggle_presentation_mode(),
            Command::ToggleEditorSync => self.command_toggle_editor_sync(),
            Command::EditorChanged => self.command_editor_changed(),

            // ── Scene ─────────────────────────────────────────────────
            Command::SelectScene(scene) => self.command_select_scene(scene),
            Command::ReorderScenes(new_order) => self.command_reorder_scenes(new_order),
            Command::SetTransition {
                from_scene,
                transition,
            } => self.command_set_transition(from_scene, transition),
            Command::SetPlayTarget { from_scene, target } => {
                self.command_set_play_target(from_scene, target)
            },
            Command::SetSceneDuration { scene, duration_s } => {
                self.command_set_scene_duration(scene, duration_s)
            },
            Command::DuplicateScene(scene) => self.command_duplicate_scene(scene),
            Command::DeleteScene(scene) => self.command_delete_scene(scene),
            Command::AddScene(scene) => self.command_add_scene(scene),

            // ── Actor ─────────────────────────────────────────────────
            Command::CreateActor {
                ty,
                label,
                position,
                props,
            } => self.command_create_actor(ty, label, position, props),
            Command::RenameActor {
                old_label,
                new_label,
            } => self.command_rename_actor(old_label, new_label),
            Command::DuplicateActor(original_label) => self.command_duplicate_actor(original_label),
            Command::DuplicateSelectedActors => self.command_duplicate_selected_actors(),
            Command::DeleteSelectedActors => self.command_delete_selected_actors(),
            Command::ReparentActor { actor, new_parent } => {
                self.command_reparent_actor(actor, new_parent)
            },
            Command::ExtractScene {
                actor_labels,
                new_scene_name,
            } => self.command_extract_scene(actor_labels, new_scene_name),
            Command::MoveToScene {
                actor_labels,
                target_scene,
            } => self.command_move_to_scene(actor_labels, target_scene),
            Command::ToggleActorVisibility(actor) => self.command_toggle_actor_visibility(actor),
            Command::ToggleActorLock(actor) => self.command_toggle_actor_lock(actor),

            // ── Property / Inspector ──────────────────────────────────
            Command::PropertyEdit(edit) => self.command_property_edit(edit),
            Command::SetConfigProperty { key, value } => {
                self.command_set_config_property(key, value)
            },
            Command::DetachCallout {
                actor,
                from,
                to,
                label_at,
            } => self.command_detach_callout(actor, from, to, label_at),

            // ── Keyframe ──────────────────────────────────────────────
            Command::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            } => self.command_set_keyframe_easing(scene, actor, property, time_s, easing),
            Command::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            } => self.command_delete_keyframe(scene, actor, property, time_s),
            Command::SetSelectedKeyframes(keyframes) => {
                self.command_set_selected_keyframes(keyframes)
            },
            Command::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            } => self.command_move_keyframe(scene, actor, property, old_time_s, new_time_s),
            Command::SetKeyframeValue {
                scene,
                actor,
                property,
                time_s,
                value,
            } => self.command_set_keyframe_value(scene, actor, property, time_s, value),
            Command::MoveKeyframes(specs) => self.command_move_keyframes(specs),
            Command::DeleteKeyframes(ids) => self.command_delete_keyframes(ids),
            Command::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            } => {
                self.command_resize_action(verb, targets, old_start_s, new_start_s, new_duration_s)
            },

            // ── Clipboard ─────────────────────────────────────────────
            Command::PasteActors => self.command_paste_actors(),

            // ── Undo / Redo ───────────────────────────────────────────
            Command::Undo => self.command_undo(),
            Command::Redo => self.command_redo(),

            // ── Navigation ────────────────────────────────────────────
            Command::ScrollToLine(line, column) => self.command_scroll_to_line(line, column),

            // ── Viewport ──────────────────────────────────────────────
            Command::ZoomToSelection => self.command_zoom_to_selection(),
            Command::ZoomToAll => self.command_zoom_to_all(),

            // ── Alignment ─────────────────────────────────────────────
            Command::AlignActors(alignment) => self.command_align_actors(alignment),
            Command::DistributeActors(axis) => self.command_distribute_actors(axis),

            // ── Group / Ungroup ───────────────────────────────────────
            Command::GroupSelectedActors => self.command_group_selected_actors(),
            Command::UngroupSelectedActors => self.command_ungroup_selected_actors(),

            // ── Insertion Palette ─────────────────────────────────────
            // Snapshot marker only: the palette applies its insertion before
            // recording this command, so dispatch has nothing left to do.
            Command::InsertionFromPalette => vec![],

            // ── Find / Replace ────────────────────────────────────────
            // Handled in place by `perform_find_replace_all`; dispatch is a no-op.
            Command::FindReplaceAll => vec![],
        }
    }

    /// Document open flow, including the dirty-document confirmation.
    fn command_open_file(&mut self, path: PathBuf) -> Vec<Effect> {
        if self.document_store.source.is_dirty() {
            self.ui_store.unsaved_changes.open(
                format!("Save changes before opening \"{}\"?", path.display()),
                Command::OpenFile(path).into(),
            );
            return vec![];
        }
        match file::submit_open_file(&mut self.load_worker, &mut self.plugin_manager, path.clone())
        {
            Ok(token) => {
                self.pending_open = Some(token);
                // The new document will have its own snapshot state; a
                // rebuild response for the old document must not land.
                self.preview_store.pending_rebuild_at = None;
                self.preview_store.in_flight_rebuild = None;
                self.preview_store.rebuild_in_progress = false;
                self.preview_store.preview.error = None;
                self.preview_store.preview.status = format!("Opening {}…", path.display());
                self.preview_store.preview_dirty = true;
            },
            Err(err) => {
                self.pending_open = None;
                self.preview_store.preview.set_status_error(format!(
                    "Open failed • {} • {}",
                    path.display(),
                    err
                ));
            },
        }
        vec![]
    }

    fn command_toggle_expand_dir(&mut self, path: PathBuf) -> Vec<Effect> {
        file::handle_toggle_expand_dir(&mut self.workspace_store, &self.document_store, path)
    }

    /// Workspace switch flow, including the dirty-document confirmation.
    fn command_switch_workspace(&mut self, path: PathBuf) -> Vec<Effect> {
        if self.document_store.source.is_dirty() {
            self.ui_store.unsaved_changes.open(
                format!("Save changes before switching workspace to \"{}\"?", path.display()),
                Command::SwitchWorkspace(path).into(),
            );
            return vec![];
        }
        file::handle_switch_workspace(&mut self.workspace_store, &self.document_store, path)
    }

    fn command_save(&mut self) -> Vec<Effect> {
        if self.recovery_prompt_pending() {
            return vec![Effect::Toast(self.recovery_prompt_save_blocked())];
        }
        file::handle_save(&mut self.document_store, &mut self.preview_store)
    }

    fn command_reload(&mut self) -> Vec<Effect> {
        if self.document_store.source.is_dirty() {
            self.ui_store
                .unsaved_changes
                .open("Save changes before reloading?".to_string(), Command::Reload.into());
            return vec![];
        }
        file::handle_reload(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            &mut self.workspace_store,
        )
    }

    fn command_rebuild(&mut self) -> Vec<Effect> {
        // A manual rebuild supersedes any debounced rebuild and runs the
        // same off-thread worker as ordinary source edits, so the UI
        // thread never blocks on parse/build.
        self.preview_store.pending_rebuild_at = None;
        self.preview_store.preview.error = None;
        if let Some(token) = file::handle_rebuild_submit(
            &mut self.rebuild_worker,
            &mut self.document_store,
            &mut self.preview_store,
        ) {
            self.preview_store.in_flight_rebuild = Some(token);
        } else {
            self.preview_store.in_flight_rebuild = None;
        }
        vec![]
    }

    fn command_reload_plugins(&mut self) -> Vec<Effect> {
        if self.plugin_manager.reload() {
            self.apply_plugin_reload();
        } else {
            self.preview_store.preview.set_status_info("Plugins already up to date");
        }
        vec![]
    }

    fn command_toggle_playback(&mut self) -> Vec<Effect> {
        playback::handle_toggle_playback(&mut self.preview_store)
    }

    fn command_scrub_to(&mut self, next_time: f64) -> Vec<Effect> {
        playback::handle_scrub_to(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
            next_time,
        )
    }

    fn command_prev_keyframe(&mut self) -> Vec<Effect> {
        playback::handle_prev_keyframe(
            &self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_next_keyframe(&mut self) -> Vec<Effect> {
        playback::handle_next_keyframe(
            &self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_frame_step_forward(&mut self) -> Vec<Effect> {
        playback::handle_frame_step_forward(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_frame_step_backward(&mut self) -> Vec<Effect> {
        playback::handle_frame_step_backward(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_step_next(&mut self) -> Vec<Effect> {
        playback::handle_step_next(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_step_previous(&mut self) -> Vec<Effect> {
        playback::handle_step_previous(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_toggle_presentation_mode(&mut self) -> Vec<Effect> {
        playback::handle_toggle_presentation_mode(&mut self.preview_store)
    }

    fn command_toggle_editor_sync(&mut self) -> Vec<Effect> {
        playback::handle_toggle_editor_sync(&mut self.ui_store)
    }

    fn command_editor_changed(&mut self) -> Vec<Effect> {
        playback::handle_editor_changed(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_select_scene(&mut self, scene: String) -> Vec<Effect> {
        self.ui_store.selection.selection.clear_tapped_place();
        self.ui_store.selection.selected_keyframes.clear();
        scene::handle_select_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
        )
    }

    fn command_reorder_scenes(&mut self, new_order: Vec<String>) -> Vec<Effect> {
        scene::handle_reorder_scenes(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            new_order,
        )
    }

    fn command_set_transition(
        &mut self,
        from_scene: String,
        transition: Transition,
    ) -> Vec<Effect> {
        property::handle_set_transition(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            from_scene,
            transition,
        )
    }

    fn command_set_play_target(
        &mut self,
        from_scene: String,
        target: Option<String>,
    ) -> Vec<Effect> {
        property::handle_set_play_target(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            from_scene,
            target,
        )
    }

    fn command_set_scene_duration(
        &mut self,
        scene: String,
        duration_s: Option<f64>,
    ) -> Vec<Effect> {
        property::handle_set_scene_duration(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
            duration_s,
        )
    }

    fn command_duplicate_scene(&mut self, scene: String) -> Vec<Effect> {
        scene::handle_duplicate_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
        )
    }

    fn command_delete_scene(&mut self, scene: String) -> Vec<Effect> {
        scene::handle_delete_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
        )
    }

    fn command_add_scene(&mut self, scene: String) -> Vec<Effect> {
        scene::handle_add_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
        )
    }

    fn command_create_actor(
        &mut self,
        ty: String,
        label: String,
        position: [f32; 2],
        props: Vec<Property>,
    ) -> Vec<Effect> {
        actor::handle_create_actor(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            ty,
            label,
            position,
            props,
        )
    }

    fn command_rename_actor(&mut self, old_label: String, new_label: String) -> Vec<Effect> {
        actor::handle_rename_actor(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            old_label,
            new_label,
        )
    }

    fn command_duplicate_actor(&mut self, original_label: String) -> Vec<Effect> {
        actor::handle_duplicate_actor(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            original_label,
        )
    }

    fn command_duplicate_selected_actors(&mut self) -> Vec<Effect> {
        let labels: Vec<String> = self.ui_store.selection.selected_actors.iter().cloned().collect();
        let mut effects = Vec::new();
        for label in labels {
            effects.extend(actor::handle_duplicate_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                label,
            ));
        }
        effects
    }

    fn command_delete_selected_actors(&mut self) -> Vec<Effect> {
        actor::handle_delete_selected_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
        )
    }

    fn command_reparent_actor(&mut self, actor: String, new_parent: Option<String>) -> Vec<Effect> {
        actor::handle_reparent_actor(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            actor,
            new_parent,
        )
    }

    fn command_extract_scene(
        &mut self,
        actor_labels: Vec<String>,
        new_scene_name: String,
    ) -> Vec<Effect> {
        actor::handle_extract_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            actor_labels,
            new_scene_name,
        )
    }

    fn command_move_to_scene(
        &mut self,
        actor_labels: Vec<String>,
        target_scene: String,
    ) -> Vec<Effect> {
        actor::handle_move_to_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            actor_labels,
            target_scene,
        )
    }

    fn command_toggle_actor_visibility(&mut self, actor: String) -> Vec<Effect> {
        actor::handle_toggle_actor_visibility(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            actor,
        )
    }

    fn command_toggle_actor_lock(&mut self, actor: String) -> Vec<Effect> {
        actor::handle_toggle_actor_lock(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            actor,
        )
    }

    fn command_property_edit(&mut self, edit: PropertyEdit) -> Vec<Effect> {
        self.handle_property_edit(edit);
        vec![]
    }

    fn command_set_config_property(&mut self, key: String, value: Expr) -> Vec<Effect> {
        property::handle_set_config_property(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            key,
            value,
        )
    }

    fn command_detach_callout(
        &mut self,
        actor: String,
        from: [f32; 2],
        to: [f32; 2],
        label_at: [f32; 2],
    ) -> Vec<Effect> {
        self.handle_detach_callout(actor, from, to, label_at);
        vec![]
    }

    fn command_set_keyframe_easing(
        &mut self,
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
        easing: Easing,
    ) -> Vec<Effect> {
        keyframe::handle_set_keyframe_easing(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
            actor,
            property,
            time_s,
            easing,
        )
    }

    fn command_delete_keyframe(
        &mut self,
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
    ) -> Vec<Effect> {
        keyframe::handle_delete_keyframe(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
            actor,
            property,
            time_s,
        )
    }

    fn command_set_selected_keyframes(&mut self, keyframes: Vec<KeyframeId>) -> Vec<Effect> {
        ui::handle_set_selected_keyframes(&mut self.ui_store, keyframes)
    }

    fn command_move_keyframe(
        &mut self,
        scene: Option<String>,
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    ) -> Vec<Effect> {
        keyframe::handle_move_keyframe(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
            actor,
            property,
            old_time_s,
            new_time_s,
        )
    }

    fn command_set_keyframe_value(
        &mut self,
        scene: Option<String>,
        actor: String,
        property: String,
        time_s: f64,
        value: PropertyValue,
    ) -> Vec<Effect> {
        keyframe::handle_set_keyframe_value(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
            actor,
            property,
            time_s,
            value,
        )
    }

    fn command_move_keyframes(&mut self, specs: Vec<MoveKeyframeSpec>) -> Vec<Effect> {
        keyframe::handle_move_keyframes(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            specs,
        )
    }

    fn command_delete_keyframes(&mut self, ids: Vec<KeyframeId>) -> Vec<Effect> {
        keyframe::handle_delete_keyframes(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            ids,
        )
    }

    fn command_resize_action(
        &mut self,
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    ) -> Vec<Effect> {
        keyframe::handle_resize_action(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            verb,
            targets,
            old_start_s,
            new_start_s,
            new_duration_s,
        )
    }

    fn command_paste_actors(&mut self) -> Vec<Effect> {
        actor::handle_paste_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
        )
    }

    fn command_undo(&mut self) -> Vec<Effect> {
        ui::handle_undo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
    }

    fn command_redo(&mut self) -> Vec<Effect> {
        ui::handle_redo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
    }

    fn command_scroll_to_line(&mut self, line: usize, column: usize) -> Vec<Effect> {
        // Source navigation must reveal the editor, not focus a hidden pane.
        let detail_visible = if self.ui_store.view.compact {
            self.ui_store.view.compact_drawer == Some(crate::app::panels::CompactDrawer::Detail)
        } else {
            crate::app::persistence::detail_visible(&self.ui_store.view.tree)
        };
        if !detail_visible
            || crate::app::persistence::active_detail_tab(&self.ui_store.view.tree)
                != Some(crate::app::WorkspaceTab::Code)
        {
            ui::handle_show_code(&mut self.ui_store);
        }
        ui::handle_scroll_to_line(&mut self.document_store, line, column)
    }

    fn command_zoom_to_selection(&mut self) -> Vec<Effect> {
        ui::handle_zoom_to_selection(&mut self.preview_store, &self.ui_store, &self.document_store)
    }

    fn command_zoom_to_all(&mut self) -> Vec<Effect> {
        ui::handle_zoom_to_all(&mut self.preview_store, &self.document_store)
    }

    fn command_align_actors(&mut self, alignment: Align) -> Vec<Effect> {
        actor::handle_align_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            alignment,
        )
    }

    fn command_distribute_actors(&mut self, axis: Axis) -> Vec<Effect> {
        actor::handle_distribute_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            axis,
        )
    }

    fn command_group_selected_actors(&mut self) -> Vec<Effect> {
        actor::handle_group_selected_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
        )
    }

    fn command_ungroup_selected_actors(&mut self) -> Vec<Effect> {
        actor::handle_ungroup_selected_actors(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
        )
    }
}
