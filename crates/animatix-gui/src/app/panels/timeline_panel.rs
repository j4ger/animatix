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

use std::collections::HashSet;

use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::components::button::{play_pause_button, toolbar_action_button, toolbar_separator, toolbar_toggle_button};
use crate::app::design_tokens::*;
use crate::app::PreviewPaneState;
use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

pub(crate) struct TimelineContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub selected_actors: &'a mut HashSet<String>,
    /// Cached actor labels (recomputed in behavior.rs when stale).
    pub actor_labels: &'a [String],
    /// Cached per-actor keyframe property lists.
    pub actor_keyframes: &'a [(String, Vec<(u64, &'static str)>)],
}

/// Render the entire timeline panel.
pub(crate) fn timeline_panel_ui(ctx: &mut TimelineContext<'_>, ui: &mut egui::Ui) {
    render_timeline_content(
        ui,
        ctx.preview,
        ctx.timeline,
        ctx.composition,
        ctx.active_scene,
        ctx.commands,
        ctx.collapsed_actors,
        ctx.selected_actors,
        ctx.actor_labels,
        ctx.actor_keyframes,
    );
}


/// Width of the track label column on the left.
use crate::app::design_tokens::{
    TIMELINE_LABEL_COL_WIDTH as LABEL_COL_WIDTH,
    TIMELINE_TRACK_ROW_HEIGHT as TRACK_ROW_HEIGHT,
    TIMELINE_RULER_HEIGHT as RULER_HEIGHT,
    TIMELINE_RANGE_HEIGHT as RANGE_HEIGHT,
    TIMELINE_KF_HALF as KF_DIAMOND_HALF,
    TIMELINE_PLAYBACK_STRIP_HEIGHT as PLAYBACK_STRIP_HEIGHT,
};

fn action_category_color(cat: animatix::timeline::ActionCategory) -> Color32 {
    use animatix::timeline::ActionCategory;
    match cat {
        ActionCategory::Entrance => GREEN,
        ActionCategory::Motion => ACCENT_BLUE,
        ActionCategory::Exit => RED,
        ActionCategory::Effect => AMBER,
        ActionCategory::Reorder => PURPLE,
        ActionCategory::Reveal => ACCENT_CYAN,
    }
}

/// Build a flat list of (actor_label, depth) in tree order (depth-first).
fn build_actor_tree(timeline: &Timeline, collapsed: &HashSet<String>) -> Vec<(String, usize)> {
    let mut result = Vec::new();
    for root in timeline.root_actor_labels() {
        add_actor_and_children(timeline, root, 0, collapsed, &mut result);
    }
    result
}

fn add_actor_and_children(
    timeline: &Timeline,
    label: &str,
    depth: usize,
    collapsed: &HashSet<String>,
    result: &mut Vec<(String, usize)>,
) {
    result.push((label.to_string(), depth));
    if !collapsed.contains(label) {
        if let Some(track) = timeline.get_track(label) {
            for child in &track.children {
                add_actor_and_children(timeline, child, depth + 1, collapsed, result);
            }
        }
    }
}

/// Collect keyframe times for all properties of an actor.
fn collect_actor_keyframes(track: &animatix::timeline::AnimationTrack) -> Vec<(u64, &'static str)> {
    let mut result = Vec::new();
    use animatix::timeline::PropertyTrack;
    fn push<T>(result: &mut Vec<(u64, &'static str)>, opt: &Option<PropertyTrack<T>>, name: &'static str) {
        if let Some(pt) = opt {
            result.extend(pt.keyframes.keys().copied().map(|ms| (ms, name)));
        }
    }
    push(&mut result, &track.position, "position");
    push(&mut result, &track.motion_offset, "motion_offset");
    push(&mut result, &track.rotation, "rotation");
    push(&mut result, &track.scale, "scale");
    push(&mut result, &track.size, "size");
    push(&mut result, &track.color, "color");
    push(&mut result, &track.opacity, "opacity");
    push(&mut result, &track.stroke_width, "stroke_width");
    push(&mut result, &track.stroke_color, "stroke_color");
    push(&mut result, &track.stroke_progress, "stroke_progress");
    push(&mut result, &track.fill_opacity, "fill_opacity");
    push(&mut result, &track.text_content, "text_content");
    push(&mut result, &track.font_family, "font_family");
    push(&mut result, &track.font_size, "font_size");
    push(&mut result, &track.shape_type, "shape_type");
    push(&mut result, &track.line_from, "line_from");
    push(&mut result, &track.line_to, "line_to");
    push(&mut result, &track.arc_angles, "arc_angles");
    push(&mut result, &track.points, "points");
    push(&mut result, &track.commands, "commands");
    push(&mut result, &track.layout_size, "layout_size");
    push(&mut result, &track.vector_paths, "vector_paths");
    result.sort_by_key(|(ms, _)| *ms);
    result.dedup_by(|a, b| a.0 == b.0);
    result
}

fn render_timeline_content(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    _active_scene: Option<&str>,
    commands: &mut ActionQueue,
    collapsed_actors: &mut HashSet<String>,
    selected_actors: &mut HashSet<String>,
    _actor_labels: &[String],
    _actor_keyframes: &[(String, Vec<(u64, &'static str)>)],
) {
    let duration_s = preview.playback.duration_s.max(0.1);
    let panel_id = ui.id().with("timeline_panel");

    // ── Keyframe drag state ──
    // (actor_label, property_name, keyframe_ms, target_time_s)
    let kf_drag_id = panel_id.with("kf_drag");
    let kf_drag_data_id = kf_drag_id.with("data");
    let kf_drag: Option<(String, &'static str, u64, f64)> = ui.data(|d| d.get_temp(kf_drag_data_id));
    let mut new_kf_drag: Option<(String, &'static str, u64, f64)> = kf_drag.clone();

    // ── Keyframe multi-select state ──
    let kf_multi_select_id = panel_id.with("kf_multi");
    let mut multi_selected: Vec<(String, u64)> = ui.data(|d| d.get_temp(kf_multi_select_id)).unwrap_or_default();
    let shift_held = ui.input(|i| i.modifiers.shift);

    // actor_labels and actor_keyframes are precomputed by behavior.rs but we
    // compute the actor tree directly from the timeline for correctness.

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

    // ── All content lives inside the ScrollArea ──
    // ScrollArea handles: scroll offset persistence, wheel/mouse scrolling,
    // clipping, and content culling.
    egui::ScrollArea::vertical()
        .id_salt("timeline_scroll")
        .show(ui, |ui| {
            // Compute layout metrics from the *inner* Ui so they exactly match
            // the ScrollArea content rect (no mismatch with outer captures).
            let scroll_rect = ui.available_rect_before_wrap();
            let left_edge = scroll_rect.left();
            let available = scroll_rect.width();
            let label_col_w = LABEL_COL_WIDTH.min(available * 0.3);
            let bar_origin_x = left_edge + label_col_w;
            let bar_width = (available - label_col_w).max(80.0);

            let zoom = preview.timeline_zoom.max(0.1);
            let visible_s = duration_s / zoom as f64;
            // When zoomed out (zoom < 1.0) the full timeline fits on screen,
            // so there is nothing to scroll — clamp to 0.
            let max_scroll = if zoom <= 1.0 { 0.0 } else { duration_s - visible_s };
            let scroll_s = preview.timeline_scroll_offset.clamp(0.0, max_scroll);

            // Mouse-wheel zoom on the timeline bar area
            {
                let bar_rect = Rect::from_min_max(
                    Pos2::new(bar_origin_x, scroll_rect.top()),
                    Pos2::new(scroll_rect.right(), scroll_rect.bottom()),
                );
                let bar_resp = ui.interact(bar_rect, ui.id().with("timeline_bar_wheel"), Sense::hover());
                if bar_resp.hovered() {
                    let wheel = ui.input(|i| i.smooth_scroll_delta);
                    if wheel.x != 0.0 || wheel.y != 0.0 {
                        let zoom_delta = if wheel.y != 0.0 { -wheel.y * 0.002 } else { -wheel.x * 0.002 };
                        let old_zoom = preview.timeline_zoom;
                        let new_zoom = (old_zoom * (1.0 + zoom_delta)).clamp(0.25, 8.0);
                        if (new_zoom - old_zoom).abs() > 0.001 {
                            // Keep the time under the cursor stable when zooming
                            if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                                let cursor_time = if cursor.x >= bar_origin_x && cursor.x <= bar_origin_x + bar_width {
                                    let frac = ((cursor.x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                                    scroll_s + frac * visible_s
                                } else {
                                    scroll_s + visible_s / 2.0
                                };
                                let new_visible = duration_s / new_zoom as f64;
                                let max_scroll = if new_zoom <= 1.0 { 0.0 } else { duration_s - new_visible };
                                preview.timeline_scroll_offset = (cursor_time - (cursor.x - bar_origin_x) as f64 / bar_width as f64 * new_visible)
                                    .clamp(0.0, max_scroll);
                            }
                            preview.timeline_zoom = new_zoom;
                        }
                    }
                }
            }

            let time_to_x = |t: f64| -> f32 {
                let frac = ((t - scroll_s) / visible_s).clamp(0.0, 1.0) as f32;
                bar_origin_x + frac * bar_width
            };

            let x_to_time = |x: f32| -> f64 {
                let frac = ((x - bar_origin_x) / bar_width).clamp(0.0, 1.0) as f64;
                scroll_s + frac * visible_s
            };

            let bar_interaction = |ui: &egui::Ui,
                                    bar_rect: Rect,
                                    id_salt: &str,
                                    cmds: &mut ActionQueue| {
                let bar_id = ui.id().with(id_salt);
                let response = ui.interact(bar_rect, bar_id, Sense::click_and_drag());
                if response.clicked() || response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let new_time = x_to_time(pos.x);
                        cmds.push_back(ShellAction::Command(Command::ScrubTo(new_time)));
                    }
                }
            };

            let mut content_y = scroll_rect.top();

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

            // Build actor tree from timeline (all actors, not just roots)
            let actor_tree: Vec<(String, usize)> = timeline.map(|tl| build_actor_tree(tl, collapsed_actors)).unwrap_or_default();
            let actor_track_count = actor_tree.len();
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

            // ── Playback strip (sub-ui, needs mutable borrow before main painter) ──
            {
                let strip_rect = Rect::from_min_size(
                    Pos2::new(scroll_rect.left(), strip_top),
                    Vec2::new(scroll_rect.width(), PLAYBACK_STRIP_HEIGHT),
                );

                ui.scope_builder(egui::UiBuilder::new().max_rect(strip_rect), |ui| {
                    // Background fill
                    ui.painter().rect_filled(
                        Rect::from_min_max(
                            Pos2::new(scroll_rect.left(), strip_top),
                            Pos2::new(scroll_rect.right(), strip_bot),
                        ),
                        0.0,
                        BG_BASE,
                    );

                    ui.horizontal(|ui| {
                        ui.add_space(SPACE_S);

                        // Go to start
                        if toolbar_action_button(ui, egui_phosphor::regular::SKIP_BACK, None, "Go to start (Home)", false).clicked() {
                            commands.push_back(ShellAction::Command(Command::ScrubTo(0.0)));
                        }

                        // Previous keyframe
                        if toolbar_action_button(ui, egui_phosphor::regular::CARET_LEFT, None, "Previous keyframe", false).clicked() {
                            commands.push_back(ShellAction::Command(Command::PrevKeyframe));
                        }

                        // Play / Pause
                        if play_pause_button(ui, preview.playback.is_playing).clicked() {
                            commands.push_back(ShellAction::Command(Command::TogglePlayback));
                        }

                        // Next keyframe
                        if toolbar_action_button(ui, egui_phosphor::regular::CARET_RIGHT, None, "Next keyframe", false).clicked() {
                            commands.push_back(ShellAction::Command(Command::NextKeyframe));
                        }

                        // Go to end
                        if toolbar_action_button(ui, egui_phosphor::regular::SKIP_FORWARD, None, "Go to end (End)", false).clicked() {
                            commands.push_back(ShellAction::Command(Command::ScrubTo(preview.playback.duration_s)));
                        }

                        toolbar_separator(ui);

                        // Speed dropdown
                        const SPEEDS: [(f32, &str); 4] = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×"), (4.0, "4×")];
                        let si = SPEEDS.iter().position(|(v, _)| (*v - preview.playback.playback_speed).abs() < f32::EPSILON).unwrap_or(1);
                        if toolbar_action_button(ui, SPEEDS[si].1, None, "Playback speed", false).clicked() {
                            preview.playback.playback_speed = SPEEDS[(si + 1) % SPEEDS.len()].0;
                        }

                        // Loop toggle
                        let loop_active = preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();
                        if toolbar_toggle_button(ui, egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE, None, "Toggle loop playback", loop_active, false).clicked() {
                            if loop_active {
                                preview.playback.loop_start_s = None;
                                preview.playback.loop_end_s = None;
                            } else {
                                preview.playback.loop_start_s = Some(0.0);
                                preview.playback.loop_end_s = Some(preview.playback.duration_s);
                            }
                        }

                        toolbar_separator(ui);

                        // Zoom controls
                        let zoom_text = format!("{:.0}%", preview.timeline_zoom * 100.0);
                        if ui.button(egui::RichText::new(zoom_text).monospace().size(FONT_SIZE_S).color(TEXT_SECONDARY))
                            .on_hover_text("Reset zoom")
                            .clicked()
                        {
                            preview.timeline_zoom = 1.0;
                            preview.timeline_scroll_offset = 0.0;
                        }
                        if toolbar_action_button(ui, egui_phosphor::regular::MINUS, None, "Zoom out", false).clicked() {
                            preview.timeline_zoom = (preview.timeline_zoom * 0.8).max(0.25);
                        }
                        if toolbar_action_button(ui, egui_phosphor::regular::PLUS, None, "Zoom in", false).clicked() {
                            preview.timeline_zoom = (preview.timeline_zoom * 1.25).min(8.0);
                        }

                        // Time display (right-aligned)
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let time_text = format!("{:.2}s / {:.2}s",
                                preview.playback.current_time_s,
                                preview.playback.duration_s);
                            ui.add(egui::Label::new(
                                egui::RichText::new(time_text)
                                    .font(FontId::monospace(FONT_SIZE_S))
                                    .color(TEXT_PRIMARY),
                            ).selectable(false));
                        });
                    });
                });

                // Bottom border
                let painter = ui.painter();
                painter.line_segment(
                    [Pos2::new(scroll_rect.left(), strip_bot - 1.0), Pos2::new(scroll_rect.right(), strip_bot - 1.0)],
                    Stroke::new(STROKE_WIDTH, BORDER));
            }

            // ── Get painter (immutable borrow) and draw everything ──
            let painter = ui.painter();

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
                        painter.line_segment([Pos2::new(x, ruler_bot - 6.0), Pos2::new(x, ruler_bot)], Stroke::new(STROKE_WIDTH, BORDER));
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
                painter.text(Pos2::new(scroll_rect.left() + SPACE_S, (st_top + st_bot) / 2.0), Align2::LEFT_CENTER,
                    format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional), TEXT_MUTED);

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, st_top), Pos2::new(scroll_rect.right(), st_bot));
                render_scene_blocks(painter, comp, bar_area, &time_to_x, text_dim(), duration_s);

                for (src_name, edge) in &comp.edges {
                    let Some(src_scene) = comp.scenes.get(src_name) else { continue };
                    let Some(src_start) = comp.scene_start_times.get(src_name).copied() else { continue };
                    let src_right = time_to_x(src_start + src_scene.duration_s);
                    let Some(tgt_start) = comp.scene_start_times.get(&edge.to_scene).copied() else { continue };
                    let tgt_left = time_to_x(tgt_start);
                    if tgt_left <= src_right { continue; }
                    let cy = bar_area.center().y;
                    painter.line_segment([Pos2::new(src_right, cy), Pos2::new(tgt_left, cy)], Stroke::new(STROKE_WIDTH, TEXT_MUTED));
                    painter.add(egui::Shape::convex_polygon(
                        vec![Pos2::new(tgt_left, cy), Pos2::new(tgt_left - 4.0, cy - 2.5), Pos2::new(tgt_left - 4.0, cy + 2.5)],
                        TEXT_MUTED, Stroke::NONE));
                }

                draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);
                bar_interaction(ui, bar_area, "scene_track", commands);
                painter.line_segment([Pos2::new(playhead_x, bar_area.top() - 2.0), Pos2::new(playhead_x, bar_area.bottom() + 2.0)], Stroke::new(1.5, TEXT_PRIMARY));
                painter.line_segment([Pos2::new(scroll_rect.left(), st_bot), Pos2::new(scroll_rect.right(), st_bot)], Stroke::new(STROKE_WIDTH, BORDER));
            }

            // ── Actor tracks (tree structure, all actors) ──
            for (track_idx, (actor_label, depth)) in actor_tree.iter().enumerate() {
                let is_collapsed = collapsed_actors.contains(actor_label);
                let has_children = timeline.and_then(|tl| tl.get_track(actor_label)).map_or(false, |t| !t.children.is_empty());
                let is_selected = selected_actors.contains(actor_label);
                let at_top = actor_first_top + track_idx as f32 * TRACK_ROW_HEIGHT;
                let at_bot = at_top + TRACK_ROW_HEIGHT;
                let track_rect = Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(scroll_rect.right(), at_bot));

                // Alternating row background
                if track_idx % 2 == 0 {
                    painter.rect_filled(track_rect, 0.0, row_alt());
                }

                // Selection highlight
                if is_selected {
                    painter.rect_filled(track_rect, 0.0, accent_selection());
                    let accent = Rect::from_min_size(track_rect.min, Vec2::new(2.0, track_rect.height()));
                    painter.rect_filled(accent, 0.0, ACCENT_BLUE);
                }

                // Label column background
                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), at_top), Pos2::new(bar_origin_x, at_bot)), 0.0, BG_BASE);

                // Indent based on depth
                let indent = *depth as f32 * 14.0;

                // Chevron toggle button (only if actor has children)
                let chevron_x = scroll_rect.left() + SPACE_S + indent;
                if has_children {
                    let chevron_icon = if is_collapsed {
                        egui_phosphor::regular::CARET_RIGHT
                    } else {
                        egui_phosphor::regular::CARET_DOWN
                    };
                    let chevron_rect = Rect::from_min_size(
                        Pos2::new(chevron_x, at_top + 2.0),
                        Vec2::new(14.0, TRACK_ROW_HEIGHT - 4.0),
                    );
                    let chevron_resp = ui.interact(
                        chevron_rect,
                        ui.id().with(("actor_chevron", actor_label)),
                        Sense::click(),
                    );
                    painter.text(
                        chevron_rect.center(),
                        Align2::CENTER_CENTER,
                        chevron_icon,
                        FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                        if chevron_resp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED },
                    );
                    if chevron_resp.clicked() {
                        if is_collapsed {
                            collapsed_actors.remove(actor_label);
                        } else {
                            collapsed_actors.insert(actor_label.clone());
                        }
                    }
                }

                // Track label
                let label_x = chevron_x + if has_children { 16.0 } else { 4.0 } + SPACE_S;
                let label_text = if actor_label.chars().count() > 16 {
                    actor_label.chars().take(15).collect::<String>() + "…"
                } else {
                    actor_label.clone()
                };
                painter.text(
                    Pos2::new(label_x, track_rect.center().y),
                    Align2::LEFT_CENTER,
                    &label_text,
                    FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                    if is_selected { TEXT_PRIMARY } else { TEXT_SECONDARY },
                );

                let bar_area = Rect::from_min_max(Pos2::new(bar_origin_x, at_top), Pos2::new(scroll_rect.right(), at_bot));

                // Click on label to select actor
                let label_area = Rect::from_min_max(
                    Pos2::new(scroll_rect.left(), at_top),
                    Pos2::new(bar_origin_x, at_bot),
                );
                let label_resp = ui.interact(label_area, ui.id().with(("actor_label", track_idx)), Sense::click());
                if label_resp.clicked() {
                    let modifiers = ui.ctx().input(|i| i.modifiers);
                    let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
                    if multi {
                        if selected_actors.contains(actor_label) {
                            selected_actors.remove(actor_label);
                        } else {
                            selected_actors.insert(actor_label.clone());
                        }
                    } else {
                        selected_actors.clear();
                        selected_actors.insert(actor_label.clone());
                    }
                }

                // Action blocks
                if let Some(tl) = timeline {
                    for event in &tl.action_events {
                        if !event.targets.contains(actor_label) { continue; }
                        let left = time_to_x(event.start_time_ms as f64 / 1000.0);
                        let right = time_to_x((event.start_time_ms + event.duration_ms) as f64 / 1000.0);
                        if right > bar_area.left() && left < bar_area.right() {
                            let br = Rect::from_min_max(
                                Pos2::new(left.max(bar_area.left()), bar_area.top() + 3.0),
                                Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 3.0),
                            );
                            let color = action_category_color(event.category);
                            painter.rect_filled(br, RADIUS_S, color.linear_multiply(0.5));
                            painter.rect_stroke(br, RADIUS_S, Stroke::new(STROKE_WIDTH, color), egui::StrokeKind::Outside);
                            if br.width() > 40.0 {
                                painter.text(br.center(), Align2::CENTER_CENTER, &event.verb, FontId::monospace(FONT_SIZE_XS), TEXT_PRIMARY);
                            }
                        }
                    }
                }

                // Keyframe diamonds (computed from timeline)
                if let Some(tl) = timeline {
                    if let Some(track) = tl.get_track(actor_label) {
                        let kf_props = collect_actor_keyframes(track);
                        for (kf_ms, prop) in kf_props {
                            let kf_s = kf_ms as f64 / 1000.0;
                            let kf_x = time_to_x(kf_s);
                            if kf_x < bar_area.left() || kf_x > bar_area.right() { continue; }
                            let is_act = (kf_s - preview.playback.current_time_s).abs() < 0.01;
                            let is_ms = multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms);
                            let is_drag = kf_drag.as_ref().is_some_and(|(l, _, t, _)| l == actor_label && *t == kf_ms);
                            let ds = if is_drag { KF_DIAMOND_HALF * 1.5 } else { KF_DIAMOND_HALF };
                            let kc = if is_ms { ACCENT_BLUE } else if is_act { TEXT_PRIMARY } else { AMBER };
                            let cy = bar_area.center().y;
                            let dr = Rect::from_center_size(Pos2::new(kf_x, cy), Vec2::new((ds * 3.0).max(16.0), (ds * 3.0).max(16.0)));
                            let dresp = ui.interact(dr, ui.id().with(("kf_diamond", track_idx, kf_ms)), Sense::click_and_drag());
                            painter.add(egui::Shape::convex_polygon(
                                vec![Pos2::new(kf_x, cy - ds), Pos2::new(kf_x + ds, cy), Pos2::new(kf_x, cy + ds), Pos2::new(kf_x - ds, cy)],
                                if dresp.hovered() || is_drag { kc } else { kc.linear_multiply(0.7) }, Stroke::NONE));

                            let dresp = if dresp.hovered() && !is_drag { dresp.on_hover_text(format!("{prop} @ {:.2}s", kf_s)) } else { dresp };

                            dresp.context_menu(|ui| {
                                ui.set_min_width(140.0);
                                ui.strong(format!("{} @ {:.2}s", prop, kf_s));
                                ui.separator();
                                ui.menu_button("Easing", |ui| {
                                    for &(id_str, display_name) in animatix_syntax::easing::EASING_REGISTRY {
                                        if ui.selectable_label(false, display_name).clicked() {
                                            let variant = animatix_syntax::easing::parse_easing_name(id_str).unwrap_or(animatix_syntax::easing::Easing::Linear);
                                            commands.push_back(ShellAction::Command(Command::SetKeyframeEasing {
                                                actor: actor_label.clone(),
                                                property: prop.to_string(),
                                                time_s: kf_s,
                                                easing: variant,
                                            }));
                                            ui.close();
                                        }
                                    }
                                });
                                ui.separator();
                                if ui.button(format!("{} Delete keyframe", egui_phosphor::regular::TRASH)).clicked() {
                                    commands.push_back(ShellAction::Command(Command::DeleteKeyframe {
                                        actor: actor_label.clone(),
                                        property: prop.to_string(),
                                        time_s: kf_s,
                                    }));
                                    ui.close();
                                }
                            });

                            if dresp.clicked() {
                                if shift_held {
                                    if let Some(p) = multi_selected.iter().position(|(l, t)| l == actor_label && *t == kf_ms) { multi_selected.remove(p); }
                                    else { multi_selected.push((actor_label.clone(), kf_ms)); }
                                } else {
                                    multi_selected.clear();
                                    multi_selected.push((actor_label.clone(), kf_ms));
                                    commands.push_back(ShellAction::Command(Command::ScrubTo(kf_s)));
                                }
                            }
                            if dresp.drag_started() {
                                new_kf_drag = Some((actor_label.clone(), prop, kf_ms, kf_s));
                                if !shift_held && !multi_selected.iter().any(|(l, t)| l == actor_label && *t == kf_ms) {
                                    multi_selected.clear();
                                    multi_selected.push((actor_label.clone(), kf_ms));
                                }
                            }
                            if is_drag {
                                if let Some(pos) = dresp.interact_pointer_pos() {
                                    let nt = x_to_time(pos.x);
                                    let snapped = (nt * 10.0).round() / 10.0;
                                    new_kf_drag = Some((actor_label.clone(), prop, kf_ms, snapped));
                                    let gx = time_to_x(snapped);
                                    painter.line_segment([Pos2::new(gx, bar_area.top()), Pos2::new(gx, bar_area.bottom())], Stroke::new(STROKE_WIDTH, AMBER.linear_multiply(0.5)));
                                    let g = painter.layout_no_wrap(format!("{:.1}s → {:.1}s", kf_s, snapped), FontId::monospace(FONT_SIZE_XS), TEXT_PRIMARY);
                                    let tr = Rect::from_min_size(Pos2::new(gx - g.size().x / 2.0, bar_area.top() - 16.0), g.size() + Vec2::new(8.0, 4.0));
                                    painter.rect_filled(tr, RADIUS_S, BG_SURFACE);
                                    painter.galley(tr.min + Vec2::new(4.0, 2.0), g, TEXT_PRIMARY);
                                }
                            }
                            if dresp.drag_stopped() && is_drag {
                                if let Some((ref actor, prop_name, _, n)) = new_kf_drag {
                                    if (n - kf_s).abs() > 0.01 {
                                        commands.push_back(ShellAction::Command(Command::MoveKeyframe {
                                            actor: actor.clone(),
                                            property: prop_name.to_string(),
                                            old_time_s: kf_s,
                                            new_time_s: n,
                                        }));
                                    }
                                }
                                new_kf_drag = None;
                            }
                        }
                    }
                }

                draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, &time_to_x);

                // Track bar interaction (scrubbing)
                let resp = ui.interact(bar_area, ui.id().with(("actor_track", track_idx)), Sense::click_and_drag());
                if resp.clicked() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let click_s = x_to_time(pos.x);
                        commands.push_back(ShellAction::Command(Command::ScrubTo(click_s)));
                    }
                } else if resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let nt = x_to_time(pos.x);
                        commands.push_back(ShellAction::Command(Command::ScrubTo(nt)));
                    }
                }

                // Per-track playhead
                painter.line_segment([Pos2::new(playhead_x, bar_area.top()), Pos2::new(playhead_x, bar_area.bottom())], Stroke::new(STROKE_WIDTH, text_faint()));

                // Track separator
                painter.line_segment([Pos2::new(scroll_rect.left(), at_bot), Pos2::new(scroll_rect.right(), at_bot)], Stroke::new(STROKE_WIDTH, BORDER));
            }

            // ── Range slider for work/export region ──
            {
                painter.rect_filled(Rect::from_min_max(Pos2::new(scroll_rect.left(), rs_top), Pos2::new(bar_origin_x, rs_bot)), 0.0, BG_BASE);
                painter.text(Pos2::new(scroll_rect.left() + SPACE_S, (rs_top + rs_bot) / 2.0), Align2::LEFT_CENTER, "Region",
                    FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional), TEXT_MUTED);

                let range_bar = Rect::from_min_max(Pos2::new(bar_origin_x, rs_top), Pos2::new(scroll_rect.right(), rs_bot));
                let loop_active = preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();

                painter.rect_filled(range_bar, RADIUS_S, BG_WIDGET);

                if loop_active {
                    // Loop is active — show draggable range handles
                    let ws = preview.playback.loop_start_s.unwrap_or(0.0);
                    let we = preview.playback.loop_end_s.unwrap_or(duration_s);
                    let wx = time_to_x(ws);
                    let wy = time_to_x(we);

                    if (wy - wx).abs() > 2.0 {
                        painter.rect_filled(Rect::from_min_max(Pos2::new(wx, range_bar.top() + 2.0), Pos2::new(wy, range_bar.bottom() - 2.0)), RADIUS_S, ACCENT_BLUE.linear_multiply(0.3));
                    }

                    let hs = Vec2::new(12.0, RANGE_HEIGHT);
                    let sh = Rect::from_center_size(Pos2::new(wx, range_bar.center().y), hs);
                    let sr = ui.interact(sh, ui.id().with("range_start_handle"), Sense::click_and_drag());
                    if sr.dragged() {
                        if let Some(pos) = sr.interact_pointer_pos() {
                            preview.playback.loop_start_s = Some(x_to_time(pos.x).min(we - 0.05));
                        }
                    }
                    painter.rect_filled(sh, RADIUS_S, ACCENT_BLUE);

                    let eh = Rect::from_center_size(Pos2::new(wy, range_bar.center().y), hs);
                    let er = ui.interact(eh, ui.id().with("range_end_handle"), Sense::click_and_drag());
                    if er.dragged() {
                        if let Some(pos) = er.interact_pointer_pos() {
                            preview.playback.loop_end_s = Some(x_to_time(pos.x).max(ws + 0.05));
                        }
                    }
                    painter.rect_filled(eh, RADIUS_S, ACCENT_BLUE);
                } else {
                    // Loop is off — show full-duration static indicator
                    painter.rect_filled(range_bar.shrink2(Vec2::new(0.0, 2.0)), RADIUS_S, BG_WIDGET);
                    let mid = range_bar.center();
                    painter.text(mid, Align2::CENTER_CENTER, "Enable loop to set region", FontId::monospace(FONT_SIZE_XS), TEXT_MUTED);
                }
            }

            // ── Global playhead ──
            if playhead_x >= bar_origin_x && playhead_x <= bar_origin_x + bar_width + 2.0 {
                painter.line_segment([Pos2::new(playhead_x, ruler_top), Pos2::new(playhead_x, content_bottom)], Stroke::new(1.5, AMBER));
            }

            // ── Save keyframe drag + multi-select state ──
            ui.data_mut(|d| {
                if let Some(drag) = new_kf_drag.clone() { d.insert_temp(kf_drag_data_id, drag); }
                else { d.remove::<(String, &'static str, u64, f64)>(kf_drag_data_id); }
                d.insert_temp(kf_multi_select_id, multi_selected.clone());
            });

            // Draw clip rect border
            ui.painter().rect_stroke(scroll_rect, 0.0, Stroke::new(STROKE_WIDTH, BORDER), egui::StrokeKind::Inside);
        });
}
