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
use crate::app::design_tokens::*;
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

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
/// Height of the playback strip at the top of the timeline.
const PLAYBACK_STRIP_HEIGHT: f32 = 28.0;

fn action_category_color(cat: animatix::timeline::ActionCategory) -> Color32 {
    use animatix::timeline::ActionCategory;
    match cat {
        ActionCategory::Entrance => GREEN,
        ActionCategory::Motion => ACCENT_BLUE,
        ActionCategory::Exit => RED,
        ActionCategory::Effect => AMBER,
        ActionCategory::Reorder => Color32::from_rgb(156, 39, 176),
        ActionCategory::Reveal => ACCENT_CYAN,
    }
}

/// Trait for types that contain keyframe times.
trait KeyframeSource {
    fn keyframe_times(&self) -> Vec<u64>;
}

impl<T> KeyframeSource for animatix::timeline::PropertyTrack<T> {
    fn keyframe_times(&self) -> Vec<u64> {
        self.keyframes.keys().copied().collect()
    }
}

fn push_kf_props(result: &mut Vec<(u64, &'static str)>, opt: &Option<impl KeyframeSource>, name: &'static str) {
    if let Some(pt) = opt {
        result.extend(pt.keyframe_times().into_iter().map(|ms| (ms, name)));
    }
}

fn push_kf_times(result: &mut Vec<u64>, opt: &Option<impl KeyframeSource>) {
    if let Some(pt) = opt {
        result.extend(pt.keyframe_times());
    }
}

/// Collect all keyframe times and their property names from a track.
fn collect_track_keyframe_props(track: &animatix::timeline::AnimationTrack) -> Vec<(u64, &'static str)> {
    let mut result = Vec::new();
    push_kf_props(&mut result, &track.position, "position");
    push_kf_props(&mut result, &track.motion_offset, "motion_offset");
    push_kf_props(&mut result, &track.rotation, "rotation");
    push_kf_props(&mut result, &track.scale, "scale");
    push_kf_props(&mut result, &track.size, "size");
    push_kf_props(&mut result, &track.color, "color");
    push_kf_props(&mut result, &track.opacity, "opacity");
    push_kf_props(&mut result, &track.stroke_width, "stroke_width");
    push_kf_props(&mut result, &track.stroke_color, "stroke_color");
    push_kf_props(&mut result, &track.stroke_progress, "stroke_progress");
    push_kf_props(&mut result, &track.fill_opacity, "fill_opacity");
    push_kf_props(&mut result, &track.text_content, "text_content");
    push_kf_props(&mut result, &track.font_family, "font_family");
    push_kf_props(&mut result, &track.font_size, "font_size");
    push_kf_props(&mut result, &track.shape_type, "shape_type");
    push_kf_props(&mut result, &track.line_from, "line_from");
    push_kf_props(&mut result, &track.line_to, "line_to");
    push_kf_props(&mut result, &track.arc_angles, "arc_angles");
    push_kf_props(&mut result, &track.points, "points");
    push_kf_props(&mut result, &track.commands, "commands");
    push_kf_props(&mut result, &track.layout_size, "layout_size");
    push_kf_props(&mut result, &track.vector_paths, "vector_paths");
    result.sort_by_key(|(ms, _)| *ms);
    result.dedup_by(|a, b| a.0 == b.0);
    result
}

/// Collect all keyframe times from a track.
fn collect_track_keyframe_times(track: &animatix::timeline::AnimationTrack) -> Vec<u64> {
    let mut result = Vec::new();
    push_kf_times(&mut result, &track.position);
    push_kf_times(&mut result, &track.motion_offset);
    push_kf_times(&mut result, &track.rotation);
    push_kf_times(&mut result, &track.scale);
    push_kf_times(&mut result, &track.size);
    push_kf_times(&mut result, &track.color);
    push_kf_times(&mut result, &track.opacity);
    push_kf_times(&mut result, &track.stroke_width);
    push_kf_times(&mut result, &track.stroke_color);
    push_kf_times(&mut result, &track.stroke_progress);
    push_kf_times(&mut result, &track.fill_opacity);
    push_kf_times(&mut result, &track.text_content);
    push_kf_times(&mut result, &track.font_family);
    push_kf_times(&mut result, &track.font_size);
    push_kf_times(&mut result, &track.shape_type);
    push_kf_times(&mut result, &track.line_from);
    push_kf_times(&mut result, &track.line_to);
    push_kf_times(&mut result, &track.arc_angles);
    push_kf_times(&mut result, &track.points);
    push_kf_times(&mut result, &track.commands);
    push_kf_times(&mut result, &track.layout_size);
    push_kf_times(&mut result, &track.vector_paths);
    result
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
    let duration_s = preview.playback.duration_s.max(0.1);
    let panel_id = ui.id().with("timeline_panel");

    // ── Keyframe drag state ──
    let kf_drag_id = panel_id.with("kf_drag");
    let kf_drag_data_id = kf_drag_id.with("data");
    let kf_drag: Option<(String, u64, f64)> = ui.data(|d| d.get_temp(kf_drag_data_id));
    let mut new_kf_drag: Option<(String, u64, f64)> = kf_drag.clone();

    // ── Keyframe multi-select state ──
    let kf_multi_select_id = panel_id.with("kf_multi");
    let mut multi_selected: Vec<(String, u64)> = ui.data(|d| d.get_temp(kf_multi_select_id)).unwrap_or_default();
    let shift_held = ui.input(|i| i.modifiers.shift);

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

    // Allocate the full timeline panel rect (outer frame)
    let (scroll_rect, _scroll_response) = ui.allocate_exact_size(
        Vec2::new(available, ui.available_height().max(60.0)),
        Sense::hover(),
    );

    // ── Helper: pixel X for a given time ──
    let time_to_x = |t: f64| -> f32 {
        let frac = (t / duration_s).clamp(0.0, 1.0) as f32;
        bar_origin_x + frac * bar_width
    };

    // ── Helper: draw loop region (function, not closure, to avoid borrow conflicts) ──
    fn draw_loop_region(
        p: &egui::Painter,
        y_top: f32,
        y_bot: f32,
        preview: &PreviewPaneState,
        time_to_x: &dyn Fn(f64) -> f32,
    ) {
        if let (Some(ls), Some(le)) = (preview.playback.loop_start_s, preview.playback.loop_end_s) {
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

    /// Render colored block segments for each scene in a composition track.
    fn render_scene_blocks(
        painter: &egui::Painter,
        composition: &Composition,
        track_rect: egui::Rect,
        time_to_x: &dyn Fn(f64) -> f32,
        label_color: Color32,
        duration_s: f64,
    ) {
        let palette = [track_block_1(), track_block_2(), track_block_3(), track_block_4(), track_block_5()];
        for (idx, sn) in composition.declaration_order.iter().enumerate() {
            let Some(scene) = composition.scenes.get(sn) else { continue };
            let Some(start_s) = composition.scene_start_times.get(sn).copied() else { continue };
            let end_s = (start_s + scene.duration_s).min(duration_s);
            if end_s <= start_s { continue; }
            let sr = Rect::from_min_max(
                Pos2::new(time_to_x(start_s), track_rect.top()),
                Pos2::new(time_to_x(end_s), track_rect.bottom()),
            );
            painter.rect_filled(sr, 2.0, palette[idx % palette.len()]);
            if sr.width() > 24.0 {
                painter.text(
                    sr.center(),
                    Align2::CENTER_CENTER,
                    sn.as_str(),
                    FontId::monospace(FONT_SIZE_XS),
                    label_color,
                );
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
                preview.playback.current_time_s = new_time;
            }
        }
    };

    // ── All content lives inside the ScrollArea ──
    // ScrollArea handles: scroll offset persistence, wheel/mouse scrolling,
    // clipping, and content culling.
    egui::ScrollArea::vertical()
        .id_salt("timeline_scroll")
        .show(ui, |ui| {
            let mut content_y = ui.cursor().min.y;

            // ── Compute y positions for all sections ──
            let strip_top = content_y;
            let strip_bot = strip_top + PLAYBACK_STRIP_HEIGHT;
            content_y = strip_bot;

            let ruler_top = content_y;
            let ruler_bot = ruler_top + RULER_HEIGHT;
            content_y = ruler_bot;

            let scene_track_top = content_y;
            let scene_track_bot = scene_track_top + TRACK_ROW_HEIGHT;
            if composition.is_some() {
                content_y = scene_track_bot;
            }

            let actor_track_count = actor_labels.len();
            let actor_first_top = content_y;
            let actor_last_bot = actor_first_top + actor_track_count as f32 * TRACK_ROW_HEIGHT;
            content_y = actor_last_bot;

            let rs_top = content_y;
            let rs_bot = rs_top + RANGE_HEIGHT;
            content_y = rs_bot;

            let content_bottom = content_y;

            // ── Allocate total space (releases mutable borrow of ui) ──
            ui.allocate_space(Vec2::new(
                scroll_rect.width(),
                content_bottom - ui.cursor().min.y,
            ));

            // ── Get painter (immutable borrow) and draw everything ──
            let painter = ui.painter();

            // ── Playback strip ──
            {
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(scroll_rect.left(), strip_top),
                        Pos2::new(scroll_rect.right(), strip_bot),
                    ),
                    0.0,
                    BG_BASE,
                );

                let mut cx = scroll_rect.left() + SPACE_S;
                let cy = (strip_top + strip_bot) / 2.0;

                // Go to start
                let start_btn = Rect::from_min_size(Pos2::new(cx, cy - 10.0), Vec2::new(20.0, 20.0));
                let start_r = ui.interact(start_btn, ui.id().with("tl_start"), Sense::click());
                painter.text(start_btn.center(), Align2::CENTER_CENTER, egui_phosphor::regular::SKIP_BACK,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), if start_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                if start_r.clicked() { commands.push_back(Command::ScrubTo(0.0)); }
                cx += 22.0;

                // Previous keyframe
                let prev_btn = Rect::from_min_size(Pos2::new(cx, cy - 10.0), Vec2::new(20.0, 20.0));
                let prev_r = ui.interact(prev_btn, ui.id().with("tl_prev"), Sense::click());
                painter.text(prev_btn.center(), Align2::CENTER_CENTER, egui_phosphor::regular::CARET_LEFT,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), if prev_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                if prev_r.clicked() { commands.push_back(Command::PrevKeyframe); }
                cx += 22.0;

                // Play / Pause
                let play_btn = Rect::from_min_size(Pos2::new(cx, cy - 10.0), Vec2::new(24.0, 20.0));
                let play_r = ui.interact(play_btn, ui.id().with("tl_play"), Sense::click());
                let play_icon = if preview.playback.is_playing { egui_phosphor::regular::PAUSE } else { egui_phosphor::regular::PLAY };
                let play_c = if preview.playback.is_playing { ACCENT_BLUE } else { TEXT_PRIMARY };
                painter.text(play_btn.center(), Align2::CENTER_CENTER, play_icon,
                    FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional), if play_r.hovered() { play_c } else { TEXT_MUTED });
                if play_r.clicked() { commands.push_back(Command::TogglePlayback); }
                cx += 26.0;

                // Next keyframe
                let next_btn = Rect::from_min_size(Pos2::new(cx, cy - 10.0), Vec2::new(20.0, 20.0));
                let next_r = ui.interact(next_btn, ui.id().with("tl_next"), Sense::click());
                painter.text(next_btn.center(), Align2::CENTER_CENTER, egui_phosphor::regular::CARET_RIGHT,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), if next_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                if next_r.clicked() { commands.push_back(Command::NextKeyframe); }
                cx += 22.0;

                // Go to end
                let end_btn = Rect::from_min_size(Pos2::new(cx, cy - 10.0), Vec2::new(20.0, 20.0));
                let end_r = ui.interact(end_btn, ui.id().with("tl_end"), Sense::click());
                painter.text(end_btn.center(), Align2::CENTER_CENTER, egui_phosphor::regular::SKIP_FORWARD,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), if end_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                if end_r.clicked() { commands.push_back(Command::ScrubTo(preview.playback.duration_s)); }
                cx += 28.0;

                // Speed dropdown
                const SPEEDS: [(f32, &str); 4] = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×"), (4.0, "4×")];
                let si = SPEEDS.iter().position(|(v, _)| (*v - preview.playback.playback_speed).abs() < f32::EPSILON).unwrap_or(1);
                let speed_btn = Rect::from_min_size(Pos2::new(cx, cy - 9.0), Vec2::new(32.0, 18.0));
                let speed_r = ui.interact(speed_btn, ui.id().with("tl_speed"), Sense::click());
                painter.rect_filled(speed_btn, RADIUS_S as u8, if speed_r.hovered() { BG_WIDGET } else { BG_SURFACE });
                painter.text(speed_btn.center(), Align2::CENTER_CENTER, SPEEDS[si].1,
                    FontId::monospace(FONT_SIZE_XS), if speed_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED });
                if speed_r.clicked() { preview.playback.playback_speed = SPEEDS[(si + 1) % SPEEDS.len()].0; }
                cx += 38.0;

                // Loop toggle
                let loop_active = preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();
                let loop_btn = Rect::from_min_size(Pos2::new(cx, cy - 9.0), Vec2::new(20.0, 18.0));
                let loop_r = ui.interact(loop_btn, ui.id().with("tl_loop"), Sense::click());
                let loop_c = if loop_active { ACCENT_CYAN } else if loop_r.hovered() { TEXT_PRIMARY } else { TEXT_MUTED };
                painter.text(loop_btn.center(), Align2::CENTER_CENTER, egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), loop_c);
                if loop_r.clicked() {
                    if loop_active { preview.playback.loop_start_s = None; preview.playback.loop_end_s = None; }
                    else { preview.playback.loop_start_s = Some(0.0); preview.playback.loop_end_s = Some(preview.playback.duration_s); }
                }

                // Time display (right-aligned)
                let time_text = format!("{:02}:{:02.2} / {:02}:{:02.2}",
                    preview.playback.current_time_s as i32 / 60, preview.playback.current_time_s % 60.0,
                    preview.playback.duration_s as i32 / 60, preview.playback.duration_s % 60.0);
                let time_sz = painter.layout(time_text.clone(), FontId::monospace(FONT_SIZE_S), TEXT_PRIMARY, f32::INFINITY).rect.size();
                painter.text(Pos2::new(scroll_rect.right() - SPACE_S - time_sz.x, cy), Align2::LEFT_CENTER, time_text,
                    FontId::monospace(FONT_SIZE_S), TEXT_PRIMARY);

                painter.line_segment(
                    [Pos2::new(scroll_rect.left(), strip_bot - 1.0), Pos2::new(scroll_rect.right(), strip_bot - 1.0)],
                    Stroke::new(1.0, BORDER));
            }

            // ── Ruler ──
            {
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(scroll_rect.left(), ruler_top), Pos2::new(scroll_rect.right(), ruler_bot)),
                    0.0, BG_SURFACE);
                painter.rect_filled(
                    Rect::from_min_max(Pos2::new(scroll_rect.left(), ruler_top), Pos2::new(bar_origin_x, ruler_bot)),
                    0.0, BG_BASE);

                let tick_step = if duration_s <= 2.0 { 0.25 } else if duration_s <= 5.0 { 0.5 }
                    else if duration_s <= 15.0 { 1.0 } else if duration_s <= 45.0 { 5.0 } else { 10.0 };
                let mut t = 0.0;
                while t <= duration_s {
                    let x = time_to_x(t);
                    if x >= bar_origin_x && x <= bar_origin_x + bar_width {
                        painter.line_segment([Pos2::new(x, ruler_bot - 6.0), Pos2::new(x, ruler_bot)], Stroke::new(1.0, BORDER));
                        painter.text(Pos2::new(x, ruler_top + RULER_HEIGHT * 0.35), Align2::CENTER_CENTER,
                            if tick_step >= 1.0 { format!("{:.0}s", t) } else { format!("{:.1}s", t) },
                            FontId::monospace(FONT_SIZE_XS), TEXT_MUTED);
                    }
                    t += tick_step;
                }
            }

            // ── Playhead X position ──
            let playhead_x = time_to_x(preview.playback.current_time_s);

            // ── Scene track (composition only) ──
            if let Some(comp) = composition {
                let st_top = scene_track_top;
                let st_bot = scene_track_bot;

                let label_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), st_top), Pos2::new(bar_origin_x, st_bot));
                painter.rect_filled(label_rect, 0.0, BG_BASE);
                painter.text(Pos2::new(bar_origin_x - SPACE_S, (st_top + st_bot) / 2.0), Align2::RIGHT_CENTER,
                    format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), TEXT_MUTED);

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, st_top), Pos2::new(scroll_rect.right(), st_bot));
                render_scene_blocks(&painter, comp, bar_area, &time_to_x, text_dim(), duration_s);

                for (src_name, edge) in &comp.edges {
                    let Some(src_scene) = comp.scenes.get(src_name) else { continue };
                    let Some(src_start) = comp.scene_start_times.get(src_name).copied() else { continue };
                    let src_right = time_to_x(src_start + src_scene.duration_s);
                    let Some(tgt_start) = comp.scene_start_times.get(&edge.to_scene).copied() else { continue };
                    let tgt_left = time_to_x(tgt_start);
                    if tgt_left <= src_right { continue; }
                    let cy = bar_area.center().y;
                    painter.line_segment([Pos2::new(src_right, cy), Pos2::new(tgt_left, cy)], Stroke::new(1.0, TEXT_MUTED));
                    painter.add(egui::Shape::convex_polygon(
                        vec![Pos2::new(tgt_left, cy), Pos2::new(tgt_left - 4.0, cy - 2.5), Pos2::new(tgt_left - 4.0, cy + 2.5)],
                        TEXT_MUTED, Stroke::NONE));
                }

                draw_loop_region(&painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);
                bar_interaction(ui, bar_area, "scene_track", commands, preview);
                painter.line_segment([Pos2::new(playhead_x, bar_area.top() - 2.0), Pos2::new(playhead_x, bar_area.bottom() + 2.0)], Stroke::new(1.5, TEXT_PRIMARY));
                painter.line_segment([Pos2::new(scroll_rect.left(), st_bot), Pos2::new(scroll_rect.right(), st_bot)], Stroke::new(1.0, BORDER));
            }

            // ── Actor tracks ──
            for (track_idx, actor_label) in actor_labels.iter().enumerate() {
                let at_top = actor_first_top + track_idx as f32 * TRACK_ROW_HEIGHT;
                let at_bot = at_top + TRACK_ROW_HEIGHT;
                let track_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(scroll_rect.right(), at_bot));
                if track_idx % 2 == 0 { painter.rect_filled(track_rect, 0.0, row_alt()); }

                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(bar_origin_x, at_bot)), 0.0, BG_BASE);
                let label = if actor_label.len() > 16 { format!("{}…", &actor_label[..15]) } else { actor_label.clone() };
                painter.text(Pos2::new(bar_origin_x - SPACE_S, track_rect.center().y), Align2::RIGHT_CENTER, label,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), TEXT_SECONDARY);

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, at_top), Pos2::new(scroll_rect.right(), at_bot));

                // Action blocks
                if let Some(tl) = timeline {
                    for event in &tl.action_events {
                        if !event.targets.contains(actor_label) { continue; }
                        let left = time_to_x(event.start_time_ms as f64 / 1000.0);
                        let right = time_to_x((event.start_time_ms + event.duration_ms) as f64 / 1000.0);
                        if right > bar_area.left() && left < bar_area.right() {
                            let br = Rect::from_min_max(Pos2::new(left.max(bar_area.left()), bar_area.top() + 2.0), Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 2.0));
                            let color = action_category_color(event.category);
                            painter.rect_filled(br, RADIUS_S, color.linear_multiply(0.6));
                            painter.rect_stroke(br, RADIUS_S, Stroke::new(1.0, color), egui::StrokeKind::Outside);
                            if br.width() > 30.0 { painter.text(br.center(), Align2::CENTER_CENTER, &event.verb, FontId::monospace(FONT_SIZE_XS), TEXT_PRIMARY); }
                        }
                    }
                }

                // Keyframe diamonds
                if let Some(tl) = timeline {
                    if let Some(track) = tl.get_track(actor_label) {
                        for &(kf_ms, prop) in &collect_track_keyframe_props(track) {
                            let kf_s = kf_ms as f64 / 1000.0;
                            let kf_x = time_to_x(kf_s);
                            if kf_x < bar_area.left() || kf_x > bar_area.right() { continue; }
                            let is_act = (kf_s - preview.playback.current_time_s).abs() < 0.01;
                            let is_ms = multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms);
                            let is_drag = kf_drag.as_ref().is_some_and(|(l, t, _)| l == actor_label && *t == kf_ms);
                            let ds = if is_drag { KF_DIAMOND_HALF * 1.5 } else { KF_DIAMOND_HALF };
                            let kc = if is_ms { ACCENT_BLUE } else if is_act { TEXT_PRIMARY } else { AMBER };
                            let cy = bar_area.center().y;
                            let dr = Rect::from_center_size(Pos2::new(kf_x, cy), Vec2::new((ds * 3.0).max(16.0), (ds * 3.0).max(16.0)));
                            let dresp = ui.interact(dr, ui.id().with(("kf_diamond", actor_label.clone(), kf_ms)), Sense::click_and_drag());
                            painter.add(egui::Shape::convex_polygon(
                                vec![Pos2::new(kf_x, cy - ds), Pos2::new(kf_x + ds, cy), Pos2::new(kf_x, cy + ds), Pos2::new(kf_x - ds, cy)],
                                if dresp.hovered() || is_drag { kc } else { kc.linear_multiply(0.7) }, Stroke::NONE));

                            let dresp = if dresp.hovered() && !is_drag { dresp.on_hover_text(format!("{prop} @ {:.2}s", kf_s)) } else { dresp };
                            if dresp.clicked() {
                                if shift_held {
                                    if let Some(p) = multi_selected.iter().position(|(l, t)| l == actor_label && *t == kf_ms) { multi_selected.remove(p); }
                                    else { multi_selected.push((actor_label.clone(), kf_ms)); }
                                } else { multi_selected.clear(); multi_selected.push((actor_label.clone(), kf_ms));
                                    commands.push_back(Command::ScrubTo(kf_s)); preview.playback.current_time_s = kf_s; }
                            }
                            if dresp.drag_started() {
                                new_kf_drag = Some((actor_label.clone(), kf_ms, kf_s));
                                if !shift_held && !multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms) {
                                    multi_selected.clear(); multi_selected.push((actor_label.clone(), kf_ms)); }
                            }
                            if is_drag {
                                if let Some(pos) = dresp.interact_pointer_pos() {
                                    let nt = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64 * duration_s;
                                    let snapped = (nt * 10.0).round() / 10.0;
                                    new_kf_drag = Some((actor_label.clone(), kf_ms, snapped));
                                    let gx = time_to_x(snapped);
                                    painter.line_segment([Pos2::new(gx, bar_area.top()), Pos2::new(gx, bar_area.bottom())], Stroke::new(1.0, AMBER.linear_multiply(0.5)));
                                    let g = painter.layout_no_wrap(format!("{:.1}s → {:.1}s", kf_s, snapped), FontId::monospace(FONT_SIZE_XS), TEXT_PRIMARY);
                                    let tr = Rect::from_min_size(Pos2::new(gx - g.size().x / 2.0, bar_area.top() - 16.0) - Vec2::new(0.0, 0.0), g.size() + Vec2::new(8.0, 4.0));
                                    painter.rect_filled(tr, RADIUS_S, BG_SURFACE);
                                    painter.galley(tr.min + Vec2::new(4.0, 2.0), g, TEXT_PRIMARY);
                                }
                            }
                            if dresp.drag_stopped() && is_drag {
                                if let Some((_, _, n)) = new_kf_drag { if (n - kf_s).abs() > 0.01 { commands.push_back(Command::ScrubTo(n)); preview.playback.current_time_s = n; } }
                                new_kf_drag = None;
                            }
                        }
                    }
                }

                draw_loop_region(&painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

                let resp = ui.interact(bar_area, ui.id().with(format!("actor_track_{}", actor_label)), Sense::click_and_drag());
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        if let Some(tl) = timeline {
                            if let Some(track) = tl.get_track(actor_label) {
                                let click_s = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64 * duration_s;
                                let thr = (bar_width / duration_s as f32) * 6.0;
                                let snapped = collect_track_keyframe_times(track).iter().find(|&&kf_ms| ((kf_ms as f64 / 1000.0) - click_s).abs() < thr as f64).copied();
                                if let Some(kf_ms) = snapped {
                                    let js = kf_ms as f64 / 1000.0;
                                    commands.push_back(Command::ScrubTo(js)); preview.playback.current_time_s = js;
                                } else { commands.push_back(Command::ScrubTo(click_s)); preview.playback.current_time_s = click_s; }
                            }
                        }
                    }
                } else if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let nt = ((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64 * duration_s;
                        commands.push_back(Command::ScrubTo(nt)); preview.playback.current_time_s = nt;
                    }
                }

                painter.line_segment([Pos2::new(playhead_x, bar_area.top()), Pos2::new(playhead_x, bar_area.bottom())], Stroke::new(1.0, text_faint()));
                painter.line_segment([Pos2::new(scroll_rect.left(), at_bot), Pos2::new(scroll_rect.right(), at_bot)], Stroke::new(1.0, BORDER));
            }

            // ── Range slider for work/export region ──
            {
                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), rs_top), Pos2::new(bar_origin_x, rs_bot)), 0.0, BG_BASE);
                painter.text(Pos2::new(bar_origin_x - SPACE_S, (rs_top + rs_bot) / 2.0), Align2::RIGHT_CENTER, "Region",
                    FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional), TEXT_MUTED);

                let range_bar = Rect::from_min_max(Pos2::new(bar_origin_x, rs_top), Pos2::new(scroll_rect.right(), rs_bot));
                let ws = preview.playback.loop_start_s.unwrap_or(0.0);
                let we = preview.playback.loop_end_s.unwrap_or(duration_s);
                let wx = time_to_x(ws);
                let wy = time_to_x(we);

                painter.rect_filled(range_bar, RADIUS_S, BG_WIDGET);
                if (wy - wx).abs() > 2.0 {
                    painter.rect_filled(Rect::from_min_max(Pos2::new(wx, range_bar.top() + 2.0), Pos2::new(wy, range_bar.bottom() - 2.0)), RADIUS_S, ACCENT_BLUE.linear_multiply(0.3));
                }

                let hs = Vec2::new(10.0, RANGE_HEIGHT - 2.0);
                let sh = Rect::from_center_size(Pos2::new(wx, range_bar.center().y), hs);
                let sr = ui.interact(sh, ui.id().with("range_start_handle"), Sense::click_and_drag());
                if sr.dragged() {
                    if let Some(pos) = sr.interact_pointer_pos() {
                        preview.playback.loop_start_s = Some((((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64 * duration_s).min(we - 0.05));
                    }
                }
                painter.rect_filled(sh, RADIUS_S, ACCENT_BLUE);

                let eh = Rect::from_center_size(Pos2::new(wy, range_bar.center().y), hs);
                let er = ui.interact(eh, ui.id().with("range_end_handle"), Sense::click_and_drag());
                if er.dragged() {
                    if let Some(pos) = er.interact_pointer_pos() {
                        preview.playback.loop_end_s = Some((((pos.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64 * duration_s).max(ws + 0.05));
                    }
                }
                painter.rect_filled(eh, RADIUS_S, ACCENT_BLUE);
            }

            // ── Global playhead ──
            if playhead_x >= bar_origin_x && playhead_x <= bar_origin_x + bar_width + 2.0 {
                painter.line_segment([Pos2::new(playhead_x, scroll_rect.top()), Pos2::new(playhead_x, content_bottom)], Stroke::new(1.5, AMBER));
            }

            // ── Save keyframe drag + multi-select state ──
            ui.data_mut(|d| {
                if let Some(drag) = new_kf_drag.clone() { d.insert_temp(kf_drag_data_id, drag); }
                else { d.remove::<(String, u64, f64)>(kf_drag_data_id); }
                d.insert_temp(kf_multi_select_id, multi_selected.clone());
            });
        });

    // Draw clip rect border outside the ScrollArea
    ui.painter().rect_stroke(scroll_rect, 0.0, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
}
