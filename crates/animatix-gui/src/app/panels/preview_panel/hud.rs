//! Preview HUD overlays.

use super::*;

pub(super) fn presentation_hud_ui(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    canvas_rect: egui::Rect,
) {
    use egui_phosphor::regular as icons;

    let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
    let is_hovered = pointer_pos.map(|p| canvas_rect.contains(p)).unwrap_or(false);
    let is_paused_at_step = ctx.preview.playback.paused_at_step || !ctx.preview.playback.is_playing;

    // Show HUD when hovered or when playback is paused at a step
    if !is_hovered && !is_paused_at_step {
        return;
    }

    let theme = eparts::theme(ui);
    let hud_width = 300.0_f32.min(canvas_rect.width() - 24.0);
    let hud_height = 36.0;

    if hud_width < 120.0 {
        return;
    }

    let hud_rect = egui::Rect::from_center_size(
        egui::pos2(canvas_rect.center().x, canvas_rect.max.y - 28.0),
        egui::vec2(hud_width, hud_height),
    );

    ui.painter()
        .add(theme.elevation.overlay.as_shape(hud_rect, egui::CornerRadius::same(18)));

    ui.painter().rect_filled(hud_rect, 18.0, theme.palette.overlay.floating_hud_bg);

    ui.painter().rect_stroke(
        hud_rect,
        18.0,
        egui::Stroke::new(1.0, theme.palette.border.subtle),
        egui::StrokeKind::Inside,
    );

    let inner_rect = hud_rect.shrink2(egui::vec2(10.0, 2.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

            // Previous step button
            let prev_btn = ui.add(
                Button::ghost("")
                    .with_icon(icons::CARET_LEFT)
                    .with_tooltip("Previous step (PageUp / Left-Arrow / Right-Click)"),
            );
            if prev_btn.clicked() {
                ctx.commands.push_back(PlaybackCommand::StepPrevious.into());
            }

            // Step counter / status
            let pause_pts = &ctx.preview.playback.pause_points_s;
            let step_label = if pause_pts.is_empty() {
                format!(
                    "{:.1}s / {:.1}s",
                    ctx.preview.playback.current_time_s, ctx.preview.playback.duration_s
                )
            } else {
                let total_steps = pause_pts.len() + 1;
                let current_step = pause_pts
                    .iter()
                    .filter(|&&p| p <= ctx.preview.playback.current_time_s + 1e-4)
                    .count()
                    + 1;
                format!("Step {} / {}", current_step, total_steps)
            };

            let label_color = if is_paused_at_step {
                theme.palette.status.success
            } else {
                theme.palette.text.primary
            };

            ui.add(
                egui::Label::new(
                    egui::RichText::new(step_label)
                        .font(TextRole::Body.font_id())
                        .color(label_color),
                )
                .selectable(false),
            );

            // Next / Play button
            let play_icon = if ctx.preview.playback.is_playing {
                icons::PAUSE
            } else {
                icons::PLAY
            };
            let next_btn = ui.add(
                Button::ghost("")
                    .with_icon(play_icon)
                    .with_tooltip("Next step (Space / PageDown / Click)"),
            );
            if next_btn.clicked() {
                ctx.commands.push_back(PlaybackCommand::StepNext.into());
            }

            toolbar_separator(ui);

            // Exit button
            let exit_btn = ui.add(
                Button::ghost("Exit")
                    .with_icon(icons::X)
                    .with_tooltip("Exit Presentation Mode (Esc / F5)"),
            );
            if exit_btn.clicked() {
                ctx.commands.push_back(PlaybackCommand::TogglePresentationMode.into());
            }
        });
    });
}

/// Floating Tool Island HUD: an elevated pill hovering over the preview canvas (design doc §12.2).
pub(super) fn preview_floating_hud_ui(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    canvas_rect: egui::Rect,
) {
    use egui_phosphor::regular as icons;

    let theme = eparts::theme(ui);
    let hud_width = 460.0_f32.min(canvas_rect.width() - 24.0);
    let hud_height = 36.0;

    if hud_width < 120.0 {
        return; // Don't render HUD if viewport is too narrow
    }

    let hud_rect = egui::Rect::from_center_size(
        egui::pos2(canvas_rect.center().x, canvas_rect.min.y + 24.0),
        egui::vec2(hud_width, hud_height),
    );

    // 1. Elevated shadow
    ui.painter()
        .add(theme.elevation.overlay.as_shape(hud_rect, egui::CornerRadius::same(18)));

    // 2. Obsidian glassmorphic pill background
    ui.painter().rect_filled(hud_rect, 18.0, theme.palette.overlay.floating_hud_bg);

    // 3. Dual-tone border: outer subtle + top rim highlight
    ui.painter().rect_stroke(
        hud_rect,
        18.0,
        egui::Stroke::new(1.0, theme.palette.border.subtle),
        egui::StrokeKind::Inside,
    );
    let top_rim = egui::Rect::from_min_max(
        egui::pos2(hud_rect.min.x + 18.0, hud_rect.min.y),
        egui::pos2(hud_rect.max.x - 18.0, hud_rect.min.y + 1.0),
    );
    ui.painter().rect_filled(top_rim, 0.5, theme.palette.border.rim_highlight);

    // 4. Interactive contents inside the floating capsule
    let inner_rect = hud_rect.shrink2(egui::vec2(8.0, 2.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner_rect), |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(2.0, 0.0);

            // Tool switcher: 6 primary tools
            let tools: [(ToolMode, &str, &str, &str); 6] = [
                (ToolMode::Select, icons::CURSOR, "Select", "Select tool (V)"),
                (ToolMode::Move, icons::HAND_GRABBING, "Move", "Move tool (G)"),
                (ToolMode::Rotate, icons::ARROW_CLOCKWISE, "Rotate", "Rotate tool (R)"),
                (ToolMode::Scale, icons::ARROWS_OUT_SIMPLE, "Scale", "Scale tool (S)"),
                (ToolMode::Vertex, icons::POLYGON, "Vertex", "Vertex tool (A)"),
                (ToolMode::Pivot, icons::CROSSHAIR, "Pivot", "Pivot tool (P)"),
            ];
            for (mode, icon, label, tip) in tools {
                let active = *ctx.tool_mode == mode;
                let resp = ui.add(Button::ghost("").with_icon(icon).active(active));
                text_tooltip(ui, resp.id.with(("tool", label)), &resp, tip);
                if resp.clicked() {
                    *ctx.tool_mode = mode;
                    ctx.preview.status = format!("Tool: {label}");
                }
            }

            toolbar_separator(ui);

            // Snapping toggle
            let snap = ctx.preview.snap.snap_enabled;
            let snap_resp = ui.add(Button::ghost("").with_icon(icons::MAGNET).active(snap));
            text_tooltip(
                ui,
                snap_resp.id.with("snap_tip"),
                &snap_resp,
                if snap {
                    "Snapping on (hold Alt to bypass)"
                } else {
                    "Snapping off"
                },
            );
            if snap_resp.clicked() {
                ctx.preview.snap.snap_enabled = !snap;
            }

            toolbar_separator(ui);

            // View toggles
            let grid = ctx.preview.overlay.show_grid;
            let grid_resp = ui.add(Button::ghost("").with_icon(icons::GRID_FOUR).active(grid));
            text_tooltip(ui, grid_resp.id.with("grid_tip"), &grid_resp, "Toggle grid");
            if grid_resp.clicked() {
                ctx.preview.overlay.show_grid = !grid;
            }

            let guides = ctx.preview.overlay.show_guides;
            let guides_resp = ui.add(Button::ghost("").with_icon(icons::RULER).active(guides));
            text_tooltip(ui, guides_resp.id.with("guides_tip"), &guides_resp, "Toggle guides");
            if guides_resp.clicked() {
                ctx.preview.overlay.show_guides = !guides;
            }

            let labels = ctx.preview.overlay.show_actor_labels;
            let labels_resp = ui.add(Button::ghost("").with_icon(icons::TEXT_T).active(labels));
            text_tooltip(
                ui,
                labels_resp.id.with("labels_tip"),
                &labels_resp,
                "Toggle actor labels",
            );
            if labels_resp.clicked() {
                ctx.preview.overlay.show_actor_labels = !labels;
            }

            toolbar_separator(ui);

            // Zoom menu
            let zoom = ctx.preview.viewport.preview_zoom;
            let zoom_label = if (zoom - 1.0).abs() < 0.05 {
                "100%"
            } else if (zoom - 1.5).abs() < 0.05 {
                "150%"
            } else if (zoom - 2.0).abs() < 0.05 {
                "200%"
            } else {
                "Fit"
            };
            ui.menu_button(
                RichText::new(zoom_label)
                    .size(TextRole::Caption.size())
                    .color(theme.palette.text.secondary),
                |ui| {
                    ui.set_min_width(80.0);
                    if ui.stable_selectable_label(false, "Fit").clicked() {
                        ctx.preview.fit_zoom_requested = true;
                        ui.close();
                    }
                    for (z, name) in [(1.0_f32, "100%"), (1.5, "150%"), (2.0, "200%")] {
                        if ui.stable_selectable_label((zoom - z).abs() < 0.05, name).clicked() {
                            ctx.preview.viewport.preview_zoom = z;
                            ctx.preview.viewport.preview_pan = Vec2::new(
                                ctx.scene_dimensions.width as f32 / 2.0,
                                ctx.scene_dimensions.height as f32 / 2.0,
                            );
                            ui.close();
                        }
                    }
                },
            );
        });
    });
}

// ─── Free functions for the preview canvas ─────────────────────────────────
