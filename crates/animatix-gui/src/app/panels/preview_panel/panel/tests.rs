use super::*;
use crate::app::PreviewPaneState;
use crate::app::commands::ActionQueue;
use crate::app::interaction::keyboard::ShortcutRegistry;
use crate::app::preview::performance::PerformanceMetrics;
use animatix::timeline::SceneDimensions;
use egui::{Event, Pos2, Rect, Shape, TextureId, pos2, vec2};

const SCENE_IMAGE: TextureId = TextureId::User(91);
const DIMENSIONS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

struct Frame {
    canvas: Rect,
    guides: Vec<[Pos2; 2]>,
}

fn frame(ctx: &egui::Context, preview: &mut PreviewPaneState, events: Vec<Event>) -> Frame {
    let mut performance = PerformanceMetrics::new();
    let mut actions = ActionQueue::default();
    let mut selection = Default::default();
    let mut selected = Default::default();
    let mut pc = PreviewContext {
        scene_dimensions: DIMENSIONS,
        preview,
        preview_texture_id: Some(SCENE_IMAGE),
        commands: &mut actions,
        selection: &mut selection,
        selected_actors: &mut selected,
        hit_regions: &[],
        timeline: None,
        composition: None,
        active_scene: None,
        performance_metrics: &mut performance,
        debug_layout: false,
        preview_fullscreen: false,
        debug_spacing: false,
        rebuild_in_progress: false,
        has_scene: true,
        source_text: None,
    };
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        events,
        ..Default::default()
    };
    let mut warning = egui::Color32::TRANSPARENT;
    let shortcuts = ShortcutRegistry::new();
    let mut output = ctx.run_ui(input, |ui| {
        warning = eparts::theme(ui).palette.status.warning;
        preview_panel_ui(&mut pc, ui, &shortcuts);
    });
    output.textures_delta.clear();
    let mut canvas = None;
    let mut guides = Vec::new();
    for clipped in output.shapes {
        match clipped.shape {
            Shape::Rect(rect) if rect.fill_texture_id() == SCENE_IMAGE => {
                canvas = Some(rect.rect);
            },
            Shape::Mesh(mesh) if mesh.texture_id == SCENE_IMAGE => {
                canvas = Some(mesh.calc_bounds());
            },
            Shape::LineSegment { points, stroke } if stroke.color == warning => {
                assert!(canvas.is_some(), "a guide would be painted under the opaque scene");
                guides.push(points);
            },
            _ => {},
        }
    }
    Frame {
        canvas: canvas.expect("preview image painted"),
        guides,
    }
}

fn click(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn ruler_drags_render_over_the_image_and_store_scene_coords() {
    let ctx = egui::Context::default();
    let mut preview = PreviewPaneState::new(1.0, DIMENSIONS);
    preview.viewport.preview_pan = vec2(960.0, 540.0);
    preview.overlay.show_guides = true;
    let canvas = frame(&ctx, &mut preview, vec![]).canvas;
    let drop = canvas.center() - vec2(40.0, 25.0);
    let points = [
        (
            pos2(drop.x, canvas.top() - 5.0),
            [pos2(canvas.left(), drop.y), pos2(canvas.right(), drop.y)],
        ),
        (
            pos2(canvas.left() - 5.0, drop.y),
            [pos2(drop.x, canvas.top()), pos2(drop.x, canvas.bottom())],
        ),
    ];
    for (i, (start, guide)) in points.into_iter().enumerate() {
        frame(&ctx, &mut preview, vec![Event::PointerMoved(start), click(start, true)]);
        let active = frame(&ctx, &mut preview, vec![Event::PointerMoved(drop)]);
        assert_eq!(active.guides.len(), i + 1);
        assert_eq!(active.guides[i], guide);
        assert_eq!(preview.guides.horizontal_guides.len(), i);
        assert!(preview.guides.vertical_guides.is_empty());
        let released = frame(&ctx, &mut preview, vec![click(drop, false)]);
        assert_eq!(released.guides.len(), i + 1);
        assert_eq!(released.guides[i], guide);
    }
    let scene_drop = preview::PreviewTransform::new(DIMENSIONS, canvas, 1.0, vec2(960.0, 540.0))
        .screen_to_scene(drop);
    assert!((f64::from(preview.guides.horizontal_guides[0]) - scene_drop.y).abs() < 0.1);
    assert!((f64::from(preview.guides.vertical_guides[0]) - scene_drop.x).abs() < 0.1);

    preview.overlay.show_guides = false;
    assert!(frame(&ctx, &mut preview, vec![]).guides.is_empty());
    preview.viewport.preview_zoom = 2.0;
    preview.overlay.show_guides = true;
    let zoomed = frame(&ctx, &mut preview, vec![]);
    assert_eq!(zoomed.guides.len(), 2);
    assert!((zoomed.guides[0][0].y - (canvas.center().y - 50.0)).abs() < 0.1);
    assert!((zoomed.guides[1][0].x - (canvas.center().x - 80.0)).abs() < 0.1);
    assert!((f64::from(preview.guides.horizontal_guides[0]) - scene_drop.y).abs() < 0.1);
    assert!((f64::from(preview.guides.vertical_guides[0]) - scene_drop.x).abs() < 0.1);
}
