//! Preview transform and pan-clamping helpers.

use animatix::timeline::SceneDimensions;
use egui::Vec2;

use crate::app::preview;

use super::PreviewContext;

impl PreviewContext<'_> {
    pub(crate) fn preview_transform(&self, preview_rect: egui::Rect) -> preview::PreviewTransform {
        preview::PreviewTransform::new(
            self.scene_dimensions,
            preview_rect,
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
        )
    }

    pub(crate) fn clamp_pan(&self, pan: Vec2, preview_rect: egui::Rect) -> Vec2 {
        Self::clamp_pan_value(
            pan,
            preview_rect,
            self.scene_dimensions,
            self.preview.viewport.preview_zoom,
        )
    }

    /// Pure clamping math — extracted so it can be unit-tested.
    pub(crate) fn clamp_pan_value(
        pan: Vec2,
        preview_rect: egui::Rect,
        scene_dimensions: SceneDimensions,
        zoom: f32,
    ) -> Vec2 {
        let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, Vec2::ZERO);
        let (scale, _) = tx.scale();
        let scene_w = scene_dimensions.width as f64;
        let scene_h = scene_dimensions.height as f64;
        let preview_w = preview_rect.width() as f64;
        let preview_h = preview_rect.height() as f64;

        let visible_w = (preview_w * scale).min(scene_w);
        let visible_h = (preview_h * scale).min(scene_h);
        let half_w = visible_w / 2.0;
        let half_h = visible_h / 2.0;

        Vec2::new(
            pan.x.clamp(half_w as f32, (scene_w - half_w) as f32),
            pan.y.clamp(half_h as f32, (scene_h - half_h) as f32),
        )
    }

    pub(crate) fn preview_screen_to_scene(
        &self,
        preview_rect: egui::Rect,
        screen: egui::Pos2,
    ) -> kurbo::Point {
        self.preview_transform(preview_rect).screen_to_scene(screen)
    }

    pub(crate) fn preview_scene_to_screen(
        &self,
        preview_rect: egui::Rect,
        scene: kurbo::Point,
    ) -> egui::Pos2 {
        self.preview_transform(preview_rect).scene_to_screen(scene)
    }
}
