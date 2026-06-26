//! Command palette: Cmd+Shift+P searchable list of all commands.

use crate::app::components::dialog::{DialogSpec, self};
use crate::app::GuiShell;
use crate::app::commands::ViewAction;
use crate::app::commands::{ActorCommand, DocumentCommand, PlaybackCommand, ShellAction, ViewCommand};
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::border;
use crate::app::design_tokens::semantic::surface;

use crate::app::design_tokens::semantic::text;

use crate::app::design_tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;

struct PaletteItem {
    label: String,
    icon: &'static str,
    action: ShellAction,
    keywords: &'static str,
}

impl GuiShell {
    pub(crate) fn command_palette_ui(&mut self, ui: &mut egui::Ui) {
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let spec = DialogSpec::new("command_palette", [480.0, 400.0])
            .with_min_size([400.0, 300.0])
            .with_max_size([600.0, 500.0])
            .with_anchor_offset([0.0, -80.0]);

        let mut commands = Vec::new();
        let mut body_close = false;

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let close = dialog::title_row(ui, "Command Palette");
            ui.add_space(sp.base.space_3);

            // Search input
            let search_resp = ui.add(
                egui::TextEdit::singleline(&mut self.ui_store.command_palette_query)
                    .hint_text("Type a command…")
                    .font(TextRole::Body.font_id())
                    .desired_width(f32::INFINITY)
                    .id_source("cmd_palette_search"),
            );
            search_resp.request_focus();
            ui.add_space(sp.base.space_3);
            ui.separator();
            ui.add_space(sp.base.space_2);

            let query = self.ui_store.command_palette_query.to_lowercase();
            let items = self.build_palette_items();
            let filtered: Vec<&PaletteItem> = items
                .iter()
                .filter(|item| {
                    item.label.to_lowercase().contains(&query)
                        || item.keywords.to_lowercase().contains(&query)
                })
                .collect();

            // Clamp selected index after filtering
            if self.ui_store.command_palette_selected >= filtered.len() {
                self.ui_store.command_palette_selected = filtered.len().saturating_sub(1);
            }

            // Keyboard navigation
            let mut enter_pressed = false;
            ui.input(|i| {
                if i.key_pressed(egui::Key::ArrowDown) {
                    let len = filtered.len();
                    if len > 0 {
                        self.ui_store.command_palette_selected =
                            (self.ui_store.command_palette_selected + 1) % len;
                    }
                }
                if i.key_pressed(egui::Key::ArrowUp) {
                    let len = filtered.len();
                    if len > 0 {
                        self.ui_store.command_palette_selected =
                            (self.ui_store.command_palette_selected + len - 1) % len;
                    }
                }
                if i.key_pressed(egui::Key::Enter) {
                    enter_pressed = true;
                }
            });
            if enter_pressed && !filtered.is_empty() {
                let item = filtered[self.ui_store.command_palette_selected];
                commands.push(item.action.clone());
                self.ui_store.command_palette_query.clear();
                body_close = true;
            }

            if filtered.is_empty() {
                ui.label(
                    egui::RichText::new("No commands match your search")
                        .size(TextRole::BodyS.size())
                        .color(text::MUTED),
                );
            } else {
                egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                    for (idx, item) in filtered.iter().enumerate() {
                        let is_selected = idx == self.ui_store.command_palette_selected;

                        let resp = ui.add(
                            egui::Button::new(
                                egui::RichText::new(format!("{}  {}", item.icon, item.label))
                                    .size(TextRole::Body.size())
                                    .color(text::PRIMARY),
                            )
                            .fill(if is_selected {
                                accent::PRIMARY.linear_multiply(0.15)
                            } else {
                                surface::WIDGET
                            })
                            .stroke(egui::Stroke::new(STROKE_WIDTH, border::DEFAULT))
                            .corner_radius(RADIUS_M)
                            .min_size(egui::vec2(ui.available_width(), sp.base.row_m)),
                        );
                        if resp.clicked() {
                            commands.push(item.action.clone());
                            self.ui_store.command_palette_query.clear();
                            body_close = true;
                        }
                    }
                });
            }

            close || body_close
        });

        if !open {
            self.ui_store.view.command_palette_open = false;
        }

        for action in commands {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }

    fn build_palette_items(&self) -> Vec<PaletteItem> {
        let mut items = Vec::new();
        let has_selection = !self.ui_store.selection.selected_actors.is_empty();

        items.push(PaletteItem {
            label: "Save".into(),
            icon: egui_phosphor::regular::FLOPPY_DISK,
            action: DocumentCommand::Save.into(),
            keywords: "save file disk",
        });
        items.push(PaletteItem {
            label: "Reload".into(),
            icon: egui_phosphor::regular::ARROW_CLOCKWISE,
            action: DocumentCommand::Reload.into(),
            keywords: "reload refresh",
        });
        items.push(PaletteItem {
            label: "Rebuild".into(),
            icon: egui_phosphor::regular::ARROWS_CLOCKWISE,
            action: DocumentCommand::Rebuild.into(),
            keywords: "rebuild compile",
        });
        items.push(PaletteItem {
            label: "Export…".into(),
            icon: egui_phosphor::regular::DOWNLOAD,
            action: ShellAction::View(ViewAction::OpenExportDialog),
            keywords: "export render video gif image",
        });
        items.push(PaletteItem {
            label: "Toggle Playback".into(),
            icon: egui_phosphor::regular::PLAY,
            action: PlaybackCommand::TogglePlayback.into(),
            keywords: "play pause playback",
        });
        items.push(PaletteItem {
            label: "Undo".into(),
            icon: egui_phosphor::regular::ARROW_U_UP_LEFT,
            action: DocumentCommand::Undo.into(),
            keywords: "undo revert",
        });
        items.push(PaletteItem {
            label: "Redo".into(),
            icon: egui_phosphor::regular::ARROW_U_UP_RIGHT,
            action: DocumentCommand::Redo.into(),
            keywords: "redo forward",
        });

        if has_selection {
            items.push(PaletteItem {
                label: "Delete Selected Actors".into(),
                icon: egui_phosphor::regular::TRASH,
                action: ActorCommand::DeleteSelectedActors.into(),
                keywords: "delete remove actors",
            });
            items.push(PaletteItem {
                label: "Duplicate Selected Actors".into(),
                icon: egui_phosphor::regular::COPY,
                action: ActorCommand::DuplicateSelectedActors.into(),
                keywords: "duplicate copy actors",
            });
            items.push(PaletteItem {
                label: "Group Selected Actors".into(),
                icon: egui_phosphor::regular::SQUARES_FOUR,
                action: ActorCommand::GroupSelectedActors.into(),
                keywords: "group container",
            });
            items.push(PaletteItem {
                label: "Zoom to Selection".into(),
                icon: egui_phosphor::regular::MAGNIFYING_GLASS_PLUS,
                action: ViewCommand::ZoomToSelection.into(),
                keywords: "zoom fit selection",
            });
        }

        // Align / Distribute commands (requires selection)
        if has_selection {
            items.push(PaletteItem {
                label: "Align Left".into(),
                icon: egui_phosphor::regular::ALIGN_LEFT,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Left).into(),
                keywords: "align left actors selection",
            });
            items.push(PaletteItem {
                label: "Align Center".into(),
                icon: egui_phosphor::regular::ALIGN_CENTER_HORIZONTAL_SIMPLE,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Center).into(),
                keywords: "align center horizontal actors",
            });
            items.push(PaletteItem {
                label: "Align Right".into(),
                icon: egui_phosphor::regular::ALIGN_RIGHT,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Right).into(),
                keywords: "align right actors",
            });
            items.push(PaletteItem {
                label: "Align Top".into(),
                icon: egui_phosphor::regular::ALIGN_TOP,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Top).into(),
                keywords: "align top actors",
            });
            items.push(PaletteItem {
                label: "Align Middle".into(),
                icon: egui_phosphor::regular::ALIGN_CENTER_VERTICAL_SIMPLE,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Middle).into(),
                keywords: "align middle vertical actors",
            });
            items.push(PaletteItem {
                label: "Align Bottom".into(),
                icon: egui_phosphor::regular::ALIGN_BOTTOM,
                action: ActorCommand::AlignActors(crate::app::commands::Align::Bottom).into(),
                keywords: "align bottom actors",
            });
            items.push(PaletteItem {
                label: "Distribute Horizontally".into(),
                icon: egui_phosphor::regular::ARROWS_OUT_LINE_HORIZONTAL,
                action: ActorCommand::DistributeActors(crate::app::commands::Axis::Horizontal)
                    .into(),
                keywords: "distribute horizontal evenly space actors",
            });
            items.push(PaletteItem {
                label: "Distribute Vertically".into(),
                icon: egui_phosphor::regular::ARROWS_OUT_LINE_VERTICAL,
                action: ActorCommand::DistributeActors(crate::app::commands::Axis::Vertical).into(),
                keywords: "distribute vertical evenly space actors",
            });
        }

        items.push(PaletteItem {
            label: "Zoom to Fit All".into(),
            icon: egui_phosphor::regular::ARROWS_IN,
            action: ViewCommand::ZoomToAll.into(),
            keywords: "zoom fit all",
        });

        items
    }
}
