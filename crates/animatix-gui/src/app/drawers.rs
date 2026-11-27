//! Compact-mode overlay drawers for narrow-screen layouts.
//!
//! The Navigator becomes a drawer at narrow widths. Inspector can be opened
//! as a drawer at any width and pinned when there is room.

use egui::Stroke;

use super::GuiShell;
use crate::app::commands::ActionQueue;
use crate::app::components::button::Button;
use crate::app::design_tokens::spatial::{RADIUS_L, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels;

impl GuiShell {
    /// Compact-mode sidebar overlay drawer.
    ///
    /// Renders the same content as the docked sidebar (via
    /// `sidebar_tab_content_ui`) in a floating left panel, opened from the icon
    /// rail. Shown only while the drawer is the active compact drawer.
    pub(crate) fn compact_sidebar_drawer_ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: egui::Rect,
        commands: &mut ActionQueue,
    ) {
        if self.ui_store.view.compact_drawer != Some(panels::CompactDrawer::Sidebar) {
            return;
        }
        let theme = eparts::theme(ui);
        let width = (screen.width() * 0.40).clamp(240.0, 360.0);
        let margin = 8.0;
        let mut close = false;
        let active_tab = self.ui_store.sidebar_tab;

        egui::Area::new(egui::Id::unique("compact_sidebar_drawer"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.left() + margin, screen.top() + margin))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.set_max_height((screen.height() - 2.0 * margin).max(160.0));
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        // Swallow canvas drags underneath the drawer. Drag-only
                        // sense: egui prefers a smaller clickable widget over a
                        // big drag background, so the drawer's own controls
                        // still win the hit test.
                        let _ = ui.interact(
                            ui.max_rect(),
                            ui.scope_id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(crate::app::i18n::text(
                                    ui,
                                    crate::app::panels::sidebar_tab_message(active_tab),
                                ))
                                .size(TextRole::Heading.size())
                                .color(theme.palette.text.primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(Button::icon(egui_phosphor::regular::X).with_tooltip(
                                            crate::app::i18n::text(ui, "gui-action-close-esc"),
                                        ))
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        let mut ctx = panels::sidebar::SidebarContext {
                            document: &self.document_store.source.document,
                            expanded_dirs: &self.workspace_store.expanded_dirs,
                            file_tree: &self.workspace_store.file_tree,
                            file_tree_revision: self.workspace_store.file_tree_revision,
                            file_tree_loading: self.workspace_store.file_tree_loader.pending(),
                            commands,
                            selected_actors: &mut self.ui_store.selection.selected_actors,
                            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
                            sidebar_tab: &mut self.ui_store.sidebar_tab,
                            compact: false,
                            compact_drawer: &mut self.ui_store.view.compact_drawer,
                        };
                        panels::sidebar::sidebar_tab_content_ui(&mut ctx, ui, active_tab);
                    });
            });

        if close || (!self.modal_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.ui_store.view.compact_drawer = None;
        }
    }

    /// The unpinned Inspector, anchored to the right of the workbench.
    pub(crate) fn inspector_drawer_ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: egui::Rect,
        commands: &mut ActionQueue,
    ) {
        if self.ui_store.view.compact_drawer != Some(panels::CompactDrawer::Detail) {
            return;
        }
        let theme = eparts::theme(ui);
        let width = (screen.width() * 0.42).clamp(260.0, 420.0);
        let margin = 8.0;
        let mut close = false;
        let mut pin = false;
        let can_pin = !self.ui_store.view.compact;

        egui::Area::new(egui::Id::unique("compact_detail_drawer"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.right() - margin - width, screen.top() + margin))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.set_max_height((screen.height() - 2.0 * margin).max(160.0));
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        // Swallow canvas drags underneath the drawer (see the
                        // sidebar drawer for the rationale).
                        let _ = ui.interact(
                            ui.max_rect(),
                            ui.scope_id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(crate::app::i18n::text(
                                    ui,
                                    "gui-panel-inspector",
                                ))
                                .font(TextRole::Title.font_id()),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(Button::icon(egui_phosphor::regular::X).with_tooltip(
                                            crate::app::i18n::text(ui, "gui-action-close-esc"),
                                        ))
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                    if ui
                                        .add_enabled(
                                            can_pin,
                                            Button::icon(egui_phosphor::regular::PUSH_PIN)
                                                .with_tooltip(crate::app::i18n::text(
                                                    ui,
                                                    "gui-inspector-pin",
                                                )),
                                        )
                                        .clicked()
                                    {
                                        pin = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        let active_tl = self.document_store.source.document.active_timeline();
                        let mut ctx = panels::inspector::InspectorContext {
                            preview: &self.preview_store.preview,
                            timeline: active_tl,
                            composition: self.document_store.source.document.composition.as_ref(),
                            active_scene: self
                                .document_store
                                .source
                                .document
                                .active_scene
                                .as_deref(),
                            selected_actors: &mut self.ui_store.selection.selected_actors,
                            commands,
                            raw_statements: self
                                .document_store
                                .source
                                .document
                                .raw_statements
                                .as_deref(),
                            source_path: &self.document_store.source.document.file_path,
                            source_text: &self.document_store.source.document.source_text,
                            source_current: !self.preview_store.has_pending_rebuild()
                                && !self.preview_store.rebuild_in_progress
                                && self.preview_store.in_flight_rebuild.is_none()
                                && self.document_store.source.document.last_rebuild_error.is_none(),
                            scene_dimensions: self.document_store.source.document.scene_dimensions,
                        };
                        panels::inspector::inspector_panel_ui(&mut ctx, ui);
                    });
            });

        if pin {
            crate::app::persistence::show_inspector(&mut self.ui_store.view.tree);
            close = true;
        }
        if close || (!self.modal_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.ui_store.view.compact_drawer = None;
            if let Some(restore) = self.ui_store.view.compact_restore.as_mut() {
                restore.detail_visible = false;
            }
        }
    }
}
