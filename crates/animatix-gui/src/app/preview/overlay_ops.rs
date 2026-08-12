//! Screen-space overlay operations for the preview canvas.
//!
//! Preview overlays are generated as a list of [`PreviewOverlayOp`] and executed
//! by a single painter pass. This decouples overlay geometry/state from egui
//! painting so behavior can be unit-tested without a GPU or a live UI frame.
//!
//! Geometry is authored in scene coordinates. The executor receives a
//! [`PreviewTransform`] and maps scene geometry to the canvas. Screen-space
//! decorations that depend on egui layout (tooltip boxes, mouse-cycle badges,
//! HUD text) use the explicit [`ScreenOverlayOp`] variants instead of mixing
//! coordinate spaces inside one op.

use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use super::{
    ActorProps, PreviewTransform, local_to_world, pivot_world, rotation_handle_world,
    world_handle_positions,
};
use crate::app::design_tokens::spatial::STROKE_WIDTH;

/// Colors used by the layout debug overlay.
struct LayoutDebugStyle {
    container_color: Color32,
    slot_color: Color32,
    size_color: Color32,
    spacing_color: Color32,
}

impl LayoutDebugStyle {
    fn from_theme(theme: &eparts::Theme) -> Self {
        Self {
            container_color: theme.accent.primary,
            slot_color: Color32::from_rgba_premultiplied(
                theme.status.warning.r(),
                theme.status.warning.g(),
                theme.status.warning.b(),
                150,
            ),
            size_color: theme.status.warning,
            spacing_color: Color32::from_rgba_premultiplied(200, 80, 80, 60),
        }
    }
}

const RADIUS_M: f32 = 4.0;

/// A stroke used by overlay operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStroke {
    pub width: f32,
    pub color: Color32,
}

impl OverlayStroke {
    pub fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

impl From<Stroke> for OverlayStroke {
    fn from(stroke: Stroke) -> Self {
        Self::new(stroke.width, stroke.color)
    }
}

impl From<&OverlayStroke> for Stroke {
    fn from(stroke: &OverlayStroke) -> Self {
        Stroke::new(stroke.width, stroke.color)
    }
}

/// A screen-space overlay decoration that depends on egui layout or pointer
/// position. Screen ops are only used where scene-space geometry would not
/// preserve the intended visual size or anchor.
#[derive(Clone, Debug)]
pub enum ScreenOverlayOp {
    RectStroke {
        rect: Rect,
        corner_radius: f32,
        stroke: OverlayStroke,
        stroke_kind: StrokeKind,
    },
    RectFill {
        rect: Rect,
        corner_radius: f32,
        color: Color32,
    },
    Text {
        pos: Pos2,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
}

/// A drawable preview overlay primitive.
///
/// Scene-space geometry is authored in scene coordinates. Screen decorations
/// are wrapped in [`PreviewOverlayOp::Screen`] and painted directly.
#[derive(Clone, Debug)]
pub enum PreviewOverlayOp {
    Line {
        from: kurbo::Point,
        to: kurbo::Point,
        stroke: OverlayStroke,
    },
    DashedLine {
        from: kurbo::Point,
        to: kurbo::Point,
        dash_len: f32,
        gap_len: f32,
        stroke: OverlayStroke,
    },
    RectStroke {
        rect: kurbo::Rect,
        corner_radius: f32,
        stroke: OverlayStroke,
        stroke_kind: StrokeKind,
    },
    RectFill {
        rect: kurbo::Rect,
        corner_radius: f32,
        color: Color32,
    },
    CircleFill {
        center: kurbo::Point,
        radius: f32,
        color: Color32,
    },
    CircleStroke {
        center: kurbo::Point,
        radius: f32,
        stroke: OverlayStroke,
    },
    Arc {
        center: kurbo::Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke: OverlayStroke,
    },
    Text {
        pos: kurbo::Point,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
    Arrow {
        from: kurbo::Point,
        to: kurbo::Point,
        stroke: OverlayStroke,
    },
    Badge {
        pos: kurbo::Point,
        text: String,
        bg: Color32,
        fg: Color32,
        border_stroke: OverlayStroke,
    },
    Screen(ScreenOverlayOp),
}

/// Convert scene units per screen unit into scene-space overlay sizes.
fn px_to_scene(tx: &PreviewTransform, px: f32) -> f32 {
    (px as f64 * tx.scale().0) as f32
}

/// Convert a scene-space point to screen space through the preview transform.
fn scene_to_screen(tx: &PreviewTransform, point: kurbo::Point) -> Pos2 {
    tx.scene_to_screen(point)
}

/// Convert a scene-space rectangle to screen space.
fn scene_rect_to_screen(tx: &PreviewTransform, rect: &kurbo::Rect) -> Rect {
    let min = scene_to_screen(tx, kurbo::Point::new(rect.x0, rect.y0));
    let max = scene_to_screen(tx, kurbo::Point::new(rect.x1, rect.y1));
    Rect::from_min_max(min, max)
}

fn scene_radius_to_screen(tx: &PreviewTransform, radius: f32) -> f32 {
    (radius as f64 / tx.scale().0) as f32
}

/// Convert a screen-space rectangle back to scene space.
fn screen_rect_to_scene(tx: &PreviewTransform, rect: Rect) -> kurbo::Rect {
    let min = tx.screen_to_scene(rect.min);
    let max = tx.screen_to_scene(rect.max);
    kurbo::Rect::new(min.x.min(max.x), min.y.min(max.y), min.x.max(max.x), min.y.max(max.y))
}

/// The scene-space viewport currently visible in the preview rect.
fn scene_viewport(tx: &PreviewTransform) -> kurbo::Rect {
    screen_rect_to_scene(tx, tx.preview_rect)
}

/// Execute overlay operations onto the canvas painter.
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
            PreviewOverlayOp::Arrow { from, to, stroke } => {
                let from = scene_to_screen(tx, *from);
                let to = scene_to_screen(tx, *to);
                painter.arrow(from, to - from, stroke);
            },
            PreviewOverlayOp::Badge {
                pos,
                text,
                bg,
                fg,
                border_stroke,
            } => {
                let pos = scene_to_screen(tx, *pos);
                let galley = painter.layout_no_wrap(text.clone(), FontId::proportional(12.0), *fg);
                let size = galley.size() + Vec2::new(8.0, 4.0);
                let rect = Rect::from_center_size(pos, size);
                painter.rect_filled(rect, 2.0, *bg);
                painter.rect_stroke(rect, 2.0, border_stroke, StrokeKind::Outside);
                painter.galley(rect.min + Vec2::new(4.0, 2.0), galley, *fg);
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
fn push_dashed(
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

fn push_line(
    ops: &mut Vec<PreviewOverlayOp>,
    from: kurbo::Point,
    to: kurbo::Point,
    stroke: OverlayStroke,
) {
    ops.push(PreviewOverlayOp::Line { from, to, stroke });
}

fn push_rect_stroke(
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

fn push_rect_fill(
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

fn push_circle_fill(
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

fn push_circle_stroke(
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

fn push_text(
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

fn push_screen_rect_stroke(
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

fn push_screen_rect_fill(
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

fn push_screen_text(
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

const DASH_LEN: f32 = 4.0;
const GAP_LEN: f32 = 3.0;
const HANDLE_SIZE: f32 = 8.0;
const ROTATION_OFFSET: f32 = 24.0;
const ROTATION_RADIUS: f32 = 6.0;
const CROSS_SIZE: f32 = 8.0;

fn scale_handle_positions(sel_rect: kurbo::Rect) -> [kurbo::Point; 8] {
    [
        kurbo::Point::new(sel_rect.x0, sel_rect.y0),
        kurbo::Point::new(sel_rect.x1, sel_rect.y0),
        kurbo::Point::new(sel_rect.x1, sel_rect.y1),
        kurbo::Point::new(sel_rect.x0, sel_rect.y1),
        kurbo::Point::new(sel_rect.x0 + (sel_rect.x1 - sel_rect.x0) / 2.0, sel_rect.y0),
        kurbo::Point::new(sel_rect.x1, sel_rect.y0 + (sel_rect.y1 - sel_rect.y0) / 2.0),
        kurbo::Point::new(sel_rect.x0 + (sel_rect.x1 - sel_rect.x0) / 2.0, sel_rect.y1),
        kurbo::Point::new(sel_rect.x0, sel_rect.y0 + (sel_rect.y1 - sel_rect.y0) / 2.0),
    ]
}

/// Generate the single-selection outline, handles, rotation ring, and pivot.
pub fn selection_overlay_ops(
    theme: &eparts::Theme,
    props: Option<&ActorProps>,
    fallback_rect: Option<kurbo::Rect>,
    is_dragging: bool,
    pixels_per_point: f32,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let selection_color = theme.accent.primary;
    let accent_hover_color = theme.accent.hover;
    let text_primary = theme.text.primary;
    let text_faint = theme.text.faint;
    let cross_color = theme.status.warning;

    let stroke = if is_dragging {
        OverlayStroke::new(1.5, accent_hover_color)
    } else {
        OverlayStroke::new(1.5, selection_color)
    };

    if let Some(p) = props {
        let hw = p.size[0] / 2.0;
        let hh = p.size[1] / 2.0;
        let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
        let world_corners: [kurbo::Point; 4] =
            std::array::from_fn(|i| local_to_world(local_corners[i], p.position, p.rotation));

        for i in 0..4 {
            let next = (i + 1) % 4;
            push_line(&mut ops, world_corners[i], world_corners[next], stroke);
        }
        if is_dragging {
            for i in 0..4 {
                push_dashed(
                    &mut ops,
                    world_corners[i],
                    world_corners[(i + 1) % 4],
                    DASH_LEN,
                    GAP_LEN,
                    OverlayStroke::new(STROKE_WIDTH, text_faint),
                );
            }
        }

        let handle_world = world_handle_positions(p);
        let corner_radius = px_to_scene(&tx, HANDLE_SIZE * 0.6 * pixels_per_point);
        for &pos in handle_world[..4].iter() {
            push_circle_fill(&mut ops, pos, corner_radius, text_primary);
            push_circle_stroke(
                &mut ops,
                pos,
                corner_radius,
                OverlayStroke::new(1.5, selection_color),
            );
        }

        let edge_handle_px = px_to_scene(&tx, HANDLE_SIZE * 0.7 * pixels_per_point);
        for &pos in handle_world[4..].iter() {
            let handle_rect = kurbo::Rect::from_center_size(
                pos,
                kurbo::Size::new(edge_handle_px as f64, edge_handle_px as f64),
            );
            push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }

        if !is_dragging {
            let arc_radius = px_to_scene(&tx, HANDLE_SIZE * 1.5 * pixels_per_point);
            let arc_stroke = OverlayStroke::new(STROKE_WIDTH, selection_color.gamma_multiply(0.5));
            let arc_angles: [(f32, f32); 4] = [
                (std::f32::consts::PI, 3.0 * std::f32::consts::PI / 2.0),
                (3.0 * std::f32::consts::PI / 2.0, 2.0 * std::f32::consts::PI),
                (0.0, std::f32::consts::PI / 2.0),
                (std::f32::consts::PI / 2.0, std::f32::consts::PI),
            ];
            for i in 0..4 {
                ops.push(PreviewOverlayOp::Arc {
                    center: handle_world[i],
                    radius: arc_radius,
                    start_angle: arc_angles[i].0,
                    end_angle: arc_angles[i].1,
                    stroke: arc_stroke,
                });
            }
        }

        let rot_world = rotation_handle_world(p);
        let top_center_local = [0.0_f32, -hh];
        let top_center_world = local_to_world(top_center_local, p.position, p.rotation);
        push_line(
            &mut ops,
            top_center_world,
            rot_world,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        let rot_radius = px_to_scene(&tx, ROTATION_RADIUS * pixels_per_point);
        push_circle_fill(&mut ops, rot_world, rot_radius, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_world,
            rot_radius,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );

        let pivot_world_pt = pivot_world(p);
        let pivot_world = kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64);
        let cross_size = px_to_scene(&tx, CROSS_SIZE * pixels_per_point);
        push_line(
            &mut ops,
            kurbo::Point::new(pivot_world.x - cross_size as f64, pivot_world.y),
            kurbo::Point::new(pivot_world.x + cross_size as f64, pivot_world.y),
            OverlayStroke::new(1.5, cross_color),
        );
        push_line(
            &mut ops,
            kurbo::Point::new(pivot_world.x, pivot_world.y - cross_size as f64),
            kurbo::Point::new(pivot_world.x, pivot_world.y + cross_size as f64),
            OverlayStroke::new(1.5, cross_color),
        );
        push_circle_stroke(
            &mut ops,
            pivot_world,
            cross_size + px_to_scene(&tx, 2.0 * pixels_per_point),
            OverlayStroke::new(STROKE_WIDTH, cross_color),
        );
    } else if let Some(sel_rect) = fallback_rect {
        push_rect_stroke(&mut ops, sel_rect, 0.0, stroke, StrokeKind::Outside);
        if is_dragging {
            let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
            let corners = [
                kurbo::Point::new(sel_rect.x0, sel_rect.y0),
                kurbo::Point::new(sel_rect.x1, sel_rect.y0),
                kurbo::Point::new(sel_rect.x1, sel_rect.y1),
                kurbo::Point::new(sel_rect.x0, sel_rect.y1),
            ];
            for i in 0..4 {
                push_dashed(
                    &mut ops,
                    corners[i],
                    corners[(i + 1) % 4],
                    DASH_LEN,
                    GAP_LEN,
                    dash_stroke,
                );
            }
        }
        let handle_positions = scale_handle_positions(sel_rect);
        let handle_px = px_to_scene(&tx, HANDLE_SIZE);
        for pos in &handle_positions {
            let handle_rect = kurbo::Rect::from_center_size(
                *pos,
                kurbo::Size::new(handle_px as f64, handle_px as f64),
            );
            push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
            push_rect_stroke(
                &mut ops,
                handle_rect,
                1.0,
                OverlayStroke::new(STROKE_WIDTH, selection_color),
                StrokeKind::Outside,
            );
        }
        let top_center =
            kurbo::Point::new(sel_rect.x0 + (sel_rect.x1 - sel_rect.x0) / 2.0, sel_rect.y0);
        let rot_center = kurbo::Point::new(
            top_center.x,
            top_center.y - px_to_scene(&tx, ROTATION_OFFSET) as f64,
        );
        push_line(
            &mut ops,
            top_center,
            rot_center,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
        let rot_radius = px_to_scene(&tx, ROTATION_RADIUS);
        push_circle_fill(&mut ops, rot_center, rot_radius, text_primary);
        push_circle_stroke(
            &mut ops,
            rot_center,
            rot_radius,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
        );
    }

    ops
}

/// Generate the union bounding box and handles for multi-selection.
pub fn multi_selection_overlay_ops(
    theme: &eparts::Theme,
    scene_rects: &[kurbo::Rect],
    is_dragging: bool,
    pixels_per_point: f32,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    if scene_rects.is_empty() {
        return ops;
    }

    let mut min = kurbo::Point::new(scene_rects[0].x0, scene_rects[0].y0);
    let mut max = kurbo::Point::new(scene_rects[0].x1, scene_rects[0].y1);
    for rect in &scene_rects[1..] {
        min.x = min.x.min(rect.x0);
        min.y = min.y.min(rect.y0);
        max.x = max.x.max(rect.x1);
        max.y = max.y.max(rect.y1);
    }
    let union_rect = kurbo::Rect::new(min.x, min.y, max.x, max.y);

    let selection_color = theme.accent.primary;
    let accent_hover_color = theme.accent.hover;
    let text_primary = theme.text.primary;
    let text_faint = theme.text.faint;
    let stroke = if is_dragging {
        OverlayStroke::new(1.5, accent_hover_color)
    } else {
        OverlayStroke::new(1.5, selection_color)
    };

    push_rect_stroke(&mut ops, union_rect, 0.0, stroke, StrokeKind::Outside);
    if is_dragging {
        let dash_stroke = OverlayStroke::new(STROKE_WIDTH, text_faint);
        let corners = [
            kurbo::Point::new(union_rect.x0, union_rect.y0),
            kurbo::Point::new(union_rect.x1, union_rect.y0),
            kurbo::Point::new(union_rect.x1, union_rect.y1),
            kurbo::Point::new(union_rect.x0, union_rect.y1),
        ];
        for i in 0..4 {
            push_dashed(&mut ops, corners[i], corners[(i + 1) % 4], DASH_LEN, GAP_LEN, dash_stroke);
        }
    }

    let handle_positions = scale_handle_positions(union_rect);
    let handle_px = px_to_scene(&tx, HANDLE_SIZE * pixels_per_point);
    for pos in &handle_positions {
        let handle_rect = kurbo::Rect::from_center_size(
            *pos,
            kurbo::Size::new(handle_px as f64, handle_px as f64),
        );
        push_rect_fill(&mut ops, handle_rect, 1.0, text_primary);
        push_rect_stroke(
            &mut ops,
            handle_rect,
            1.0,
            OverlayStroke::new(STROKE_WIDTH, selection_color),
            StrokeKind::Outside,
        );
    }

    ops
}

/// Generate dashed ghost outlines for an actor at another point in time.
pub fn ghost_overlay_ops(props: &ActorProps, color: Color32) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    let dash_stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for i in 0..4 {
        push_dashed(
            &mut ops,
            world_corners[i],
            world_corners[(i + 1) % 4],
            DASH_LEN,
            GAP_LEN,
            dash_stroke,
        );
    }
    ops
}

/// Generate the reorder ghost, insertion line, badge, and shift arrows.
pub fn reorder_overlay_ops(
    theme: &eparts::Theme,
    props: &ActorProps,
    target_index: usize,
    sibling_positions: &[(String, [f32; 2])],
    tx: PreviewTransform,
    is_row: bool,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let ghost_color = theme.accent.hover;
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    for i in 0..4 {
        let next = (i + 1) % 4;
        push_line(
            &mut ops,
            world_corners[i],
            world_corners[next],
            OverlayStroke::new(1.5, ghost_color),
        );
    }

    let coords: Vec<f32> = sibling_positions
        .iter()
        .map(|(_, pos)| if is_row { pos[0] } else { pos[1] })
        .collect();
    let insertion_coord = if coords.is_empty() {
        if is_row {
            props.position[0]
        } else {
            props.position[1]
        }
    } else if target_index == 0 {
        if coords.len() == 1 {
            coords[0]
        } else {
            coords[0] - (coords[1] - coords[0]) * 0.5
        }
    } else if target_index >= coords.len() {
        if coords.len() == 1 {
            coords[0]
        } else {
            let last = coords[coords.len() - 1];
            let prev = coords[coords.len() - 2];
            last + (last - prev) * 0.5
        }
    } else {
        (coords[target_index - 1] + coords[target_index]) * 0.5
    };

    let accent_color = theme.accent.primary;
    let viewport = scene_viewport(&tx);
    let insertion_badge_pos = if is_row {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(insertion_coord as f64, 0.0));
        push_line(
            &mut ops,
            kurbo::Point::new(insertion_coord as f64, viewport.y0),
            kurbo::Point::new(insertion_coord as f64, viewport.y1),
            OverlayStroke::new(2.5, accent_color),
        );
        tx.screen_to_scene(Pos2::new(insertion_pt.x, tx.preview_rect.top() + 16.0))
    } else {
        let insertion_pt = tx.scene_to_screen(kurbo::Point::new(0.0, insertion_coord as f64));
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, insertion_coord as f64),
            kurbo::Point::new(viewport.x1, insertion_coord as f64),
            OverlayStroke::new(2.5, accent_color),
        );
        tx.screen_to_scene(Pos2::new(tx.preview_rect.left() + 16.0, insertion_pt.y))
    };

    ops.push(PreviewOverlayOp::Badge {
        pos: insertion_badge_pos,
        text: format!("→ {}", target_index + 1),
        bg: theme.overlay.badge_bg,
        fg: theme.text.primary,
        border_stroke: OverlayStroke::new(STROKE_WIDTH, accent_color),
    });

    let shift_color = theme.status.warning_subtle;
    for (i, (_, pos)) in sibling_positions.iter().enumerate() {
        let scene_pos = kurbo::Point::new(pos[0] as f64, pos[1] as f64);
        let arrow_size = px_to_scene(&tx, 8.0);
        let (dx, dy) = if i == target_index {
            if is_row {
                (arrow_size, 0.0)
            } else {
                (0.0, arrow_size)
            }
        } else if target_index > 0 && i == target_index - 1 {
            if is_row {
                (-arrow_size, 0.0)
            } else {
                (0.0, -arrow_size)
            }
        } else {
            continue;
        };
        ops.push(PreviewOverlayOp::Arrow {
            from: scene_pos,
            to: kurbo::Point::new(scene_pos.x + dx as f64, scene_pos.y + dy as f64),
            stroke: OverlayStroke::new(1.5, shift_color),
        });
    }

    let tooltip_pos = tx.preview_rect.left_top() + Vec2::new(10.0, 10.0);
    let tooltip_text = format!("Reorder: move to position {}", target_index + 1);
    let font = egui::FontId::proportional(12.0);
    let galley_size = painter_text_size(&tooltip_text, font.clone());
    let tooltip_rect = Rect::from_min_size(tooltip_pos, galley_size + Vec2::new(12.0, 8.0));
    push_screen_rect_fill(&mut ops, tooltip_rect, 4.0, theme.overlay.tooltip_bg);
    push_screen_rect_stroke(
        &mut ops,
        tooltip_rect,
        4.0,
        OverlayStroke::new(STROKE_WIDTH, accent_color),
        StrokeKind::Outside,
    );
    push_screen_text(
        &mut ops,
        tooltip_rect.min + Vec2::new(6.0, 4.0),
        Align2::LEFT_TOP,
        tooltip_text,
        font,
        theme.text.primary,
    );

    ops
}

/// Generate the hover dashed outline and actor-name tooltip.
pub fn hover_highlight_ops(
    theme: &eparts::Theme,
    hovered_actor: &str,
    hover_rect: kurbo::Rect,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let hover_color = theme.accent.ghost;
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
    push_screen_rect_fill(&mut ops, tooltip_rect, RADIUS_M, theme.overlay.badge_bg);
    push_screen_rect_stroke(
        &mut ops,
        tooltip_rect,
        RADIUS_M,
        OverlayStroke::new(STROKE_WIDTH, theme.border.default),
        StrokeKind::Outside,
    );
    push_screen_text(
        &mut ops,
        tooltip_rect.left_center() + Vec2::new(4.0, -text_size.y / 2.0),
        Align2::LEFT_TOP,
        hovered_actor,
        font,
        theme.text.primary,
    );
    ops
}

/// Generate the click-cycle indicator near the pointer.
pub fn cycle_indicator_ops(
    theme: &eparts::Theme,
    mouse_pos: Pos2,
    cycle_index: usize,
    total_candidates: usize,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    if total_candidates <= 1 {
        return ops;
    }
    let indicator_text = format!("{}/{}", cycle_index + 1, total_candidates);
    let indicator_pos = egui::pos2(mouse_pos.x + 16.0, mouse_pos.y - 8.0);
    let font = egui::FontId::proportional(12.0);
    let size = painter_text_size(&indicator_text, font.clone());
    let rect = Rect::from_center_size(indicator_pos, size + Vec2::new(6.0, 3.0));
    push_screen_rect_fill(&mut ops, rect, RADIUS_M, theme.accent.strong);
    push_screen_text(
        &mut ops,
        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
        Align2::LEFT_TOP,
        indicator_text,
        font,
        theme.text.primary,
    );
    ops
}

/// Generate motion-path lines, keyframe dots, and time labels.
pub fn motion_path_ops(
    theme: &eparts::Theme,
    timeline: &Timeline,
    selected_actors: impl Iterator<Item = String>,
    current_time_s: f64,
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    for actor in selected_actors {
        let track = match timeline.get_track(&actor) {
            Some(track) => track,
            None => continue,
        };
        let pos_track = match &track.geometry.position {
            Some(track) => track,
            None => continue,
        };
        if pos_track.keyframes().len() < 2 {
            continue;
        }
        let mut kf_points: Vec<(u64, [f32; 2])> = Vec::new();
        for (&time_ms, (val, _)) in pos_track.keyframes() {
            kf_points.push((time_ms, *val));
        }
        kf_points.sort_by_key(|(t, _)| *t);

        let path_color = theme.accent.primary.gamma_multiply(0.6);
        let path_stroke = OverlayStroke::new(1.5, path_color);
        for i in 0..kf_points.len().saturating_sub(1) {
            let p1 = kurbo::Point::new(kf_points[i].1[0] as f64, kf_points[i].1[1] as f64);
            let p2 = kurbo::Point::new(kf_points[i + 1].1[0] as f64, kf_points[i + 1].1[1] as f64);
            push_line(&mut ops, p1, p2, path_stroke);
        }

        for (time_ms, pos) in &kf_points {
            let scene_point = kurbo::Point::new(pos[0] as f64, pos[1] as f64);
            let current_time_ms = (current_time_s * 1000.0) as u64;
            let is_current = *time_ms == current_time_ms;
            let dot_color = if is_current {
                theme.status.warning
            } else {
                theme.accent.primary
            };
            let dot_radius = if is_current {
                px_to_scene(&tx, 5.0)
            } else {
                px_to_scene(&tx, 3.5)
            };
            push_circle_fill(&mut ops, scene_point, dot_radius, dot_color);
            if is_current {
                push_circle_stroke(
                    &mut ops,
                    scene_point,
                    dot_radius + px_to_scene(&tx, 2.0),
                    OverlayStroke::new(1.0, theme.status.warning),
                );
            }
            let time_label = format!("{:.1}s", *time_ms as f64 / 1000.0);
            push_text(
                &mut ops,
                kurbo::Point::new(
                    scene_point.x,
                    scene_point.y - dot_radius as f64 - px_to_scene(&tx, 4.0) as f64,
                ),
                Align2::CENTER_BOTTOM,
                time_label,
                egui::FontId::monospace(10.0),
                theme.text.muted,
            );
        }
    }
    ops
}

/// Generate the container/layout debug overlay.
pub fn layout_debug_ops(
    theme: &eparts::Theme,
    timeline: &Timeline,
    time_ms: u64,
    tx: PreviewTransform,
    draw_spacing: bool,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let style = LayoutDebugStyle::from_theme(theme);
    let viewport = scene_viewport(&tx);
    for (container_label, metadata) in timeline.container_metadata() {
        let Some(track) = timeline.get_track(container_label) else {
            continue;
        };
        let pos = track
            .geometry
            .position
            .as_ref()
            .map(|p| p.evaluate(time_ms))
            .unwrap_or([0.0, 0.0]);
        let size = track.geometry.size.as_ref().map(|s| s.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
        let half_w = size[0] / 2.0;
        let half_h = size[1] / 2.0;
        let container_rect = kurbo::Rect::new(
            (pos[0] - half_w) as f64,
            (pos[1] - half_h) as f64,
            (pos[0] + half_w) as f64,
            (pos[1] + half_h) as f64,
        );
        if container_rect.intersect(viewport).area() <= 0.0 {
            continue;
        }
        push_rect_stroke(
            &mut ops,
            container_rect,
            0.0,
            OverlayStroke::new(1.5, style.container_color),
            StrokeKind::Outside,
        );
        let kind_str = format!("{:?}", metadata.layout_type);
        push_text(
            &mut ops,
            kurbo::Point::new(
                container_rect.x0 + px_to_scene(&tx, 2.0) as f64,
                container_rect.y0 + px_to_scene(&tx, 2.0) as f64,
            ),
            Align2::LEFT_TOP,
            format!("{} ({})", container_label, kind_str),
            egui::FontId::monospace(10.0),
            style.container_color,
        );

        let layout_children = timeline.layout_children_for(container_label);
        for child in &layout_children {
            let Some(child_track) = timeline.get_track(&child.label) else {
                continue;
            };
            let child_pos = child_track
                .geometry
                .position
                .as_ref()
                .map(|p| p.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_size = child_track
                .geometry
                .size
                .as_ref()
                .map(|s| s.evaluate(time_ms))
                .unwrap_or([0.0, 0.0]);
            let child_rect = kurbo::Rect::new(
                (child_pos[0] - child_size[0] / 2.0) as f64,
                (child_pos[1] - child_size[1] / 2.0) as f64,
                (child_pos[0] + child_size[0] / 2.0) as f64,
                (child_pos[1] + child_size[1] / 2.0) as f64,
            );
            push_rect_stroke(
                &mut ops,
                child_rect,
                0.0,
                OverlayStroke::new(1.0, style.slot_color),
                StrokeKind::Outside,
            );
            let layout_s = child_track.layout_size_get(time_ms).unwrap_or([0.0, 0.0]);
            push_text(
                &mut ops,
                kurbo::Point::new(
                    child_rect.x0 + px_to_scene(&tx, 2.0) as f64,
                    child_rect.y1 - px_to_scene(&tx, 12.0) as f64,
                ),
                Align2::LEFT_BOTTOM,
                format!("{:.0}×{:.0}", layout_s[0], layout_s[1]),
                egui::FontId::monospace(9.0),
                style.size_color,
            );
        }

        if draw_spacing {
            let is_row = metadata.layout_type == animatix::timeline::LayoutType::Row;
            let gap = metadata.gap;
            let children_ordered: Vec<([f32; 2], [f32; 2])> = layout_children
                .iter()
                .filter_map(|child| {
                    let track = timeline.get_track(&child.label)?;
                    let child_pos = track
                        .geometry
                        .position
                        .as_ref()
                        .map(|p| p.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    let child_size = track
                        .geometry
                        .size
                        .as_ref()
                        .map(|s| s.evaluate(time_ms))
                        .unwrap_or([0.0, 0.0]);
                    Some((child_pos, child_size))
                })
                .collect();
            if children_ordered.len() >= 2 && gap.iter().any(|&g| g > 0.0) {
                for pair in children_ordered.windows(2) {
                    let (pos_a, size_a) = pair[0];
                    let (pos_b, size_b) = pair[1];
                    let (gap_center, gap_size) = if is_row {
                        let right_a = pos_a[0] + size_a[0] / 2.0;
                        let left_b = pos_b[0] - size_b[0] / 2.0;
                        ((right_a + left_b) / 2.0, (left_b - right_a).abs().max(1.0))
                    } else {
                        let bottom_a = pos_a[1] + size_a[1] / 2.0;
                        let top_b = pos_b[1] - size_b[1] / 2.0;
                        ((bottom_a + top_b) / 2.0, (top_b - bottom_a).abs().max(1.0))
                    };
                    if gap_size <= 0.0 {
                        continue;
                    }
                    let (gap_tl, gap_br) = if is_row {
                        (
                            kurbo::Point::new(
                                (gap_center - gap_size / 2.0) as f64,
                                (pos_a[1] - size_a[1] / 2.0) as f64,
                            ),
                            kurbo::Point::new(
                                (gap_center + gap_size / 2.0) as f64,
                                (pos_a[1] + size_a[1] / 2.0) as f64,
                            ),
                        )
                    } else {
                        (
                            kurbo::Point::new(
                                (pos_a[0] - size_a[0] / 2.0) as f64,
                                (gap_center - gap_size / 2.0) as f64,
                            ),
                            kurbo::Point::new(
                                (pos_a[0] + size_a[0] / 2.0) as f64,
                                (gap_center + gap_size / 2.0) as f64,
                            ),
                        )
                    };
                    push_rect_fill(
                        &mut ops,
                        kurbo::Rect::new(gap_tl.x, gap_tl.y, gap_br.x, gap_br.y),
                        0.0,
                        style.spacing_color,
                    );
                }
            }
        }
    }
    ops
}

fn painter_text_size(text: &str, font: FontId) -> Vec2 {
    // Approximate size without a painter. The overlay executor is responsible
    // for actual glyph layout; this keeps op generation deterministic.
    let chars = text.chars().count().max(1) as f32;
    Vec2::new(chars * font.size * 0.62, font.size * 1.2)
}

/// Generate the scene bounds outline, clipped to the visible viewport.
pub fn scene_bounds_ops(theme: &eparts::Theme, tx: PreviewTransform) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let scene_rect = kurbo::Rect::new(
        0.0,
        0.0,
        tx.scene_dimensions.width as f64,
        tx.scene_dimensions.height as f64,
    );
    let visible = scene_rect.intersect(scene_viewport(&tx));
    if visible.area() <= 0.0 {
        return ops;
    }
    push_rect_stroke(
        &mut ops,
        visible,
        0.0,
        OverlayStroke::new(STROKE_WIDTH, theme.border.strong),
        StrokeKind::Inside,
    );
    ops
}

/// Generate actor label text near each hit region.
pub fn actor_label_ops(
    theme: &eparts::Theme,
    hit_regions: &[(String, kurbo::Rect)],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    for (label, bounds) in hit_regions {
        let pos = kurbo::Point::new(
            (bounds.x0 + bounds.x1) / 2.0,
            bounds.y0 - px_to_scene(&tx, 4.0) as f64,
        );
        push_text(
            &mut ops,
            pos,
            Align2::CENTER_BOTTOM,
            label,
            egui::FontId::monospace(10.0),
            theme.text.muted,
        );
    }
    ops
}

/// Generate the canvas grid overlay.
pub fn grid_ops(
    theme: &eparts::Theme,
    tx: PreviewTransform,
    grid_size: f32,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let grid_color = theme.lines.grid;
    let grid_size = grid_size.max(1.0);
    let viewport = scene_viewport(&tx);
    let x0 = (viewport.x0 / grid_size as f64).floor() as i32 * grid_size as i32;
    let y0 = (viewport.y0 / grid_size as f64).floor() as i32 * grid_size as i32;
    let x1 = (viewport.x1 / grid_size as f64).ceil() as i32 * grid_size as i32;
    let y1 = (viewport.y1 / grid_size as f64).ceil() as i32 * grid_size as i32;

    let stroke = OverlayStroke::new(STROKE_WIDTH, grid_color);
    let mut x = x0 as f32;
    while x <= x1 as f32 {
        push_line(
            &mut ops,
            kurbo::Point::new(x as f64, viewport.y0),
            kurbo::Point::new(x as f64, viewport.y1),
            stroke,
        );
        x += grid_size;
    }
    let mut y = y0 as f32;
    while y <= y1 as f32 {
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, y as f64),
            kurbo::Point::new(viewport.x1, y as f64),
            stroke,
        );
        y += grid_size;
    }
    ops
}

/// Generate snap guide lines for a drag interaction.
pub fn snap_guide_ops(
    color: Color32,
    horizontal: &[f32],
    vertical: &[f32],
    tx: PreviewTransform,
) -> Vec<PreviewOverlayOp> {
    let mut ops = Vec::new();
    let viewport = scene_viewport(&tx);
    let stroke = OverlayStroke::new(STROKE_WIDTH, color);
    for &sy in horizontal {
        push_line(
            &mut ops,
            kurbo::Point::new(viewport.x0, sy as f64),
            kurbo::Point::new(viewport.x1, sy as f64),
            stroke,
        );
    }
    for &sx in vertical {
        push_line(
            &mut ops,
            kurbo::Point::new(sx as f64, viewport.y0),
            kurbo::Point::new(sx as f64, viewport.y1),
            stroke,
        );
    }
    ops
}

#[cfg(test)]
mod tests {
    use animatix::timeline::SceneDimensions;

    use super::*;
    use crate::app::preview::PreviewTransform;

    fn theme() -> eparts::Theme {
        eparts::Theme::dark()
    }

    fn tx() -> PreviewTransform {
        PreviewTransform::new(
            SceneDimensions {
                width: 100,
                height: 100,
            },
            Rect::from_min_max(Pos2::ZERO, Pos2::new(200.0, 200.0)),
            1.0,
            Vec2::new(50.0, 50.0),
        )
    }

    #[test]
    fn selection_overlay_emits_outline_and_handles() {
        let props = ActorProps {
            position: [50.0, 50.0],
            size: [20.0, 10.0],
            rotation: 0.0,
            pivot_offset: [0.0, 0.0],
        };
        let ops = selection_overlay_ops(&theme(), Some(&props), None, false, 1.0, tx());
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })),
            "selection overlay should emit bounding-box lines"
        );
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::CircleFill { .. })),
            "selection overlay should emit handle fills"
        );
        assert!(
            ops.iter().any(|op| matches!(op, PreviewOverlayOp::Arc { .. })),
            "selection overlay should emit rotation arcs"
        );
    }

    #[test]
    fn hover_highlight_emits_dashed_outline_and_label() {
        let rect = kurbo::Rect::new(10.0, 10.0, 30.0, 20.0);
        let ops = hover_highlight_ops(&theme(), "box", rect, tx());
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::DashedLine { .. })));
        assert!(
            ops.iter()
                .any(|op| matches!(op, PreviewOverlayOp::Screen(ScreenOverlayOp::Text { .. })))
        );
    }

    #[test]
    fn motion_paths_emit_lines_dots_and_labels() {
        // Minimal timeline with two position keyframes.
        let timeline = Timeline::new();
        let mut timeline = timeline;
        let mut track = animatix::timeline::AnimationTrack::new("box".to_string());
        track
            .geometry
            .position
            .get_or_insert_with(|| animatix::timeline::PropertyTrack::new([0.0, 0.0]))
            .add_keyframe(0, [0.0, 0.0], animatix::timeline::Easing::Linear);
        track.geometry.position.as_mut().unwrap().add_keyframe(
            1000,
            [100.0, 0.0],
            animatix::timeline::Easing::Linear,
        );
        timeline.tracks_mut().insert("box".to_string(), track);

        let ops =
            motion_path_ops(&theme(), &timeline, std::iter::once("box".to_string()), 0.0, tx());
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::CircleFill { .. })));
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }

    #[test]
    fn scene_bounds_grid_and_snap_lines_emit_expected_ops() {
        let t = tx();
        let bounds = scene_bounds_ops(&theme(), t);
        assert!(bounds.iter().any(|op| matches!(op, PreviewOverlayOp::RectStroke { .. })));

        let grid = grid_ops(&theme(), t, 20.0);
        assert!(grid.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));

        let snaps = snap_guide_ops(egui::Color32::RED, &[25.0], &[75.0], t);
        assert!(snaps.iter().any(|op| matches!(op, PreviewOverlayOp::Line { .. })));
    }

    #[test]
    fn actor_label_ops_emit_text_per_hit_region() {
        let ops = actor_label_ops(
            &theme(),
            &[("box".to_string(), kurbo::Rect::new(0.0, 0.0, 10.0, 10.0))],
            tx(),
        );
        assert!(ops.iter().any(|op| matches!(op, PreviewOverlayOp::Text { .. })));
    }
}
