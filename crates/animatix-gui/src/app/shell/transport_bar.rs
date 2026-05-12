use crate::app::panels::UiActions;
use crate::app::theme::*;
use crate::app::PreviewPaneState;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::SceneDimensions;
use egui::{Align, Color32, RichText, Stroke, Vec2};

/// Renders the unified transport bar at the bottom of the window.
///
/// Single-row layout: transport controls, scrubber, time, status.
pub(crate) fn transport_bar_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    scene_dimensions: SceneDimensions,
    timeline_markers: &[f64],
    actor_count: usize,
    keyframe_count: usize,
    _is_dirty: bool,
    _has_error: bool,
    diagnostics: &[Diagnostic],
    actions: &mut UiActions,
    editor_sync_enabled: bool,
    keyframe_mode: bool,
    cursor_time_s: Option<f64>,
) {
    let warning_color = Color32::from_rgb(255, 214, 102);

    let frame_response = egui::Frame::new()
        .fill(BG_BASE)
        .inner_margin(egui::Margin::symmetric(SPACE_XL as i8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

                // Play / Pause
                let play_icon = if preview.is_playing {
                    egui_phosphor::regular::PAUSE
                } else {
                    egui_phosphor::regular::PLAY
                };
                let play_color = if preview.is_playing { ACCENT_BLUE } else { TEXT_PRIMARY };
                let play_btn = egui::Button::new(
                    RichText::new(play_icon).size(16.0).color(play_color),
                )
                .fill(BG_WIDGET)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(30.0, ROW_L));
                if ui
                    .add(play_btn)
                    .on_hover_text("Play/Pause (Space)")
                    .clicked()
                {
                    actions.toggle_playback = true;
                }

                // Skip back
                let prev_btn = egui::Button::new(
                    RichText::new(egui_phosphor::regular::SKIP_BACK)
                        .size(13.0)
                        .color(TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(26.0, ROW_L));
                if ui
                    .add(prev_btn)
                    .on_hover_text("Previous keyframe (,)")
                    .clicked()
                {
                    actions.prev_keyframe = true;
                }

                // Skip forward
                let next_btn = egui::Button::new(
                    RichText::new(egui_phosphor::regular::SKIP_FORWARD)
                        .size(13.0)
                        .color(TEXT_MUTED),
                )
                .fill(Color32::TRANSPARENT)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(26.0, ROW_L));
                if ui
                    .add(next_btn)
                    .on_hover_text("Next keyframe (.)")
                    .clicked()
                {
                    actions.next_keyframe = true;
                }

                ui.add_space(SPACE_M);

                // Editor sync
                let sync_color = if editor_sync_enabled { ACCENT_BLUE } else { TEXT_MUTED };
                let sync_btn = egui::Button::new(
                    RichText::new(egui_phosphor::regular::LINK)
                        .size(FONT_SIZE_L)
                        .color(sync_color),
                )
                .fill(Color32::TRANSPARENT)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(26.0, ROW_L));
                if ui
                    .add(sync_btn)
                    .on_hover_text("Sync editor to timeline (S)")
                    .clicked()
                {
                    actions.toggle_editor_sync = true;
                }

                // Keyframe mode
                let kf_icon = if keyframe_mode {
                    egui_phosphor::regular::KEY
                } else {
                    egui_phosphor::regular::CIRCLE
                };
                let kf_color = if keyframe_mode { AMBER } else { TEXT_MUTED };
                let kf_btn = egui::Button::new(
                    RichText::new(kf_icon).size(FONT_SIZE_L).color(kf_color),
                )
                .fill(Color32::TRANSPARENT)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(26.0, ROW_L));
                if ui
                    .add(kf_btn)
                    .on_hover_text("Keyframe mode: K — create timestamps on edit")
                    .clicked()
                {
                    actions.toggle_keyframe_mode = true;
                }

                ui.add_space(10.0);

                // Scrubber — the hero element
                let right_reserve = if diagnostics.is_empty() { 180.0 } else { 260.0 };
                let scrubber_width = ui.available_width() - right_reserve;
                let mut scrub = preview.current_time_s;
                if paint_transport_scrubber(
                    ui,
                    &mut scrub,
                    preview.duration_s,
                    timeline_markers,
                    preview.is_playing,
                    scrubber_width.max(120.0),
                    cursor_time_s,
                ) {
                    actions.scrub_to = Some(scrub);
                }

                ui.add_space(10.0);

                // Right cluster: time, status (right-to-left so they hug the edge)
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(SPACE_M, 0.0);

                    let errors = diagnostics.iter().filter(|d| d.is_error()).count();
                    let warnings = diagnostics.iter().filter(|d| !d.is_error()).count();

                    // 1. Diagnostics badge (rightmost) — clickable button
                    let badge_response = if diagnostics.is_empty() {
                        crate::app::components::icon_button_colored(
                            ui,
                            egui_phosphor::regular::CHECK,
                            "Build successful",
                            GREEN,
                            GREEN,
                        )
                    } else if errors > 0 {
                        crate::app::components::badge_button(
                            ui,
                            egui_phosphor::regular::X,
                            errors,
                            RED,
                            RED,
                            "Click to toggle diagnostics panel",
                        )
                    } else {
                        crate::app::components::badge_button(
                            ui,
                            egui_phosphor::regular::WARNING,
                            warnings,
                            warning_color,
                            warning_color,
                            "Click to toggle diagnostics panel",
                        )
                    };

                    // Detailed tooltip for non-empty diagnostic sets
                    let badge_response = if diagnostics.is_empty() {
                        badge_response
                    } else {
                        badge_response.on_hover_ui(|ui| {
                            ui.strong(format!(
                                "{} diagnostic{}",
                                diagnostics.len(),
                                if diagnostics.len() == 1 { "" } else { "s" }
                            ));
                            ui.add_space(SPACE_S);
                            for d in diagnostics.iter().take(10) {
                                let prefix = if d.is_error() {
                                    format!("{} ", egui_phosphor::regular::X)
                                } else {
                                    format!("{} ", egui_phosphor::regular::WARNING)
                                };
                                let loc = if let (Some(l), Some(c)) =
                                    (d.location.line, d.location.column)
                                {
                                    format!("line {l}, col {c}: ")
                                } else {
                                    String::new()
                                };
                                let msg = d.message.lines().next().unwrap_or(&d.message);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{prefix}{loc}{msg}")).size(FONT_SIZE_M),
                                    )
                                    .selectable(false),
                                );
                            }
                            if diagnostics.len() > 10 {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!(
                                            "... and {} more",
                                            diagnostics.len() - 10
                                        ))
                                        .size(FONT_SIZE_M)
                                        .weak(),
                                    )
                                    .selectable(false),
                                );
                            }
                        })
                    };

                    if badge_response.clicked() {
                        actions.toggle_diagnostics_panel = true;
                    }

                    // 2. Time pill (left of status, closest to scrubber)
                    let stats_tooltip = format!(
                        "Resolution: {}×{}\nActors: {}\nKeyframes: {}",
                        scene_dimensions.width,
                        scene_dimensions.height,
                        actor_count,
                        keyframe_count
                    );
                    egui::Frame::new()
                        .fill(BG_SURFACE)
                        .corner_radius(egui::CornerRadius::same(RADIUS_L as u8))
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "{:.2}s / {:.2}s",
                                        preview.current_time_s,
                                        preview.duration_s
                                    ))
                                    .monospace()
                                    .size(FONT_SIZE_L)
                                    .color(TEXT_PRIMARY),
                                )
                                .selectable(false),
                            );
                        })
                        .response
                        .on_hover_text(stats_tooltip);
                });
            });
        });

    // Subtle top hairline
    let bar_rect = frame_response.response.rect;
    ui.painter().line_segment(
        [
            egui::pos2(bar_rect.left(), bar_rect.top()),
            egui::pos2(bar_rect.right(), bar_rect.top()),
        ],
        Stroke::new(1.0, BORDER),
    );
}

/// Paint the transport bar's full-width timeline scrubber with keyframe markers.
fn paint_transport_scrubber(
    ui: &mut egui::Ui,
    current_time_s: &mut f64,
    duration_s: f64,
    markers_s: &[f64],
    is_playing: bool,
    width: f32,
    cursor_time_s: Option<f64>,
) -> bool {
    let height = ROW_S;
    let desired_size = Vec2::new(width.max(120.0), height);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    let duration_s = duration_s.max(0.1);
    let painter = ui.painter_at(rect);
    let track_rect = rect.shrink2(Vec2::new(SPACE_S, 5.0));
    let fraction = crate::app::preview::timeline_fraction(*current_time_s, duration_s);
    let playhead_x = egui::lerp(track_rect.left()..=track_rect.right(), fraction);

    // Track
    painter.rect_filled(track_rect, RADIUS_M, BG_SURFACE);
    painter.rect_stroke(
        track_rect,
        RADIUS_M,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );

    // Played portion
    let played_rect = egui::Rect::from_min_max(
        track_rect.min,
        egui::pos2(playhead_x.max(track_rect.left()), track_rect.bottom()),
    );
    let played_color = if is_playing { ACCENT_BLUE } else { Color32::from_rgb(50, 60, 100) };
    painter.rect_filled(played_rect, RADIUS_M, played_color);

    // Tick marks
    for tick in crate::app::preview::timeline_tick_times(duration_s) {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            crate::app::preview::timeline_fraction(tick, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 3.0),
                egui::pos2(x, track_rect.bottom() - 3.0),
            ],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 12)),
        );
    }

    // Keyframe markers
    for marker in markers_s {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            crate::app::preview::timeline_fraction(*marker, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 2.0),
                egui::pos2(x, track_rect.bottom() - 2.0),
            ],
            Stroke::new(1.5, AMBER),
        );
    }

    // Cursor indicator (editor → timeline sync)
    if let Some(cursor_t) = cursor_time_s {
        if cursor_t >= 0.0 && cursor_t <= duration_s {
            let cursor_fraction = crate::app::preview::timeline_fraction(cursor_t, duration_s);
            let cursor_x = egui::lerp(track_rect.left()..=track_rect.right(), cursor_fraction);
            let top = track_rect.top() - 4.0;
            let y = track_rect.top() - 1.0;
            let color = Color32::from_rgb(100, 220, 255);
            painter.line_segment(
                [egui::pos2(cursor_x, top), egui::pos2(cursor_x - 3.0, y)],
                Stroke::new(1.0, color),
            );
            painter.line_segment(
                [egui::pos2(cursor_x, top), egui::pos2(cursor_x + 3.0, y)],
                Stroke::new(1.0, color),
            );
            painter.line_segment(
                [egui::pos2(cursor_x - 3.0, y), egui::pos2(cursor_x + 3.0, y)],
                Stroke::new(1.0, color),
            );
        }
    }

    // Refined playhead — thin line, no circle
    painter.line_segment(
        [
            egui::pos2(playhead_x, track_rect.top() - 3.0),
            egui::pos2(playhead_x, track_rect.bottom() + 3.0),
        ],
        Stroke::new(1.5, TEXT_PRIMARY),
    );

    // Interaction
    if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
        *current_time_s = crate::app::preview::time_from_pointer_x(
            track_rect,
            response.interact_pointer_pos().unwrap().x,
            duration_s,
        );
        return true;
    }

    false
}
