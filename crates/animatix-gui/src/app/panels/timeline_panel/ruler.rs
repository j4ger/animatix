//! Timeline ruler, zoom bar, edge interaction, and playhead rendering.

use egui::{Align2, Pos2, Rect, Sense, Stroke, Vec2};

use super::TimelineCoords;
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::button::Button;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;

/// Shift-scrub within six screen points of an authored keyframe. The sorted
/// times are collected once per gesture, including composition scene offsets.
pub(super) fn scrub_ruler(
    ui: &egui::Ui,
    bar: Rect,
    coords: &TimelineCoords,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    times: impl FnOnce() -> Vec<f64>,
) {
    let id = ui.scope_id().with("ruler");
    let response = ui.interact(bar, id, Sense::click_and_drag());
    let cache = id.with("snap_times");
    let shift = ui.input(|i| i.modifiers.shift);
    let dragging = response.dragged() || response.clicked();
    if dragging && let Some(pos) = response.interact_pointer_pos() {
        let raw = coords.x_to_time(pos.x);
        let target = if shift {
            ui.data_mut(|data| {
                let sorted = data.get_temp_mut_or_insert_with(cache, times);
                nearest_keyframe(sorted, raw, coords.visible_s * 6.0 / f64::from(coords.bar_width))
            })
        } else {
            None
        };
        if let Some(time) = target {
            if response.clicked()
                || response.drag_started()
                || preview.playback.current_time_s() != time
            {
                preview.keyframe_landing = Some((time, std::time::Instant::now()));
            }
        } else {
            preview.keyframe_landing = None;
        }
        commands.push_back(Command::ScrubTo(target.unwrap_or(raw)).into());
    }
    if !response.dragged() || !shift {
        ui.data_mut(|d| d.remove::<Vec<f64>>(cache));
    }
    if let Some((time, start)) = preview.keyframe_landing {
        let elapsed = start.elapsed().as_secs_f32();
        if elapsed < eparts::tokens::motion::FAST
            && eparts::motion_preference(ui) == eparts::MotionPreference::Full
        {
            let x = coords.time_to_x(time);
            let color = eparts::theme(ui)
                .palette
                .text
                .primary
                .gamma_multiply(1.0 - elapsed / eparts::tokens::motion::FAST);
            let painter = ui.painter_at(bar);
            for edge in [x - 3.0, x + 3.0] {
                painter.vline(edge, bar.top()..=bar.bottom(), Stroke::new(1.0, color));
            }
            ui.ctx().request_repaint();
        } else {
            preview.keyframe_landing = None;
        }
    }
}

fn nearest_keyframe(sorted: &[f64], time: f64, tolerance: f64) -> Option<f64> {
    let after = sorted.partition_point(|t| *t < time);
    sorted[after.saturating_sub(1)..(after + 1).min(sorted.len())]
        .iter()
        .copied()
        .min_by(|a, b| (a - time).abs().total_cmp(&(b - time).abs()))
        .filter(|t| (t - time).abs() <= tolerance)
}

/// Timeline-local controls: zoom. Playback transport is global (`shell::transport`).
pub(super) fn render_timeline_zoom_bar(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    size: eparts::Size,
) {
    use egui_phosphor::regular as icons;
    let out = (icons::MINUS, 0.8, "gui-timeline-zoom-out");
    let zoom_in = (icons::PLUS, 1.25, "gui-timeline-zoom-in");
    let (first, last) = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        (zoom_in, out)
    } else {
        (out, zoom_in)
    };

    zoom_step(ui, preview, first, size);
    let zoom_text = format!("{:.0}%", preview.timeline_zoom * 100.0);
    if ui
        .add(
            Button::secondary(zoom_text)
                .with_size(size)
                .with_tooltip(crate::app::i18n::text(ui, "gui-timeline-reset-zoom")),
        )
        .clicked()
    {
        preview.timeline_zoom = 1.0;
        preview.timeline_scroll_offset = 0.0;
    }
    zoom_step(ui, preview, last, size);
}

/// Reserve all three controls before laying out the toolbar to avoid wrapping
/// a second, almost-empty row when the paragraph control consumes spare room.
pub(super) fn zoom_controls_width(ui: &egui::Ui, size: eparts::Size) -> f32 {
    let density = eparts::density(ui);
    let reset_label = ui.painter().layout_no_wrap(
        "800%".into(),
        TextRole::BodyS.font_id(),
        egui::Color32::PLACEHOLDER,
    );
    2.0 * (size.row_height_for(density) + size.pad_x_for(density) + ui.spacing().item_spacing.x)
        + reset_label.size().x
}

fn zoom_step(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    (icon, factor, tooltip): (&'static str, f32, &'static str),
    size: eparts::Size,
) {
    if ui
        .add(
            Button::icon(icon)
                .with_size(size)
                .with_tooltip(crate::app::i18n::text(ui, tooltip)),
        )
        .clicked()
    {
        preview.timeline_zoom = (preview.timeline_zoom * factor).clamp(0.25, 8.0);
        if preview.timeline_zoom <= 1.0 {
            preview.timeline_scroll_offset = 0.0;
        }
    }
}

pub(super) fn marker_band(ruler_bar: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(ruler_bar.left(), ruler_bar.bottom() - 9.0),
        ruler_bar.right_bottom(),
    )
}

/// Render the time scale ruler ticks and time labels.
pub(crate) fn render_ruler(
    painter: &egui::Painter,
    scroll_rect: Rect,
    ruler_top: f32,
    ruler_bot: f32,
    coords: &TimelineCoords,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(scroll_rect.left(), ruler_top),
            Pos2::new(scroll_rect.right(), ruler_bot),
        ),
        0.0,
        theme.palette.surface.surface,
    );
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(scroll_rect.left(), ruler_top),
            Pos2::new(coords.bar_origin_x, ruler_bot),
        ),
        0.0,
        theme.palette.surface.base,
    );

    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_4, (ruler_top + ruler_bot) * 0.5),
        Align2::LEFT_CENTER,
        crate::app::i18n::text_for(
            crate::app::i18n::current_language(painter.ctx()),
            "gui-timeline-tracks",
        ),
        TextRole::BodyMedium.font_id(),
        theme.palette.text.secondary,
    );

    let bar = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, ruler_top),
        Pos2::new(scroll_rect.right(), ruler_bot),
    );
    let band = marker_band(bar);
    let labels = Rect::from_min_max(bar.min, Pos2::new(bar.right(), band.top()));
    let labels_painter = painter.with_clip_rect(labels.intersect(painter.clip_rect()));
    let font = TextRole::Mono.font_id();
    let color = theme.palette.text.muted;
    let tick_step = if coords.visible_s <= 2.0 {
        0.25
    } else if coords.visible_s <= 5.0 {
        0.5
    } else if coords.visible_s <= 15.0 {
        1.0
    } else if coords.visible_s <= 45.0 {
        5.0
    } else {
        10.0
    };
    let mut t = (coords.scroll_s / tick_step).floor() * tick_step;
    let mut label_edge = bar.left();
    while t <= coords.scroll_s + coords.visible_s {
        let x = coords.time_to_x(t);
        if x >= bar.left() && x <= bar.right() {
            painter.line_segment(
                [Pos2::new(x, ruler_bot - 4.0), Pos2::new(x, ruler_bot)],
                Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            );
            let left = (x + 4.0).max(bar.left() + 8.0);
            if left >= label_edge {
                let text = if tick_step >= 1.0 {
                    format!("{t:.0}s")
                } else if tick_step >= 0.5 {
                    format!("{t:.1}s")
                } else {
                    format!("{t:.2}s")
                };
                let galley = labels_painter.layout_no_wrap(text, font.clone(), color);
                if left + galley.size().x <= bar.right() {
                    label_edge = left + galley.size().x + 4.0;
                    labels_painter.galley(
                        Pos2::new(left, labels.center().y - galley.size().y * 0.5),
                        galley,
                        color,
                    );
                }
            }
        }
        t += tick_step;
    }
}

/// Render the range slider for the work/export region.
pub(crate) fn render_range_slider(
    ui: &mut egui::Ui,
    rs_top: f32,
    rs_bot: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    duration_s: f64,
    preview: &mut PreviewPaneState,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    painter.rect_filled(
        Rect::from_min_max(
            Pos2::new(scroll_rect.left(), rs_top),
            Pos2::new(coords.bar_origin_x, rs_bot),
        ),
        0.0,
        theme.palette.surface.base,
    );
    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_2, (rs_top + rs_bot) / 2.0),
        Align2::LEFT_CENTER,
        crate::app::i18n::text(ui, "gui-timeline-loop"),
        TextRole::BodyS.font_id(),
        theme.palette.text.muted,
    );

    let range_bar = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, rs_top),
        Pos2::new(scroll_rect.right(), rs_bot),
    );
    let loop_active =
        preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();

    painter.rect_filled(range_bar, RADIUS_S, theme.palette.surface.widget);

    if loop_active {
        // Loop is active — show draggable range handles
        let ws = preview.playback.loop_start_s.unwrap_or(0.0);
        let we = preview.playback.loop_end_s.unwrap_or(duration_s);
        let wx = coords.time_to_x(ws);
        let wy = coords.time_to_x(we);

        if (wy - wx).abs() > 2.0 {
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(wx, range_bar.top() + 2.0),
                    Pos2::new(wy, range_bar.bottom() - 2.0),
                ),
                RADIUS_S,
                theme.palette.accent.primary.linear_multiply(0.3),
            );
        }

        let hs = Vec2::new(12.0, sp.timeline.range_height);
        let sh = Rect::from_center_size(Pos2::new(wx, range_bar.center().y), hs);
        let sr = ui.interact(sh, ui.scope_id().with("range_start_handle"), Sense::click_and_drag());
        if sr.dragged()
            && let Some(pos) = sr.interact_pointer_pos()
        {
            let end = preview.playback.loop_end_s.unwrap_or(duration_s);
            preview.playback.loop_start_s = Some(coords.x_to_time(pos.x).min(end - 0.05));
        }
        painter.rect_filled(sh, RADIUS_S, theme.palette.accent.primary);

        let eh = Rect::from_center_size(Pos2::new(wy, range_bar.center().y), hs);
        let er = ui.interact(eh, ui.scope_id().with("range_end_handle"), Sense::click_and_drag());
        if er.dragged()
            && let Some(pos) = er.interact_pointer_pos()
        {
            let start = preview.playback.loop_start_s.unwrap_or(0.0);
            preview.playback.loop_end_s = Some(coords.x_to_time(pos.x).max(start + 0.05));
        }
        painter.rect_filled(eh, RADIUS_S, theme.palette.accent.primary);

        // Reciprocal enforcement: ensure end > start + 0.05
        if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s)
            && le <= ls + 0.05
        {
            let mid = (ls + le) / 2.0;
            preview.playback.loop_start_s = Some((mid - 0.025).max(0.0));
            preview.playback.loop_end_s = Some((mid + 0.025).min(duration_s));
        }
    } else {
        // Loop is off — show full-duration static indicator
        painter.rect_filled(
            range_bar.shrink2(Vec2::new(0.0, 2.0)),
            RADIUS_S,
            theme.palette.surface.widget,
        );
        let mid = range_bar.center();
        painter.text(
            mid,
            Align2::CENTER_CENTER,
            "Enable a loop to adjust its range",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
    }
}

/// Render the global vertical playhead line, or directional edge indicators when off-screen.
pub(crate) fn render_playhead(
    painter: &egui::Painter,
    playhead_x: f32,
    ruler_top: f32,
    upper_bottom: f32,
    lower: std::ops::Range<f32>,
    coords: &TimelineCoords,
    theme: eparts::Theme,
) {
    if playhead_x >= coords.bar_origin_x
        && playhead_x <= coords.bar_origin_x + coords.bar_width + 2.0
    {
        let tab =
            Rect::from_center_size(Pos2::new(playhead_x, ruler_top + 3.0), Vec2::new(7.0, 6.0));
        painter.rect_filled(tab, 2.0, theme.palette.status.warning);
        let stroke = Stroke::new(1.5, theme.palette.status.warning);
        for vertical in [tab.bottom()..upper_bottom, lower] {
            if !vertical.is_empty() {
                painter.vline(playhead_x, vertical.start..=vertical.end, stroke);
            }
        }
    } else if playhead_x < coords.bar_origin_x {
        // Off-screen to the left: draw left-pointing arrow at visible edge
        let tip_x = coords.bar_origin_x + 6.0;
        let tip_y = ruler_top + 8.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(tip_x, tip_y - 4.0),
                Pos2::new(tip_x - 6.0, tip_y),
                Pos2::new(tip_x, tip_y + 4.0),
            ],
            theme.palette.status.warning,
            Stroke::NONE,
        ));
    } else if playhead_x > coords.bar_origin_x + coords.bar_width {
        // Off-screen to the right: draw right-pointing arrow at visible edge
        let tip_x = coords.bar_origin_x + coords.bar_width - 6.0;
        let tip_y = ruler_top + 8.0;
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(tip_x, tip_y - 4.0),
                Pos2::new(tip_x + 6.0, tip_y),
                Pos2::new(tip_x, tip_y + 4.0),
            ],
            theme.palette.status.warning,
            Stroke::NONE,
        ));
    }
}

#[cfg(test)]
mod snap_tests {
    use super::nearest_keyframe;

    #[test]
    fn snap_only_within_the_visible_tolerance() {
        let keys = [0.5, 1.0, 1.04];
        assert_eq!(nearest_keyframe(&[], 0.0, 0.01), None);
        assert_eq!(nearest_keyframe(&keys, 0.49, 0.02), Some(0.5));
        assert_eq!(nearest_keyframe(&keys, 1.03, 0.02), Some(1.04));
        assert_eq!(nearest_keyframe(&keys, 1.05, 0.02), Some(1.04));
        assert_eq!(nearest_keyframe(&keys, 0.7, 0.02), None);
    }
}
