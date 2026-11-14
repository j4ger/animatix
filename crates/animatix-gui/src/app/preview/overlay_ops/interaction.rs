use super::execute::*;
use super::layout::painter_text_size;
use super::*;

pub fn hover_highlight_ops(
    theme: &eparts::Theme,
    hovered_actor: &str,
    hover_rect: kurbo::Rect,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hover_color = theme.palette.accent.ghost;
    let corners = [
        kurbo::Point::new(hover_rect.x0, hover_rect.y0),
        kurbo::Point::new(hover_rect.x1, hover_rect.y0),
        kurbo::Point::new(hover_rect.x1, hover_rect.y1),
        kurbo::Point::new(hover_rect.x0, hover_rect.y1),
    ];
    for i in 0..4 {
        push_dashed(
            &mut ops,
            corners[i],
            corners[(i + 1) % 4],
            DASH_LEN,
            GAP_LEN,
            OverlayStroke::new(STROKE_WIDTH, hover_color),
        );
    }

    let hover_screen = scene_rect_to_screen(&tx, &hover_rect);
    let tooltip_pos = egui::pos2(hover_screen.center().x, hover_screen.top() - 20.0);
    let font = egui::FontId::proportional(12.0);
    let text_size = painter_text_size(hovered_actor, font.clone());
    let tooltip_rect = Rect::from_center_size(tooltip_pos, text_size + Vec2::new(8.0, 4.0));
    push_screen_rect_fill(&mut ops, tooltip_rect, RADIUS_M, theme.palette.overlay.badge_bg);
    push_screen_rect_stroke(
        &mut ops,
        tooltip_rect,
        RADIUS_M,
        OverlayStroke::new(STROKE_WIDTH, theme.palette.border.default),
        StrokeKind::Outside,
    );
    push_screen_text(
        &mut ops,
        tooltip_rect.left_center() + Vec2::new(4.0, -text_size.y / 2.0),
        Align2::LEFT_TOP,
        hovered_actor,
        font,
        theme.palette.text.primary,
    );
    ops
}
