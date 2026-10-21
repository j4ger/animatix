//! Timeline ruler, zoom bar, edge interaction, and playhead rendering.

use std::hash::Hash;

use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::TimelineCoords;
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, PlaybackCommand};
use crate::app::components::button::Button;
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;

/// Helper to snap a time `nt` to nearby speech words or markers if within tolerance (~0.06s),
/// otherwise falling back to snapping to the standard FPS grid.
pub fn snap_to_speech_or_fps(
    nt: f64,
    snap_fps: f32,
    speech_targets: &[(f64, String)],
) -> (f64, Option<String>) {
    let tolerance = 0.06; // ~3.5 frames at 60fps
    let mut closest_match: Option<(f64, &str, f64)> = None;

    for (target_t, label) in speech_targets {
        let diff = (nt - target_t).abs();
        if diff <= tolerance
            && closest_match.map(|(_, _, best_diff)| diff < best_diff).unwrap_or(true)
        {
            closest_match = Some((*target_t, label.as_str(), diff));
        }
    }

    if let Some((target_t, label, _)) = closest_match {
        (target_t, Some(label.to_string()))
    } else {
        let fps_snapped = (nt * snap_fps as f64).round() / snap_fps as f64;
        (fps_snapped, None)
    }
}

/// Handle click-and-drag scrubbing on a timeline bar.
pub(crate) fn bar_interaction(
    ui: &egui::Ui,
    bar_rect: Rect,
    id_salt: impl Hash,
    cmds: &mut ActionQueue,
    x_to_time: impl Fn(f32) -> f64,
) {
    let bar_id = ui.id().with(id_salt);
    let response = ui.interact(bar_rect, bar_id, Sense::click_and_drag());
    if response.clicked() || response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            let new_time = x_to_time(pos.x);
            cmds.push_back(PlaybackCommand::ScrubTo(new_time).into());
        }
    }
}

/// Timeline-local controls: zoom. Playback transport is global (`shell::transport`).
pub(crate) fn render_timeline_zoom_bar(
    ui: &mut egui::Ui,
    strip_rect: egui::Rect,
    preview: &mut PreviewPaneState,
) {
    let theme = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    ui.scope_builder(egui::UiBuilder::new().max_rect(strip_rect), |ui| {
        ui.painter().rect_filled(strip_rect, 0.0, theme.palette.surface.base);
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(sp.base.space_2);
            let zoom_text = format!("{:.0}%", preview.timeline_zoom * 100.0);
            let zoom_btn = ui.add(Button::ghost(zoom_text).with_size(eparts::Size::Sm));
            text_tooltip(ui, zoom_btn.id.with("reset_zoom_tip"), &zoom_btn, "Reset timeline zoom");
            if zoom_btn.clicked() {
                preview.timeline_zoom = 1.0;
                preview.timeline_scroll_offset = 0.0;
            }
            if ui
                .add(
                    Button::ghost("")
                        .with_icon(egui_phosphor::regular::MINUS)
                        .with_size(eparts::Size::Sm)
                        .with_tooltip("Zoom out (Ctrl+wheel)"),
                )
                .clicked()
            {
                let new_zoom = (preview.timeline_zoom * 0.8).max(0.25);
                if new_zoom <= 1.0 {
                    preview.timeline_scroll_offset = 0.0;
                }
                preview.timeline_zoom = new_zoom;
            }
            if ui
                .add(
                    Button::ghost("")
                        .with_icon(egui_phosphor::regular::PLUS)
                        .with_size(eparts::Size::Sm)
                        .with_tooltip("Zoom in (Ctrl+wheel)"),
                )
                .clicked()
            {
                let new_zoom = (preview.timeline_zoom * 1.25).min(8.0);
                if new_zoom <= 1.0 {
                    preview.timeline_scroll_offset = 0.0;
                }
                preview.timeline_zoom = new_zoom;
            }
        });
    });
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
    while t <= coords.scroll_s + coords.visible_s {
        let x = coords.time_to_x(t);
        if x >= coords.bar_origin_x && x <= coords.bar_origin_x + coords.bar_width {
            painter.line_segment(
                [Pos2::new(x, ruler_bot - 6.0), Pos2::new(x, ruler_bot)],
                Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            );
            painter.text(
                Pos2::new(x, ruler_top + sp.timeline.ruler_height * 0.35),
                Align2::CENTER_CENTER,
                if tick_step >= 1.0 {
                    format!("{:.0}s", t)
                } else {
                    format!("{:.1}s", t)
                },
                FontId::monospace(10.0), // 10px mono: no TextRole
                theme.palette.text.muted,
            );
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
        "Region",
        TextRole::Micro.font_id(),
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
        let sr = ui.interact(sh, ui.id().with("range_start_handle"), Sense::click_and_drag());
        if sr.dragged() {
            if let Some(pos) = sr.interact_pointer_pos() {
                let end = preview.playback.loop_end_s.unwrap_or(duration_s);
                preview.playback.loop_start_s = Some(coords.x_to_time(pos.x).min(end - 0.05));
            }
        }
        painter.rect_filled(sh, RADIUS_S, theme.palette.accent.primary);

        let eh = Rect::from_center_size(Pos2::new(wy, range_bar.center().y), hs);
        let er = ui.interact(eh, ui.id().with("range_end_handle"), Sense::click_and_drag());
        if er.dragged() {
            if let Some(pos) = er.interact_pointer_pos() {
                let start = preview.playback.loop_start_s.unwrap_or(0.0);
                preview.playback.loop_end_s = Some(coords.x_to_time(pos.x).max(start + 0.05));
            }
        }
        painter.rect_filled(eh, RADIUS_S, theme.palette.accent.primary);

        // Reciprocal enforcement: ensure end > start + 0.05
        if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s) {
            if le <= ls + 0.05 {
                let mid = (ls + le) / 2.0;
                preview.playback.loop_start_s = Some((mid - 0.025).max(0.0));
                preview.playback.loop_end_s = Some((mid + 0.025).min(duration_s));
            }
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
            "Enable loop to set region",
            FontId::monospace(10.0), // 10px mono: no TextRole
            theme.palette.text.muted,
        );
    }
}

/// Render the global vertical playhead line, or directional edge indicators when off-screen.
pub(crate) fn render_playhead(
    painter: &egui::Painter,
    playhead_x: f32,
    ruler_top: f32,
    content_bottom: f32,
    coords: &TimelineCoords,
    theme: eparts::Theme,
) {
    if playhead_x >= coords.bar_origin_x
        && playhead_x <= coords.bar_origin_x + coords.bar_width + 2.0
    {
        painter.line_segment(
            [
                Pos2::new(playhead_x, ruler_top),
                Pos2::new(playhead_x, content_bottom),
            ],
            Stroke::new(1.5, theme.palette.status.warning),
        );
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
mod tests {
    use super::*;

    #[test]
    fn test_snap_to_speech_or_fps() {
        let targets = vec![
            (1.50, "@intro".to_string()),
            (2.25, "\"hello\"".to_string()),
        ];

        // Close to @intro (within 0.06s tolerance) -> snaps to @intro
        let (snapped, label) = snap_to_speech_or_fps(1.52, 60.0, &targets);
        assert!((snapped - 1.50).abs() < 1e-4);
        assert_eq!(label.as_deref(), Some("@intro"));

        // Close to "hello" -> snaps to "hello"
        let (snapped, label) = snap_to_speech_or_fps(2.22, 60.0, &targets);
        assert!((snapped - 2.25).abs() < 1e-4);
        assert_eq!(label.as_deref(), Some("\"hello\""));

        // Far from speech targets -> falls back to FPS grid snap
        let (snapped, label) = snap_to_speech_or_fps(3.821, 60.0, &targets);
        let expected_fps_snap = (3.821 * 60.0_f64).round() / 60.0_f64;
        assert!((snapped - expected_fps_snap).abs() < 1e-4);
        assert_eq!(label, None);
    }
}
