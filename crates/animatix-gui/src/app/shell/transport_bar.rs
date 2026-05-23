use crate::app::commands::{Command, CommandQueue};
use crate::app::design_tokens::*;
use crate::app::{PanelState, PreviewPaneState};
use animatix::composition::Composition;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::SceneDimensions;
use egui::{Align, Align2, Color32, FontId, RichText, Stroke, Vec2};

/// Renders the unified transport bar at the bottom of the window.
///
/// Single-row layout: transport controls, scrubber, time, status.
#[allow(clippy::too_many_arguments)]
pub(crate) fn transport_bar_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    panel_state: &mut PanelState,
    scene_dimensions: SceneDimensions,
    timeline_markers: &[f64],
    actor_count: usize,
    keyframe_count: usize,
    _is_dirty: bool,
    _has_error: bool,
    diagnostics: &[Diagnostic],
    commands: &mut CommandQueue,
    editor_sync_enabled: bool,
    keyframe_mode: bool,
    cursor_time_s: Option<f64>,
    composition: Option<&Composition>,
    active_scene: Option<&str>,
) {
    let warning_color = AMBER;

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
                    RichText::new(play_icon).size(FONT_SIZE_XL).color(play_color),
                )
                .fill(BG_WIDGET)
                .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                .min_size(Vec2::new(ROW_M + SPACE_S, ROW_L));
                if ui
                    .add(play_btn)
                    .on_hover_text("Play/Pause (Space)")
                    .clicked()
                {
                    commands.push_back(Command::TogglePlayback);
                }

                // Speed control — compact segmented buttons
                ui.add_space(SPACE_S);
                let speeds: [(f32, &str); 4] = [(0.25, "¼×"), (0.5, "½×"), (1.0, "1×"), (2.0, "2×")];
                let current_speed = preview.playback_speed;
                for (speed_val, label) in speeds {
                    let is_active = (speed_val - current_speed).abs() < f32::EPSILON;
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .size(FONT_SIZE_S)
                            .color(if is_active { TEXT_PRIMARY } else { TEXT_MUTED }),
                    )
                    .fill(if is_active { BG_WIDGET } else { Color32::TRANSPARENT })
                    .corner_radius(egui::CornerRadius::same(RADIUS_S as u8))
                    .min_size(Vec2::new(28.0, ROW_S));
                    if ui.add(btn).on_hover_text(format!("Speed: {}×", speed_val)).clicked() {
                        preview.playback_speed = speed_val;
                    }
                }

                // Skip back
                let prev_btn = egui::Button::new(
                    RichText::new(egui_phosphor::regular::SKIP_BACK)
                        .size(FONT_SIZE_M)
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
                    commands.push_back(Command::PrevKeyframe);
                }

                // Skip forward
                let next_btn = egui::Button::new(
                    RichText::new(egui_phosphor::regular::SKIP_FORWARD)
                        .size(FONT_SIZE_M)
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
                    commands.push_back(Command::NextKeyframe);
                }

                if composition.is_some() {
                    // Scene navigation
                    let prev_scene_btn = egui::Button::new(
                        RichText::new(egui_phosphor::regular::CARET_LEFT)
                            .size(FONT_SIZE_M)
                            .color(TEXT_MUTED),
                    )
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                    .min_size(Vec2::new(26.0, ROW_L));
                    if ui
                        .add(prev_scene_btn)
                        .on_hover_text("Previous scene")
                        .clicked()
                    {
                        commands.push_back(Command::PrevScene);
                    }

                    let next_scene_btn = egui::Button::new(
                        RichText::new(egui_phosphor::regular::CARET_RIGHT)
                            .size(FONT_SIZE_M)
                            .color(TEXT_MUTED),
                    )
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(egui::CornerRadius::same(RADIUS_M as u8))
                    .min_size(Vec2::new(26.0, ROW_L));
                    if ui
                        .add(next_scene_btn)
                        .on_hover_text("Next scene")
                        .clicked()
                    {
                        commands.push_back(Command::NextScene);
                    }
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
                    commands.push_back(Command::ToggleEditorSync);
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
                    commands.push_back(Command::ToggleKeyframeMode);
                }

                ui.add_space(SPACE_S);

                // ── Loop region: A / B markers ──
                let loop_active = preview.loop_start_s.is_some() && preview.loop_end_s.is_some();
                let a_active = preview.loop_start_s.is_some();
                let b_active = preview.loop_end_s.is_some();

                // A button — set loop start to current time
                let a_color = if loop_active { ACCENT_CYAN } else if a_active { AMBER } else { TEXT_MUTED };
                let a_btn = egui::Button::new(
                    RichText::new("A")
                        .size(FONT_SIZE_S)
                        .color(a_color),
                )
                .fill(if a_active { BG_WIDGET } else { Color32::TRANSPARENT })
                .corner_radius(egui::CornerRadius::same(RADIUS_S as u8))
                .min_size(Vec2::new(22.0, ROW_S));
                if ui.add(a_btn).on_hover_text(
                    if a_active {
                        format!("Loop start: {:.2}s — click to clear", preview.loop_start_s.unwrap_or(0.0))
                    } else {
                        "Set loop start (A) at current time".to_string()
                    }
                ).clicked() {
                    if preview.loop_start_s.is_some() {
                        preview.loop_start_s = None;
                    } else {
                        preview.loop_start_s = Some(preview.current_time_s);
                    }
                }

                // B button — set loop end to current time
                let b_color = if loop_active { ACCENT_CYAN } else if b_active { AMBER } else { TEXT_MUTED };
                let b_btn = egui::Button::new(
                    RichText::new("B")
                        .size(FONT_SIZE_S)
                        .color(b_color),
                )
                .fill(if b_active { BG_WIDGET } else { Color32::TRANSPARENT })
                .corner_radius(egui::CornerRadius::same(RADIUS_S as u8))
                .min_size(Vec2::new(22.0, ROW_S));
                if ui.add(b_btn).on_hover_text(
                    if b_active {
                        format!("Loop end: {:.2}s — click to clear", preview.loop_end_s.unwrap_or(0.0))
                    } else {
                        "Set loop end (B) at current time".to_string()
                    }
                ).clicked() {
                    if preview.loop_end_s.is_some() {
                        preview.loop_end_s = None;
                    } else {
                        preview.loop_end_s = Some(preview.current_time_s);
                    }
                }

                // Clear loop button — visible when either marker is set
                if a_active || b_active {
                    let clear_btn = egui::Button::new(
                        RichText::new(egui_phosphor::regular::X)
                            .size(FONT_SIZE_XS)
                            .color(TEXT_MUTED),
                    )
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(egui::CornerRadius::same(RADIUS_S as u8))
                    .min_size(Vec2::new(16.0, ROW_S));
                    if ui.add(clear_btn).on_hover_text("Clear loop region").clicked() {
                        preview.loop_start_s = None;
                        preview.loop_end_s = None;
                    }
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
                    composition,
                    commands,
                    preview,
                    panel_state,
                ) {
                    commands.push_back(Command::ScrubTo(scrub));
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
                        commands.push_back(Command::ToggleDiagnosticsPanel);
                    }

                    // 2. Time pill (left of status, closest to scrubber)
                    let stats_tooltip = format!(
                        "Resolution: {}×{}\nActors: {}\nKeyframes: {}",
                        scene_dimensions.width,
                        scene_dimensions.height,
                        actor_count,
                        keyframe_count
                    );
                    let active_scene_label = if let Some(composition) = composition {
                        active_scene
                            .map(str::to_owned)
                            .or_else(|| {
                                let (scene, _, _) = composition.evaluate(preview.current_time_s);
                                (!scene.is_empty()).then_some(scene)
                            })
                    } else {
                        None
                    };
                    let speed_suffix = if (preview.playback_speed - 1.0).abs() > f32::EPSILON {
                        format!(" @ {}×", preview.playback_speed)
                    } else {
                        String::new()
                    };
                    let time_text = if let Some(scene) = active_scene_label.as_deref() {
                        format!("{scene} • {:.2}s / {:.2}s{}", preview.current_time_s, preview.duration_s, speed_suffix)
                    } else {
                        format!("{:.2}s / {:.2}s{}", preview.current_time_s, preview.duration_s, speed_suffix)
                    };
                    egui::Frame::new()
                        .fill(BG_SURFACE)
                        .corner_radius(egui::CornerRadius::same(RADIUS_L as u8))
                        .inner_margin(egui::Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(time_text)
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
    composition: Option<&Composition>,
    commands: &mut CommandQueue,
    preview: &mut PreviewPaneState,
    panel_state: &mut PanelState,
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

    if let Some(composition) = composition {
        let palette = [
            track_block_1(), track_block_2(), track_block_3(), track_block_4(), track_block_5(),
        ];
        let label_color = text_dim();
        let total = duration_s.max(0.1);

        for (idx, scene_name) in composition.declaration_order.iter().enumerate() {
            let Some(scene) = composition.scenes.get(scene_name) else { continue; };
            let Some(start_s) = composition.scene_start_times.get(scene_name).copied() else { continue; };
            let end_s = (start_s + scene.duration_s).min(total);
            if end_s <= start_s {
                continue;
            }

            let left = egui::lerp(track_rect.left()..=track_rect.right(), (start_s / total).clamp(0.0, 1.0) as f32);
            let right = egui::lerp(track_rect.left()..=track_rect.right(), (end_s / total).clamp(0.0, 1.0) as f32);
            let scene_rect = egui::Rect::from_min_max(
                egui::pos2(left, track_rect.top()),
                egui::pos2(right, track_rect.bottom()),
            );
            let color = palette[idx % palette.len()];
            painter.rect_filled(scene_rect, 0.0, color);

            let scene_width = scene_rect.width();
            if scene_width > 28.0 {
                painter.text(
                    scene_rect.center(),
                    Align2::CENTER_CENTER,
                    scene_name,
                    FontId::monospace(9.0),
                    label_color,
                );
            }
        }

        // Draw transition overlaps as interactive stripe regions at the boundary between scenes
        for scene_name in &composition.declaration_order {
            let Some(edge) = composition.edges.get(scene_name) else { continue; };
            if edge.transition.duration_ms == 0 {
                continue;
            }
            let Some(start_s) = composition.scene_start_times.get(scene_name).copied() else { continue; };
            let Some(scene) = composition.scenes.get(scene_name) else { continue; };
            let transition_s = edge.transition.duration_ms as f64 / 1000.0;
            let overlap_start = (start_s + scene.duration_s - transition_s).max(0.0);
            let overlap_end = (start_s + scene.duration_s).min(total);
            if overlap_end <= overlap_start {
                continue;
            }

            let left = egui::lerp(track_rect.left()..=track_rect.right(), (overlap_start / total).clamp(0.0, 1.0) as f32);
            let right = egui::lerp(track_rect.left()..=track_rect.right(), (overlap_end / total).clamp(0.0, 1.0) as f32);
            let overlap_rect = egui::Rect::from_min_max(
                egui::pos2(left, track_rect.top()),
                egui::pos2(right, track_rect.bottom()),
            );

            // Interaction region for hover + click
            let overlap_id = ui.id().with("overlap").with(scene_name);
            let overlap_base = ui.interact(overlap_rect, overlap_id, egui::Sense::click());

            // Hover tooltip: "Fade to \"Outro\" — 300ms — Ease Out"
            let easing_name = format!("{:?}", edge.transition.easing);
            let tooltip = format!(
                "{} to \"{}\" — {}ms — {}",
                transition_type_label(&edge.transition.id),
                edge.to_scene,
                edge.transition.duration_ms,
                easing_name,
            );
            let overlap_response = overlap_base.on_hover_text(tooltip);

            // Click → select source scene + signal transition editor open
            if overlap_response.clicked() {
                commands.push_back(Command::SelectScene(scene_name.clone()));
                panel_state.open_transition_editor = Some(scene_name.clone());
                return true;
            }

            // Transition-specific stripe color (brighter on hover)
            let base_color = transition_stripe_color(&edge.transition.id);
            let stripe_color = if overlap_response.hovered() {
                // Make it more visible on hover
                Color32::from_rgba_unmultiplied(
                    base_color.r().saturating_add(60),
                    base_color.g().saturating_add(60),
                    base_color.b().saturating_add(60),
                    120,
                )
            } else {
                base_color
            };
            painter.rect_filled(overlap_rect, 0.0, stripe_color);

            // Draw diagonal hatching lines for visual distinction
            let hatch_spacing = 6.0_f32;
            let mut y = overlap_rect.top();
            while y < overlap_rect.bottom() {
                let x_start = overlap_rect.left();
                let t = (y - overlap_rect.top()) / overlap_rect.height();
                let offset = t * hatch_spacing;
                painter.line_segment(
                    [
                        egui::pos2(x_start + offset, y),
                        egui::pos2(x_start + offset - hatch_spacing, y + hatch_spacing),
                    ],
                    Stroke::new(1.0, hatch_line()),
                );
                y += hatch_spacing;
            }

            // Transition label if wide enough
            let width = overlap_rect.width();
            if width > 40.0 {
                let label = transition_type_label(&edge.transition.id).to_string();
                let label_color = if overlap_response.hovered() {
                    text_hover()
                } else {
                    text_subtle()
                };
                painter.text(
                    overlap_rect.center(),
                    Align2::CENTER_CENTER,
                    label,
                    FontId::monospace(8.0),
                    label_color,
                );
            }
        }
    }

    // ── Loop region highlight ──
    if let (Some(start), Some(end)) = (preview.loop_start_s, preview.loop_end_s) {
        if end > start && end >= 0.0 && start <= duration_s {
            let left = egui::lerp(track_rect.left()..=track_rect.right(), (start / duration_s).clamp(0.0, 1.0) as f32);
            let right = egui::lerp(track_rect.left()..=track_rect.right(), (end / duration_s).clamp(0.0, 1.0) as f32);
            let loop_rect = egui::Rect::from_min_max(
                egui::pos2(left, track_rect.top()),
                egui::pos2(right, track_rect.bottom()),
            );
            painter.rect_filled(loop_rect, 0.0, loop_region());
        }
    }

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
    let played_color = if is_playing { ACCENT_BLUE } else { BG_WIDGET };
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
            Stroke::new(1.0, grid_line()),
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
            let color = ACCENT_CYAN;
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
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            // Check if click landed on a scene block
            if let Some(composition) = composition {
                let total = duration_s.max(0.1);
                for scene_name in &composition.declaration_order {
                    let Some(scene) = composition.scenes.get(scene_name) else { continue };
                    let Some(start_s) = composition.scene_start_times.get(scene_name).copied() else { continue };
                    let end_s = (start_s + scene.duration_s).min(total);
                    let left = egui::lerp(track_rect.left()..=track_rect.right(), (start_s / total).clamp(0.0, 1.0) as f32);
                    let right = egui::lerp(track_rect.left()..=track_rect.right(), (end_s / total).clamp(0.0, 1.0) as f32);
                    if pos.x >= left && pos.x <= right && pos.y >= track_rect.top() && pos.y <= track_rect.bottom() {
                        commands.push_back(Command::SelectScene(scene_name.clone()));
                        // Jump past any incoming transition to land in the stable part
                        let mut target_time = start_s;
                        for edge in composition.edges.values() {
                            if edge.to_scene == *scene_name {
                                target_time += edge.transition.duration_ms as f64 / 1000.0;
                                break;
                            }
                        }
                        *current_time_s = target_time;
                        return true;
                    }
                }
            }
            *current_time_s = crate::app::preview::time_from_pointer_x(
                track_rect,
                pos.x,
                duration_s,
            );
            return true;
        }
    } else if response.dragged() {
        if let Some(pos) = response.interact_pointer_pos() {
            *current_time_s = crate::app::preview::time_from_pointer_x(
                track_rect,
                pos.x,
                duration_s,
            );
            return true;
        }
    }

    false
}

fn transition_stripe_color(id: &str) -> Color32 {
    let palette = [
        transition_stripe_1(), transition_stripe_2(), transition_stripe_3(),
        transition_stripe_4(), transition_stripe_5(), transition_stripe_6(),
    ];
    let idx = id.bytes().fold(0u8, |acc, b| acc.wrapping_add(b)) as usize % palette.len();
    palette[idx]
}

fn transition_type_label(id: &str) -> &'static str {
    animatix::transition_registry::display_name(id)
}
