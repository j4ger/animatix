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
                ctx.commands.push_back(Command::StepPrevious.into());
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
                ctx.commands.push_back(Command::StepNext.into());
            }

            toolbar_separator(ui);

            // Exit button
            let exit_btn = ui.add(
                Button::ghost("Exit")
                    .with_icon(icons::X)
                    .with_tooltip("Exit Presentation Mode (Esc / F5)"),
            );
            if exit_btn.clicked() {
                ctx.commands.push_back(Command::TogglePresentationMode.into());
            }
        });
    });
}

/// Preview controls sit beside the viewport instead of over the artwork.
pub(super) fn preview_header_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui) {
    use crate::app::design_tokens::spatial::spatial;
    use egui_phosphor::regular as icons;

    let theme = eparts::theme(ui);
    let sp = spatial(ui);
    let max_scene_width = (ui.available_width() - 260.0).clamp(0.0, 220.0);
    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);
    ui.label(RichText::new("Preview").font(TextRole::Title.font_id()));

    if let Some(composition) = ctx.composition
        && composition.declaration_order.len() > 1
    {
        let active = ctx.active_scene.unwrap_or("Scenes");
        let open_scenes = |ui: &mut egui::Ui| {
            for name in &composition.declaration_order {
                if ui.stable_selectable_label(name == active, name).clicked() {
                    ctx.commands.push_back(Command::SelectScene(name.clone()).into());
                    ui.close();
                }
            }
        };
        if max_scene_width >= 44.0 {
            let label = format!("{active}  {}", icons::CARET_DOWN);
            let title = RichText::new(label).font(TextRole::BodyS.font_id());
            let button = ui
                .allocate_ui_with_layout(
                    Vec2::new(max_scene_width, sp.base.row_m),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| ui.add(egui::Button::new(title).truncate().frame(false)),
                )
                .inner;
            egui::Popup::menu(&button).show(open_scenes);
            button.on_hover_text("Switch scene");
        } else {
            crate::app::components::icon_menu(ui, icons::FILM_STRIP, "Switch scene", open_scenes);
        }
    }

    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let zoom = ctx.preview.viewport.preview_zoom;
        let zoom_label = format!("{:.0}%", zoom * 100.0);
        let response = ui.add(
            Button::secondary(zoom_label)
                .with_icon(icons::CARET_DOWN)
                .with_tooltip("Preview zoom"),
        );
        egui::Popup::menu(&response).show(|ui| {
            if ui.button("Fit").clicked() {
                ctx.preview.fit_zoom_requested = true;
                ui.close();
            }
            for (scale, name) in [(1.0_f32, "100%"), (1.5, "150%"), (2.0, "200%")] {
                if ui.stable_selectable_label((zoom - scale).abs() < 0.05, name).clicked() {
                    ctx.preview.viewport.preview_zoom = scale;
                    ctx.preview.viewport.preview_pan = Vec2::new(
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    );
                    ui.close();
                }
            }
        });

        for (value, icon, tooltip) in [
            (&mut ctx.preview.overlay.show_actor_labels, icons::TEXT_T, "Toggle actor labels"),
            (&mut ctx.preview.overlay.show_guides, icons::RULER, "Toggle guides"),
            (&mut ctx.preview.overlay.show_grid, icons::GRID_FOUR, "Toggle grid"),
        ] {
            if ui.add(Button::icon(icon).active(*value).with_tooltip(tooltip)).clicked() {
                *value = !*value;
            }
        }

        let dims = ctx.scene_dimensions;
        if ui.available_width() >= 80.0 {
            ui.label(
                RichText::new(format!("{} × {}", dims.width, dims.height))
                    .font(TextRole::Mono.font_id())
                    .color(theme.palette.text.secondary),
            );
        }
    });
}
