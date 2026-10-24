//! Canvas click, context-menu, and selection handling.

use crate::app::commands::{ActorCommand, Command, SceneCommand, ShellAction, ViewAction};
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

        // Filter out locked actors from hit regions for selection purposes
        let unlocked_hit_regions: Vec<(String, kurbo::Rect)> = self
            .hit_regions
            .iter()
            .filter(|(label, _)| !self.timeline.is_some_and(|t| t.is_actor_locked(label)))
            .cloned()
            .collect();

        if response.secondary_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                selection::handle_right_click(
                    self.selection,
                    &unlocked_hit_regions,
                    click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(
                            scene_dimensions,
                            _preview_rect,
                            zoom,
                            pan,
                        );
                        tx.screen_to_scene(screen)
                    },
                );
            }
        }

        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let effective: Vec<String> = if self.selected_actors.is_empty() {
                self.selection.context_menu_actors.first().cloned().into_iter().collect()
            } else {
                self.selected_actors.iter().cloned().collect()
            };
            let any_locked = effective
                .iter()
                .any(|label| self.timeline.is_some_and(|t| t.is_actor_locked(label)));
            let any_hidden = effective
                .iter()
                .any(|label| self.timeline.is_some_and(|t| !t.is_actor_visible(label)));

            let (selected, action, close, _rect) = selection::draw_context_menu(
                ui,
                self.selection,
                self.selected_actors,
                any_locked,
                any_hidden,
            );
            menu_item_clicked = close;
            if let Some(actor) = selected {
                self.selected_actors.clear();
                self.selected_actors.insert(actor);
            } else if let Some(action) = action {
                // Actions operate on the current selection; with nothing
                // selected, fall back to the actor under the cursor.
                if self.selected_actors.is_empty() {
                    self.selected_actors.extend(effective.iter().cloned());
                }
                use selection::ContextMenuAction as A;
                match action {
                    A::Duplicate => {
                        self.commands.push_back(ActorCommand::DuplicateSelectedActors.into())
                    },
                    A::Delete => self.commands.push_back(ActorCommand::DeleteSelectedActors.into()),
                    A::Group => self.commands.push_back(ActorCommand::GroupSelectedActors.into()),
                    A::Ungroup => {
                        self.commands.push_back(ActorCommand::UngroupSelectedActors.into())
                    },
                    A::ToggleLock => {
                        for label in &effective {
                            self.commands
                                .push_back(ActorCommand::ToggleActorLock(label.clone()).into());
                        }
                    },
                    A::ToggleVisibility => {
                        for label in &effective {
                            self.commands.push_back(
                                ActorCommand::ToggleActorVisibility(label.clone()).into(),
                            );
                        }
                    },
                }
            }
            if close {
                self.selection.context_menu_open = false;
            }
        }

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
        {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let modifiers = ui.ctx().input(|i| i.modifiers);
                selection::handle_click(
                    self.selection,
                    self.selected_actors,
                    &unlocked_hit_regions,
                    click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(
                            scene_dimensions,
                            _preview_rect,
                            zoom,
                            pan,
                        );
                        tx.screen_to_scene(screen)
                    },
                    &modifiers,
                );

                if let Some(comp) = self.composition {
                    if let Some(actor) = self.selected_actors.iter().next().cloned() {
                        let active_has_actor = self.active_scene.is_some_and(|scene| {
                            comp.scenes.get(scene).is_some_and(|s| s.timeline.has_actor(&actor))
                        });
                        if !active_has_actor {
                            for (scene_name, scene) in &comp.scenes {
                                if scene.timeline.has_actor(&actor) {
                                    self.commands.push_back(
                                        SceneCommand::SelectScene(scene_name.clone()).into(),
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }

                // Alt+Click on canvas actor jumps to its declaration in the code editor
                if modifiers.alt {
                    if let Some(actor) = self.selected_actors.iter().next() {
                        if let Some(src) = self.source_text {
                            let target_colon = format!("{}:", actor);
                            let target_space_colon = format!("{} :", actor);
                            let found_line = src.lines().position(|l| {
                                let trimmed = l.trim_start();
                                trimmed.starts_with(&target_colon)
                                    || trimmed.starts_with(&target_space_colon)
                            });
                            if let Some(line) = found_line {
                                self.commands.push_back(ShellAction::Command(
                                    Command::ScrollToLine(line, 0),
                                ));
                                self.commands.push_back(ShellAction::View(ViewAction::ShowCode));
                            }
                        }
                    }
                }
            }
        }

        // ── Double-click: start inline text editing for text actors ──
        if response.double_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let scene_point = {
                    let tx =
                        preview::PreviewTransform::new(scene_dimensions, _preview_rect, zoom, pan);
                    tx.screen_to_scene(click_pos)
                };

                // Find a text-type actor at the click position (skip locked actors)
                let text_actor = self
                    .hit_regions
                    .iter()
                    .filter(|(_, bounds)| {
                        // Check if click is within bounds
                        scene_point.x >= bounds.x0
                            && scene_point.x <= bounds.x1
                            && scene_point.y >= bounds.y0
                            && scene_point.y <= bounds.y1
                    })
                    .filter(|(label, _)| {
                        // Skip locked actors
                        !self.timeline.is_some_and(|t| t.is_actor_locked(label))
                    })
                    .filter(|(label, _)| self.get_text_property(label).is_some())
                    .map(|(label, _)| label.clone())
                    .next();

                if let Some(actor) = text_actor {
                    self.selected_actors.clear();
                    self.selected_actors.insert(actor.clone());
                    self.start_inline_edit(&actor, _preview_rect);
                }
            }
        }
    }
}
