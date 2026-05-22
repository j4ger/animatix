//! Scene Slices — A/B/C side-by-side scene comparison
//!
//! Figma-Variants / Photoshop-Artboards style: compare animation scenes A/B/C.
//! Operations: duplicate slice, drag actor across slices, `1`/`2`/`3` hotkeys,
//! batch export.

use crate::app::design_tokens::*;
use egui::{Color32, FontId, Pos2, Rect, Stroke, Vec2};

/// A single scene slice (variant of the current composition).
pub struct SceneSlice {
    pub name: String,
    pub scene_name: String,
    pub color: Color32,
}

/// State for scene slice comparison.
#[derive(Default)]
pub struct SceneSliceState {
    pub enabled: bool,
    pub slices: Vec<SceneSlice>,
    pub active_slice: usize,
}


impl SceneSliceState {
    /// Toggle scene slice mode.
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
        if self.enabled && self.slices.is_empty() {
            // Create default A/B slices
            self.slices.push(SceneSlice {
                name: "A".into(),
                scene_name: "default".into(),
                color: ACCENT_BLUE,
            });
            self.slices.push(SceneSlice {
                name: "B".into(),
                scene_name: "default".into(),
                color: AMBER,
            });
        }
    }

    /// Select slice by index (1-based hotkeys).
    pub fn select(&mut self, idx: usize) {
        if idx > 0 && idx <= self.slices.len() {
            self.active_slice = idx - 1;
        }
    }
}

/// Render slice tabs at the top of the preview panel.
pub fn render_slice_tabs(
    ui: &mut egui::Ui,
    state: &mut SceneSliceState,
) {
    if !state.enabled {
        return;
    }

    let available = ui.available_width();
    let tab_h = ROW_S;
    let tab_w = (available - SPACE_S * (state.slices.len() + 1) as f32) / state.slices.len() as f32;

    ui.horizontal(|ui| {
        ui.add_space(SPACE_S);
        for (i, slice) in state.slices.iter().enumerate() {
            let is_active = i == state.active_slice;
            let rect = Rect::from_min_size(
                Pos2::new(ui.cursor().min.x, ui.cursor().min.y),
                Vec2::new(tab_w.max(40.0), tab_h),
            );
            let resp = ui.interact(rect, ui.id().with(("slice_tab", i)), egui::Sense::click());

            let bg = if is_active {
                Color32::from_rgba_unmultiplied(slice.color.r(), slice.color.g(), slice.color.b(), 40)
            } else {
                Color32::TRANSPARENT
            };
            let border = if is_active { slice.color } else { BORDER };
            let text_color = if is_active { slice.color } else { TEXT_MUTED };

            ui.painter().rect_filled(rect, RADIUS_S, bg);
            ui.painter().rect_stroke(rect, RADIUS_S, Stroke::new(1.5, border), egui::StrokeKind::Outside);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{} {}", i + 1, slice.name),
                FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                text_color,
            );

            if resp.clicked() {
                state.active_slice = i;
            }

            ui.add_space(SPACE_S);
        }

        // Add slice button
        let add_rect = Rect::from_min_size(
            Pos2::new(ui.cursor().min.x, ui.cursor().min.y + (tab_h - ROW_S) / 2.0),
            Vec2::new(ROW_S, ROW_S),
        );
        let add_resp = ui.interact(add_rect, ui.id().with("slice_add"), egui::Sense::click());
        ui.painter().rect_filled(add_rect, RADIUS_S, BG_WIDGET);
        ui.painter().text(
            add_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::PLUS,
            FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
            if add_resp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED },
        );
        if add_resp.clicked() {
            let next_label = (b'A' + state.slices.len() as u8) as char;
            state.slices.push(SceneSlice {
                name: next_label.to_string(),
                scene_name: "default".into(),
                color: TEXT_MUTED,
            });
        }
    });
}
