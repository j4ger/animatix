use animatix::timeline::SceneDimensions;
use egui::{Rect, Vec2};

use super::PreviewContext;

#[test]
fn test_clamp_pan_center() {
    // Scene 1920×1080, preview 960×540, zoom=1 → full scene visible, must pan to center (960,
    // 540)
    let scene = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
    let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
    // Both axes are clamped to the exact center since all scene is visible
    assert_eq!(result.x, 960.0);
    assert_eq!(result.y, 540.0);
}

#[test]
fn test_clamp_pan_beyond_bounds() {
    // Scene 1920×1080, preview 960×540, zoom=2 → half scene visible
    // visible_w = min(960*1.0, 1920) = 960, half_w = 480
    // range: [480, 1920-480] = [480, 1440]
    let scene = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
    // Pan way beyond right edge
    let result = PreviewContext::clamp_pan_value(Vec2::new(2000.0, 1000.0), preview, scene, 2.0);
    assert_eq!(result.x, 1440.0);
    assert_eq!(result.y, 810.0);
    // Pan way beyond left/top edge
    let result = PreviewContext::clamp_pan_value(Vec2::new(-100.0, -100.0), preview, scene, 2.0);
    assert_eq!(result.x, 480.0);
    assert_eq!(result.y, 270.0);
}

#[test]
fn test_clamp_pan_zero_size_preview() {
    // Minimal preview rect (1×1 minimum via scale logic)
    let scene = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, 1.0));
    let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
    // visible_w = min(1 * huge_scale, 1920) = 1920 → half_w = 960 → range [960, 960]
    assert_eq!(result.x, 960.0);
    assert_eq!(result.y, 540.0);
}

#[test]
fn test_clamp_pan_extreme_zoom() {
    // Scene 1920×1080, preview 960×540, zoom=10 → tiny viewport in scene space
    // scale = (base_scale=2.0) / 10 = 0.2
    // visible_w = min(960 * 0.2, 1920) = min(192, 1920) = 192, half_w = 96
    // range: [96, 1920-96] = [96, 1824]
    let scene = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
    let result = PreviewContext::clamp_pan_value(Vec2::new(100.0, 100.0), preview, scene, 10.0);
    assert_eq!(result.x, 100.0); // 100 is in [96, 1824]
    assert_eq!(result.y, 100.0); // 100 is in [54, 1026]
}

#[test]
fn test_clamp_pan_scene_smaller_than_preview() {
    // Scene 100×50, preview 500×500, zoom=1 → scene is tiny compared to preview
    // px_per_scene_x = 500/100 = 5, px_per_scene_y = 500/50 = 10
    // px_per_scene = min(5, 10) = 5
    // base_scale = 1/5 = 0.2
    // scale = 0.2 / 1 = 0.2
    // visible_w = min(500*0.2, 100) = min(100, 100) = 100, half_w = 50
    // range: [50, 100-50] = [50, 50]
    // visible_h = min(500*0.2, 50) = min(100, 50) = 50, half_h = 25
    // range: [25, 50-25] = [25, 25]
    let scene = SceneDimensions {
        width: 100,
        height: 50,
    };
    let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 500.0));
    let result = PreviewContext::clamp_pan_value(Vec2::new(0.0, 0.0), preview, scene, 1.0);
    assert_eq!(result.x, 50.0);
    assert_eq!(result.y, 25.0);
}
