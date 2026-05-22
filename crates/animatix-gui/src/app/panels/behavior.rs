use egui::{Color32, Rect, Stroke, Visuals};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};

use crate::app::{WorkspaceTab, WorkspaceViewer};
use crate::app::theme::*;

pub(crate) struct WorkspaceBehavior<'a> {
    pub(crate) viewer: WorkspaceViewer<'a>,
}

impl<'a> Behavior<WorkspaceTab> for WorkspaceBehavior<'a> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut WorkspaceTab,
    ) -> UiResponse {
        match pane {
            WorkspaceTab::Sidebar => self.viewer.sidebar_ui(ui),
            WorkspaceTab::Editor => self.viewer.editor_ui(ui),
            WorkspaceTab::Preview => self.viewer.preview_ui(ui),
            WorkspaceTab::Inspector => self.viewer.inspector_ui(ui),
            WorkspaceTab::Timeline => self.viewer.timeline_ui(ui),
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
        Color32::from_rgba_unmultiplied(ACCENT_BLUE.r(), ACCENT_BLUE.g(), ACCENT_BLUE.b(), 20)
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
