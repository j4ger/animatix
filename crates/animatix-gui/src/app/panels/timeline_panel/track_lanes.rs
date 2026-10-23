//! Timeline track lanes rendering: scene tracks, speech/transcript tracks, action blocks,
//! and empty property lane affordances.

use std::collections::HashMap;

use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::{TimelineCoords, draw_loop_region};
use crate::app::PreviewPaneState;
use crate::app::commands::{
    ActionQueue, Command, DocumentCommand, PlaybackCommand, PropertyEdit, ShellAction,
};
use crate::app::components::text_tooltip;
use crate::app::design_tokens::semantic::{category, timeline};
use crate::app::design_tokens::spatial::timeline::KF_HALF as KF_DIAMOND_HALF;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::timeline_diff::lane_schema;

/// Property groups for per-property lanes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyGroup {
    Transform,
    Style,
    Shape,
    Text,
    Effects,
}

pub(crate) const PROPERTY_GROUPS: &[(PropertyGroup, &str, &[&str])] = &[
    (
        PropertyGroup::Transform,
        "Transform",
        &[
            "position",
            "motion_offset",
            "rotation",
            "scale",
            "size",
            "layout_size",
            "placement_mode",
            "position_binding",
        ],
    ),
    (
        PropertyGroup::Style,
        "Style",
        &[
            "color",
            "opacity",
            "stroke_width",
            "stroke_color",
            "stroke_progress",
            "fill_opacity",
            "line_cap",
            "line_join",
            "morph_options",
        ],
    ),
    (
        PropertyGroup::Shape,
        "Shape",
        &[
            "shape_type",
            "line_from",
            "line_to",
            "head_size",
            "arc_angles",
            "points",
            "commands",
            "vector_paths",
        ],
    ),
    (
        PropertyGroup::Text,
        "Text",
        &["text_content", "font_family", "font_size", "text_paths"],
    ),
];

pub(crate) fn property_group_for_prop(prop: &str) -> Option<PropertyGroup> {
    // Effect stage parameters (`stage.param`) form their own group; their lane
    // names are runtime strings, not registry properties.
    if prop.contains('.') {
        return Some(PropertyGroup::Effects);
    }
    for (group, _, props) in PROPERTY_GROUPS {
        if props.contains(&prop) {
            return Some(*group);
        }
    }
    None
}

pub(crate) fn property_group_color(group: PropertyGroup, theme: eparts::Theme) -> Color32 {
    match group {
        PropertyGroup::Transform => theme.palette.accent.primary,
        PropertyGroup::Style => theme.palette.status.success,
        PropertyGroup::Shape => theme.palette.status.warning,
        PropertyGroup::Text => theme.palette.accent.cyan,
        PropertyGroup::Effects => theme.palette.accent.cyan,
    }
}

pub(crate) fn action_category_color(
    cat: animatix::timeline::ActionCategory,
    theme: eparts::Theme,
) -> Color32 {
    use animatix::timeline::ActionCategory;
    match cat {
        ActionCategory::Entrance => theme.palette.status.success,
        ActionCategory::Motion => theme.palette.accent.primary,
        ActionCategory::Exit => theme.palette.status.error,
        ActionCategory::Effect => theme.palette.status.warning,
        ActionCategory::Reorder => category::ACTION,
        ActionCategory::Reveal => theme.palette.accent.cyan,
    }
}

/// Action block edge for resizing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Edge {
    Left,
    Right,
}

/// Action block drag state tuple:
/// `(track_idx, event_start_ms, edge, initial_pointer_x, original_start_s, original_duration_s)`
pub(crate) type ActionDragState = (usize, u64, Edge, f32, f64, f64);

/// Render colored block segments for each scene in a composition track,
/// with a keyframe density strip and duration label inside each block.
fn render_scene_blocks(
    painter: &egui::Painter,
    composition: &Composition,
    track_rect: Rect,
    coords: &TimelineCoords,
    theme: eparts::Theme,
    label_color: Color32,
    duration_s: f64,
    scene_keyframe_times: &HashMap<String, Vec<f64>>,
) {
    let palette = [
        timeline::track_block_1(),
        timeline::track_block_2(),
        timeline::track_block_3(),
        timeline::track_block_4(),
        timeline::track_block_5(),
    ];
    for (idx, sn) in composition.declaration_order.iter().enumerate() {
        let Some(scene) = composition.scenes.get(sn) else {
            continue;
        };
        let Some(start_s) = composition.scene_start_times.get(sn).copied() else {
            continue;
        };
        let end_s = (start_s + scene.duration_s).min(duration_s);
        if end_s <= start_s {
            continue;
        }
        let sr = Rect::from_min_max(
            Pos2::new(coords.time_to_x(start_s), track_rect.top()),
            Pos2::new(coords.time_to_x(end_s), track_rect.bottom()),
        );
        painter.rect_filled(sr, 2.0, palette[idx % palette.len()]);

        // ── Keyframe density strip ──
        if let Some(times) = scene_keyframe_times.get(sn) {
            let strip_y = sr.bottom() - 6.0;
            let strip_h = 4.0;
            // Semi-transparent background for the density strip
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(sr.left(), strip_y),
                    Pos2::new(sr.right(), strip_y + strip_h),
                ),
                0.0,
                Color32::BLACK.linear_multiply(0.3),
            );
            // Draw tiny vertical marks for each keyframe
            for &kf_time_s in times {
                let kf_x = coords.time_to_x(start_s + kf_time_s);
                if kf_x >= sr.left() && kf_x <= sr.right() {
                    painter.line_segment(
                        [Pos2::new(kf_x, strip_y), Pos2::new(kf_x, strip_y + strip_h)],
                        Stroke::new(1.0, theme.palette.accent.primary),
                    );
                }
            }
        }

        // ── Scene name label ──
        if sr.width() > 24.0 {
            painter.text(
                Pos2::new(sr.center().x, sr.center().y - 2.0),
                Align2::CENTER_CENTER,
                sn.as_str(),
                FontId::monospace(10.0), // 10px mono: no TextRole
                label_color,
            );
        }

        // ── Duration label (bottom-left of block) ──
        if sr.width() > 40.0 {
            let dur = scene.duration_s;
            let dur_text = if dur < 1.0 {
                format!("{}ms", (dur * 1000.0).round() as u64)
            } else {
                format!("{:.1}s", dur)
            };
            painter.text(
                Pos2::new(sr.left() + 4.0, sr.bottom() - 2.0),
                Align2::LEFT_BOTTOM,
                dur_text,
                FontId::monospace(10.0), // 10px mono: no TextRole
                label_color.linear_multiply(0.6),
            );
        }
    }
}

/// Render the composition scene track: scene blocks, transitions, drag reordering, and scrubbing.
pub(crate) fn render_scene_track(
    ui: &mut egui::Ui,
    comp: &Composition,
    st_top: f32,
    st_bot: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    duration_s: f64,
    scene_keyframe_times: &HashMap<String, Vec<f64>>,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    playhead_x: f32,
    panel_id: egui::Id,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let label_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), st_top),
        Pos2::new(coords.bar_origin_x, st_bot),
    );
    painter.rect_filled(label_rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_2, (st_top + st_bot) / 2.0),
        Align2::LEFT_CENTER,
        format!("{} Scenes", egui_phosphor::regular::FILM_STRIP),
        TextRole::BodyS.font_id(),
        theme.palette.text.muted,
    );

    let bar_area = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, st_top),
        Pos2::new(scroll_rect.right(), st_bot),
    );

    // ── Scene block drag state ──
    let scene_drag_id = panel_id.with("scene_drag");
    let scene_drag_data_id = scene_drag_id.with("data");
    let scene_drag: Option<(String, f32)> = ui.data(|d| d.get_temp(scene_drag_data_id));
    let mut new_scene_drag: Option<(String, f32)> = scene_drag.clone();

    render_scene_blocks(
        painter,
        comp,
        bar_area,
        coords,
        theme,
        theme.palette.text.dim,
        duration_s,
        scene_keyframe_times,
    );

    // Draw drag ghost if dragging
    if let Some((ref drag_name, drag_offset_x)) = scene_drag {
        if let Some(scene) = comp.scenes.get(drag_name) {
            if let Some(start_s) = comp.scene_start_times.get(drag_name).copied() {
                let end_s = start_s + scene.duration_s;
                let block_w = coords.time_to_x(end_s) - coords.time_to_x(start_s);
                let ghost_rect = Rect::from_min_size(
                    Pos2::new(bar_area.left() + drag_offset_x, bar_area.top()),
                    Vec2::new(block_w, bar_area.height()),
                );
                painter.rect_filled(
                    ghost_rect,
                    2.0,
                    Color32::from_rgba_premultiplied(60, 130, 230, 80),
                );
                painter.rect_stroke(
                    ghost_rect,
                    2.0,
                    Stroke::new(1.5, theme.palette.accent.primary),
                    egui::StrokeKind::Outside,
                );
                if ghost_rect.width() > 24.0 {
                    painter.text(
                        ghost_rect.center(),
                        Align2::CENTER_CENTER,
                        drag_name.as_str(),
                        FontId::monospace(10.0), // 10px mono: no TextRole
                        theme.palette.accent.primary,
                    );
                }
            }
        }
    }

    for (src_name, edge) in &comp.edges {
        let Some(src_scene) = comp.scenes.get(src_name) else {
            continue;
        };
        let Some(src_start) = comp.scene_start_times.get(src_name).copied() else {
            continue;
        };
        let src_right = coords.time_to_x(src_start + src_scene.duration_s);
        let Some(tgt_start) = comp.scene_start_times.get(&edge.to_scene).copied() else {
            continue;
        };
        let tgt_left = coords.time_to_x(tgt_start);
        if tgt_left <= src_right {
            continue;
        }
        let cy = bar_area.center().y;
        // Draw edge arrow line
        painter.line_segment(
            [Pos2::new(src_right, cy), Pos2::new(tgt_left, cy)],
            Stroke::new(STROKE_WIDTH, theme.palette.text.muted),
        );
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(tgt_left, cy),
                Pos2::new(tgt_left - 4.0, cy - 2.5),
                Pos2::new(tgt_left - 4.0, cy + 2.5),
            ],
            theme.palette.text.muted,
            Stroke::NONE,
        ));

        // ── Transition badge at midpoint ──
        let mid_x = (src_right + tgt_left) / 2.0;
        let badge_r = 7.0;
        let badge_rect =
            Rect::from_center_size(Pos2::new(mid_x, cy), Vec2::new(badge_r * 2.0, badge_r * 2.0));

        // Determine transition icon
        let icon = match edge.transition.id.as_str() {
            "fade" => "F",
            "wipe-left" | "wipe-right" | "wipe-up" | "wipe-down" => "W",
            _ => "C",
        };

        // Badge background circle
        painter.circle_filled(Pos2::new(mid_x, cy), badge_r, theme.palette.surface.surface);
        painter.circle_stroke(
            Pos2::new(mid_x, cy),
            badge_r,
            Stroke::new(1.0, theme.palette.text.muted),
        );

        // Badge icon text
        painter.text(
            badge_rect.center(),
            Align2::CENTER_CENTER,
            icon,
            FontId::monospace(10.0), // 10px mono: no TextRole
            theme.palette.text.primary,
        );

        // Tooltip on hover
        let badge_resp = ui.interact(
            badge_rect,
            ui.id().with(("transition_badge", src_name, &edge.to_scene)),
            Sense::hover(),
        );
        let dur_s = edge.transition.duration_ms as f64 / 1000.0;
        let dur_text = if dur_s < 1.0 {
            format!("{}ms", edge.transition.duration_ms)
        } else {
            format!("{:.1}s", dur_s)
        };
        text_tooltip(
            ui,
            ui.id().with(("transition_tooltip", src_name, &edge.to_scene)),
            &badge_resp,
            &format!("{:?} → {} [{}]", edge.transition.id, edge.to_scene, dur_text),
        );
    }

    draw_loop_region(painter, bar_area.top(), bar_area.bottom(), preview, coords);

    // Scene track interaction (click to scrub, drag scene blocks to reorder)
    let scene_bar_resp =
        ui.interact(bar_area, ui.id().with("scene_track"), Sense::click_and_drag());
    if scene_bar_resp.drag_started() {
        if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
            // Check if click is on a scene block
            for scene_name in &comp.declaration_order {
                if let Some(scene) = comp.scenes.get(scene_name) {
                    if let Some(start_s) = comp.scene_start_times.get(scene_name).copied() {
                        let end_s = start_s + scene.duration_s;
                        let block_x0 = coords.time_to_x(start_s);
                        let block_x1 = coords.time_to_x(end_s);
                        if pos.x >= block_x0 && pos.x <= block_x1 {
                            new_scene_drag = Some((scene_name.clone(), pos.x - bar_area.left()));
                            break;
                        }
                    }
                }
            }
        }
    }
    if scene_bar_resp.dragged() && scene_drag.is_some() {
        if let Some(pos) = scene_bar_resp.interact_pointer_pos() {
            if let Some((ref name, _)) = new_scene_drag {
                let block_w = comp
                    .scenes
                    .get(name)
                    .and_then(|s| {
                        comp.scene_start_times
                            .get(name)
                            .map(|st| coords.time_to_x(st + s.duration_s) - coords.time_to_x(*st))
                    })
                    .unwrap_or(40.0);
                new_scene_drag =
                    Some((name.clone(), (pos.x - bar_area.left() - block_w / 2.0).max(0.0)));
            }
        }
    }
    if scene_bar_resp.drag_stopped() {
        if let Some((ref drag_name, drop_offset_x)) = scene_drag {
            let block_w = comp
                .scenes
                .get(drag_name)
                .and_then(|s| {
                    comp.scene_start_times
                        .get(drag_name)
                        .map(|st| coords.time_to_x(st + s.duration_s) - coords.time_to_x(*st))
                })
                .unwrap_or(80.0);
            let drop_center_x = bar_area.left() + drop_offset_x + block_w / 2.0;
            let drop_time = coords.x_to_time(drop_center_x);

            let mut new_order = comp.declaration_order.clone();
            if let Some(drag_idx) = new_order.iter().position(|n| n == drag_name) {
                let _ = new_order.remove(drag_idx);
                let mut insert_idx = new_order.len();
                for (i, name) in new_order.iter().enumerate() {
                    if let Some(start_s) = comp.scene_start_times.get(name).copied() {
                        let mid_s = start_s
                            + comp.scenes.get(name).map(|s| s.duration_s / 2.0).unwrap_or(0.0);
                        if drop_time < mid_s {
                            insert_idx = i;
                            break;
                        }
                    }
                }
                new_order.insert(insert_idx, drag_name.clone());

                if new_order != comp.declaration_order {
                    commands.push_back(ShellAction::Command(Command::ReorderScenes(new_order)));
                }
            }
        }
        new_scene_drag = None;
    }

    // Persist scene drag state
    ui.data_mut(|d| d.insert_temp(scene_drag_data_id, new_scene_drag));

    painter.line_segment(
        [
            Pos2::new(playhead_x, bar_area.top() - 2.0),
            Pos2::new(playhead_x, bar_area.bottom() + 2.0),
        ],
        Stroke::new(1.5, theme.palette.text.primary),
    );
    painter.line_segment(
        [
            Pos2::new(scroll_rect.left(), st_bot),
            Pos2::new(scroll_rect.right(), st_bot),
        ],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
}

/// Render the speech / audio transcript track (words, markers, timing).
pub(crate) fn render_speech_track(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    sp_top: f32,
    sp_bot: f32,
    scroll_rect: Rect,
    coords: &TimelineCoords,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    playhead_x: f32,
    panel_id: egui::Id,
    theme: eparts::Theme,
    sp: &Spatial,
) {
    let painter = ui.painter();
    let label_rect = Rect::from_min_max(
        Pos2::new(scroll_rect.left(), sp_top),
        Pos2::new(coords.bar_origin_x, sp_bot),
    );
    painter.rect_filled(label_rect, 0.0, theme.palette.surface.base);
    painter.text(
        Pos2::new(scroll_rect.left() + sp.base.space_2, (sp_top + sp_bot) / 2.0),
        Align2::LEFT_CENTER,
        format!("{} Speech", egui_phosphor::regular::TEXT_ALIGN_CENTER),
        TextRole::BodyS.font_id(),
        theme.palette.text.muted,
    );

    let bar_area = Rect::from_min_max(
        Pos2::new(coords.bar_origin_x, sp_top),
        Pos2::new(scroll_rect.right(), sp_bot),
    );
    painter.rect_filled(bar_area, 0.0, theme.components.list.odd.bg);

    let current_time_s = preview.playback.current_time_s();

    // Background click to seek
    let speech_resp =
        ui.interact(bar_area, panel_id.with("speech_track_bar"), Sense::click_and_drag());
    if speech_resp.clicked() || speech_resp.dragged() {
        if let Some(pos) = speech_resp.interact_pointer_pos() {
            let t = coords.x_to_time(pos.x);
            commands.push_back(PlaybackCommand::ScrubTo(t).into());
        }
    }

    if let Some(tl) = timeline {
        for (actor_id, transcript) in tl.transcripts_iter() {
            let actor_label = tl.get_entity_label(actor_id).unwrap_or("");
            // Draw words
            for seg in &transcript.segments {
                if !seg.words.is_empty() {
                    for w in &seg.words {
                        let x0 = coords.time_to_x(w.start_s);
                        let x1 = coords.time_to_x(w.end_s);
                        if x1 < coords.bar_origin_x || x0 > scroll_rect.right() {
                            continue;
                        }
                        let pill_min_x = x0.max(coords.bar_origin_x);
                        let pill_max_x = x1.min(scroll_rect.right()).max(pill_min_x + 4.0);
                        let pill_rect = Rect::from_min_max(
                            Pos2::new(pill_min_x, sp_top + 3.0),
                            Pos2::new(pill_max_x, sp_bot - 3.0),
                        );

                        let is_active = current_time_s >= w.start_s && current_time_s < w.end_s;
                        let (bg_color, text_color, border_stroke) = if is_active {
                            (
                                theme.palette.accent.primary,
                                theme.palette.text.primary,
                                Stroke::new(1.0, theme.palette.accent.primary),
                            )
                        } else {
                            (
                                theme.palette.surface.widget,
                                theme.palette.text.secondary,
                                Stroke::new(1.0, theme.palette.border.subtle),
                            )
                        };

                        painter.rect_filled(pill_rect, 3.0, bg_color);
                        painter.rect_stroke(
                            pill_rect,
                            3.0,
                            border_stroke,
                            egui::StrokeKind::Inside,
                        );

                        if pill_rect.width() > 14.0 {
                            let galley = painter.layout_no_wrap(
                                w.word.clone(),
                                FontId::monospace(9.0),
                                text_color,
                            );
                            if galley.size().x <= pill_rect.width() - 4.0 {
                                painter.galley(
                                    Pos2::new(
                                        pill_rect.center().x - galley.size().x / 2.0,
                                        pill_rect.center().y - galley.size().y / 2.0,
                                    ),
                                    galley,
                                    text_color,
                                );
                            } else if pill_rect.width() > 18.0 {
                                painter.text(
                                    pill_rect.center(),
                                    Align2::CENTER_CENTER,
                                    "…",
                                    FontId::monospace(9.0),
                                    text_color,
                                );
                            }
                        }

                        let word_id = panel_id.with((
                            "speech_word",
                            actor_label,
                            w.word.as_str(),
                            w.start_s.to_bits(),
                        ));
                        let w_resp = ui.interact(pill_rect, word_id, Sense::click());
                        if w_resp.clicked() {
                            commands.push_back(PlaybackCommand::ScrubTo(w.start_s).into());
                        }
                        text_tooltip(
                            ui,
                            word_id.with("tip"),
                            &w_resp,
                            &format!("\"{}\"  ({:.2}s – {:.2}s)", w.word, w.start_s, w.end_s),
                        );
                    }
                } else {
                    // Line-level cue fallback
                    let x0 = coords.time_to_x(seg.start_s);
                    let x1 = coords.time_to_x(seg.end_s);
                    if x1 >= coords.bar_origin_x && x0 <= scroll_rect.right() {
                        let seg_min_x = x0.max(coords.bar_origin_x);
                        let seg_max_x = x1.min(scroll_rect.right()).max(seg_min_x + 8.0);
                        let seg_rect = Rect::from_min_max(
                            Pos2::new(seg_min_x, sp_top + 4.0),
                            Pos2::new(seg_max_x, sp_bot - 4.0),
                        );
                        painter.rect_filled(seg_rect, 2.0, theme.palette.surface.widget);
                        painter.rect_stroke(
                            seg_rect,
                            2.0,
                            Stroke::new(1.0, theme.palette.border.subtle),
                            egui::StrokeKind::Inside,
                        );
                        let galley = painter.layout_no_wrap(
                            seg.text.clone(),
                            FontId::monospace(9.0),
                            theme.palette.text.secondary,
                        );
                        if galley.size().x <= seg_rect.width() - 4.0 {
                            painter.galley(
                                Pos2::new(
                                    seg_rect.min.x + 3.0,
                                    seg_rect.center().y - galley.size().y / 2.0,
                                ),
                                galley,
                                theme.palette.text.secondary,
                            );
                        }
                    }
                }
            }

            // Draw markers (@intro, @feature, etc.)
            for (marker_name, &marker_time) in &transcript.markers {
                let mx = coords.time_to_x(marker_time);
                if mx < coords.bar_origin_x || mx > scroll_rect.right() {
                    continue;
                }
                let marker_color = theme.palette.status.warning;
                painter.line_segment(
                    [Pos2::new(mx, sp_top + 10.0), Pos2::new(mx, sp_bot)],
                    Stroke::new(1.0, marker_color.linear_multiply(0.7)),
                );
                let pts = vec![
                    Pos2::new(mx, sp_top + 1.0),
                    Pos2::new(mx + 4.0, sp_top + 5.5),
                    Pos2::new(mx, sp_top + 10.0),
                    Pos2::new(mx - 4.0, sp_top + 5.5),
                ];
                painter.add(egui::Shape::convex_polygon(pts, marker_color, Stroke::NONE));

                let marker_id = panel_id.with(("speech_marker", actor_label, marker_name.as_str()));
                let m_rect =
                    Rect::from_min_max(Pos2::new(mx - 5.0, sp_top), Pos2::new(mx + 5.0, sp_bot));
                let m_resp = ui.interact(m_rect, marker_id, Sense::click());
                if m_resp.clicked() {
                    commands.push_back(PlaybackCommand::ScrubTo(marker_time).into());
                }
                text_tooltip(
                    ui,
                    marker_id.with("tip"),
                    &m_resp,
                    &format!("@{}  ({:.2}s)", marker_name, marker_time),
                );
            }
        }
    }

    // Playhead tick
    painter.line_segment(
        [Pos2::new(playhead_x, sp_top), Pos2::new(playhead_x, sp_bot)],
        Stroke::new(1.5, theme.palette.text.primary),
    );
    // Border bottom
    painter.line_segment(
        [
            Pos2::new(scroll_rect.left(), sp_bot),
            Pos2::new(scroll_rect.right(), sp_bot),
        ],
        Stroke::new(STROKE_WIDTH, theme.palette.border.default),
    );
}

/// Render action blocks for a track row.
pub(crate) fn render_action_blocks(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    actor_label: &str,
    track_idx: usize,
    bar_area: Rect,
    coords: &TimelineCoords,
    action_drag: Option<&ActionDragState>,
    new_action_drag: &mut Option<ActionDragState>,
    commands: &mut ActionQueue,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    for event in &timeline.action_events {
        if !event.targets.iter().any(|t| t == actor_label) {
            continue;
        }
        let start_s = event.start_time_ms as f64 / 1000.0;
        let end_s = (event.start_time_ms + event.duration_ms) as f64 / 1000.0;
        let duration_s = event.duration_ms as f64 / 1000.0;
        let left = coords.time_to_x(start_s);
        let right = coords.time_to_x(end_s);
        if right > bar_area.left() && left < bar_area.right() {
            let br = Rect::from_min_max(
                Pos2::new(left.max(bar_area.left()), bar_area.top() + 3.0),
                Pos2::new(right.min(bar_area.right()), bar_area.bottom() - 3.0),
            );
            let color = action_category_color(event.category, theme);

            // Check if this block is being dragged
            let is_action_drag = action_drag
                .is_some_and(|(ti, ms, _, _, _, _)| *ti == track_idx && *ms == event.start_time_ms);

            // Draw drag handles (left/right edges)
            let handle_w = 8.0;
            if br.width() > handle_w * 2.0 + 4.0 {
                let left_handle =
                    Rect::from_min_max(br.left_top(), Pos2::new(br.left() + handle_w, br.bottom()));
                let right_handle = Rect::from_min_max(
                    Pos2::new(br.right() - handle_w, br.top()),
                    br.right_bottom(),
                );
                let handle_color = if is_action_drag {
                    theme.palette.accent.primary
                } else {
                    color.linear_multiply(0.7)
                };
                painter.rect_filled(left_handle, RADIUS_S, handle_color);
                painter.rect_filled(right_handle, RADIUS_S, handle_color);
            }

            // Draw main block body
            painter.rect_filled(br, RADIUS_S, color.linear_multiply(0.5));
            painter.rect_stroke(
                br,
                RADIUS_S,
                Stroke::new(
                    if is_action_drag { 2.0 } else { STROKE_WIDTH },
                    if is_action_drag {
                        theme.palette.accent.primary
                    } else {
                        color
                    },
                ),
                egui::StrokeKind::Outside,
            );
            if br.width() > 40.0 {
                painter.text(
                    br.center(),
                    Align2::CENTER_CENTER,
                    &event.verb,
                    FontId::monospace(10.0), // 10px mono: no TextRole
                    theme.palette.text.primary,
                );
            }

            // Interaction: click_and_drag for resize
            let action_resp = ui.interact(
                br,
                ui.id().with(("action_block", track_idx, event.start_time_ms)),
                Sense::click_and_drag(),
            );

            // Drag start: detect which edge by proximity
            if action_resp.drag_started() {
                if let Some(pos) = action_resp.interact_pointer_pos() {
                    let dist_left = (pos.x - br.left()).abs();
                    let dist_right = (pos.x - br.right()).abs();
                    let edge = if dist_left <= dist_right {
                        Edge::Left
                    } else {
                        Edge::Right
                    };
                    *new_action_drag =
                        Some((track_idx, event.start_time_ms, edge, pos.x, start_s, duration_s));
                }
            }

            // During drag: visual feedback
            if is_action_drag {
                if let Some(&(_, _, edge, init_x, orig_start, orig_dur)) = action_drag {
                    if let Some(pos) = action_resp.interact_pointer_pos() {
                        let dx = pos.x - init_x;
                        let dt = (dx / (coords.bar_width / coords.visible_s as f32)) as f64;
                        match edge {
                            Edge::Right => {
                                let new_end_x = coords.time_to_x(orig_start + orig_dur + dt);
                                painter.line_segment(
                                    [
                                        Pos2::new(new_end_x, br.top()),
                                        Pos2::new(new_end_x, br.bottom()),
                                    ],
                                    Stroke::new(2.0, theme.palette.accent.primary),
                                );
                                let new_dur = (orig_dur + dt).max(0.1);
                                let dur_text = if new_dur < 1.0 {
                                    format!("{}ms", (new_dur * 1000.0).round())
                                } else {
                                    format!("{:.2}s", new_dur)
                                };
                                painter.text(
                                    Pos2::new(new_end_x, br.top() - 4.0),
                                    Align2::CENTER_BOTTOM,
                                    dur_text,
                                    FontId::monospace(10.0), // 10px mono: no TextRole
                                    theme.palette.accent.primary,
                                );
                            },
                            Edge::Left => {
                                let new_start_x = coords.time_to_x(orig_start + dt);
                                painter.line_segment(
                                    [
                                        Pos2::new(new_start_x, br.top()),
                                        Pos2::new(new_start_x, br.bottom()),
                                    ],
                                    Stroke::new(2.0, theme.palette.accent.primary),
                                );
                            },
                        }
                    }
                }
            }

            // Drag stop: emit ResizeAction command
            if action_resp.drag_stopped() {
                if let Some((_, _, edge, init_x, orig_start, orig_dur)) = *new_action_drag {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        let dx = pos.x - init_x;
                        let dt = (dx / (coords.bar_width / coords.visible_s as f32)) as f64;
                        let (new_start_s, new_duration_s) = match edge {
                            Edge::Right => (orig_start, (orig_dur + dt).max(0.1)),
                            Edge::Left => {
                                let ns = (orig_start + dt).max(0.0);
                                (ns, (orig_dur - dt).max(0.1))
                            },
                        };
                        commands.push_back(ShellAction::Command(Command::ResizeAction {
                            verb: event.verb.clone(),
                            targets: event.targets.clone(),
                            old_start_s: orig_start,
                            new_start_s,
                            new_duration_s,
                        }));
                    }
                }
                *new_action_drag = None;
            }

            // Tooltip on hover (when not dragging)
            if !is_action_drag {
                text_tooltip(
                    ui,
                    action_resp.id.with("tooltip"),
                    &action_resp,
                    &format!(
                        "{:?}: {}\n{:.2}s → {:.2}s\nDrag edges to resize\nTargets: {}",
                        event.category,
                        event.verb,
                        start_s,
                        end_s,
                        event.targets.join(", ")
                    ),
                );
            }
        }
    }
}

/// Render an empty property lane: click affordance and ghost diamond to key the property at the playhead.
pub(crate) fn render_empty_property_lane(
    ui: &mut egui::Ui,
    track: &animatix::timeline::AnimationTrack,
    actor_label: &str,
    prop_name: &str,
    track_idx: usize,
    prop_bar_area: Rect,
    coords: &TimelineCoords,
    group_col: Color32,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    theme: eparts::Theme,
) {
    let painter = ui.painter();
    let playhead_s = preview.playback.current_time_s();
    let add_id = ui.id().with(("lane_add", track_idx, prop_name));
    let add_resp = ui.interact(prop_bar_area, add_id, Sense::click());
    if add_resp.hovered() {
        painter.rect_filled(
            prop_bar_area,
            0.0,
            theme.palette.accent.selection.linear_multiply(0.4),
        );
    }
    // Ghost diamond at the playhead as an affordance.
    let gx = coords.time_to_x(playhead_s);
    if gx >= prop_bar_area.left() && gx <= prop_bar_area.right() {
        painter.add(egui::Shape::convex_polygon(
            vec![
                Pos2::new(gx, prop_bar_area.center().y - KF_DIAMOND_HALF),
                Pos2::new(gx + KF_DIAMOND_HALF, prop_bar_area.center().y),
                Pos2::new(gx, prop_bar_area.center().y + KF_DIAMOND_HALF),
                Pos2::new(gx - KF_DIAMOND_HALF, prop_bar_area.center().y),
            ],
            group_col.linear_multiply(0.35),
            Stroke::new(STROKE_WIDTH, group_col),
        ));
    }
    text_tooltip(
        ui,
        add_id.with("tooltip"),
        &add_resp,
        &format!("{}: add keyframe at playhead", prop_name),
    );
    if add_resp.clicked() {
        if prop_name.contains('.') {
            let value = track
                .effects
                .sample_lane(prop_name, (playhead_s * 1000.0) as u64)
                .unwrap_or(animatix::timeline::PropertyValue::F32(0.0));
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: prop_name.to_string(),
                    value,
                    create_keyframe: true,
                    time_s: Some(playhead_s),
                })
                .into(),
            );
        } else if let Some(schema) = lane_schema(&track.caps, &track.actor_type, prop_name) {
            let value = animatix::timeline::read_property_value_or_default(
                track,
                schema,
                (playhead_s * 1000.0) as u64,
            );
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    actor: actor_label.to_string(),
                    property: schema.name.to_string(),
                    value,
                    create_keyframe: true,
                    time_s: Some(playhead_s),
                })
                .into(),
            );
        } else {
            tracing::warn!(
                property = %prop_name,
                "no writable schema for property lane; cannot add keyframe"
            );
        }
    }
}
