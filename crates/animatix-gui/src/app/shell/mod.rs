pub mod export_dialog;
pub mod insertion_palette;
pub mod shortcut_cheat_sheet;
pub mod toolbar;
pub mod settings;

use crate::app::commands::{Command, DragEvent, Effect, ShellAction, ViewAction};
use crate::app::handlers::*;
use crate::app::GuiShell;

impl GuiShell {
    /// Handle a single action, returning any collected side effects.
    pub(crate) fn handle_action(&mut self, action: ShellAction) -> Vec<Effect> {
        match action {
            ShellAction::Command(cmd) => self.handle_command(cmd),
            ShellAction::View(view) => self.handle_view_action(view),
            ShellAction::Drag(drag) => self.handle_drag_event(drag),
        }
    }

    fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::OpenFile(path) => file::handle_open_file(
                &mut self.document_store,
                &mut self.workspace_store,
                &mut self.preview_store,
                &mut self.ui_store,
                path,
            ),
            Command::ToggleExpandDir(path) => {
                file::handle_toggle_expand_dir(&mut self.workspace_store, &self.document_store, path)
            }
            Command::SwitchWorkspace(path) => file::handle_switch_workspace(
                &mut self.workspace_store,
                &self.document_store,
                path,
            ),
            Command::Save => file::handle_save(&mut self.document_store, &mut self.preview_store),
            Command::Reload => file::handle_reload(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.workspace_store,
            ),
            Command::Rebuild => {
                file::handle_rebuild(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::ScrubTo(next_time) => playback::handle_scrub_to(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                next_time,
            ),
            Command::TogglePlayback => playback::handle_toggle_playback(&mut self.preview_store),
            Command::ToggleEditorSync => playback::handle_toggle_editor_sync(&mut self.ui_store),
            Command::EditorChanged => playback::handle_editor_changed(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::PrevKeyframe => playback::handle_prev_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::NextKeyframe => playback::handle_next_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::SelectScene(scene) => {
                scene::handle_select_scene(&mut self.document_store, &mut self.preview_store, scene)
            }
            Command::ReorderScenes(new_order) => {
                scene::handle_reorder_scenes(&mut self.document_store, &mut self.preview_store, new_order)
            }
            Command::DuplicateScene(scene) => {
                scene::handle_duplicate_scene(&mut self.document_store, &mut self.preview_store, scene)
            }
            Command::DeleteScene(scene) => {
                scene::handle_delete_scene(&mut self.document_store, &mut self.preview_store, &mut self.ui_store, scene)
            }
            Command::CreateActor { ty, label, position } => actor::handle_create_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                ty,
                label,
                position,
            ),
            Command::DuplicateActor(original_label) => actor::handle_duplicate_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                original_label,
            ),
            Command::DeleteSelectedActors => actor::handle_delete_selected_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::PasteActors => actor::handle_paste_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::SetTransition {
                from_scene,
                transition,
            } => property::handle_set_transition(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                transition,
            ),
            Command::SetPlayTarget {
                from_scene,
                target,
            } => property::handle_set_play_target(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                target,
            ),
            Command::RenameActor { old_label, new_label } => actor::handle_rename_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                old_label,
                new_label,
            ),
            Command::SetKeyframeEasing {
                actor,
                property,
                time_s,
                easing,
            } => keyframe::handle_set_keyframe_easing(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                time_s,
                easing,
            ),
            Command::DeleteKeyframe {
                actor,
                property,
                time_s,
            } => keyframe::handle_delete_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                time_s,
            ),
            Command::MoveKeyframe {
                actor,
                property,
                old_time_s,
                new_time_s,
            } => keyframe::handle_move_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                property,
                old_time_s,
                new_time_s,
            ),
            Command::ReparentActor { actor, new_parent } => actor::handle_reparent_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                new_parent,
            ),
            Command::ExtractScene {
                actor_labels,
                new_scene_name,
            } => actor::handle_extract_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                new_scene_name,
            ),
            Command::MoveToScene {
                actor_labels,
                target_scene,
            } => actor::handle_move_to_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                target_scene,
            ),
            Command::ToggleActorVisibility(actor) => actor::handle_toggle_actor_visibility(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
            ),
            Command::ToggleActorLock(actor) => actor::handle_toggle_actor_lock(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
            ),
            Command::PropertyEdit(edit) => {
                self.handle_property_edit(edit);
                vec![]
            }
            Command::Undo => {
                ui::handle_undo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::Redo => {
                ui::handle_redo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
            }
            Command::ScrollToLine(line, column) => ui::handle_scroll_to_line(&mut self.document_store, line, column),
        }
    }

    fn handle_view_action(&mut self, view: ViewAction) -> Vec<Effect> {
        match view {
            ViewAction::ShowInspector => ui::handle_show_inspector(&mut self.ui_store),
            ViewAction::OpenExportDialog => {
                ui::handle_open_export_dialog(&mut self.export_store, &self.document_store)
            }
        }
    }

    fn handle_drag_event(&mut self, drag: DragEvent) -> Vec<Effect> {
        let effects = match drag {
            DragEvent::DragEnded => ui::handle_drag_ended(&mut self.ui_store),
            DragEvent::InspectorInputDragStarted => {
                ui::handle_inspector_input_drag_started(&mut self.ui_store)
            }
            DragEvent::InspectorInputDragEnded => {
                ui::handle_inspector_input_drag_ended(&mut self.ui_store)
            }
        };

        // Flush any deferred source edits once the drag interaction ends.
        if matches!(drag, DragEvent::DragEnded | DragEvent::InspectorInputDragEnded) {
            self.flush_pending_drag_edits();
        }

        effects
    }
}
