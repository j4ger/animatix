//! Preview panel: canvas with rulers, zoom/pan, drag interaction, and overlays.

use animatix::timeline::SceneDimensions;
use egui::{RichText, Vec2};
use eparts::widget::UiExt;

use crate::app::commands::{ActorCommand, DocumentCommand, PlaybackCommand};
use crate::app::components::button::{Button, toolbar_separator};
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::{RADIUS_L, STROKE_WIDTH, preview as preview_spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::{RULER_SIZE, nice_tick_interval};
pub(crate) use crate::app::preview::context::PreviewContext;
use crate::app::preview::{self, DragState, ToolMode, fit_preview, selection};

/// Tool and view controls for the canvas, kept next to the surface they affect
/// rather than in the global toolbar (design doc §12.2).
fn preview_header_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui) {
    use egui_phosphor::regular as icons;

    let theme = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    egui::Frame::new()
        .fill(theme.palette.surface.base)
        .inner_margin(egui::Margin::symmetric(sp.base.space_2 as i8, sp.base.space_1 as i8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);

                // Tool switcher: the visible mode state that used to be
                // keyboard-only.
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

                // Snapping toggle (there was previously no way to see or
                // change this).
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

                // View toggles
                let grid = ctx.preview.overlay.show_grid;
                let grid_resp = ui.stable_selectable_label(grid, "Grid");
                text_tooltip(ui, grid_resp.id.with("grid_tip"), &grid_resp, "Toggle grid");
                if grid_resp.clicked() {
                    ctx.preview.overlay.show_grid = !grid;
                }

                let guides = ctx.preview.overlay.show_guides;
                let guides_resp = ui.stable_selectable_label(guides, "Guides");
                text_tooltip(ui, guides_resp.id.with("guides_tip"), &guides_resp, "Toggle guides");
                if guides_resp.clicked() {
                    ctx.preview.overlay.show_guides = !guides;
                }

                let labels = ctx.preview.overlay.show_actor_labels;
                let labels_resp = ui.stable_selectable_label(labels, "Labels");
                text_tooltip(
                    ui,
                    labels_resp.id.with("labels_tip"),
                    &labels_resp,
                    "Toggle actor labels",
                );
                if labels_resp.clicked() {
                    ctx.preview.overlay.show_actor_labels = !labels;
                }

                // Zoom, pinned right.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                            .size(TextRole::BodyS.size())
                            .color(theme.palette.text.secondary),
                        |ui| {
                            ui.set_min_width(80.0);
                            if ui.stable_selectable_label(false, "Fit").clicked() {
                                ctx.preview.fit_zoom_requested = true;
                                ui.close();
                            }
                            for (z, name) in [(1.0_f32, "100%"), (1.5, "150%"), (2.0, "200%")] {
                                if ui
                                    .stable_selectable_label((zoom - z).abs() < 0.05, name)
                                    .clicked()
                                {
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
        });
}

// ─── Free functions for the preview canvas ─────────────────────────────────

fn preview_screen_to_scene(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    screen: egui::Pos2,
    zoom: f32,
    pan: Vec2,
) -> kurbo::Point {
    let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
    tx.screen_to_scene(screen)
}

fn preview_scene_to_screen(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    scene: kurbo::Point,
    zoom: f32,
    pan: Vec2,
) -> egui::Pos2 {
    let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
    tx.scene_to_screen(scene)
}

/// Handle an in-flight Library/Asset row drag (`panels::LibraryDragPayload`).
///
/// While a payload exists and the pointer is over the canvas we draw a drop
/// highlight; on release over the canvas we resolve the scene position with the
/// same transform the OS file-drop path uses and push `CreateActor`. The payload
/// is cleared on release anywhere (inside or outside) and on Escape.
fn library_drag_drop_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui, preview_rect: egui::Rect) {
    let Some(payload) = crate::app::panels::library_drag(ui.ctx()) else {
        return;
    };

    let pointer = ui.ctx().input(|i| i.pointer.latest_pos());
    let over_preview = pointer.is_some_and(|p| preview_rect.contains(p));

    if over_preview {
        // Drop highlight: accent outline plus a translucent fill so the target
        // region reads clearly against the scene.
        let theme = eparts::theme(ui);
        ui.painter().rect_filled(preview_rect, RADIUS_L, theme.palette.accent.faint);
        ui.painter().rect_stroke(
            preview_rect,
            RADIUS_L,
            egui::Stroke::new(2.0, theme.palette.accent.primary),
            egui::StrokeKind::Inside,
        );
        if let Some(mouse) = pointer {
            ui.painter().text(
                mouse + Vec2::new(12.0, -12.0),
                egui::Align2::LEFT_CENTER,
                &payload.ty,
                TextRole::BodyS.font_id(),
                theme.palette.text.primary,
            );
        }
    }

    let released = ui.input(|i| i.pointer.any_released());
    let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escape {
        crate::app::panels::clear_library_drag(ui.ctx());
        return;
    }
    if !released {
        return;
    }

    if over_preview {
        if let Some(mouse) = pointer {
            let scene = preview_screen_to_scene(
                ctx.scene_dimensions,
                preview_rect,
                mouse,
                ctx.preview.viewport.preview_zoom,
                ctx.preview.viewport.preview_pan,
            );
            let label = crate::app::utils::labels::unique_label(
                ctx.timeline,
                &crate::app::panels::library_drag_label_base(&payload),
            );
            ctx.commands.push_back(
                ActorCommand::CreateActor {
                    ty: payload.ty.clone(),
                    label,
                    position: [scene.x as f32, scene.y as f32],
                    props: payload.props.clone(),
                }
                .into(),
            );
        }
    }
    // Always clear on release so a drag that ends off-canvas leaves no payload.
    crate::app::panels::clear_library_drag(ui.ctx());
}

// ─── Main preview_panel_ui function ─────────────────────────────────────────

pub(crate) fn preview_panel_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    ctx.preview.overlay.set_theme(theme);
    // Preview uses zero-margin frame to maximize canvas area.
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                preview_header_ui(ctx, ui);

                // Handle fit-zoom request from the global toolbar.
                if ctx.preview.fit_zoom_requested {
                    ctx.preview.fit_zoom_requested = false;
                    let avail = ui.available_size_before_wrap();
                    let preview_avail = Vec2::new(
                        (avail.x - RULER_SIZE).max(200.0),
                        (avail.y - RULER_SIZE).max(180.0),
                    );
                    let desired = fit_preview(ctx.scene_dimensions, preview_avail);
                    ctx.preview.viewport.preview_zoom =
                        desired.x / ctx.scene_dimensions.width as f32;
                    ctx.preview.viewport.preview_pan = Vec2::new(
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    );
                }

                let available = ui.available_size_before_wrap();
                let preview_available = Vec2::new(
                    (available.x - RULER_SIZE).max(200.0),
                    (available.y - RULER_SIZE).max(180.0),
                );
                let desired = fit_preview(ctx.scene_dimensions, preview_available);
                let total_size = desired + Vec2::new(RULER_SIZE, RULER_SIZE);
                let (total_rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
                let preview_rect = egui::Rect::from_min_size(
                    egui::pos2(total_rect.min.x + RULER_SIZE, total_rect.min.y + RULER_SIZE),
                    desired,
                );
                let response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
                ui.painter().rect_stroke(
                    preview_rect,
                    RADIUS_L,
                    egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
                    egui::StrokeKind::Outside,
                );
                ui.painter().rect_filled(preview_rect, RADIUS_L, theme.palette.surface.base);

                // ── Rulers ──
                let ruler_bg = theme.palette.surface.panel;
                let ruler_tick_color = theme.palette.text.muted;
                let ruler_text_color = theme.palette.text.muted;
                let ruler_label_color = theme.palette.text.secondary;

                let h_ruler_rect = egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x, preview_rect.min.y - RULER_SIZE),
                    Vec2::new(preview_rect.width(), RULER_SIZE),
                );
                let v_ruler_rect = egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y),
                    Vec2::new(RULER_SIZE, preview_rect.height()),
                );
                let corner_rect = egui::Rect::from_min_size(
                    egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y - RULER_SIZE),
                    Vec2::new(RULER_SIZE, RULER_SIZE),
                );
                let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default);

                ui.painter().rect_filled(corner_rect, 0.0, ruler_bg);
                ui.painter()
                    .rect_stroke(corner_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);

                let scene_tl = preview_screen_to_scene(
                    ctx.scene_dimensions,
                    preview_rect,
                    preview_rect.left_top(),
                    ctx.preview.viewport.preview_zoom,
                    ctx.preview.viewport.preview_pan,
                );
                let scene_br = preview_screen_to_scene(
                    ctx.scene_dimensions,
                    preview_rect,
                    preview_rect.right_bottom(),
                    ctx.preview.viewport.preview_zoom,
                    ctx.preview.viewport.preview_pan,
                );
                let visible_w = (scene_br.x - scene_tl.x) as f32;
                let visible_h = (scene_br.y - scene_tl.y) as f32;

                // Horizontal ruler
                ui.painter().rect_filled(h_ruler_rect, 0.0, ruler_bg);
                ui.painter().rect_stroke(
                    h_ruler_rect,
                    0.0,
                    ruler_stroke,
                    egui::StrokeKind::Outside,
                );
                let h_interval =
                    nice_tick_interval(visible_w, h_ruler_rect.width() / 60.0).max(1.0);
                let h_start = ((scene_tl.x as f32) / h_interval).floor() as i32 * h_interval as i32;
                let h_end = ((scene_br.x as f32) / h_interval).ceil() as i32 * h_interval as i32;
                let mut tick_x = h_start as f32;
                while tick_x <= h_end as f32 {
                    let screen_pt = preview_scene_to_screen(
                        ctx.scene_dimensions,
                        preview_rect,
                        kurbo::Point::new(tick_x as f64, scene_tl.y),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    if screen_pt.x >= h_ruler_rect.min.x && screen_pt.x <= h_ruler_rect.max.x {
                        let rel_x = screen_pt.x - h_ruler_rect.min.x;
                        let is_major = (tick_x as i32) % (h_interval as i32 * 5) == 0;
                        let tick_h = if is_major {
                            RULER_SIZE * 0.6
                        } else {
                            RULER_SIZE * 0.3
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y),
                                egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y - tick_h),
                            ],
                            egui::Stroke::new(
                                STROKE_WIDTH,
                                if is_major {
                                    ruler_label_color
                                } else {
                                    ruler_tick_color
                                },
                            ),
                        );
                        if is_major {
                            ui.painter().text(
                                egui::pos2(
                                    h_ruler_rect.min.x + rel_x,
                                    h_ruler_rect.min.y + RULER_SIZE * 0.3,
                                ),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", tick_x as i32),
                                TextRole::Micro.font_id(),
                                ruler_text_color,
                            );
                        }
                    }
                    tick_x += h_interval;
                }

                // Vertical ruler
                ui.painter().rect_filled(v_ruler_rect, 0.0, ruler_bg);
                ui.painter().rect_stroke(
                    v_ruler_rect,
                    0.0,
                    ruler_stroke,
                    egui::StrokeKind::Outside,
                );
                let v_interval =
                    nice_tick_interval(visible_h, v_ruler_rect.height() / 60.0).max(1.0);
                let v_start = ((scene_tl.y as f32) / v_interval).floor() as i32 * v_interval as i32;
                let v_end = ((scene_br.y as f32) / v_interval).ceil() as i32 * v_interval as i32;
                let mut tick_y = v_start as f32;
                while tick_y <= v_end as f32 {
                    let screen_pt = preview_scene_to_screen(
                        ctx.scene_dimensions,
                        preview_rect,
                        kurbo::Point::new(scene_tl.x, tick_y as f64),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    if screen_pt.y >= v_ruler_rect.min.y && screen_pt.y <= v_ruler_rect.max.y {
                        let rel_y = screen_pt.y - v_ruler_rect.min.y;
                        let is_major = (tick_y as i32) % (v_interval as i32 * 5) == 0;
                        let tick_w = if is_major {
                            RULER_SIZE * 0.6
                        } else {
                            RULER_SIZE * 0.3
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(v_ruler_rect.max.x, v_ruler_rect.min.y + rel_y),
                                egui::pos2(v_ruler_rect.max.x - tick_w, v_ruler_rect.min.y + rel_y),
                            ],
                            egui::Stroke::new(
                                STROKE_WIDTH,
                                if is_major {
                                    ruler_label_color
                                } else {
                                    ruler_tick_color
                                },
                            ),
                        );
                        if is_major {
                            ui.painter().text(
                                egui::pos2(
                                    v_ruler_rect.min.x + RULER_SIZE * 0.3,
                                    v_ruler_rect.min.y + rel_y,
                                ),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", tick_y as i32),
                                TextRole::Micro.font_id(),
                                ruler_text_color,
                            );
                        }
                    }
                    tick_y += v_interval;
                }

                // ── Ruler drag interaction ──
                let ruler_drag_id = ui.id().with("guide_ruler_drag_v2");
                let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
                let h_ruler_resp = ui.allocate_rect(h_ruler_rect, egui::Sense::drag());
                let v_ruler_resp = ui.allocate_rect(v_ruler_rect, egui::Sense::drag());

                if h_ruler_resp.drag_started() {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        ui.data_mut(|d| {
                            d.insert_temp(ruler_drag_id, Some((false, scene.y as f32, mouse)))
                        });
                    }
                }
                if v_ruler_resp.drag_started() {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        ui.data_mut(|d| {
                            d.insert_temp(ruler_drag_id, Some((true, scene.x as f32, mouse)))
                        });
                    }
                }

                let ruler_drag_active: Option<(bool, f32, egui::Pos2)> =
                    ui.data(|d| d.get_temp(ruler_drag_id));
                if let Some((is_vertical, _start_val, _start_pos)) = ruler_drag_active {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        let guide_color = theme.palette.status.warning;
                        if is_vertical {
                            let ghost_screen = ctx.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(scene.x, 0.0),
                            );
                            if ghost_screen.x >= preview_rect.min.x
                                && ghost_screen.x <= preview_rect.max.x
                            {
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(ghost_screen.x, preview_rect.min.y),
                                        egui::pos2(ghost_screen.x, preview_rect.max.y),
                                    ],
                                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                                );
                            }
                        } else {
                            let ghost_screen = ctx.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(0.0, scene.y),
                            );
                            if ghost_screen.y >= preview_rect.min.y
                                && ghost_screen.y <= preview_rect.max.y
                            {
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(preview_rect.min.x, ghost_screen.y),
                                        egui::pos2(preview_rect.max.x, ghost_screen.y),
                                    ],
                                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                                );
                            }
                        }
                    }
                }

                let pointer_released = ui.input(|i| i.pointer.any_released());
                if let Some((is_vertical, _start_val, start_pos)) = ruler_drag_active {
                    if pointer_released
                        || h_ruler_resp.drag_stopped()
                        || v_ruler_resp.drag_stopped()
                    {
                        if let Some(mouse) = raw_pointer_pos {
                            // Require at least 5 px of movement to avoid accidental clicks
                            let dragged_far_enough = (mouse - start_pos).length() >= 5.0;
                            if dragged_far_enough && preview_rect.contains(mouse) {
                                let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                                if is_vertical {
                                    ctx.preview.guides.vertical_guides.push(scene.x as f32);
                                } else {
                                    ctx.preview.guides.horizontal_guides.push(scene.y as f32);
                                }
                            }
                        }
                        ui.data_mut(|d| d.remove::<Option<(bool, f32, egui::Pos2)>>(ruler_drag_id));
                    }
                }

                // ── Draw existing guides ──
                if ctx.preview.overlay.show_guides {
                    let guide_color = theme.palette.status.warning;
                    for &guide_y in &ctx.preview.guides.horizontal_guides {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(0.0, guide_y as f64),
                        );
                        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(preview_rect.min.x, screen_pt.y),
                                    egui::pos2(preview_rect.max.x, screen_pt.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, guide_color),
                            );
                        }
                    }
                    for &guide_x in &ctx.preview.guides.vertical_guides {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(guide_x as f64, 0.0),
                        );
                        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(screen_pt.x, preview_rect.min.y),
                                    egui::pos2(screen_pt.x, preview_rect.max.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, guide_color),
                            );
                        }
                    }
                }

                // ── Scroll zoom ──
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta);
                    if scroll.y != 0.0 {
                        let zoom_factor = 1.0 + scroll.y * 0.001;
                        let new_zoom = (ctx.preview.viewport.preview_zoom * zoom_factor)
                            .clamp(preview_spatial::MIN_ZOOM, 10.0);
                        let prev_zoom = ctx.preview.viewport.preview_zoom;
                        if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                            let cursor_in_rect = preview_rect.contains(cursor);
                            if cursor_in_rect && prev_zoom > 0.01 {
                                let scene_at_cursor =
                                    ctx.preview_screen_to_scene(preview_rect, cursor);
                                let rel = cursor - preview_rect.center();
                                ctx.preview.viewport.preview_zoom = new_zoom;
                                let tx = preview::PreviewTransform::new(
                                    ctx.scene_dimensions,
                                    preview_rect,
                                    new_zoom,
                                    Vec2::ZERO,
                                );
                                let (new_scale, _) = tx.scale();
                                let new_pan = Vec2::new(
                                    (scene_at_cursor.x - rel.x as f64 * new_scale) as f32,
                                    (scene_at_cursor.y - rel.y as f64 * new_scale) as f32,
                                );
                                ctx.preview.viewport.preview_pan =
                                    ctx.clamp_pan(new_pan, preview_rect);
                                ctx.preview.status = format!(
                                    "Zoom: {:.0}%",
                                    ctx.preview.viewport.preview_zoom * 100.0
                                );
                            }
                        } else {
                            ctx.preview.viewport.preview_zoom = new_zoom;
                            ctx.preview.status =
                                format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
                        }
                    }
                }

                // ── Middle-click pan ──
                if ui.input(|i| i.pointer.middle_down()) {
                    if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        if preview_rect.contains(mouse) {
                            let delta = ui.input(|i| i.pointer.delta());
                            if delta != Vec2::ZERO {
                                let tx = preview::PreviewTransform::new(
                                    ctx.scene_dimensions,
                                    preview_rect,
                                    ctx.preview.viewport.preview_zoom,
                                    Vec2::ZERO,
                                );
                                let (scale, _) = tx.scale();
                                let new_pan = Vec2::new(
                                    ctx.preview.viewport.preview_pan.x - delta.x * scale as f32,
                                    ctx.preview.viewport.preview_pan.y - delta.y * scale as f32,
                                );
                                ctx.preview.viewport.preview_pan =
                                    ctx.clamp_pan(new_pan, preview_rect);
                            }
                        }
                    }
                }

                // Clear snap lines from previous frame
                ctx.preview.snap.snap_lines_h.clear();
                ctx.preview.snap.snap_lines_v.clear();
                ctx.preview.snap.snap_line_color = None;
                ctx.preview.snap.snap_hud_label = None;

                // ── Time Lens ──
                let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
                let t_held = !wants_keyboard
                    && ui.input(|i| i.key_pressed(egui::Key::T) || i.key_down(egui::Key::T));

                let all_kf = if t_held {
                    let mut all_kf: Vec<f64> = if let Some(tl) = ctx.timeline {
                        tl.root_actor_labels()
                            .iter()
                            .flat_map(|label| {
                                tl.get_track(label)
                                    .map(animatix::timeline::collect_all_keyframe_times)
                                    .unwrap_or_default()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    all_kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    all_kf.dedup_by(|a, b| (*a - *b).abs() < 0.001);
                    all_kf
                } else {
                    Vec::new()
                };
                if let Some(new_time) = ctx.preview.time_lens.update_and_show(
                    ui,
                    ctx.preview.playback.current_time_s(),
                    ctx.preview.playback.duration_s,
                    &all_kf,
                ) {
                    ctx.commands.push_back(PlaybackCommand::ScrubTo(new_time).into());
                }

                let is_dragging = !matches!(ctx.drag_state, DragState::None);
                crate::app::preview::gesture_router::GestureRouter::handle_preview_gestures(
                    ctx,
                    ui,
                    preview_rect,
                    &response,
                );

                let pointer_pos = ui
                    .ctx()
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| preview_rect.contains(*p));
                let scene_dimensions = ctx.scene_dimensions;
                let zoom = ctx.preview.viewport.preview_zoom;
                let pan = ctx.preview.viewport.preview_pan;
                let screen_to_scene = move |screen: egui::Pos2| {
                    preview_screen_to_scene(scene_dimensions, preview_rect, screen, zoom, pan)
                };

                if !ctx.selection.context_menu_open {
                    let unlocked_hit_regions: Vec<(String, kurbo::Rect)> = ctx
                        .hit_regions
                        .iter()
                        .filter(|(label, _)| {
                            !ctx.timeline
                                .and_then(|t| t.get_track(label))
                                .map(|tr| tr.locked)
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    selection::update_hover(
                        ctx.selection,
                        &unlocked_hit_regions,
                        pointer_pos,
                        screen_to_scene,
                        is_dragging,
                    );
                } else {
                    ctx.selection.hovered_actor = None;
                }

                ctx.handle_preview_selection(ui, preview_rect, &response);
                ctx.render_preview_cursor_feedback(ui, preview_rect);
                ctx.render_preview_overlays(ui, preview_rect);
                ctx.render_preview_content(ui, preview_rect);

                let overlay_tx = ctx.preview_transform(preview_rect);

                // ── Scene bounds overlay ──
                if ctx.preview.overlay.show_scene_bounds {
                    let ops =
                        crate::app::preview::overlay_ops::scene_bounds_ops(&theme, overlay_tx);
                    crate::app::preview::overlay_ops::execute_overlay_ops(
                        ui.painter(),
                        &ops,
                        &overlay_tx,
                    );
                }

                // ── Actor labels overlay ──
                if ctx.preview.overlay.show_actor_labels {
                    let ops = crate::app::preview::overlay_ops::actor_label_ops(
                        &theme,
                        ctx.hit_regions,
                        overlay_tx,
                    );
                    crate::app::preview::overlay_ops::execute_overlay_ops(
                        ui.painter(),
                        &ops,
                        &overlay_tx,
                    );
                }

                // Draw grid overlay
                if ctx.preview.overlay.show_grid {
                    let ops = crate::app::preview::overlay_ops::grid_ops(
                        &theme,
                        overlay_tx,
                        ctx.preview.overlay.grid_size,
                    );
                    crate::app::preview::overlay_ops::execute_overlay_ops(
                        ui.painter(),
                        &ops,
                        &overlay_tx,
                    );
                }

                // ── Layout debug overlay ──
                if ctx.debug_layout {
                    ctx.render_layout_debug(ui, preview_rect);
                }

                // ── Motion paths ──
                ctx.render_motion_paths(ui, preview_rect);

                // ── Draw snap indicator lines ──
                if let Some(color) = ctx.preview.snap.snap_line_color {
                    let ops = crate::app::preview::overlay_ops::snap_guide_ops(
                        color,
                        &ctx.preview.snap.snap_lines_h,
                        &ctx.preview.snap.snap_lines_v,
                        overlay_tx,
                    );
                    crate::app::preview::overlay_ops::execute_overlay_ops(
                        ui.painter(),
                        &ops,
                        &overlay_tx,
                    );
                }

                ctx.render_preview_selection_overlay(ui, preview_rect, is_dragging);

                // ── File drop ──
                if response.hovered() {
                    let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
                    for file in dropped_files {
                        if let Some(path) = file.path {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let path_str = path.to_string_lossy().to_string();

                            // .amx files: open directly instead of creating an actor
                            if ext == "amx" {
                                ctx.commands.push_back(DocumentCommand::OpenFile(path).into());
                                continue;
                            }

                            let drop_pos =
                                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                                    let scene = preview_screen_to_scene(
                                        ctx.scene_dimensions,
                                        preview_rect,
                                        mouse,
                                        ctx.preview.viewport.preview_zoom,
                                        ctx.preview.viewport.preview_pan,
                                    );
                                    [scene.x as f32, scene.y as f32]
                                } else {
                                    [
                                        ctx.scene_dimensions.width as f32 / 2.0,
                                        ctx.scene_dimensions.height as f32 / 2.0,
                                    ]
                                };
                            let label = crate::app::utils::labels::unique_label(
                                None,
                                if ext == "svg" { "svg" } else { "image" },
                            );
                            let (ty, props) = if ext == "svg" {
                                (
                                    "Svg".to_string(),
                                    vec![animatix_syntax::ast::Property {
                                        name: "url".into(),
                                        value: animatix_syntax::ast::Expr::Str(path_str),
                                        value_span: None,
                                        trailing_comment: None,
                                    }],
                                )
                            } else {
                                (
                                    "Image".to_string(),
                                    vec![animatix_syntax::ast::Property {
                                        name: "url".into(),
                                        value: animatix_syntax::ast::Expr::Str(path_str),
                                        value_span: None,
                                        trailing_comment: None,
                                    }],
                                )
                            };
                            ctx.commands.push_back(
                                ActorCommand::CreateActor {
                                    ty,
                                    label,
                                    position: drop_pos,
                                    props,
                                }
                                .into(),
                            );
                        }
                    }
                }

                // ── Library drag-to-place ──
                // A Library/Asset row sets a payload while it is dragged; we own
                // the scene transform, so the drop point is resolved here.
                library_drag_drop_ui(ctx, ui, preview_rect);

                // Inline text editor (double-click on text actors)
                ctx.render_inline_text_editor(ui, preview_rect);

                // Floating property cards for selected actors (hide when inline editing)
                if !is_dragging
                    && ctx.selected_actors.len() == 1
                    && ctx.preview.inline_edit.is_none()
                {
                    if let Some(actor) = ctx.selected_actors.iter().next() {
                        if let Some(props) = ctx.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64,
                                    props.position[1] as f64,
                                ),
                                preview_rect,
                                ctx.scene_dimensions,
                                preview_rect.size(),
                                ctx.preview.viewport.preview_zoom,
                                ctx.preview.viewport.preview_pan,
                            );
                            preview::property_popup::show_property_popup(
                                ui,
                                actor,
                                &props,
                                screen_pos,
                                ctx.commands,
                                is_dragging,
                                ctx.timeline,
                                ctx.preview.playback.current_time_s(),
                                ctx.scene_dimensions,
                                ctx.preview.viewport.preview_zoom,
                                preview_rect,
                                ctx.preview.viewport.preview_pan,
                                ctx.active_scene,
                            );
                        }
                    }
                }
            });
        });
}
