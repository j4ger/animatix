use egui::{Color32, Rect, Stroke, Visuals};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};

use crate::app::WorkspaceTab;
use crate::app::design_tokens::*;
use crate::app::panels::{sidebar, editor, inspector_panel, timeline, preview_panel};
use crate::app::stores;
use crate::app::stores::{DocumentStore, WorkspaceStore, PreviewStore};
use crate::app::commands::CommandQueue;
use crate::app::preview;
use crate::app::preview::selection;
use std::collections::{HashMap, HashSet};

pub(crate) struct WorkspaceBehavior<'a> {
    pub(crate) document_store: &'a mut DocumentStore,
    pub(crate) workspace_store: &'a mut WorkspaceStore,
    pub(crate) preview_store: &'a mut PreviewStore,
    pub(crate) commands: &'a mut CommandQueue,
    pub(crate) preview_texture_id: Option<egui::TextureId>,
    pub(crate) collapsed_actors: &'a mut HashSet<String>,
    pub(crate) selected_actors: &'a mut HashSet<String>,
    pub(crate) hit_regions: &'a [(String, kurbo::Rect)],
    pub(crate) drag_state: &'a mut preview::DragState,
    pub(crate) selection: &'a mut selection::SelectionState,
    pub(crate) pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub(crate) tool_mode: &'a mut preview::ToolMode,
    pub(crate) sidebar_tab: &'a mut crate::app::panels::SidebarTab,
    pub(crate) property_view_mode: &'a mut crate::app::panels::inspector::PropertyViewMode,
    pub(crate) keyframe_view_mode: &'a mut crate::app::panels::inspector::KeyframeViewMode,
    pub(crate) keyframe_mode: bool,
    pub(crate) rotation_snap_degrees: f32,
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
                    active_scene: self.document_store.document.active_scene.as_deref(),
                    is_composition: self.document_store.document.is_composition(),
                    composition: self.document_store.document.composition.as_ref(),
                    current_file: &self.document_store.document.file_path,
                    expanded_dirs: &mut self.workspace_store.expanded_dirs,
                    file_tree: &self.workspace_store.file_tree,
                    preview: &mut self.preview_store.preview,
                    commands: self.commands,
                    scene_dimensions: self.document_store.document.scene_dimensions,
                    timeline: self.document_store.document.timeline.as_ref(),
                    selected_actors: self.selected_actors,
                    collapsed_actors: self.collapsed_actors,
                    sidebar_tab: self.sidebar_tab,
                };
                sidebar::sidebar_ui(&mut ctx, ui);
            }
            WorkspaceTab::Editor => {
                let diagnostics = self.document_store.combined_diagnostics();
                let mut ctx = editor::EditorContext {
                    editor: &mut self.document_store.editor,
                    diagnostics: &diagnostics,
                    source_dirty: &mut self.document_store.document.source_text,
                    commands: self.commands,
                    is_playing: self.preview_store.preview.playback.is_playing,
                };
                editor::editor_ui(&mut ctx, ui);
            }
            WorkspaceTab::Preview => {
                let mut ctx = preview_panel::PreviewContext {
                    scene_dimensions: self.document_store.document.scene_dimensions,
                    preview: &mut self.preview_store.preview,
                    preview_texture_id: self.preview_texture_id,
                    commands: self.commands,
                    drag_state: self.drag_state,
                    selection: self.selection,
                    selected_actors: self.selected_actors,
                    hit_regions: self.hit_regions,
                    timeline: self.document_store.document.timeline.as_ref(),
                    pivot_offsets: self.pivot_offsets,
                    tool_mode: self.tool_mode,
                    rotation_snap_degrees: self.rotation_snap_degrees,
                    composition: self.document_store.document.composition.as_ref(),
                    active_scene: self.document_store.document.active_scene.as_deref(),
                    keyframe_mode: self.keyframe_mode,
                };
                preview_panel::preview_ui(&mut ctx, ui);
            }
            WorkspaceTab::Inspector => {
                let mut ctx = inspector_panel::InspectorContext {
                    preview: &mut self.preview_store.preview,
                    timeline: self.document_store.document.timeline.as_ref(),
                    composition: self.document_store.document.composition.as_ref(),
                    active_scene: self.document_store.document.active_scene.as_deref(),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                    keyframe_mode: self.keyframe_mode,
                    scene_dimensions: self.document_store.document.scene_dimensions,
                    pivot_offsets: self.pivot_offsets,
                    property_view_mode: self.property_view_mode,
                    keyframe_view_mode: self.keyframe_view_mode,
                };
                inspector_panel::inspector_ui(&mut ctx, ui);
            }
            WorkspaceTab::Timeline => {
                // Populate hot-path caches if stale (use free fn to avoid borrow conflict)
                if !self.document_store.cache_valid {
                    let tl = self.document_store.document.timeline.as_ref();
                    stores::document_store::rebuild_cache(
                        &mut self.document_store.cached_actor_labels,
                        &mut self.document_store.cached_actor_keyframes,
                        &mut self.document_store.cache_valid,
                        tl,
                    );
                }
                let mut ctx = timeline::TimelineContext {
                    preview: &mut self.preview_store.preview,
                    timeline: self.document_store.document.timeline.as_ref(),
                    composition: self.document_store.document.composition.as_ref(),
                    active_scene: self.document_store.document.active_scene.as_deref(),
                    commands: self.commands,
                    collapsed_actors: self.collapsed_actors,
                    actor_labels: &self.document_store.cached_actor_labels,
                    actor_keyframes: &self.document_store.cached_actor_keyframes,
                };
                timeline::timeline_ui(&mut ctx, ui);
            }
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &WorkspaceTab) -> egui::WidgetText {
        match pane {
            WorkspaceTab::Sidebar => "Sidebar".into(),
            WorkspaceTab::Editor => "Editor".into(),
            WorkspaceTab::Preview => "Preview".into(),
            WorkspaceTab::Inspector => "Inspector".into(),
            WorkspaceTab::Timeline => "Timeline".into(),
        }
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
        }
    }

    // ─── Modern Minimal Tile Styling ───────────────────────────────────────

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        1.0
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        22.0
    }

    fn tab_bar_color(&self, visuals: &Visuals) -> Color32 {
        visuals.extreme_bg_color
    }

    fn tab_bg_color(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            visuals.panel_fill
        } else {
            Color32::TRANSPARENT
        }
    }

    fn tab_outline_stroke(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Stroke {
        if state.active {
            Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
        } else {
            Stroke::NONE
        }
    }

    fn tab_bar_hline_stroke(&self, visuals: &Visuals) -> Stroke {
        Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
    }

    fn tab_text_color(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            visuals.widgets.active.text_color()
        } else {
            visuals.widgets.noninteractive.text_color()
        }
    }

    fn resize_stroke(
        &self,
        style: &egui::Style,
        resize_state: egui_tiles::ResizeState,
    ) -> Stroke {
        match resize_state {
            egui_tiles::ResizeState::Idle => {
                Stroke::new(1.0, style.visuals.widgets.noninteractive.bg_stroke.color)
            }
            egui_tiles::ResizeState::Hovering => {
                Stroke::new(1.0, ACCENT_BLUE)
            }
            egui_tiles::ResizeState::Dragging => {
                Stroke::new(1.0, ACCENT_BLUE)
            }
        }
    }

    fn drag_preview_stroke(&self, _visuals: &Visuals) -> Stroke {
        Stroke::new(1.0, ACCENT_BLUE)
    }

    fn drag_preview_color(&self, _visuals: &Visuals) -> Color32 {
        accent_faint()
    }

    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        style: &egui::Style,
        _tile_id: TileId,
        rect: Rect,
    ) {
        // Subtle 1px border around each tile for definition
        painter.rect_stroke(
            rect,
            RADIUS_M,
            Stroke::new(1.0, style.visuals.widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
}
