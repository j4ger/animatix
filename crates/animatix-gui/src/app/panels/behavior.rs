use std::collections::HashSet;

use egui::{Color32, RichText, Stroke, Visuals};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};

use crate::app::WorkspaceTab;
use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::{RADIUS_M, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::interaction::keyboard::ShortcutRegistry;
use crate::app::panels::{curves_panel, editor, inspector, preview_panel, sidebar, timeline_panel};
use crate::app::preview::selection;
use crate::app::stores::{DocumentStore, PreviewStore, WorkspaceStore};

pub(crate) struct WorkspaceBehavior<'a> {
    pub(crate) voiceover_history: &'a crate::app::document::narration::VoiceoverHistory,
    pub(crate) document_store: &'a mut DocumentStore,
    pub(crate) workspace_store: &'a mut WorkspaceStore,
    pub(crate) preview_store: &'a mut PreviewStore,
    pub(crate) commands: &'a mut ActionQueue,
    pub(crate) preview_texture_id: Option<egui::TextureId>,
    pub(crate) shortcut_registry: &'a ShortcutRegistry,
    pub(crate) collapsed_actors: &'a mut HashSet<String>,
    pub(crate) expanded_properties: &'a mut HashSet<String>,
    pub(crate) selected_actors: &'a mut HashSet<String>,
    pub(crate) hit_regions: &'a [(String, kurbo::Rect)],
    pub(crate) selection: &'a mut selection::SelectionState,
    pub(crate) selected_keyframes: &'a mut Vec<crate::app::document::timeline_diff::KeyframeId>,
    pub(crate) sidebar_tab: &'a mut crate::app::panels::SidebarTab,
    /// True when the window is narrow enough for the compact sidebar icon rail.
    pub(crate) compact: bool,
    /// Compact-mode overlay drawer slot (opened by the sidebar rail).
    pub(crate) compact_drawer: &'a mut Option<crate::app::panels::CompactDrawer>,
    pub(crate) preview_fullscreen: bool,
    pub(crate) language: crate::app::i18n::Language,
    pub(crate) debug_layout: bool,
    pub(crate) debug_spacing: bool,
}

impl<'a> Behavior<WorkspaceTab> for WorkspaceBehavior<'a> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut WorkspaceTab,
    ) -> UiResponse {
        match pane {
            WorkspaceTab::Sidebar => {
                let mut ctx = sidebar::SidebarContext {
                    document: &self.document_store.source.document,
                    expanded_dirs: &self.workspace_store.expanded_dirs,
                    file_tree: &self.workspace_store.file_tree,
                    file_tree_revision: self.workspace_store.file_tree_revision,
                    file_tree_loading: self.workspace_store.file_tree_loader.pending(),
                    commands: self.commands,
                    selected_actors: self.selected_actors,
                    collapsed_actors: self.collapsed_actors,
                    sidebar_tab: self.sidebar_tab,
                    compact: self.compact,
                    compact_drawer: self.compact_drawer,
                };
                sidebar::sidebar_ui(&mut ctx, ui);
            },
            WorkspaceTab::Code => {
                // The single shared source editor.
                let diagnostics = self.document_store.combined_diagnostics();
                let mut ctx = editor::EditorContext {
                    fps: self.preview_store.preview.playback.fps,
                    playhead_time: self.preview_store.preview.playback.current_time_s(),
                    voiceover_history: self.voiceover_history,
                    source_epoch: self.document_store.source.epoch(),
                    editor: &mut self.document_store.source.editor,
                    diagnostics: &diagnostics,
                    document: &self.document_store.source.document,
                    commands: self.commands,
                };
                editor::editor_ui(&mut ctx, ui);
            },
            WorkspaceTab::Preview => {
                let active_tl = self.document_store.source.document.active_timeline();
                let has_scene = active_tl.map(|tl| !tl.tracks().is_empty()).unwrap_or(false);
                let source_current = !self.preview_store.has_pending_rebuild()
                    && !self.preview_store.rebuild_in_progress
                    && self.preview_store.in_flight_rebuild.is_none()
                    && self.document_store.source.document.last_rebuild_error.is_none();
                let mut ctx = preview_panel::PreviewContext {
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                    preview: &mut self.preview_store.preview,
                    preview_texture_id: self.preview_texture_id,
                    commands: self.commands,
                    selection: self.selection,
                    selected_actors: self.selected_actors,
                    hit_regions: self.hit_regions,
                    timeline: active_tl,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    performance_metrics: &mut self.preview_store.performance_metrics,
                    debug_layout: self.debug_layout,
                    preview_fullscreen: self.preview_fullscreen,
                    debug_spacing: self.debug_spacing,
                    rebuild_in_progress: self.preview_store.rebuild_in_progress,
                    has_scene,
                    source_text: source_current
                        .then_some(self.document_store.source.document.source_text.as_str()),
                };
                preview_panel::preview_panel_ui(&mut ctx, ui, self.shortcut_registry);
            },
            WorkspaceTab::Inspector => {
                ui.horizontal(|ui| {
                    ui.strong(crate::app::i18n::text(ui, "gui-panel-inspector"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                eparts::Button::icon(egui_phosphor::regular::PUSH_PIN_SLASH)
                                    .with_tooltip(crate::app::i18n::text(
                                        ui,
                                        "gui-inspector-unpin",
                                    )),
                            )
                            .clicked()
                        {
                            self.commands.push_back(crate::app::commands::ShellAction::View(
                                crate::app::commands::ViewAction::UnpinInspector,
                            ));
                        }
                    });
                });
                let active_tl = self.document_store.source.document.active_timeline();
                let mut ctx = inspector::InspectorContext {
                    preview: &self.preview_store.preview,
                    timeline: active_tl,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                    raw_statements: self.document_store.source.document.raw_statements.as_deref(),
                    source_path: &self.document_store.source.document.file_path,
                    source_text: &self.document_store.source.document.source_text,
                    source_current: !self.preview_store.has_pending_rebuild()
                        && !self.preview_store.rebuild_in_progress
                        && self.preview_store.in_flight_rebuild.is_none()
                        && self.document_store.source.document.last_rebuild_error.is_none(),
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                };
                inspector::inspector_panel_ui(&mut ctx, ui);
            },
            WorkspaceTab::Timeline => {
                // Resolve the active timeline using the centralized API.
                let resolved_timeline = self.document_store.source.document.active_timeline();
                // Populate hot-path caches if stale (use free fn to avoid borrow conflict)
                if !self.document_store.source.cache_valid {
                    crate::app::source_store::rebuild_cache(
                        &mut self.document_store.source.cached_hit_regions,
                        &mut self.document_store.source.cached_actor_bounds,
                        &mut self.document_store.source.cache_valid,
                        resolved_timeline,
                    );
                }
                let active_scene =
                    self.document_store.source.document.active_scene.as_deref().or_else(|| {
                        self.document_store
                            .source
                            .document
                            .composition
                            .as_ref()
                            .and_then(|c| c.declaration_order.first().map(String::as_str))
                    });
                let audio_segments = self.document_store.source.document.all_audio_segments();
                let audio_revision = self
                    .document_store
                    .source
                    .document
                    .audio_waveform_revision(self.document_store.generation.0);
                let mut ctx = timeline_panel::TimelineContext {
                    voiceover_history: self.voiceover_history,
                    anchor_changes: self.document_store.source.editor.narration_anchor_changes(),
                    diagnostics: &self.document_store.source.document.diagnostics,
                    source_epoch: self.document_store.source.epoch(),
                    source_text: &self.document_store.source.document.source_text,
                    named_cues: self.document_store.source.document.source_timing.named_cues(),
                    source_current: !self.preview_store.has_pending_rebuild()
                        && !self.preview_store.rebuild_in_progress
                        && self.preview_store.in_flight_rebuild.is_none()
                        && self.document_store.source.document.last_rebuild_error.is_none(),
                    statements: self
                        .document_store
                        .source
                        .document
                        .raw_statements
                        .as_deref()
                        .unwrap_or(&[]),
                    audio_segments: &audio_segments,
                    audio_revision,
                    preview: &mut self.preview_store.preview,
                    timeline: resolved_timeline,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene,
                    commands: self.commands,
                    collapsed_actors: self.collapsed_actors,
                    expanded_properties: self.expanded_properties,
                    selected_actors: self.selected_actors,
                    selected_keyframes: self.selected_keyframes,
                };
                timeline_panel::render_timeline_content(&mut ctx, ui);
            },
            WorkspaceTab::Curves => {
                let resolved_timeline = self.document_store.source.document.active_timeline();
                let active_scene =
                    self.document_store.source.document.active_scene.as_deref().or_else(|| {
                        self.document_store
                            .source
                            .document
                            .composition
                            .as_ref()
                            .and_then(|c| c.declaration_order.first().map(String::as_str))
                    });
                let composition = self.document_store.source.document.composition.as_ref();
                let scene =
                    composition.and_then(|c| active_scene.and_then(|name| c.scenes.get(name)));
                let mut ctx = curves_panel::CurvesContext {
                    preview: &self.preview_store.preview,
                    timeline: resolved_timeline,
                    scene_offset_s: composition
                        .and_then(|c| active_scene.and_then(|name| c.scene_start_times.get(name)))
                        .copied()
                        .unwrap_or(0.0),
                    scene_duration_s: scene
                        .map(|scene| scene.duration_s)
                        .unwrap_or(self.preview_store.preview.playback.duration_s),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                };
                curves_panel::curves_panel_ui(&mut ctx, ui);
            },
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &WorkspaceTab) -> egui::WidgetText {
        let title = match pane {
            WorkspaceTab::Sidebar => "gui-panel-project",
            WorkspaceTab::Code => "gui-editor-title",
            WorkspaceTab::Preview => "gui-panel-preview",
            WorkspaceTab::Inspector => "gui-panel-inspector",
            WorkspaceTab::Timeline => "gui-panel-timeline",
            WorkspaceTab::Curves => "gui-panel-curves",
        };
        RichText::new(crate::app::i18n::text_for(self.language, title))
            .font(TextRole::BodyMedium.font_id())
            .into()
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: false,
            prune_single_child_tabs: false,
            ..Default::default()
        }
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        3.0
    }

    /// Pixel floor for every tile: panes never collapse into unusable slivers
    /// (design doc §9 "px 兜底").
    fn min_size(&self) -> f32 {
        // The compact sidebar is intentionally a narrow rail. A larger global
        // tile floor overrides its explicit share and leaves a blank column.
        if self.compact {
            super::COMPACT_RAIL_WIDTH
        } else {
            120.0
        }
    }

    fn is_tile_draggable(
        &self,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
    ) -> bool {
        false
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        38.0
    }

    fn tab_bar_color(&self, visuals: &Visuals) -> Color32 {
        visuals.panel_fill
    }

    // Comparison tabs retain the tile's interaction ID and full click target.
    fn tab_ui(
        &mut self,
        tiles: &mut egui_tiles::Tiles<WorkspaceTab>,
        ui: &mut egui::Ui,
        id: egui::Id,
        tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Response {
        use egui_phosphor::regular as icons;
        let theme = eparts::theme(ui);
        let sp = spatial(ui).base;
        let title = self.tab_title_for_tile(tiles, tile_id);
        let icon = match tiles.get(tile_id) {
            Some(egui_tiles::Tile::Pane(tab)) => match tab {
                WorkspaceTab::Sidebar => icons::FOLDER,
                WorkspaceTab::Code => icons::CODE,
                WorkspaceTab::Preview => icons::CORNERS_OUT,
                WorkspaceTab::Inspector => icons::FADERS_HORIZONTAL,
                WorkspaceTab::Timeline => icons::FILM_STRIP,
                WorkspaceTab::Curves => icons::BEZIER_CURVE,
            },
            _ => icons::STACK,
        };
        let label = ui.painter().layout_no_wrap(
            title.text().to_owned(),
            TextRole::BodyMedium.font_id(),
            Color32::PLACEHOLDER,
        );
        let icon_w = sp.component.icon_slot_width;
        let width = sp.space_5 * 2.0 + icon_w + sp.space_3 + label.size().x;
        let (_, hit) = ui.allocate_space(egui::vec2(width, ui.available_height()));
        let draggable = self.is_tile_draggable(tiles, tile_id);
        let response = ui.interact(
            hit,
            id,
            if draggable {
                egui::Sense::click_and_drag()
            } else {
                egui::Sense::click()
            },
        );
        let response = if draggable {
            response.on_hover_cursor(self.tab_hover_cursor_icon())
        } else {
            response
        };
        response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Tab, ui.is_enabled(), state.active, title.text())
        });
        if ui.is_rect_visible(hit) && !state.is_being_dragged {
            let face = hit.shrink2(egui::vec2(sp.space_1, sp.space_2));
            let slots = theme.components.tab;
            let slot = if state.active {
                slots.active
            } else if response.hovered() {
                slots.hover
            } else {
                slots.inactive
            };
            let hover = crate::app::components::anim::animate_bool_eased(
                ui.ctx(),
                id.with("hover"),
                response.hovered(),
                eparts::tokens::motion::HOVER,
            );
            let selected = crate::app::components::anim::animate_bool_eased(
                ui.ctx(),
                id.with("selection"),
                state.active,
                eparts::tokens::motion::HOVER,
            );
            if hover > 0.0 || selected > 0.0 {
                let fill = eparts::tokens::util::lerp_color(
                    slots.hover.bg.gamma_multiply(hover),
                    slots.active.bg,
                    selected,
                );
                ui.painter().rect_filled(face, RADIUS_M, fill);
            }
            if response.has_focus() {
                ui.painter().rect_stroke(
                    face,
                    RADIUS_M,
                    Stroke::new(STROKE_WIDTH, theme.focus_ring()),
                    egui::StrokeKind::Inside,
                );
            }
            let left = hit.left() + sp.space_5;
            let icon_center = egui::pos2(left + icon_w * 0.5, hit.center().y);
            ui.painter().text(
                icon_center,
                egui::Align2::CENTER_CENTER,
                icon,
                TextRole::Title.font_id(),
                if state.active {
                    slot.indicator
                } else {
                    slot.fg
                },
            );
            let text_pos =
                egui::pos2(left + icon_w + sp.space_3, hit.center().y - label.size().y * 0.5);
            ui.painter().galley_with_override_text_color(text_pos, label, slot.fg);
        }
        self.on_tab_button(tiles, tile_id, response)
    }

    fn tab_bar_hline_stroke(&self, _visuals: &Visuals) -> Stroke {
        Stroke::NONE
    }

    fn resize_stroke(&self, style: &egui::Style, resize_state: egui_tiles::ResizeState) -> Stroke {
        let accent = style.visuals.hyperlink_color;
        match resize_state {
            egui_tiles::ResizeState::Idle => {
                Stroke::new(STROKE_WIDTH, style.visuals.widgets.noninteractive.bg_stroke.color)
            },
            egui_tiles::ResizeState::Hovering => Stroke::new(STROKE_WIDTH, accent),
            egui_tiles::ResizeState::Dragging => Stroke::new(STROKE_WIDTH, accent),
        }
    }

    fn drag_preview_stroke(&self, visuals: &Visuals) -> Stroke {
        Stroke::new(STROKE_WIDTH, visuals.hyperlink_color)
    }

    fn drag_preview_color(&self, visuals: &Visuals) -> Color32 {
        visuals.selection.bg_fill
    }
}
