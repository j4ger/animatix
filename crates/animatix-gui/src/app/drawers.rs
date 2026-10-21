//! Compact-mode overlay drawers for narrow-screen layouts.
//!
//! Provides the slide-out drawers for Sidebar and Detail (Inspector/Code)
//! that take over when the window is too narrow for docked side panels.

use egui::Stroke;

use super::{GuiShell, WorkspaceTab};
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

        egui::Area::new(egui::Id::new("compact_sidebar_drawer"))
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
                            ui.id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(crate::app::panels::sidebar_tab_label(
                                    active_tab,
                                ))
                                .size(TextRole::Heading.size())
                                .color(theme.palette.text.primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            Button::icon(egui_phosphor::regular::X)
                                                .with_tooltip("Close (Esc)"),
                                        )
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        let timeline = self.document_store.source.document.timeline.as_ref();
                        let asset_cache = timeline.map(|t| t.asset_cache());
                        let mut ctx = panels::sidebar::SidebarContext {
                            active_scene: self
                                .document_store
                                .source
                                .document
                                .active_scene
                                .as_deref(),
                            is_composition: self.document_store.source.document.is_composition(),
                            composition: self.document_store.source.document.composition.as_ref(),
                            current_file: &self.document_store.source.document.file_path,
                            expanded_dirs: &mut self.workspace_store.expanded_dirs,
                            file_tree: &self.workspace_store.file_tree,
                            preview: &mut self.preview_store.preview,
                            commands,
                            scene_dimensions: self.document_store.source.document.scene_dimensions,
                            timeline,
                            selected_actors: &mut self.ui_store.selection.selected_actors,
                            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
                            sidebar_tab: &mut self.ui_store.sidebar_tab,
                            editor: &mut self.document_store.source.editor,
                            components: &self.document_store.source.document.components,
                            asset_cache,
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

    /// Compact-mode detail overlay drawer (Inspector or Code).
    ///
    /// The docked detail column is hidden while compact, so this is the only
    /// place the active detail tab renders — no double render. The tab is shared
    /// with the dock tree, so toolbar/preset switches land here too.
    pub(crate) fn compact_detail_drawer_ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: egui::Rect,
        commands: &mut ActionQueue,
    ) {
        if self.ui_store.view.compact_drawer != Some(panels::CompactDrawer::Detail) {
            return;
        }
        // Fall back to Inspector if the tree has no active detail tab.
        let active_tab = crate::app::persistence::active_detail_tab(&self.ui_store.view.tree)
            .unwrap_or(WorkspaceTab::Inspector);

        let theme = eparts::theme(ui);
        let width = (screen.width() * 0.42).clamp(260.0, 420.0);
        let margin = 8.0;
        let mut close = false;
        let mut switch_to: Option<WorkspaceTab> = None;

        egui::Area::new(egui::Id::new("compact_detail_drawer"))
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
                            ui.id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            for (tab, label) in [
                                (WorkspaceTab::Inspector, "Inspector"),
                                (WorkspaceTab::Code, "Code"),
                            ] {
                                let active = active_tab == tab;
                                let resp = ui.add(Button::ghost(label).active(active));
                                if resp.clicked() && !active {
                                    switch_to = Some(tab);
                                }
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            Button::icon(egui_phosphor::regular::X)
                                                .with_tooltip("Close (Esc)"),
                                        )
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        match active_tab {
                            WorkspaceTab::Code => {
                                let diagnostics = self.document_store.combined_diagnostics();
                                let mut ctx = panels::editor::EditorContext {
                                    editor: &mut self.document_store.source.editor,
                                    diagnostics: &diagnostics,
                                    source_dirty: &mut self
                                        .document_store
                                        .source
                                        .document
                                        .source_text,
                                    commands,
                                    is_playing: self.preview_store.preview.playback.is_playing,
                                };
                                panels::editor::editor_ui(&mut ctx, ui);
                            },
                            _ => {
                                let active_tl =
                                    self.document_store.source.document.active_timeline();
                                let mut ctx = panels::inspector::InspectorContext {
                                    preview: &mut self.preview_store.preview,
                                    timeline: active_tl,
                                    composition: self
                                        .document_store
                                        .source
                                        .document
                                        .composition
                                        .as_ref(),
                                    active_scene: self
                                        .document_store
                                        .source
                                        .document
                                        .active_scene
                                        .as_deref(),
                                    selected_actors: &mut self.ui_store.selection.selected_actors,
                                    commands,
                                    keyframe_mode: self.ui_store.keyframe_mode,
                                    scene_dimensions: self
                                        .document_store
                                        .source
                                        .document
                                        .scene_dimensions,
                                    pivot_offsets: &mut self.ui_store.pivot_offsets,
                                    property_view_mode: &mut self.ui_store.property_view_mode,
                                    keyframe_view_mode: &mut self.ui_store.keyframe_view_mode,
                                };
                                panels::inspector::inspector_panel_ui(&mut ctx, ui);
                            },
                        }
                    });
            });

        if let Some(tab) = switch_to {
            crate::app::persistence::set_active_detail_tab(&mut self.ui_store.view.tree, tab);
        }
        if close || (!self.modal_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.ui_store.view.compact_drawer = None;
        }
    }
}
