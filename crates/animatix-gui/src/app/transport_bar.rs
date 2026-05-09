use super::*;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::SceneDimensions;

/// Renders the unified transport bar at the bottom of the window.
///
/// Row 1: Play/Pause, Prev/Next keyframe, full-width scrubber, time display
/// Row 2: Resolution, actor count, keyframe count, build status
pub(super) fn transport_bar_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    scene_dimensions: SceneDimensions,
    timeline_markers: &[f64],
    actor_count: usize,
    keyframe_count: usize,
    is_dirty: bool,
    has_error: bool,
    diagnostics: &[Diagnostic],
    actions: &mut UiActions,
    editor_sync_enabled: bool,
    keyframe_mode: bool,
    cursor_time_s: Option<f64>,
) {
    let bg = Color32::from_rgb(16, 18, 22);
    let border = Color32::from_rgb(32, 36, 44);

    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Row 1: Transport controls + scrubber + time
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                // Play/Pause button
                let play_label = if preview.is_playing { "⏸" } else { "▶" };
                let play_btn = egui::Button::new(
                    RichText::new(play_label).size(16.0).color(
                        if preview.is_playing {
                            Color32::from_rgb(84, 110, 255)
                        } else {
                            Color32::from_rgb(228, 232, 243)
                        },
                    ),
                )
                .fill(Color32::from_rgb(32, 36, 44))
                .min_size(Vec2::new(32.0, 28.0));

                if ui.add(play_btn).on_hover_text("Play/Pause (Space)").clicked() {
                    actions.toggle_playback = true;
                }

                // Prev keyframe
                let prev_btn = egui::Button::new(RichText::new("⏮").size(13.0).color(Color32::from_rgb(150, 158, 175)))
                    .fill(Color32::from_rgb(28, 31, 38))
                    .min_size(Vec2::new(28.0, 28.0));
                if ui.add(prev_btn).on_hover_text("Previous keyframe (,)").clicked() {
                    actions.prev_keyframe = true;
                }

                // Next keyframe
                let next_btn = egui::Button::new(RichText::new("⏭").size(13.0).color(Color32::from_rgb(150, 158, 175)))
                    .fill(Color32::from_rgb(28, 31, 38))
                    .min_size(Vec2::new(28.0, 28.0));
                if ui.add(next_btn).on_hover_text("Next keyframe (.)").clicked() {
                    actions.next_keyframe = true;
                }

                ui.add_space(4.0);

                // Editor sync toggle
                let sync_btn = egui::Button::new(
                    RichText::new("🔗").size(12.0).color(
                        if editor_sync_enabled {
                            Color32::from_rgb(84, 110, 255)
                        } else {
                            Color32::from_rgb(90, 96, 110)
                        },
                    ),
                )
                .fill(Color32::from_rgb(28, 31, 38))
                .min_size(Vec2::new(28.0, 28.0));
                if ui.add(sync_btn)
                    .on_hover_text("Sync editor to timeline (S)")
                    .clicked()
                {
                    actions.toggle_editor_sync = true;
                }

                // Keyframe mode toggle
                let kf_btn = egui::Button::new(
                    RichText::new(if keyframe_mode { "🔑" } else { "○" }).size(12.0).color(
                        if keyframe_mode {
                            Color32::from_rgb(255, 196, 92)
                        } else {
                            Color32::from_rgb(90, 96, 110)
                        },
                    ),
                )
                .fill(Color32::from_rgb(28, 31, 38))
                .min_size(Vec2::new(28.0, 28.0));
                if ui.add(kf_btn)
                    .on_hover_text("Keyframe mode: K — create timestamps on edit")
                    .clicked()
                {
                    actions.toggle_keyframe_mode = true;
                }

                ui.add_space(6.0);

                // Full-width scrubber
                let scrubber_width = ui.available_width() - 130.0; // reserve space for time display
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

                ui.add_space(6.0);

                // Time display
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{:.2}s / {:.2}s",
                            preview.current_time_s, preview.duration_s
                        ))
                        .monospace()
                        .size(13.0)
                        .color(Color32::from_rgb(228, 232, 243)),
                    );
                });
            });

            ui.add_space(4.0);

            // Row 2: Metadata
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 0.0);

                let muted = Color32::from_rgb(90, 96, 110);
                let success = Color32::from_rgb(80, 200, 140);
                let error_color = Color32::from_rgb(255, 100, 100);
                let warning_color = Color32::from_rgb(255, 214, 102);

                // Resolution
                ui.label(
                    RichText::new(format!("{}×{}", scene_dimensions.width, scene_dimensions.height))
                        .size(11.0)
                        .color(muted),
                );

                dot_separator(ui, muted);

                // Actor count
                ui.label(
                    RichText::new(format!("{} actors", actor_count))
                        .size(11.0)
                        .color(muted),
                );

                dot_separator(ui, muted);

                // Keyframe count
                ui.label(
                    RichText::new(format!("{} keyframes", keyframe_count))
                        .size(11.0)
                        .color(muted),
                );

                dot_separator(ui, muted);

                // Build status: always render same widget structure for layout stability.
                let errors = diagnostics.iter().filter(|d| d.is_error()).count();
                let warnings = diagnostics.iter().filter(|d| !d.is_error()).count();

                let status_text = if diagnostics.is_empty() {
                    "✓ Built".to_string()
                } else if errors > 0 && warnings > 0 {
                    format!("⚠ {} errors, {} warnings", errors, warnings)
                } else if errors > 1 {
                    format!("⚠ {} errors", errors)
                    } else if errors == 1 {
                        "⚠ 1 error".to_string()
                } else if warnings > 1 {
                    format!("⚠ {} warnings", warnings)
                } else if warnings == 1 {
                    "⚠ 1 warning".to_string()
                } else {
                    "⚠ Diagnostics".to_string()
                };

                let status_color = if diagnostics.is_empty() {
                    success
                } else if errors > 0 {
                    error_color
                } else {
                    warning_color
                };

                // Status badge (clickable when there are diagnostics)
                let status_response = ui.selectable_label(
                    false,
                    RichText::new(status_text).size(11.0).color(status_color),
                );

                // Tooltip with all diagnostics on hover
                if !diagnostics.is_empty() {
                    let status_response = status_response.on_hover_ui(|ui| {
                        ui.strong(format!("{} diagnostic{}", diagnostics.len(), if diagnostics.len() == 1 { "" } else { "s" }));
                        ui.add_space(4.0);
                        for d in diagnostics.iter().take(10) {
                            let prefix = if d.is_error() { "✗ " } else { "⚠ " };
                            let loc = if let (Some(l), Some(c)) = (d.location.line, d.location.column) {
                                format!("line {l}, col {c}: ")
                            } else { String::new() };
                            let msg = d.message.lines().next().unwrap_or(&d.message);
                            ui.label(RichText::new(format!("{prefix}{loc}{msg}")).size(11.0));
                        }
                        if diagnostics.len() > 10 {
                            ui.label(RichText::new(format!("... and {} more", diagnostics.len() - 10)).size(11.0).weak());
                        }
                    });

                    // Click to jump to first diagnostic with a line number
                    if status_response.clicked() {
                        if let Some(first) = diagnostics.iter().find(|d| d.location.line.is_some()) {
                            actions.scroll_to_line = first.location.line.map(|l| l.saturating_sub(1));
                        }
                    }
                }

                // Show first diagnostic message inline (truncated)
                if let Some(first) = diagnostics.first() {
                    let msg = first.message.lines().next().unwrap_or(&first.message);
                    let truncated = if msg.len() > 50 {
                        format!("{}...", &msg[..50])
                    } else {
                        msg.to_string()
                    };
                    ui.label(
                        RichText::new(truncated)
                            .size(11.0)
                            .color(Color32::from_rgb(180, 180, 180)),
                    );
                } else {
                    // Invisible placeholder so layout doesn't shift when diagnostics appear
                    ui.label(RichText::new("").size(11.0));
                }

                // Right-aligned hints
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if is_dirty {
                        badge_small(ui, "Modified", Color32::from_rgb(120, 74, 26), Color32::from_rgb(255, 217, 153));
                    } else {
                        badge_small(ui, "Saved", Color32::from_rgb(32, 84, 54), Color32::from_rgb(188, 247, 214));
                    }
                    ui.label(
                        RichText::new("⌘S save  •  ⌘E explorer  •  ⌘I inspector")
                            .size(10.0)
                            .color(Color32::from_rgb(60, 65, 78)),
                    );
                });
            });
        });
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
    let height = 24.0;
    let desired_size = Vec2::new(width.max(120.0), height);
    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click_and_drag());
    let duration_s = duration_s.max(0.1);
    let painter = ui.painter_at(rect);
    let track_rect = rect.shrink2(Vec2::new(4.0, 7.0));
    let fraction = super::preview::timeline_fraction(*current_time_s, duration_s);
    let playhead_x = egui::lerp(track_rect.left()..=track_rect.right(), fraction);

    // Track background
    painter.rect_filled(track_rect, 5.0, Color32::from_rgb(24, 27, 33));
    painter.rect_stroke(
        track_rect,
        5.0,
        Stroke::new(1.0, Color32::from_rgb(40, 44, 52)),
        egui::StrokeKind::Outside,
    );

    // Played portion
    let played_rect = egui::Rect::from_min_max(
        track_rect.min,
        egui::pos2(playhead_x.max(track_rect.left()), track_rect.bottom()),
    );
    painter.rect_filled(
        played_rect,
        5.0,
        if is_playing {
            Color32::from_rgb(84, 110, 255)
        } else {
            Color32::from_rgb(60, 78, 160)
        },
    );

    // Tick marks
    for tick in super::preview::timeline_tick_times(duration_s) {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            super::preview::timeline_fraction(tick, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 3.0),
                egui::pos2(x, track_rect.bottom() - 3.0),
            ],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
        );
    }

    // Keyframe markers (amber)
    for marker in markers_s {
        let x = egui::lerp(
            track_rect.left()..=track_rect.right(),
            super::preview::timeline_fraction(*marker, duration_s),
        );
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top() + 1.0),
                egui::pos2(x, track_rect.bottom() - 1.0),
            ],
            Stroke::new(2.0, Color32::from_rgb(255, 196, 92)),
        );
    }

    // Cursor position indicator (editor → timeline sync)
    // Shows where the editor cursor is on the timeline
    if let Some(cursor_t) = cursor_time_s {
        if cursor_t >= 0.0 && cursor_t <= duration_s {
            let cursor_fraction = super::preview::timeline_fraction(cursor_t, duration_s);
            let cursor_x = egui::lerp(track_rect.left()..=track_rect.right(), cursor_fraction);
            // Draw a subtle cyan triangle above the track
            let triangle_top = track_rect.top() - 5.0;
            let triangle_y = track_rect.top() - 1.0;
            painter.line_segment(
                [
                    egui::pos2(cursor_x, triangle_top),
                    egui::pos2(cursor_x - 3.0, triangle_y),
                ],
                Stroke::new(1.5, Color32::from_rgb(100, 220, 255)),
            );
            painter.line_segment(
                [
                    egui::pos2(cursor_x, triangle_top),
                    egui::pos2(cursor_x + 3.0, triangle_y),
                ],
                Stroke::new(1.5, Color32::from_rgb(100, 220, 255)),
            );
            painter.line_segment(
                [
                    egui::pos2(cursor_x - 3.0, triangle_y),
                    egui::pos2(cursor_x + 3.0, triangle_y),
                ],
                Stroke::new(1.5, Color32::from_rgb(100, 220, 255)),
            );
        }
    }

    // Playhead
    painter.line_segment(
        [
            egui::pos2(playhead_x, track_rect.top() - 2.0),
            egui::pos2(playhead_x, track_rect.bottom() + 2.0),
        ],
        Stroke::new(2.0, Color32::WHITE),
    );
    painter.circle_filled(
        egui::pos2(playhead_x, track_rect.center().y),
        4.0,
        Color32::WHITE,
    );

    // Interaction
    if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some() {
        *current_time_s = super::preview::time_from_pointer_x(
            track_rect,
            response.interact_pointer_pos().unwrap().x,
            duration_s,
        );
        return true;
    }

    false
}

fn dot_separator(ui: &mut egui::Ui, color: Color32) {
    ui.label(RichText::new("•").size(9.0).color(color));
}

fn badge_small(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(label).size(10.0).color(text));
        });
}
