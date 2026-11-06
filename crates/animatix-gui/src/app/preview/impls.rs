use animatix::timeline::SceneDimensions;
use egui::{Pos2, Vec2};

use crate::app::design_tokens::spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;

use super::PreviewTransform;

impl PreviewTransform {
    pub fn new(
        scene_dimensions: SceneDimensions,
        preview_rect: egui::Rect,
        zoom: f32,
        pan: Vec2,
    ) -> Self {
        Self {
            scene_dimensions,
            preview_rect,
            zoom,
            pan,
        }
    }

    /// Uniform scale preserving aspect ratio: the whole scene fits in the rect.
    pub fn scale(&self) -> (f64, f64) {
        let desired = self.preview_rect.size();
        let scene_w = self.scene_dimensions.width as f64;
        let scene_h = self.scene_dimensions.height as f64;

        // Pixels per scene pixel at zoom = 1.0 (screen / scene)
        let px_per_scene_x = desired.x.max(1.0) as f64 / scene_w;
        let px_per_scene_y = desired.y.max(1.0) as f64 / scene_h;
        let px_per_scene = px_per_scene_x.min(px_per_scene_y);

        // Scene pixels per screen pixel = inverse
        let base_scale = 1.0 / px_per_scene;
        let z = self.zoom.max(PREVIEW_MIN_ZOOM) as f64;
        let scale = base_scale / z;
        (scale, scale)
    }

    /// Display rect of the uniformly scaled scene (letterboxed on aspect mismatch).
    pub fn display_rect(&self) -> egui::Rect {
        let desired = self.preview_rect.size();
        let scene_w = self.scene_dimensions.width as f64;
        let scene_h = self.scene_dimensions.height as f64;

        let px_per_scene_x = desired.x.max(1.0) as f64 / scene_w;
        let px_per_scene_y = desired.y.max(1.0) as f64 / scene_h;
        let px_per_scene = px_per_scene_x.min(px_per_scene_y);

        let z = self.zoom.max(PREVIEW_MIN_ZOOM) as f64;
        let display_w = (scene_w * px_per_scene * z).min(desired.x as f64);
        let display_h = (scene_h * px_per_scene * z).min(desired.y as f64);

        egui::Rect::from_center_size(
            self.preview_rect.center(),
            egui::vec2(display_w as f32, display_h as f32),
        )
    }

    /// Convert a screen position (e.g. mouse cursor) to scene coordinates.
    pub fn screen_to_scene(&self, screen: Pos2) -> kurbo::Point {
        let (scale_x, scale_y) = self.scale();
        let center_x = self.preview_rect.center().x as f64;
        let center_y = self.preview_rect.center().y as f64;
        kurbo::Point::new(
            self.pan.x as f64 + (screen.x as f64 - center_x) * scale_x,
            self.pan.y as f64 + (screen.y as f64 - center_y) * scale_y,
        )
    }

    /// Convert a scene coordinate to a screen position.
    pub fn scene_to_screen(&self, scene: kurbo::Point) -> Pos2 {
        let (scale_x, scale_y) = self.scale();
        let center_x = self.preview_rect.center().x as f64;
        let center_y = self.preview_rect.center().y as f64;
        Pos2::new(
            (center_x + (scene.x - self.pan.x as f64) / scale_x) as f32,
            (center_y + (scene.y - self.pan.y as f64) / scale_y) as f32,
        )
    }
}
