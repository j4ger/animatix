use super::*;

pub fn execute_overlay_ops(
    painter: &egui::Painter,
    ops: &[PreviewOverlayOp],
    tx: &PreviewTransform,
) {
    for op in ops {
        match op {
            PreviewOverlayOp::Line { from, to, stroke } => {
                painter
                    .line_segment([scene_to_screen(tx, *from), scene_to_screen(tx, *to)], stroke);
            },
            PreviewOverlayOp::DashedLine {
                from,
                to,
                dash_len,
                gap_len,
                stroke,
            } => {
                let from = scene_to_screen(tx, *from);
                let to = scene_to_screen(tx, *to);
                let total = from.distance(to);
                let mut pos = 0.0;
                while pos < total {
                    let t0 = pos / total;
                    let t1 = ((pos + dash_len).min(total)) / total;
                    let p0 =
                        Pos2::new(from.x + (to.x - from.x) * t0, from.y + (to.y - from.y) * t0);
                    let p1 =
                        Pos2::new(from.x + (to.x - from.x) * t1, from.y + (to.y - from.y) * t1);
                    painter.line_segment([p0, p1], stroke);
                    pos += dash_len + gap_len;
                }
            },
            PreviewOverlayOp::RectStroke {
                rect,
                corner_radius,
                stroke,
                stroke_kind,
            } => {
                painter.rect_stroke(
                    scene_rect_to_screen(tx, rect),
                    *corner_radius,
                    stroke,
                    *stroke_kind,
                );
            },
            PreviewOverlayOp::RectFill {
                rect,
                corner_radius,
                color,
            } => {
                painter.rect_filled(scene_rect_to_screen(tx, rect), *corner_radius, *color);
            },
            PreviewOverlayOp::CircleFill {
                center,
                radius,
                color,
            } => {
                let screen_radius = scene_radius_to_screen(tx, *radius);
                painter.circle_filled(scene_to_screen(tx, *center), screen_radius, *color);
            },
            PreviewOverlayOp::CircleStroke {
                center,
                radius,
                stroke,
            } => {
                let screen_radius = scene_radius_to_screen(tx, *radius);
                painter.circle_stroke(scene_to_screen(tx, *center), screen_radius, stroke);
            },
            PreviewOverlayOp::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                stroke,
            } => {
                let center = scene_to_screen(tx, *center);
                let radius = scene_radius_to_screen(tx, *radius);
                let segments = 8;
                for i in 0..segments {
                    let t0 = i as f32 / segments as f32;
                    let t1 = (i + 1) as f32 / segments as f32;
                    let a0 = start_angle + (end_angle - start_angle) * t0;
                    let a1 = start_angle + (end_angle - start_angle) * t1;
                    let p0 = Pos2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin());
                    let p1 = Pos2::new(center.x + radius * a1.cos(), center.y + radius * a1.sin());
                    painter.line_segment([p0, p1], stroke);
                }
            },
            PreviewOverlayOp::Text {
                pos,
                anchor,
                text,
                font,
                color,
            } => {
                painter.text(scene_to_screen(tx, *pos), *anchor, text, font.clone(), *color);
            },
            PreviewOverlayOp::Screen(screen) => execute_screen_op(painter, screen),
        }
    }
}

fn execute_screen_op(painter: &egui::Painter, op: &ScreenOverlayOp) {
    match op {
        ScreenOverlayOp::RectStroke {
            rect,
            corner_radius,
            stroke,
            stroke_kind,
        } => {
            painter.rect_stroke(*rect, *corner_radius, stroke, *stroke_kind);
        },
        ScreenOverlayOp::RectFill {
            rect,
            corner_radius,
            color,
        } => {
            painter.rect_filled(*rect, *corner_radius, *color);
        },
        ScreenOverlayOp::Text {
            pos,
            anchor,
            text,
            font,
            color,
        } => {
            painter.text(*pos, *anchor, text, font.clone(), *color);
        },
    }
}

/// Add a dashed line op to the given list.
pub(super) fn push_dashed(
    ops: &mut Vec<PreviewOverlayOp>,
    from: kurbo::Point,
    to: kurbo::Point,
    dash_len: f32,
    gap_len: f32,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::DashedLine {
        from,
        to,
        dash_len,
        gap_len,
        stroke,
    });
}

pub(super) fn push_line(
    ops: &mut Vec<PreviewOverlayOp>,
    from: kurbo::Point,
    to: kurbo::Point,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::Line { from, to, stroke });
}

pub(super) fn push_rect_stroke(
    ops: &mut Vec<PreviewOverlayOp>,
    rect: kurbo::Rect,
    corner_radius: f32,
    stroke: OverlayStroke,
    stroke_kind: StrokeKind,
) {
    ops.push(PreviewOverlayOp::RectStroke {
        rect,
        corner_radius,
        stroke,
        stroke_kind,
    });
}

pub(super) fn push_rect_fill(
    ops: &mut Vec<PreviewOverlayOp>,
    rect: kurbo::Rect,
    corner_radius: f32,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::RectFill {
        rect,
        corner_radius,
        color,
    });
}

pub(super) fn push_circle_fill(
    ops: &mut Vec<PreviewOverlayOp>,
    center: kurbo::Point,
    radius: f32,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::CircleFill {
        center,
        radius,
        color,
    });
}

pub(super) fn push_circle_stroke(
    ops: &mut Vec<PreviewOverlayOp>,
    center: kurbo::Point,
    radius: f32,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::CircleStroke {
        center,
        radius,
        stroke,
    });
}

pub(super) fn push_text(
    ops: &mut Vec<PreviewOverlayOp>,
    pos: kurbo::Point,
    anchor: Align2,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::Text {
        pos,
        anchor,
        text: text.into(),
        font,
        color,
    });
}

pub(super) fn push_screen_rect_stroke(
    ops: &mut Vec<PreviewOverlayOp>,
    rect: Rect,
    corner_radius: f32,
    stroke: OverlayStroke,
    stroke_kind: StrokeKind,
) {
    ops.push(PreviewOverlayOp::Screen(ScreenOverlayOp::RectStroke {
        rect,
        corner_radius,
        stroke,
        stroke_kind,
    }));
}

pub(super) fn push_screen_rect_fill(
    ops: &mut Vec<PreviewOverlayOp>,
    rect: Rect,
    corner_radius: f32,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::Screen(ScreenOverlayOp::RectFill {
        rect,
        corner_radius,
        color,
    }));
}

pub(super) fn push_screen_text(
    ops: &mut Vec<PreviewOverlayOp>,
    pos: Pos2,
    anchor: Align2,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
) {
    ops.push(PreviewOverlayOp::Screen(ScreenOverlayOp::Text {
        pos,
        anchor,
        text: text.into(),
        font,
        color,
    }));
}

pub(super) const DASH_LEN: f32 = 4.0;
pub(super) const GAP_LEN: f32 = 3.0;
