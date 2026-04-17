use super::{DEFAULT_PREVIEW_SIZE, TIMELINE_HEIGHT};
use animatix::timeline::SceneDimensions;
use egui::{Color32, Stroke, Vec2};

pub(super) fn fit_preview(dimensions: SceneDimensions, available: Vec2) -> Vec2 {
    let aspect = if dimensions.width == 0 || dimensions.height == 0 {
        DEFAULT_PREVIEW_SIZE.width as f32 / DEFAULT_PREVIEW_SIZE.height as f32
    } else {
        dimensions.width as f32 / dimensions.height as f32
    };
    let width_limited_height = available.x / aspect;
    if width_limited_height <= available.y {
        Vec2::new(available.x, width_limited_height)
    } else {
        Vec2::new(available.y * aspect, available.y)
    }
}

pub(super) fn paint_timeline_scrubber(
    ui: &mut egui::Ui,
    current_time_s: &mut f64,
    duration_s: f64,
    markers_s: &[f64],
    is_playing: bool,
) -> bool {
    let desired_size = Vec2::new(ui.available_width().max(120.0), TIMELINE_HEIGHT);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    let duration_s = duration_s.max(0.1);
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    let track_rect = rect.shrink2(Vec2::new(6.0, 11.0));
    let fraction = timeline_fraction(*current_time_s, duration_s);
    let playhead_x = egui::lerp(track_rect.left()..=track_rect.right(), fraction);
    let played_rect = egui::Rect::from_min_max(
        track_rect.min,
        egui::pos2(playhead_x.max(track_rect.left()), track_rect.bottom()),
    );

    painter.rect_filled(track_rect, 7.0, Color32::from_rgb(28, 31, 38));
    painter.rect_stroke(
        track_rect,
        7.0,
        Stroke::new(1.0, Color32::from_rgb(56, 60, 73)),
        egui::StrokeKind::Outside,
    );
    painter.rect_filled(
        played_rect,
        7.0,
        if is_playing {
            Color32::from_rgb(84, 110, 255)
        } else {
            Color32::from_rgb(76, 92, 148)
        },
    );

    for tick in timeline_tick_times(duration_s) {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            timeline_fraction(tick, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 4.0),
                egui::pos2(x, track_rect.bottom() - 4.0),
            ],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 24)),
        );
    }

    for marker in markers_s {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            timeline_fraction(*marker, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 2.0),
                egui::pos2(x, track_rect.bottom() - 2.0),
            ],
            Stroke::new(1.5, Color32::from_rgb(255, 196, 92)),
        );
    }

    painter.line_segment(
        [
            egui::pos2(playhead_x, track_rect.top() - 3.0),
            egui::pos2(playhead_x, track_rect.bottom() + 3.0),
        ],
        Stroke::new(2.0, Color32::WHITE),
    );
    painter.circle_filled(
        egui::pos2(playhead_x, track_rect.center().y),
        5.0,
        Color32::WHITE,
    );

    painter.text(
        rect.left_bottom() + Vec2::new(0.0, -1.0),
        egui::Align2::LEFT_BOTTOM,
        format_time_label(0.0),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.text_color(),
    );
    painter.text(
        rect.right_bottom() + Vec2::new(0.0, -1.0),
        egui::Align2::RIGHT_BOTTOM,
        format_time_label(duration_s),
        egui::TextStyle::Small.resolve(ui.style()),
        visuals.text_color(),
    );

    if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
        *current_time_s = time_from_pointer_x(
            track_rect,
            response.interact_pointer_pos().unwrap().x,
            duration_s,
        );
        return true;
    }

    false
}

pub(super) fn timeline_fraction(current_time_s: f64, duration_s: f64) -> f32 {
    (current_time_s / duration_s.max(0.1)).clamp(0.0, 1.0) as f32
}

pub(super) fn time_from_pointer_x(rect: egui::Rect, pointer_x: f32, duration_s: f64) -> f64 {
    let width = rect.width().max(1.0);
    let normalized = ((pointer_x - rect.left()) / width).clamp(0.0, 1.0) as f64;
    normalized * duration_s.max(0.1)
}

fn timeline_tick_times(duration_s: f64) -> Vec<f64> {
    let duration_s = duration_s.max(0.1);
    let step = if duration_s <= 2.0 {
        0.25
    } else if duration_s <= 5.0 {
        0.5
    } else if duration_s <= 15.0 {
        1.0
    } else if duration_s <= 45.0 {
        5.0
    } else {
        10.0
    };

    let mut ticks = Vec::new();
    let mut tick = 0.0;
    while tick < duration_s {
        ticks.push(tick);
        tick += step;
    }
    ticks.push(duration_s);
    ticks
}

fn format_time_label(time_s: f64) -> String {
    if time_s >= 60.0 {
        let minutes = (time_s / 60.0).floor() as u32;
        let seconds = time_s % 60.0;
        format!("{minutes}:{seconds:04.1}")
    } else {
        format!("{time_s:.1}s")
    }
}
