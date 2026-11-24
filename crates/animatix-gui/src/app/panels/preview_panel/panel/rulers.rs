//! Preview ruler marks, labels, and drag-to-create guides.

use super::*;

const MAJOR_TICK: f32 = 8.0;
const MINOR_TICK: f32 = 4.0;
const LABEL_PAD: f32 = 2.0;
const LABEL_GAP: f32 = 3.0;

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

pub(super) fn ruler_inset(ui: &egui::Ui) -> Vec2 {
    // Measure the active UI font so fallback fonts cannot consume the label lane.
    let size = ui
        .painter()
        .layout_no_wrap("-8888".into(), TextRole::Micro.font_id(), egui::Color32::PLACEHOLDER)
        .size();
    let allowance = LABEL_PAD + MAJOR_TICK + LABEL_GAP + 1.0;
    Vec2::new((size.x + allowance).ceil(), (size.y + allowance).ceil())
}

pub(super) fn draw_rulers(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    inset: Vec2,
    theme: &eparts::Theme,
) -> Option<GuideLine> {
    let corner = egui::Rect::from_min_max(preview_rect.min - inset, preview_rect.min);
    let h = egui::Rect::from_min_max(corner.right_top(), preview_rect.right_top());
    let v = egui::Rect::from_min_max(corner.left_bottom(), preview_rect.left_bottom());
    let scene_tl = ctx.preview_screen_to_scene(preview_rect, preview_rect.left_top());
    let scene_br = ctx.preview_screen_to_scene(preview_rect, preview_rect.right_bottom());
    draw_strip(ui.painter(), theme, corner);
    draw_ruler(ui, theme, h, Axis::Horizontal, scene_tl.x..=scene_br.x, |value| {
        ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(value, scene_tl.y))
            .x
    });
    draw_ruler(ui, theme, v, Axis::Vertical, scene_tl.y..=scene_br.y, |value| {
        ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(scene_tl.x, value))
            .y
    });
    handle_ruler_drag(ctx, ui, preview_rect, h, v)
}

fn draw_strip(painter: &egui::Painter, theme: &eparts::Theme, rect: egui::Rect) {
    painter.rect_filled(rect, 0.0, theme.palette.surface.panel);
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        egui::StrokeKind::Inside,
    );
}

fn draw_ruler(
    ui: &egui::Ui,
    theme: &eparts::Theme,
    rect: egui::Rect,
    axis: Axis,
    scene: std::ops::RangeInclusive<f64>,
    to_screen: impl Fn(f64) -> f32,
) {
    use Axis::{Horizontal, Vertical};
    draw_strip(ui.painter(), theme, rect);
    let (lo, hi) = (*scene.start(), *scene.end());
    let (first, last) = match axis {
        Horizontal => (rect.left(), rect.right()),
        Vertical => (rect.top(), rect.bottom()),
    };
    let interval = f64::from(nice_tick_interval((hi - lo) as f32, (last - first) / 60.0)).max(1.0);
    // Iterate by index so the major cadence is stable across zero and fractional pans.
    let start = (lo / interval).ceil();
    let end = (hi / interval).floor();
    if !start.is_finite() || !end.is_finite() {
        return;
    }
    let mut previous_label_end = f32::NEG_INFINITY;
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    for index in (start as i64)..=(end as i64) {
        let value = index as f64 * interval;
        let along = to_screen(value);
        if !(first..=last).contains(&along) {
            continue;
        }
        let major = index.rem_euclid(5) == 0;
        let depth = if major { MAJOR_TICK } else { MINOR_TICK };
        let end_points = match axis {
            Horizontal => [
                egui::pos2(along, rect.bottom()),
                egui::pos2(along, rect.bottom() - depth),
            ],
            Vertical => [
                egui::pos2(rect.right(), along),
                egui::pos2(rect.right() - depth, along),
            ],
        };
        let color = if major {
            theme.palette.text.secondary
        } else {
            theme.palette.text.muted
        };
        painter.line_segment(end_points, egui::Stroke::new(STROKE_WIDTH, color));
        if major {
            let galley = painter.layout_no_wrap(
                format!("{value:.0}"),
                TextRole::Micro.font_id(),
                theme.palette.text.muted,
            );
            if let Some(label) =
                ruler_label_rect(rect, axis, along, galley.size(), previous_label_end)
            {
                painter.galley(label.min, galley, theme.palette.text.muted);
                previous_label_end = match axis {
                    Horizontal => label.right(),
                    Vertical => label.bottom(),
                };
            }
        }
    }
}

fn ruler_label_rect(
    strip: egui::Rect,
    axis: Axis,
    along: f32,
    size: Vec2,
    previous_end: f32,
) -> Option<egui::Rect> {
    let mut lane = strip.shrink(LABEL_PAD);
    let (anchor, align) = match axis {
        Axis::Horizontal => {
            lane.max.y = strip.bottom() - MAJOR_TICK - LABEL_GAP;
            (egui::pos2(along, lane.center().y), egui::Align2::CENTER_CENTER)
        },
        Axis::Vertical => {
            lane.max.x = strip.right() - MAJOR_TICK - LABEL_GAP;
            (egui::pos2(lane.right(), along), egui::Align2::RIGHT_CENTER)
        },
    };
    let label = align.anchor_size(anchor, size);
    let start = match axis {
        Axis::Horizontal => label.left(),
        Axis::Vertical => label.top(),
    };
    (lane.contains_rect(label) && start >= previous_end + LABEL_GAP).then_some(label)
}

fn handle_ruler_drag(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    h_ruler_rect: egui::Rect,
    v_ruler_rect: egui::Rect,
) -> Option<GuideLine> {
    let drag_id = ui.scope_id().with("guide_ruler_drag_v2");
    let h = ui.allocate_rect(h_ruler_rect, egui::Sense::drag());
    let v = ui.allocate_rect(v_ruler_rect, egui::Sense::drag());

    let started = if h.drag_started() {
        Some(Axis::Horizontal)
    } else if v.drag_started() {
        Some(Axis::Vertical)
    } else {
        None
    };
    if let Some(axis) = started
        && let Some(origin) = ui.input(|i| i.pointer.press_origin())
    {
        ui.data_mut(|d| d.insert_temp(drag_id, (axis, origin)));
    }
    let (axis, start): (Axis, egui::Pos2) = ui.data(|d| d.get_temp(drag_id))?;
    let stopped = h.drag_stopped() || v.drag_stopped() || ui.input(|i| i.pointer.any_released());
    let pointer = ui.input(|i| i.pointer.latest_pos());
    let guide = pointer.map(|position| {
        let scene = ctx.preview_screen_to_scene(preview_rect, position);
        match axis {
            Axis::Horizontal => GuideLine::Horizontal(scene.y as f32),
            Axis::Vertical => GuideLine::Vertical(scene.x as f32),
        }
    });
    if !stopped {
        return guide;
    }

    ui.data_mut(|d| d.remove::<(Axis, egui::Pos2)>(drag_id));
    // The result is persisted in scene units only when dropped in the canvas.
    if let Some(position) = pointer
        && preview_rect.contains(position)
        && (position - start).length() >= 5.0
    {
        match guide {
            Some(GuideLine::Horizontal(y)) => ctx.preview.guides.horizontal_guides.push(y),
            Some(GuideLine::Vertical(x)) => ctx.preview.guides.vertical_guides.push(x),
            None => {},
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect};

    #[test]
    fn signed_four_digit_labels_clear_major_ticks_and_strip_edges() {
        let ctx = egui::Context::default();
        crate::fonts::install_fonts(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let galley = ui.painter().layout_no_wrap(
                "-4321".into(),
                TextRole::Micro.font_id(),
                egui::Color32::WHITE,
            );
            let inset = ruler_inset(ui);
            let h = Rect::from_min_size(Pos2::ZERO, Vec2::new(140.0, inset.y));
            let v = Rect::from_min_size(Pos2::ZERO, Vec2::new(inset.x, 140.0));
            let size = galley.size();
            let x = ruler_label_rect(h, Axis::Horizontal, 60.0, size, 0.0).unwrap();
            let y = ruler_label_rect(v, Axis::Vertical, 60.0, size, 0.0).unwrap();
            assert!(x.bottom() + LABEL_GAP <= h.bottom() - MAJOR_TICK);
            assert!(y.right() + LABEL_GAP <= v.right() - MAJOR_TICK);
            assert!(ruler_label_rect(h, Axis::Horizontal, 1.0, size, 0.0).is_none());
            assert!(ruler_label_rect(h, Axis::Horizontal, 139.0, size, 0.0).is_none());
            assert!(ruler_label_rect(v, Axis::Vertical, 139.0, size, 0.0).is_none());
            assert!(ruler_label_rect(v, Axis::Vertical, 60.0, size, y.bottom()).is_none());
        });
        output.textures_delta.clear();
    }
}
