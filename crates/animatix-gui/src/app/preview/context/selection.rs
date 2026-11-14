//! Canvas click, context-menu, and selection handling.

use crate::app::commands::{Command, ShellAction, ViewAction};
use crate::app::preview::{self, DragState, selection};

use super::PreviewContext;

impl PreviewContext<'_> {
    pub(crate) fn handle_preview_selection(
        &mut self,
        ui: &mut egui::Ui,
        _preview_rect: egui::Rect,
        response: &egui::Response,
    ) {
        if ui.input(|i| i.pointer.middle_down()) {
            return;
        }

        let is_dragging = !matches!(self.drag_state, DragState::None);

        let hit_regions = self.hit_regions;

        if response.secondary_clicked()
            && !is_dragging
            && let Some(click_pos) = response.interact_pointer_pos()
        {
            self.handle_preview_right_click(_preview_rect, click_pos, hit_regions);
        }

        let menu_item_clicked = self.handle_preview_context_menu(ui);

        let mut suppress_click = false;
        if self.selection.context_menu_open
            && !menu_item_clicked
            && ui.input(|i| i.pointer.primary_clicked())
        {
            self.selection.context_menu_open = false;
            suppress_click = true;
            self.selected_actors.clear();
        }

        // ── End of context-menu / click handling ──
        if response.clicked()
            && !is_dragging
            && !self.selection.context_menu_open
            && !suppress_click
            && let Some(click_pos) = response.interact_pointer_pos()
        {
            self.handle_preview_click(ui, _preview_rect, click_pos, hit_regions);
        }

        if response.double_clicked()
            && !is_dragging
            && let Some(click_pos) = response.interact_pointer_pos()
        {
            self.handle_preview_double_click(_preview_rect, click_pos);
        }
    }

    /// Right-click opens the context menu on the actors under the cursor.
    fn handle_preview_right_click(
        &mut self,
        preview_rect: egui::Rect,
        click_pos: egui::Pos2,
        hit_regions: &[(String, kurbo::Rect)],
    ) {
        let scene_dimensions = self.scene_dimensions;
        let zoom = self.preview.viewport.preview_zoom;
        let pan = self.preview.viewport.preview_pan;
        selection::handle_right_click(self.selection, hit_regions, click_pos, move |screen| {
            let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
            tx.screen_to_scene(screen)
        });
    }

    /// Draw the open context menu and apply the actor or action it selects.
    ///
    /// Returns whether a menu item was clicked.
    fn handle_preview_context_menu(&mut self, ui: &mut egui::Ui) -> bool {
        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let (selected, close, _rect) =
                selection::draw_context_menu(ui, self.selection, self.selected_actors);
            menu_item_clicked = close;
            if let Some(actor) = selected {
                self.selected_actors.clear();
                self.selected_actors.insert(actor);
            }
            if close {
                self.selection.context_menu_open = false;
            }
        }
        menu_item_clicked
    }

    /// Left-click selection, plus the scene jump and Alt+Click declaration jump.
    fn handle_preview_click(
        &mut self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        click_pos: egui::Pos2,
        hit_regions: &[(String, kurbo::Rect)],
    ) {
        let scene_dimensions = self.scene_dimensions;
        let zoom = self.preview.viewport.preview_zoom;
        let pan = self.preview.viewport.preview_pan;
        let modifiers = ui.ctx().input(|i| i.modifiers);
        selection::handle_click(
            self.selection,
            self.selected_actors,
            hit_regions,
            click_pos,
            move |screen| {
                let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
                tx.screen_to_scene(screen)
            },
            &modifiers,
        );

        if let Some(comp) = self.composition
            && let Some(actor) = self.selected_actors.iter().next().cloned()
        {
            let active_has_actor = self.active_scene.is_some_and(|scene| {
                comp.scenes.get(scene).is_some_and(|s| s.timeline.has_actor(&actor))
            });
            if !active_has_actor {
                for (scene_name, scene) in &comp.scenes {
                    if scene.timeline.has_actor(&actor) {
                        self.commands.push_back(Command::SelectScene(scene_name.clone()).into());
                        break;
                    }
                }
            }
        }

        // Alt+Click on canvas actor jumps to its declaration in the code editor
        if modifiers.alt
            && let Some(actor) = self.selected_actors.iter().next()
            && let Some(src) = self.source_text
        {
            let target_colon = format!("{}:", actor);
            let target_space_colon = format!("{} :", actor);
            let found_line = src.lines().position(|l| {
                let trimmed = l.trim_start();
                trimmed.starts_with(&target_colon) || trimmed.starts_with(&target_space_colon)
            });
            if let Some(line) = found_line {
                self.commands.push_back(ShellAction::Command(Command::ScrollToLine(line, 0)));
            }
        }
    }

    /// Double click inspects, never edits evaluated text.
    fn handle_preview_double_click(&mut self, _preview_rect: egui::Rect, _click_pos: egui::Pos2) {
        self.commands.push_back(ShellAction::View(ViewAction::ShowInspector));
    }
}
