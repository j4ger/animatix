//! Modal dialogs and overlay screens for the application shell.
//!
//! Includes the welcome/onboarding screen, transient diagnostics peek overlay,
//! workspace switcher dialog, and unsaved changes confirmation dialog.

use std::path::PathBuf;

use animatix_syntax::diagnostics::Diagnostic;
use egui::Stroke;

use super::GuiShell;
use crate::app::commands::{ActionQueue, DocumentCommand, ViewCommand};
use crate::app::components::button::Button;
use crate::app::components::toast::Toast;
use crate::app::components::{dialog, text_tooltip};
use crate::app::design_tokens::spatial::welcome::TOP_OFFSET_FRAC as WELCOME_TOP_OFFSET_FRAC;
use crate::app::design_tokens::spatial::{
    RADIUS_L, ROW_L, SPACE_2, SPACE_3, SPACE_5, STROKE_WIDTH, spatial,
};
use crate::app::design_tokens::typography::TextRole;
use crate::app::handlers::file;
use crate::document::default_file_path;

impl GuiShell {
    /// Welcome / onboarding screen shown when no document is loaded.
    pub(crate) fn welcome_screen_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let theme = eparts::theme(ui);
        let sp = spatial(ui);
        let avail = ui.available_rect_before_wrap();
        ui.painter().rect_filled(avail, 0.0, theme.palette.surface.base);

        ui.vertical_centered(|ui| {
            ui.add_space(avail.height() * WELCOME_TOP_OFFSET_FRAC);

            // ── Centered card ──
            egui::Frame::new()
                .fill(theme.palette.surface.surface)
                .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                .corner_radius(RADIUS_L)
                .inner_margin(egui::Margin::symmetric(40, 36))
                .show(ui, |ui| {
                    ui.set_max_width(280.0);

                    ui.vertical_centered(|ui| {
                        // Icon with circular background
                        let icon_size = 56.0;
                        let (icon_rect, _) = ui.allocate_exact_size(
                            egui::vec2(icon_size, icon_size),
                            egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(
                            icon_rect.center(),
                            icon_size * 0.5,
                            theme.palette.surface.widget,
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::FILM_STRIP,
                            TextRole::Display.font_id(),
                            theme.palette.accent.primary,
                        );
                        ui.add_space(sp.base.space_5 * 1.5);

                        // Title
                        ui.label(
                            egui::RichText::new("Welcome to Animatix")
                                .font(TextRole::Heading.font_id())
                                .color(theme.palette.text.primary)
                                .strong(),
                        );
                        ui.add_space(sp.base.space_2);

                        // Subtitle
                        ui.label(
                            egui::RichText::new("Layout-first animation for creative coders")
                                .size(TextRole::Body.size())
                                .color(theme.palette.text.secondary),
                        );
                        ui.add_space(sp.base.space_5 * 2.5);

                        let btn_w = ui.available_width();

                        // Primary: Create new scene
                        let new_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::primary("Create new scene")
                                .with_icon(egui_phosphor::regular::PLUS),
                        );
                        if new_resp.clicked() {
                            let path = default_file_path();
                            match std::fs::write(&path, "#0s\n") {
                                Ok(_) => {},
                                Err(e) => {
                                    self.ui_store
                                        .toasts
                                        .push(Toast::error(format!("Failed to create scene: {e}")));
                                    return; // don't proceed to open
                                },
                            }
                            commands.push_back(DocumentCommand::OpenFile(path).into());
                        }

                        ui.add_space(sp.base.space_3);

                        // Secondary: open existing file
                        let open_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::ghost("Open existing file")
                                .with_icon(egui_phosphor::regular::FOLDER_OPEN),
                        );
                        if open_resp.clicked() {
                            if let Some(path) =
                                rfd::FileDialog::new().add_filter("Animatix", &["amx"]).pick_file()
                            {
                                commands.push_back(DocumentCommand::OpenFile(path).into());
                            }
                        }

                        ui.add_space(sp.base.space_3);

                        // Tertiary: open workspace
                        let ws_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::ghost("Open workspace")
                                .with_icon(egui_phosphor::regular::FOLDER_NOTCH),
                        );
                        if ws_resp.clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                                self.ui_store.view.welcome_open = false;
                            }
                        }
                    });
                });
        });
    }

    /// Transient diagnostics overlay raised from the status-bar chip.
    ///
    /// Anchored above the chip so the counts stay readable without docking the
    /// bottom panel. Transient by design: Escape, a click outside, or picking a
    /// diagnostic (which also scrolls the editor to it) closes it.
    pub(crate) fn diagnostics_peek_ui(
        &mut self,
        ui: &mut egui::Ui,
        chip_rect: egui::Rect,
        diagnostics: &[Diagnostic],
    ) {
        let theme = eparts::theme(ui);
        let screen = ui.ctx().viewport_rect();
        let width = 440.0f32.min(screen.width() - 16.0);
        // Anchor above the chip, nudged inside the screen if it would overflow.
        let x = (chip_rect.right() - width).max(screen.left() + 8.0);
        let y = (chip_rect.top() - 12.0).max(screen.top() + 8.0);
        let errors = diagnostics.iter().filter(|d| d.is_error()).count();
        let warnings = diagnostics.len() - errors;
        let mut close = false;
        let mut scroll_to: Option<(usize, usize)> = None;

        let overlay = egui::Area::new(egui::Id::new("diagnostics_peek"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        ui.horizontal(|ui| {
                            let (icon, color) = if errors > 0 {
                                (egui_phosphor::regular::X_CIRCLE, theme.palette.status.error)
                            } else if warnings > 0 {
                                (egui_phosphor::regular::WARNING, theme.palette.status.warning)
                            } else {
                                (egui_phosphor::regular::CHECK_CIRCLE, theme.palette.status.success)
                            };
                            ui.label(
                                egui::RichText::new(icon).size(TextRole::Body.size()).color(color),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{errors} error(s) · {warnings} warning(s)"
                                ))
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let pin = ui.add(
                                        Button::ghost("")
                                            .with_icon(egui_phosphor::regular::ARROW_SQUARE_DOWN),
                                    );
                                    text_tooltip(
                                        ui,
                                        pin.id.with("diag_peek_pin"),
                                        &pin,
                                        "Pin to the bottom panel",
                                    );
                                    if pin.clicked() {
                                        self.ui_store.view.diagnostics_panel_visible = true;
                                        close = true;
                                    }
                                    let dismiss = ui.add(
                                        Button::ghost("").with_icon(egui_phosphor::regular::X),
                                    );
                                    if dismiss.clicked() {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();
                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            if diagnostics.is_empty() {
                                ui.add_space(SPACE_2);
                                ui.label(
                                    egui::RichText::new("No diagnostics — all clear")
                                        .size(TextRole::BodyS.size())
                                        .color(theme.palette.status.success),
                                );
                            } else if let Some(target) =
                                crate::app::components::diagnostics::diagnostics_list(
                                    ui,
                                    diagnostics,
                                    &mut close,
                                )
                            {
                                scroll_to = Some((target.line, target.column));
                                close = true;
                            }
                        });
                    });
            })
            .response;

        if let Some((line, column)) = scroll_to {
            self.ui_store
                .pending_actions
                .push_back(ViewCommand::ScrollToLine(line, column).into());
        }

        let escape = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        let clicked_outside = ui.ctx().input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !overlay.rect.contains(p) && !chip_rect.contains(p))
        });
        if close || escape || clicked_outside {
            self.ui_store.view.diagnostics_peek_open = false;
        }
    }

    /// Workspace switcher dialog — small centered window for typing a directory path.
    pub(crate) fn workspace_switcher_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let spec = dialog::DialogSpec::new("workspace_switcher", [400.0, 140.0])
            .with_min_size([360.0, 120.0]);

        let mut commands = ActionQueue::default();
        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let title_close = dialog::title_row(ui, "Switch Workspace");
            let mut body_close = false;

            ui.add_space(SPACE_3);
            ui.separator();
            ui.add_space(SPACE_3);

            ui.label(
                egui::RichText::new("Directory path")
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.secondary),
            );
            ui.add_space(SPACE_2);
            eparts::TextField::new(&mut self.ui_store.workspace_switcher_path)
                .placeholder("/path/to/workspace")
                .show(ui);
            ui.add_space(SPACE_3);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm = ui.add_sized([80.0, 28.0], Button::primary("Switch"));
                    if confirm.clicked() {
                        let path = PathBuf::from(&self.ui_store.workspace_switcher_path);
                        commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                        body_close = true;
                    }

                    let cancel = ui.add_sized([80.0, 28.0], Button::ghost("Cancel"));
                    if cancel.clicked() {
                        body_close = true;
                    }
                });
            });

            title_close || body_close
        });

        if !open {
            self.ui_store.view.workspace_switcher_open = false;
        }

        for cmd in commands {
            let effects = self.handle_action(cmd);
            self.apply_effects(effects);
        }
    }

    /// Confirmation dialog for unsaved changes (Save / Discard / Cancel).
    pub(crate) fn unsaved_changes_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let spec = dialog::DialogSpec::new("unsaved_changes", [400.0, 200.0])
            .with_min_size([360.0, 180.0]);

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let title_close = dialog::title_row(
                ui,
                &format!("{}  Unsaved changes", egui_phosphor::regular::FLOPPY_DISK),
            );
            let mut body_close = false;

            ui.add_space(SPACE_3);
            ui.separator();
            ui.add_space(SPACE_3);

            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.ui_store.unsaved_changes.message)
                        .size(TextRole::Body.size())
                        .color(theme.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_5);

            let mut save_failed = false;
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Save button
                    let save = ui.add_sized(
                        [90.0, ROW_L],
                        Button::primary("Save").with_icon(egui_phosphor::regular::FLOPPY_DISK),
                    );
                    if save.clicked() {
                        if self.recovery_prompt_pending() {
                            // Saving here would clear a sidecar the user has not
                            // decided on yet; make them resolve that first.
                            self.ui_store.toasts.push(self.recovery_prompt_save_blocked());
                            save_failed = true;
                        } else if let Err(err) = file::save_document(&mut self.document_store) {
                            // Keep the dialog open if saving fails so unsaved edits are not lost.
                            self.preview_store
                                .preview
                                .set_status_error(format!("Save failed: {err}"));
                            self.ui_store.toasts.push(Toast::error(format!("Save failed: {err}")));
                            save_failed = true;
                        } else {
                            let was_close = self.ui_store.unsaved_changes.pending_close;
                            self.execute_unsaved_pending_action();
                            self.ui_store.unsaved_changes.close();
                            if was_close {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            body_close = true;
                        }
                    }

                    // Discard button
                    let discard = ui.add_sized(
                        [90.0, ROW_L],
                        Button::danger("Discard").with_icon(egui_phosphor::regular::TRASH),
                    );
                    if discard.clicked() {
                        // Mark document as no longer dirty, then execute pending
                        self.document_store.source.document.is_dirty = false;
                        // The user explicitly threw the edits away, so the
                        // recovery sidecar must not resurrect them. Skipped when
                        // a recovery prompt is still pending, since that sidecar
                        // is a separate, undecided copy.
                        self.clear_recovery_for_current_document();
                        let was_close = self.ui_store.unsaved_changes.pending_close;
                        self.execute_unsaved_pending_action();
                        self.ui_store.unsaved_changes.close();
                        if was_close {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        body_close = true;
                    }

                    // Cancel button
                    let cancel = ui.add_sized([90.0, ROW_L], Button::ghost("Cancel"));
                    if cancel.clicked() {
                        self.ui_store.unsaved_changes.close();
                        body_close = true;
                    }
                });
            });

            if save_failed {
                return false;
            }

            title_close || body_close
        });

        if !open {
            self.ui_store.unsaved_changes.close();
        }
    }

    /// Execute the pending action stored in the unsaved changes dialog.
    pub(crate) fn execute_unsaved_pending_action(&mut self) {
        if let Some(action) = self.ui_store.unsaved_changes.pending_action.take() {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }
}
