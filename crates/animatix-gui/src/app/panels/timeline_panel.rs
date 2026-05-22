//! Global Timeline Panel
//!
//! A vertical-scrolling timeline with a scene track (for compositions),
//! per-actor tracks, a ruler, draggable playhead, loop-region overlay,
//! and a work/export range slider.
//!
//! Design:
//! ┌────────────────────────────────────────────────────┐
//! │  Ruler: 0s  1s  2s  3s  4s  5s                    │
//! ├────────┬──────────────────────────────────────────-┤
//! │ Scenes │ [──Intro───][─Diagram──][───Outro──]      │
//! ├────────┼──────────────────────────────────────────-┤
//! │ rect1  │ ◆         ◆        ◆                      │
//! │ rect2  │    ◆           ◆                          │
//! │ circle │ ◆  ◆     ◆                                │
//! ├────────┼──────────────────────────────────────────-┤
//! │ Region │ [◀────────────▶]                          │
//! └────────┴──────────────────────────────────────────-┘

use crate::app::commands::{Command, CommandQueue};
use crate::app::theme::*;
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Vec2};

/// Width of the track label column on the left.
const LABEL_COL_WIDTH: f32 = 120.0;
/// Height of each track row.
const TRACK_ROW_HEIGHT: f32 = 24.0;
/// Height of the ruler bar.
const RULER_HEIGHT: f32 = 22.0;
/// Height of the range slider at the bottom.
const RANGE_HEIGHT: f32 = 20.0;
/// Diamond keyframe marker half-size.
const KF_DIAMOND_HALF: f32 = 4.0;

/// A single keyframe time in seconds.
struct KfEntry {
    time_s: f64,
}

/// Render the entire timeline panel.
pub(crate) fn timeline_panel_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    _active_scene: Option<&str>,
    commands: &mut CommandQueue,
) {
    let duration_s = preview.duration_s.max(0.1);
    let panel_id = ui.id().with("timeline_panel");

    // ── Track labels sidebar width ──
    let available = ui.available_width();
    let label_col_w = LABEL_COL_WIDTH.min(available * 0.3);
    let bar_origin_x = ui.cursor().min.x + label_col_w;
    let bar_width = (available - label_col_w).max(80.0);

    // Collect tracks to display
    let actor_labels: Vec<String> = if let Some(tl) = timeline {
        tl.root_actor_labels().to_vec()
    } else {
        Vec::new()
    };

    // ── Vertical scroll area for all tracks ──
    let total_track_count = {
        let mut count = 0usize;
        if composition.is_some() {
            count += 1; // scene track
        }
        count += actor_labels.len();
        count
    };
    let total_content_height = RULER_HEIGHT
        + total_track_count as f32 * TRACK_ROW_HEIGHT
        + RANGE_HEIGHT;

    let scroll_id = panel_id.with("scroll");
    let (scroll_rect, _scroll_response) = ui.allocate_exact_size(
        Vec2::new(available, ui.available_height().max(60.0)),
        Sense::hover(),
    );
    let mut scroll_offset: f32 = ui.data(|d| d.get_temp::<f32>(scroll_id)).unwrap_or(0.0);
    let max_scroll = (total_content_height - scroll_rect.height()).max(0.0);

    // Clip to the scroll area
    let painter = ui.painter_at(scroll_rect);
    let _clip_rect = scroll_rect;

    // ── Helper to check if a y range is visible ──
    let visible = |y_top: f32, y_bot: f32| -> bool {
        y_bot >= scroll_rect.top() && y_top <= scroll_rect.bottom()
    };

    // Track the y position within the virtual canvas
    let mut virtual_y = scroll_rect.top() - scroll_offset;

    // ── Helper: pixel X for a given time ──
    let time_to_x = |t: f64| -> f32 {
        let frac = (t / duration_s).clamp(0.0, 1.0) as f32;
        bar_origin_x + frac * bar_width
    };

    // ── Draw ruler ──
    {
        let ruler_top = virtual_y;
        let ruler_bot = ruler_top + RULER_HEIGHT;
        virtual_y = ruler_bot;

        if visible(ruler_top, ruler_bot) {
            // Ruler background
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(scroll_rect.left(), ruler_top),
                    Pos2::new(scroll_rect.right(), ruler_bot),
                ),
                0.0,
                BG_SURFACE,
            );

            // Ruler label column background
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(scroll_rect.left(), ruler_top),
                    Pos2::new(bar_origin_x, ruler_bot),
                ),
                0.0,
                BG_BASE,
            );

            // Tick marks
            let tick_step = if duration_s <= 2.0 {
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

            let mut t = 0.0;
            while t <= duration_s {
                let x = time_to_x(t);
                if x >= bar_origin_x && x <= bar_origin_x + bar_width {
                    let tick_top = ruler_bot - 6.0;
                    painter.line_segment(
                        [Pos2::new(x, tick_top), Pos2::new(x, ruler_bot)],
                        Stroke::new(1.0, BORDER),
                    );

                    // Time label
                    let label = if tick_step >= 1.0 {
                        format!("{:.0}s", t)
                    } else {
                        format!("{:.1}s", t)
                    };
                    painter.text(
                        Pos2::new(x, ruler_top + RULER_HEIGHT * 0.35),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::monospace(FONT_SIZE_XS),
                        TEXT_MUTED,
                    );
                }
                t += tick_step;
            }
        }
    }

    // ── Helper: draw playhead ──
    let playhead_x = time_to_x(preview.current_time_s);

    // ── Helper: draw loop region (function, not closure, to avoid borrow conflicts) ──
    fn draw_loop_region(
        p: &egui::Painter,
        y_top: f32,
        y_bot: f32,
        preview: &PreviewPaneState,
        time_to_x: &dyn Fn(f64) -> f32,
    ) {
        if let (Some(ls), Some(le)) = (preview.loop_start_s, preview.loop_end_s) {
            if le > ls {
                let lx = time_to_x(ls);
                let rx = time_to_x(le);
                if (rx - lx).abs() > 2.0 {
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(lx, y_top), Pos2::new(rx, y_bot)),
                        0.0,
                        loop_region(),
                    );
                }
            }
        }
    }

    // ── Interaction helper for bar areas ──
    let bar_interaction = |ui: &egui::Ui,
                           bar_rect: Rect,
                           id_salt: &str,
                           cmds: &mut CommandQueue,
                           preview: &mut PreviewPaneState| {
        let bar_id = ui.id().with(id_salt);
        let response = ui.interact(bar_rect, bar_id, Sense::click_and_drag());
        if response.clicked() || response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                let frac = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                let new_time = frac * duration_s;
                cmds.push_back(Command::ScrubTo(new_time));
                preview.current_time_s = new_time;
            }
        }
    };

    // ── Scene track (composition only) ──
    if let Some(comp) = composition {
        let st_top = virtual_y;
        let st_bot = st_top + TRACK_ROW_HEIGHT;
        virtual_y = st_bot;

        if visible(st_top, st_bot) {
            let track_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), st_top),
                Pos2::new(scroll_rect.right(), st_bot),
            );

            // Label background
            let label_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), st_top),
                Pos2::new(bar_origin_x, st_bot),
            );
            painter.rect_filled(label_rect, 0.0, BG_BASE);

            // Label text
            painter.text(
                Pos2::new(bar_origin_x - SPACE_S, track_rect.center().y),
                Align2::RIGHT_CENTER,
                format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
                FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                TEXT_MUTED,
            );

            // Bar area
            let bar_area = Rect::from_min_max(
                Pos2::new(bar_origin_x, st_top),
                Pos2::new(scroll_rect.right(), st_bot),
            );

            // Scene blocks
            let palette = [
                track_block_1(),
                track_block_2(),
                track_block_3(),
                track_block_4(),
                track_block_5(),
            ];
            let total = duration_s;
            for (idx, scene_name) in comp.declaration_order.iter().enumerate() {
                let Some(scene) = comp.scenes.get(scene_name) else { continue };
                let Some(start_s) = comp.scene_start_times.get(scene_name).copied() else { continue };
                let end_s = (start_s + scene.duration_s).min(total);
                if end_s <= start_s {
                    continue;
                }

                let left = time_to_x(start_s);
                let right = time_to_x(end_s);
                let scene_rect = Rect::from_min_max(
                    Pos2::new(left, bar_area.top()),
                    Pos2::new(right, bar_area.bottom()),
                );
                let color = palette[idx % palette.len()];
                painter.rect_filled(scene_rect, 2.0, color);

                // Scene label
                let scene_width = scene_rect.width();
                if scene_width > 24.0 {
                    painter.text(
                        scene_rect.center(),
                        Align2::CENTER_CENTER,
                        scene_name.as_str(),
                        FontId::monospace(FONT_SIZE_XS),
                        text_dim(),
                    );
                }
            }

            // Loop region on scene track
            draw_loop_region(&painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

            // Scene track interaction
            bar_interaction(ui, bar_area, "scene_track", commands, preview);

            // Playhead on scene track
            let playhead_top = bar_area.top() - 2.0;
            let playhead_bot = bar_area.bottom() + 2.0;
            painter.line_segment(
                [Pos2::new(playhead_x, playhead_top), Pos2::new(playhead_x, playhead_bot)],
                Stroke::new(1.5, TEXT_PRIMARY),
            );

            // Bottom hairline
            painter.line_segment(
                [Pos2::new(scroll_rect.left(), st_bot), Pos2::new(scroll_rect.right(), st_bot)],
                Stroke::new(1.0, BORDER),
            );
        }
    }

    // ── Actor tracks ──
    for (track_idx, actor_label) in actor_labels.iter().enumerate() {
        let at_top = virtual_y;
        let at_bot = at_top + TRACK_ROW_HEIGHT;
        virtual_y = at_bot;

        if visible(at_top, at_bot) {
            let track_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), at_top),
                Pos2::new(scroll_rect.right(), at_bot),
            );

            // Alternating row background
            if track_idx % 2 == 0 {
                painter.rect_filled(track_rect, 0.0, row_alt());
            }

            // Label background
            let label_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), at_top),
                Pos2::new(bar_origin_x, at_bot),
            );
            painter.rect_filled(label_rect, 0.0, BG_BASE);

            // Label text — show actor label, truncated
            let display_label = if actor_label.len() > 16 {
                format!("{}…", &actor_label[..15])
            } else {
                actor_label.clone()
            };
            painter.text(
                Pos2::new(bar_origin_x - SPACE_S, track_rect.center().y),
                Align2::RIGHT_CENTER,
                display_label,
                FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                TEXT_SECONDARY,
            );

            // Bar area
            let bar_area = Rect::from_min_max(
                Pos2::new(bar_origin_x, at_top),
                Pos2::new(scroll_rect.right(), at_bot),
            );

            // Draw keyframe diamonds for this actor
            if let Some(tl) = timeline {
                if let Some(track) = tl.get_track(actor_label) {
                    // Collect all keyframe times from the track
                    let mut kf_times_ms: Vec<u64> = Vec::new();
                    macro_rules! extend_kf {
                        ($opt:expr) => {
                            if let Some(pt) = $opt.as_ref() {
                                kf_times_ms.extend(pt.keyframes.keys().copied());
                            }
                        };
                    }
                    extend_kf!(track.position);
                    extend_kf!(track.motion_offset);
                    extend_kf!(track.rotation);
                    extend_kf!(track.scale);
                    extend_kf!(track.size);
                    extend_kf!(track.color);
                    extend_kf!(track.opacity);
                    extend_kf!(track.stroke_width);
                    extend_kf!(track.stroke_color);
                    extend_kf!(track.stroke_progress);
                    extend_kf!(track.fill_opacity);
                    extend_kf!(track.text_content);
                    extend_kf!(track.font_family);
                    extend_kf!(track.font_size);
                    extend_kf!(track.shape_type);
                    extend_kf!(track.line_from);
                    extend_kf!(track.line_to);
                    extend_kf!(track.arc_angles);
                    if let Some(ls) = track.layout_size.as_ref() {
                        kf_times_ms.extend(ls.keyframes.keys().copied());
                    }
                    if let Some(vp) = track.vector_paths.as_ref() {
                        kf_times_ms.extend(vp.keyframes.keys().copied());
                    }

                    kf_times_ms.sort_unstable();
                    kf_times_ms.dedup();

                    for &kf_ms in &kf_times_ms {
                        let kf_s = kf_ms as f64 / 1000.0;
                        let kf_x = time_to_x(kf_s);
                        if kf_x >= bar_area.left() && kf_x <= bar_area.right() {
                            // KF diamond
                            let is_active = (kf_s - preview.current_time_s).abs() < 0.01;
                            let kf_color = if is_active { TEXT_PRIMARY } else { AMBER };
                            let pts = vec![
                                Pos2::new(kf_x, bar_area.center().y - KF_DIAMOND_HALF),
                                Pos2::new(kf_x + KF_DIAMOND_HALF, bar_area.center().y),
                                Pos2::new(kf_x, bar_area.center().y + KF_DIAMOND_HALF),
                                Pos2::new(kf_x - KF_DIAMOND_HALF, bar_area.center().y),
                            ];
                            painter.add(egui::Shape::convex_polygon(
                                pts,
                                kf_color,
                                Stroke::NONE,
                            ));
                        }
                    }
                }
            }

            // Loop region on this track
            draw_loop_region(&painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

            // Interaction: click/drag to scrub, click KF to jump
            let bar_id = ui.id().with(format!("actor_track_{}", actor_label));
            let response = ui.interact(bar_area, bar_id, Sense::click_and_drag());

            if response.clicked() {
                if let Some(pos) = response.interact_pointer_pos() {
                    // Check if click landed on a keyframe
                    if let Some(tl) = timeline {
                        if let Some(track) = tl.get_track(actor_label) {
                            let click_s = {
                                let frac = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                                frac * duration_s
                            };
                            // Check proximity to any keyframe
                            let mut kf_times_ms: Vec<u64> = Vec::new();
                            macro_rules! collect_kf {
                                ($opt:expr) => {
                                    if let Some(pt) = $opt.as_ref() {
                                        kf_times_ms.extend(pt.keyframes.keys().copied());
                                    }
                                };
                            }
                            collect_kf!(track.position);
                            collect_kf!(track.motion_offset);
                            collect_kf!(track.rotation);
                            collect_kf!(track.scale);
                            collect_kf!(track.size);
                            collect_kf!(track.color);
                            collect_kf!(track.opacity);
                            collect_kf!(track.stroke_width);
                            collect_kf!(track.stroke_color);
                            collect_kf!(track.stroke_progress);
                            collect_kf!(track.fill_opacity);
                            collect_kf!(track.text_content);
                            collect_kf!(track.font_family);
                            collect_kf!(track.font_size);
                            collect_kf!(track.shape_type);
                            collect_kf!(track.line_from);
                            collect_kf!(track.line_to);
                            collect_kf!(track.arc_angles);
                            if let Some(ls) = track.layout_size.as_ref() {
                                kf_times_ms.extend(ls.keyframes.keys().copied());
                            }

                            let snap_threshold_s = (bar_width / duration_s as f32) * 6.0; // ~6px in time
                            let snapped = kf_times_ms.iter().find(|&&kf_ms| {
                                let kf_s = kf_ms as f64 / 1000.0;
                                (kf_s - click_s).abs() < snap_threshold_s as f64
                            });

                            if let Some(&kf_ms) = snapped {
                                let jump_s = kf_ms as f64 / 1000.0;
                                commands.push_back(Command::ScrubTo(jump_s));
                                preview.current_time_s = jump_s;
                            } else {
                                commands.push_back(Command::ScrubTo(click_s));
                                preview.current_time_s = click_s;
                            }
                        }
                    }
                }
            } else if response.dragged() {
                if let Some(pos) = response.interact_pointer_pos() {
                    let frac = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                    let new_time = frac * duration_s;
                    commands.push_back(Command::ScrubTo(new_time));
                    preview.current_time_s = new_time;
                }
            }

            // Playhead on this track
            painter.line_segment(
                [
                    Pos2::new(playhead_x, bar_area.top()),
                    Pos2::new(playhead_x, bar_area.bottom()),
                ],
                Stroke::new(1.0, text_faint()),
            );

            // Bottom hairline
            painter.line_segment(
                [Pos2::new(scroll_rect.left(), at_bot), Pos2::new(scroll_rect.right(), at_bot)],
                Stroke::new(1.0, BORDER),
            );
        }
    }

    // ── Range slider for work/export region ──
    {
        let rs_top = virtual_y;
        let rs_bot = rs_top + RANGE_HEIGHT;
        virtual_y = rs_bot;

        if visible(rs_top, rs_bot) {
            let range_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), rs_top),
                Pos2::new(scroll_rect.right(), rs_bot),
            );

            // Label background
            let label_rect = Rect::from_min_max(
                Pos2::new(scroll_rect.left(), rs_top),
                Pos2::new(bar_origin_x, rs_bot),
            );
            painter.rect_filled(label_rect, 0.0, BG_BASE);

            // Label
            painter.text(
                Pos2::new(bar_origin_x - SPACE_S, range_rect.center().y),
                Align2::RIGHT_CENTER,
                "Region",
                FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                TEXT_MUTED,
            );

            // Range bar area
            let range_bar = Rect::from_min_max(
                Pos2::new(bar_origin_x, rs_top),
                Pos2::new(scroll_rect.right(), rs_bot),
            );

            // Default work region = full range
            let work_start_s = preview.loop_start_s.unwrap_or(0.0);
            let work_end_s = preview.loop_end_s.unwrap_or(duration_s);

            let ws_x = time_to_x(work_start_s);
            let we_x = time_to_x(work_end_s);

            // Draw the range bar track
            painter.rect_filled(range_bar, RADIUS_S, BG_WIDGET);

            // Draw the active region highlight
            if (we_x - ws_x).abs() > 2.0 {
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(ws_x, range_bar.top() + 2.0), Pos2::new(we_x, range_bar.bottom() - 2.0)),
                    RADIUS_S,
                    ACCENT_BLUE.linear_multiply(0.3),
                );
            }

            // Start handle
            let handle_size = Vec2::new(6.0, RANGE_HEIGHT - 4.0);
            let start_handle_rect = Rect::from_center_size(
                Pos2::new(ws_x, range_bar.center().y),
                handle_size,
            );
            let start_id = ui.id().with("range_start_handle");
            let start_resp = ui.interact(start_handle_rect, start_id, Sense::click_and_drag());
            if start_resp.dragged() {
                if let Some(pos) = start_resp.interact_pointer_pos() {
                    let frac = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                    let new_start = frac * duration_s;
                    preview.loop_start_s = Some(new_start.min(work_end_s - 0.05));
                }
            }
            painter.rect_filled(start_handle_rect, RADIUS_S, ACCENT_BLUE);

            // End handle
            let end_handle_rect = Rect::from_center_size(
                Pos2::new(we_x, range_bar.center().y),
                handle_size,
            );
            let end_id = ui.id().with("range_end_handle");
            let end_resp = ui.interact(end_handle_rect, end_id, Sense::click_and_drag());
            if end_resp.dragged() {
                if let Some(pos) = end_resp.interact_pointer_pos() {
                    let frac = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                    let new_end = frac * duration_s;
                    preview.loop_end_s = Some(new_end.max(work_start_s + 0.05));
                }
            }
            painter.rect_filled(end_handle_rect, RADIUS_S, ACCENT_BLUE);
        }
    }

    // ── Global playhead line spanning the full content height ──
    {
        let global_head_top = scroll_rect.top();
        let global_head_bot = virtual_y.min(scroll_rect.bottom());
        if playhead_x >= bar_origin_x && playhead_x <= bar_origin_x + bar_width + 2.0 {
            painter.line_segment(
                [Pos2::new(playhead_x, global_head_top), Pos2::new(playhead_x, global_head_bot)],
                Stroke::new(1.5, AMBER),
            );
        }
    }

    // ── Scroll handling ──
    {
        let scroll_handle_id = panel_id.with("scroll_handle");
        let _scroll_response = ui.interact(scroll_rect, scroll_handle_id, Sense::click_and_drag());

        // Mouse wheel scrolling
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            scroll_offset = (scroll_offset - scroll_delta * 2.0).clamp(0.0, max_scroll);
            ui.data_mut(|d| d.insert_temp(scroll_id, scroll_offset));
            ui.ctx().request_repaint();
        }

        // Clamp offset
        scroll_offset = scroll_offset.clamp(0.0, max_scroll);
        ui.data_mut(|d| d.insert_temp(scroll_id, scroll_offset));
    }

    // Draw clip rect border
    painter.rect_stroke(
        scroll_rect,
        0.0,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Inside,
    );

    // Claim the space we used
    ui.allocate_exact_size(
        Vec2::new(available, scroll_rect.height().max(60.0)),
        Sense::hover(),
    );
}